import { createHash, randomUUID } from "node:crypto";
import { inspect, isDeepStrictEqual } from "node:util";
import type { AccountContext } from "../identity/account-context.ts";
import { AccountContextService } from "../identity/account-context.ts";
import { executedBatch, presentationProjection, creatorSavePresentationProjection, ownerBatchPresentationProjection, resultBinding, resultText, resultContinuationRequest, terminalResult, type ExecutedBatch, type ExecutedCallOutcome, type PresentationProjection, type ResultBinding, type TerminalResult, type ResultContinuationRequest } from "./contracts.ts";
import { OwnerReviewContext, PeerResumePayload } from "./owner-context.ts";
import { ResultContinuation } from "./continuation.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { creatorSaveReview, creatorSaveSelection, type CreatorSaveReview, type CreatorSaveSelection } from "./creator-save-contracts.ts";
import { searchPlanEditorDraft, applySearchPlanPatch, type SearchPlanEditorDraft } from "../discovery/search-plan-contracts.ts";
import { searchPlanProjection } from "./search-plan-projection.ts";
import { ResultStoreBudget } from "./budget.ts";

export interface OwnerReviewAuthority {
  readonly kind: "creator_batch" | "creator_save" | "search_plan" | "operation";
  readonly deadline: number;
  encodedPayload(): Buffer;
  select?(selections: readonly CreatorSaveSelection[]): OwnerReviewAuthority;
  revise?(draft: SearchPlanEditorDraft): OwnerReviewAuthority;
}
export interface PreparedOwnerReview {
  readonly context: OwnerReviewContext;
  readonly presentation: PresentationProjection;
  readonly prompt?: string;
  readonly label?: string;
  readonly review?: CreatorSaveReview;
  readonly draft?: SearchPlanEditorDraft;
  readonly action: OwnerReviewAuthority;
}
export interface OwnerCallCompletion {
  output: unknown;
  continue_batch: boolean;
  outcome: ExecutedCallOutcome;
  presentation: PresentationProjection;
  terminal: TerminalResult;
  continuation: unknown | null;
}
const issuer = Symbol("submitted-owner-grant");
interface GrantContents {
  binding: ResultBinding;
  context: OwnerReviewContext;
  account: AccountContext;
  authority: OwnerReviewAuthority;
  peer: PeerResumePayload | undefined;
  locale: "en-US" | "zh-CN";
  assertValid(): void;
}
export class SubmittedOwnerGrant {
  readonly #contents: GrantContents;
  #consumed = false;
  constructor(token: symbol, contents: GrantContents) {
    if (token !== issuer) throw new Error("owner_grant_untrusted");
    this.#contents = contents;
    Object.freeze(this);
  }
  binding(): ResultBinding { return structuredClone(this.#contents.binding); }
  context(): OwnerReviewContext { return this.#contents.context; }
  locale(): "en-US" | "zh-CN" { return this.#contents.locale; }
  peerResume(): PeerResumePayload | undefined { return this.#contents.peer; }
  consume(expectedKind: OwnerReviewAuthority["kind"], context: AccountContext): OwnerReviewAuthority {
    if (this.#consumed) throw new Error("owner_grant_consumed");
    this.#contents.assertValid();
    if (context !== this.#contents.account || context.signal.aborted) throw new Error("access.account_changed");
    if (this.#contents.authority.kind !== expectedKind) throw new Error("owner_grant_kind_mismatch");
    this.#consumed = true;
    return this.#contents.authority;
  }
  toJSON(): never { throw new Error("Submitted authority must not be serialized"); }
  [inspect.custom](): string { return "SubmittedOwnerGrant { authority: [private] }"; }
}

interface ConfirmationRecord {
  binding: ResultBinding;
  owner: AccountContext;
  context: OwnerReviewContext;
  batch: ExecutedBatch;
  presentation: PresentationProjection;
  prompt: string;
  label: string;
  saveReview: CreatorSaveReview | undefined;
  searchDraft?: SearchPlanEditorDraft | undefined;
  action: OwnerReviewAuthority | undefined;
  peer: PeerResumePayload | undefined;
  deadline: number;
  presented: boolean;
  locale: "en-US" | "zh-CN";
  state: { kind: "available" } | { kind: "submitted"; action: { kind: "confirmed" } } | TerminalResult;
  accessedAt: number;
  changedAt: number;
  bytes: number;
  completedSections: number;
  continuation: ResultContinuation;
}
function digest(bytes: Uint8Array): string { return `sha256:${createHash("sha256").update(bytes).digest("hex")}`; }
function jsonBytes(value: unknown): Buffer { return Buffer.from(stringifyApplicationJson(value)); }
function ownerDigest(action: OwnerReviewAuthority, context: OwnerReviewContext, peer?: PeerResumePayload): string {
  const hash = createHash("sha256");
  for (const bytes of [Buffer.from(`${action.kind}_v1`), action.encodedPayload(), jsonBytes(context), peer?.copyBytes() ?? Buffer.alloc(0)]) {
    const length = Buffer.alloc(8); length.writeBigUInt64BE(BigInt(bytes.length));
    hash.update(length).update(bytes);
  }
  return `sha256:${hash.digest("hex")}`;
}
function prepareReview(review: PreparedOwnerReview, frame: ExecutedBatch, peer?: PeerResumePayload) {
  OwnerReviewContext.assertIssued(review.context);
  const batch = executedBatch.parse(frame);
  review.context.validatePeer(batch, peer);
  const saveReview = review.action.kind === "creator_save" ? creatorSaveReview.parse(review.review) : undefined;
  const searchDraft = review.action.kind === "search_plan" ? searchPlanEditorDraft.parse(review.draft) : undefined;
  if (saveReview && saveReview.summary.eligible_new + saveReview.summary.eligible_update === 0) throw new Error("creator_save_review_not_actionable");
  if (!["creator_batch", "creator_save", "search_plan", "operation"].includes(review.action.kind)) throw new Error("owner_review_kind_not_implemented");
  if (searchDraft && (!review.action.revise || !review.action.encodedPayload().equals(jsonBytes(searchDraft)))) throw new Error("search_plan_payload_mismatch");
  if (saveReview !== undefined && typeof review.action.select !== "function") throw new Error("creator_save_review_required");
  const presentation = (saveReview ? creatorSavePresentationProjection : presentationProjection).parse(review.presentation);
  if (searchDraft && !isDeepStrictEqual(presentation, searchPlanProjection(searchDraft))) throw new Error("search_plan_projection_mismatch");
  const prompt = saveReview || searchDraft ? "" : resultText.parse(review.prompt), label = saveReview || searchDraft ? "" : resultText.parse(review.label);
  if (!Number.isFinite(review.action.deadline) || performance.now() >= review.action.deadline) throw new Error("result_expired");
  const opaque = review.action.encodedPayload();
  const actionBudget = saveReview ? 24 * 1024 * 1024 : 1048576 + 64 * 1024;
  if (opaque.length === 0 || opaque.length > actionBudget) throw new Error("owner_action_payload");
  const payload_digest = ownerDigest(review.action, review.context, peer);
  const pending = saveReview ? { kind: "creator_save_batch", review: saveReview, payload_digest } : searchDraft ? { kind: "search_plan", draft: searchDraft, payload_digest } : { kind: "confirm", prompt, label, payload_digest };
  const initial = { kind: "available" as const };
  const heldBytes = opaque.length + jsonBytes(review.context).length + (peer?.copyBytes().length ?? 0);
  const bytes = jsonBytes([batch, presentation, pending, initial]).length + heldBytes;
  return { batch, saveReview, searchDraft, presentation, prompt, label, pending, initial, heldBytes, bytes };
}

// 密封业务确认共用的进程内存储。
export class OwnerConfirmationStore {
  readonly #accounts: AccountContextService;
  readonly #records = new Map<string, ConfirmationRecord>();
  readonly #admittedActions = new WeakSet<OwnerReviewAuthority>();
  #bytes = 0;
  #closed = false;
  readonly #budget: ResultStoreBudget;
  constructor(accounts: AccountContextService, budget = new ResultStoreBudget()) { this.#accounts = accounts; this.#budget = budget; }

  mint(review: PreparedOwnerReview, frame: ExecutedBatch, context: AccountContext, peer?: PeerResumePayload): string {
    this.#assertOpen(context);
    const { batch, saveReview, searchDraft, presentation, prompt, label, pending, initial, bytes } = prepareReview(review, frame, peer);
    if (this.#admittedActions.has(review.action)) throw new Error("owner_review_already_admitted");
    const reference = `rr_${randomUUID().replaceAll("-", "")}`;
    const view = { identity: `rv_${randomUUID().replaceAll("-", "")}`, kind: "confirmation" as const };
    this.cleanup();
    if (this.#records.size >= 10000) throw new Error("result_row_limit");
    if (this.#bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes, 1);
    const fingerprint = digest(jsonBytes({ result_ref: reference, view, origin: review.context.origin, batch, presentation, pending_action: pending, initial_state: initial }));
    const now = performance.now();
    this.#records.set(reference, { binding: { result_ref: reference, view, origin: review.context.origin, fingerprint, revision: 1n }, owner: context, context: review.context, batch, presentation, prompt, label, saveReview, searchDraft, action: review.action, peer, deadline: review.action.deadline, presented: false, locale: "en-US", state: initial, accessedAt: now, changedAt: now, bytes, completedSections: 0, continuation: new ResultContinuation() });
    this.#admittedActions.add(review.action);
    this.#changeBytes(bytes, 1);
    return reference;
  }

  mintCompleted(ownerContext: OwnerReviewContext, completion: OwnerCallCompletion, frame: ExecutedBatch, context: AccountContext): string {
    this.#assertOpen(context);
    OwnerReviewContext.assertIssued(ownerContext);
    const batch = executedBatch.parse(frame);
    const call = batch.outcomes.find(value => value.call_id === ownerContext.call_id);
    if (!call || call.operation_id !== ownerContext.operation_id || !isDeepStrictEqual(call, completion.outcome)) throw new Error("owner_review_call_mismatch");
    const presentation = creatorSavePresentationProjection.parse(completion.presentation), state = terminalResult.parse(completion.terminal);
    const continuation = completion.continuation === null ? null : resultContinuationRequest.parse(completion.continuation);
    const bytes = jsonBytes([batch, presentation, state, continuation]).length;
    this.cleanup();
    if (this.#records.size >= 10000) throw new Error("result_row_limit");
    if (this.#bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes, 1);
    const reference = `rr_${randomUUID().replaceAll("-", "")}`;
    const view = { identity: `rv_${randomUUID().replaceAll("-", "")}`, kind: "read_only" as const };
    const fingerprint = digest(jsonBytes({ result_ref: reference, view, origin: ownerContext.origin, batch, presentation, pending_action: null, initial_state: state }));
    const nextContinuation = new ResultContinuation(); nextContinuation.apply(continuation);
    const now = performance.now();
    this.#records.set(reference, { binding: { result_ref: reference, view, origin: ownerContext.origin, fingerprint, revision: 1n }, owner: context, context: ownerContext, batch, presentation, prompt: "", label: "", saveReview: undefined, action: undefined, peer: undefined, deadline: Infinity, presented: false, locale: "en-US", state, accessedAt: now, changedAt: now, bytes, completedSections: presentation.sections.length, continuation: nextContinuation });
    this.#changeBytes(bytes, 1);
    return reference;
  }

  present(reference: string, origin: string, context: AccountContext, locale: "en-US" | "zh-CN" = "en-US") {
    const record = this.#get(reference, context);
    if (record.binding.origin !== origin) throw new Error("result_origin_mismatch");
    if (record.presented) throw new Error("result_already_presented");
    record.binding.revision = this.#next(record);
    record.presented = true;
    record.locale = locale;
    record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  read(binding: ResultBinding, context: AccountContext) {
    const record = this.#bound(binding, context);
    if (!record.presented) throw new Error("result_not_presented");
    record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  current(bindingInput: ResultBinding, context: AccountContext) {
    const binding = resultBinding.parse(bindingInput), record = this.#get(binding.result_ref, context);
    if (record.binding.view.identity !== binding.view.identity || record.binding.view.kind !== binding.view.kind) throw new Error("result_view_mismatch");
    if (record.binding.origin !== binding.origin) throw new Error("result_origin_mismatch");
    if (record.binding.fingerprint !== binding.fingerprint) throw new Error("result_fingerprint_mismatch");
    if (!record.presented) throw new Error("result_not_presented");
    record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  requestEdit(binding: ResultBinding, context: AccountContext, instructionInput: string) {
    if (!instructionInput.isWellFormed() || /\p{Cc}/u.test(instructionInput)) throw new Error("edit_instruction_control");
    const instruction = instructionInput.trim();
    if (!instruction || Buffer.byteLength(instruction) > 2048) throw new Error("edit_instruction");
    const record = this.#bound(binding, context);
    if (!record.presented) throw new Error("result_not_presented");
    if (record.state.kind !== "available" || !record.action) throw new Error("result_action_consumed");
    const terminal = terminalResult.parse({ kind: "resolved", code: "edit_requested", summary: "The user requested changes. The pending action was not executed." });
    const lead = record.locale === "zh-CN" ? "用户要求修改，原操作未执行。修改要求：" : "The user requested changes; the pending action was not executed. Requested changes:";
    const continuation = resultContinuationRequest.parse({ message: `${lead}\n\n${instruction}`, requires_model_context: false, context: { kind: "edit_requested" } });
    const bytes = jsonBytes([record.batch, record.presentation, terminal, continuation]).length;
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes - record.bytes);
    const revision = this.#next(record);
    record.continuation.apply(continuation);
    this.#changeBytes(bytes - record.bytes); record.bytes = bytes;
    record.binding.revision = revision; record.state = terminal; record.action = undefined;
    record.saveReview = undefined; record.searchDraft = undefined; record.peer = undefined;
    record.changedAt = record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  cancel(binding: ResultBinding, context: AccountContext, continuationInput: unknown | null = null) {
    const record = this.#bound(binding, context);
    if (!record.presented) throw new Error("result_not_presented");
    if (record.state.kind !== "available" || !record.action) throw new Error("result_action_consumed");
    let continuation = continuationInput === null ? null : resultContinuationRequest.parse(continuationInput);
    if (continuation !== null && continuation.context.kind !== "cancelled") throw new Error("continuation_action_mismatch");
    const batch = structuredClone(record.batch);
    const mixed = batch.outcomes.some(call => call.disposition.kind === "not_run") && (record.completedSections > 0 || batch.outcomes.some(call => call.disposition.kind === "succeeded"));
    let summary = "The user cancelled this action.";
    if (mixed) {
      if (batch.outcomes.length > 8) throw new Error("cancel_execution_batch");
      const chinese = record.locale === "zh-CN";
      summary = chinese ? "已取消待执行的操作。已完成的调用及完整结果详情已保留。" : "The pending action was cancelled. Completed calls and full result details are retained.";
      for (const call of batch.outcomes) if (call.disposition.kind === "not_run") {
        call.disposition = { kind: "not_run", reason: "user_cancelled" };
        call.summary = chinese ? "后续操作已取消，此调用未执行。" : "Not run because the remaining actions were cancelled.";
      }
      if (continuation !== null) continuation = resultContinuationRequest.parse({
        message: chinese ? "待执行的操作及后续调用已取消，先前已完成的调用不应重复。完整结果详情仍保留在此结果中；后续上下文仅包含有限数量的资源引用。" : "The pending action and remaining calls were cancelled. Earlier completed calls must not be repeated. Full result details remain on this result; this continuation contains only a bounded selection of resource references.",
        requires_model_context: true,
        context: { kind: "execution", outcomes: batch.outcomes.map(call => ({ ...call, resource_refs: call.resource_refs.slice(0, 64).map(resource => ({ ...resource, label: null })) })) },
      });
    }
    const terminal: TerminalResult = { kind: "resolved", code: "user_cancelled", summary };
    const bytes = jsonBytes([batch, record.presentation, terminal, continuation]).length;
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes - record.bytes);
    const revision = this.#next(record);
    record.continuation.apply(continuation);
    this.#changeBytes(bytes - record.bytes);
    record.bytes = bytes; record.binding.revision = revision; record.action = undefined; record.saveReview = undefined; record.searchDraft = undefined; record.peer = undefined;
    record.batch = batch; record.state = terminal; record.changedAt = record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  submit(binding: ResultBinding, context: AccountContext, selections?: readonly CreatorSaveSelection[]): SubmittedOwnerGrant {
    const record = this.#bound(binding, context);
    if (!record.presented) throw new Error("result_not_presented");
    if (record.state.kind !== "available" || !record.action) throw new Error("result_action_consumed");
    if (performance.now() >= record.deadline) throw new Error("result_expired");
    let authority = record.action;
    if (record.saveReview) {
      if (selections === undefined || typeof authority.select !== "function") throw new Error("owner_review_selection_mismatch");
      authority = authority.select(selections.map(value => creatorSaveSelection.parse(value)));
      if (authority.kind !== "creator_save" || authority.deadline !== record.deadline) throw new Error("owner_review_selection_mismatch");
      if (authority.encodedPayload().length > 24 * 1024 * 1024) throw new Error("owner_action_payload");
    } else if (selections !== undefined) throw new Error("owner_review_selection_mismatch");
    const revision = this.#next(record);
    record.action = undefined;
    record.saveReview = undefined;
    record.state = { kind: "submitted", action: { kind: "confirmed" } };
    record.binding.revision = revision;
    record.changedAt = record.accessedAt = performance.now();
    const submittedBinding = structuredClone(record.binding);
    return new SubmittedOwnerGrant(issuer, { binding: submittedBinding, account: context, context: record.context, authority, peer: record.peer, locale: record.locale,
      assertValid: () => { const current = this.#bound(submittedBinding, context); if (current.state.kind !== "submitted") throw new Error("owner_grant_consumed"); },
    });
  }

  reviseSearchPlan(binding: ResultBinding, context: AccountContext, patch: unknown) {
    const record = this.#bound(binding, context);
    if (!record.presented) throw new Error("result_not_presented");
    if (record.state.kind !== "available" || !record.action || !record.searchDraft || !record.action.revise) throw new Error("search_plan_required");
    const draft = applySearchPlanPatch(record.searchDraft, patch);
    const action = record.action.revise(draft);
    if (action.kind !== "search_plan" || action.deadline !== record.deadline) throw new Error("search_plan_payload_mismatch");
    const candidate = prepareReview({ context: record.context, action, draft, presentation: searchPlanProjection(draft) }, record.batch, record.peer);
    if (this.#bytes - record.bytes + candidate.bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(candidate.bytes - record.bytes);
    const revision = this.#next(record);
    this.#changeBytes(candidate.bytes - record.bytes);
    record.bytes = candidate.bytes; record.action = action; record.searchDraft = candidate.searchDraft;
    record.presentation = candidate.presentation; record.binding.revision = revision;
    record.accessedAt = performance.now();
    this.#admittedActions.add(action);
    return this.#snapshot(record);
  }

  advance(binding: ResultBinding, context: AccountContext, review: PreparedOwnerReview, frame: ExecutedBatch, completedSections: PresentationProjection["sections"], peer?: PeerResumePayload) {
    const candidate = prepareReview(review, frame, peer);
    const record = this.#bound(binding, context);
    if (!record.presented || record.state.kind !== "submitted") throw new Error("advance_requires_submitted");
    if (review.context.origin !== record.binding.origin) throw new Error("owner_review_context_mismatch");
    if (this.#admittedActions.has(review.action)) throw new Error("owner_review_already_admitted");
    if (completedSections.length < record.completedSections || !isDeepStrictEqual(completedSections.slice(0, record.completedSections), record.presentation.sections.slice(0, record.completedSections))) throw new Error("completed_sections_mismatch");
    const appended = completedSections.slice(record.completedSections);
    if (appended.length > 82 || jsonBytes(appended).length > 4 * 1024 * 1024) throw new Error("completed_projection_bytes");
    const presentation = ownerBatchPresentationProjection.parse({ ...candidate.presentation, sections: [...completedSections, ...candidate.presentation.sections] });
    if (record.batch.outcomes.length !== candidate.batch.outcomes.length) throw new Error("owner_review_frame_mismatch");
    for (let index = 0; index < record.batch.outcomes.length; index++) {
      const prior = record.batch.outcomes[index]!, next = candidate.batch.outcomes[index]!;
      if (prior.call_id !== next.call_id || prior.operation_id !== next.operation_id || prior.disposition.kind !== "not_run" && !isDeepStrictEqual(prior, next)) throw new Error("owner_review_frame_mismatch");
    }
    const submittedIndex = record.batch.outcomes.findIndex(call => call.call_id === record.context.call_id);
    const pendingIndex = candidate.batch.outcomes.findIndex(call => call.call_id === review.context.call_id);
    if (submittedIndex < 0 || pendingIndex < 0) throw new Error("owner_review_call_missing");
    if (pendingIndex <= submittedIndex) throw new Error("owner_review_did_not_advance");
    if (candidate.batch.outcomes.slice(0, pendingIndex).some(call => call.disposition.kind !== "succeeded") || candidate.batch.outcomes.slice(pendingIndex).some(call => call.disposition.kind !== "not_run")) throw new Error("owner_review_frame_progress");
    const bytes = jsonBytes([candidate.batch, presentation, candidate.pending, candidate.initial]).length + candidate.heldBytes;
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes - record.bytes);
    const revision = this.#next(record);
    if (performance.now() >= review.action.deadline) throw new Error("result_expired");
    this.#changeBytes(bytes - record.bytes); record.bytes = bytes;
    record.context = review.context; record.batch = candidate.batch; record.presentation = presentation;
    record.prompt = candidate.prompt; record.label = candidate.label; record.saveReview = candidate.saveReview;
    record.searchDraft = candidate.searchDraft;
    record.action = review.action; record.peer = peer; record.deadline = review.action.deadline;
    record.completedSections = completedSections.length; record.state = candidate.initial; record.binding.revision = revision;
    record.changedAt = record.accessedAt = performance.now();
    this.#admittedActions.add(review.action);
    return this.#snapshot(record);
  }

  complete(binding: ResultBinding, context: AccountContext, update: { batch: ExecutedBatch; presentation: PresentationProjection; terminal: TerminalResult; continuation?: unknown | null }) {
    const record = this.#bound(binding, context);
    if (record.state.kind !== "submitted") throw new Error("terminal_requires_submitted");
    const batch = executedBatch.parse(update.batch), presentation = ownerBatchPresentationProjection.parse(update.presentation), terminal = terminalResult.parse(update.terminal);
    if (batch.outcomes.length !== record.batch.outcomes.length) throw new Error("owner_review_frame_mismatch");
    for (let index = 0; index < batch.outcomes.length; index++) {
      const prior = record.batch.outcomes[index]!, next = batch.outcomes[index]!;
      if (prior.call_id !== next.call_id || prior.operation_id !== next.operation_id || prior.disposition.kind !== "not_run" && !isDeepStrictEqual(prior, next)) throw new Error("owner_review_frame_mismatch");
    }
    const completed = batch.outcomes.find(call => call.call_id === record.context.call_id);
    if (!completed || completed.disposition.kind === "not_run") throw new Error("owner_review_call_incomplete");
    if (!isDeepStrictEqual(presentation.sections.slice(0, record.completedSections), record.presentation.sections.slice(0, record.completedSections))) throw new Error("completed_sections_mismatch");
    creatorSavePresentationProjection.parse({ ...presentation, sections: presentation.sections.slice(record.completedSections) });
    const continuation = update.continuation == null ? null : resultContinuationRequest.parse(update.continuation);
    if (continuation !== null && !["execution", "creator_save_batch", "search_plan_confirmed"].includes(continuation.context.kind)) throw new Error("continuation_action_mismatch");
    if (continuation?.context.kind === "search_plan_confirmed" && (!record.searchDraft || terminal.kind !== "resolved" || terminal.code !== "search_plan_confirmed" || continuation.context.description !== record.searchDraft.description || !isDeepStrictEqual(continuation.context.keywords, record.searchDraft.keywords))) throw new Error("continuation_action_mismatch");
    const revision = this.#next(record);
    const bytes = jsonBytes([batch, presentation, terminal, continuation]).length;
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes - record.bytes);
    record.continuation.apply(continuation);
    this.#changeBytes(bytes - record.bytes);
    record.bytes = bytes;
    record.batch = batch; record.presentation = presentation; record.state = terminal;
    record.completedSections = presentation.sections.length;
    record.peer = undefined;
    record.searchDraft = undefined;
    record.binding.revision = revision;
    record.changedAt = record.accessedAt = performance.now();
    return this.#snapshot(record);
  }

  claimContinuation(binding: ResultBinding, context: AccountContext, expectedRevision: bigint, mode: "pending" | "retry_paused") {
    const record = this.#bound(binding, context);
    this.#requireTerminal(record);
    return { binding: structuredClone(record.binding), ...record.continuation.claim(expectedRevision, mode) };
  }
  completeContinuation(binding: ResultBinding, context: AccountContext, expectedRevision: bigint, lease: string, outcome: "delivered" | "paused" | "delivery_unknown") {
    const record = this.#bound(binding, context);
    this.#requireTerminal(record);
    const bytes = outcome === "delivered" ? jsonBytes([record.batch, record.presentation, record.state, null]).length : record.bytes;
    const continuation = record.continuation.complete(expectedRevision, lease, outcome);
    this.#changeBytes(bytes - record.bytes); record.bytes = bytes;
    return { binding: structuredClone(record.binding), continuation };
  }

  cleanup(): void {
    const now = performance.now();
    for (const [reference, record] of this.#records) {
      const availableExpired = record.state.kind === "available" && (now >= record.deadline || now - record.accessedAt >= 86400000);
      const terminalExpired = ["resolved", "failed"].includes(record.state.kind) && now - record.changedAt >= 259200000;
      if (availableExpired && now - record.accessedAt < 86400000 && record.batch.outcomes.some(call => call.disposition.kind === "succeeded")) this.#expireReview(record);
      else if (availableExpired || terminalExpired) { this.#records.delete(reference); this.#changeBytes(-record.bytes, -1); }
    }
  }
  close(): void { if (this.#closed) return; this.#closed = true; this.#changeBytes(-this.#bytes, -this.#records.size); this.#records.clear(); }
  #changeBytes(delta: number, rows = 0): void { this.#budget.change(delta, rows); this.#bytes += delta; }
  #assertOpen(context: AccountContext): void { if (this.#closed) throw new Error("result_store_closed"); this.#accounts.assertCurrent(context); }
  #get(reference: string, context: AccountContext): ConfirmationRecord {
    this.#assertOpen(context);
    const record = this.#records.get(reference);
    if (!record) throw new Error("result_absent");
    if (record.owner !== context) throw new Error("access.account_changed");
    if (record.state.kind === "available" && (performance.now() >= record.deadline || performance.now() - record.accessedAt >= 86400000)) {
      if (performance.now() - record.accessedAt < 86400000 && record.batch.outcomes.some(call => call.disposition.kind === "succeeded")) this.#expireReview(record);
      else { this.#records.delete(reference); this.#changeBytes(-record.bytes, -1); throw new Error("result_expired"); }
    }
    return record;
  }
  #bound(value: ResultBinding, context: AccountContext): ConfirmationRecord {
    const binding = resultBinding.parse(value), record = this.#get(binding.result_ref, context);
    if (record.binding.view.identity !== binding.view.identity || record.binding.view.kind !== binding.view.kind) throw new Error("result_view_mismatch");
    if (record.binding.origin !== binding.origin) throw new Error("result_origin_mismatch");
    if (record.binding.fingerprint !== binding.fingerprint) throw new Error("result_fingerprint_mismatch");
    if (record.binding.revision !== binding.revision) throw new Error("result_revision_conflict");
    return record;
  }
  #next(record: ConfirmationRecord): bigint { if (record.binding.revision === 18446744073709551615n) throw new Error("result_revision"); return record.binding.revision + 1n; }
  #snapshot(record: ConfirmationRecord) {
    return structuredClone({ locale: record.locale, binding: record.binding, presented: record.presented, batch: record.batch, presentation: record.presentation, completed_sections_count: record.completedSections,
      pending_action: record.action ? record.saveReview ? { kind: "creator_save_batch" as const, review: record.saveReview } : record.searchDraft ? { kind: "search_plan" as const, draft: record.searchDraft, draft_digest: digest(jsonBytes(record.searchDraft)) } : { kind: "confirm" as const, prompt: record.prompt, label: record.label } : null,
      state: record.state, continuation: record.continuation.snapshot(),
    });
  }
  #expireReview(record: ConfirmationRecord): void {
    const chinese = record.locale === "zh-CN";
    const summary = chinese ? "当前确认已过期。已完成的调用和完整结果详情已保留，待执行的调用未运行。" : "The current review expired. Completed calls and full result details are retained; pending calls were not run.";
    const notRun = chinese ? "确认已过期，此调用未执行。" : "Not run because the review expired.";
    const message = chinese ? "当前确认已过期。先前已完成的调用不应重复，待执行的调用未运行。完整结果详情仍保留在此结果中；本次后续上下文仅包含有限数量的资源引用。" : "The current review expired. Earlier completed calls must not be repeated; pending calls were not run. Full result details remain on this result. The continuation includes only a bounded selection of resource references.";
    const batch = structuredClone(record.batch);
    for (const call of batch.outcomes) if (call.disposition.kind === "not_run") { call.disposition = { kind: "not_run", reason: "review_expired" }; call.summary = notRun; }
    const terminal: TerminalResult = { kind: "failed", code: "review_expired", summary, retryable: false };
    let request = resultContinuationRequest.parse({ message, requires_model_context: true, context: { kind: "execution", outcomes: batch.outcomes.slice(0, 8).map(call => ({ ...call, resource_refs: call.resource_refs.slice(0, 64).map(resource => ({ ...resource, label: null })) })) } });
    let bytes = jsonBytes([batch, record.presentation, terminal, request]).length;
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024 || !this.#budget.permits(bytes - record.bytes)) {
      const pending = batch.outcomes.find(call => call.disposition.kind === "not_run");
      if (!pending) throw new Error("expired_pending_call");
      request = resultContinuationRequest.parse({ message, requires_model_context: true, context: { kind: "execution", outcomes: [{ ...pending, resource_refs: [] }] } });
      bytes = jsonBytes([batch, record.presentation, terminal, request]).length;
    }
    if (this.#bytes - record.bytes + bytes > 64 * 1024 * 1024) throw new Error("result_payload_limit");
    this.#budget.check(bytes - record.bytes);
    const revision = this.#next(record);
    record.continuation.apply(request);
    this.#changeBytes(bytes - record.bytes); record.bytes = bytes;
    record.batch = batch; record.state = terminal; record.action = undefined; record.saveReview = undefined; record.searchDraft = undefined; record.peer = undefined;
    record.binding.revision = revision;
    record.changedAt = record.accessedAt = performance.now();
  }
  #requireTerminal(record: ConfirmationRecord): void {
    if (!["resolved", "failed", "unknown"].includes(record.state.kind)) throw new Error("continuation_requires_terminal_result");
  }
}
