import { createHash, randomUUID } from "node:crypto";
import { inspect, isDeepStrictEqual } from "node:util";
import { AccountContextService, type AccountContext } from "../identity/account-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { resultChoiceQuestions, validateChoiceAnswers, type ResultChoiceAnswer, type ResultChoiceQuestion } from "./choice-contracts.ts";
import { executedBatch, ownerBatchPresentationProjection, presentationProjection, resultBinding, resultContinuationRequest, terminalResult, type ExecutedBatch, type PresentationProjection, type ResultBinding, type ResultContinuationRequest, type TerminalResult } from "./contracts.ts";
import { ResultContinuation } from "./continuation.ts";
import { OwnerReviewContext, PeerResumePayload } from "./owner-context.ts";
import { ResultStoreBudget } from "./budget.ts";
import { trim } from "../creators/contracts.ts";

type ChoiceState = { kind: "available" } | { kind: "submitted"; action: { kind: "selected"; answers: ResultChoiceAnswer[] } } | TerminalResult;
export interface ChoiceInput { context: OwnerReviewContext; questions: readonly ResultChoiceQuestion[]; presentation: PresentationProjection; batch: ExecutedBatch; payload: Uint8Array; completed_sections_count?: number; peer?: PeerResumePayload }
export interface ChoiceTerminalUpdate { batch: ExecutedBatch; presentation: PresentationProjection; terminal: TerminalResult; continuation: ResultContinuationRequest | null }
interface ChoiceRecord { context: OwnerReviewContext; owner: AccountContext; binding: ResultBinding; locale: "en-US" | "zh-CN"; presented: boolean; batch: ExecutedBatch; presentation: PresentationProjection; completed: number; questions: ResultChoiceQuestion[]; payload: Buffer | null; peer: PeerResumePayload | undefined; state: ChoiceState; continuation: ResultContinuation; accessed: number; changed: number; bytes: number }
interface ChoiceGrantContents { owner: AccountContext; binding: ResultBinding; context: OwnerReviewContext; answers: ResultChoiceAnswer[]; payload: Buffer; peer: PeerResumePayload | undefined; validate(): void }
const issuer = Symbol("submitted-choice-grant");
export class SubmittedChoiceGrant {
  readonly #contents: ChoiceGrantContents;
  #consumed = false;
  constructor(token: symbol, contents: ChoiceGrantContents) { if (token !== issuer) throw new Error("choice_grant_untrusted"); this.#contents = contents; Object.freeze(this); }
  binding(): ResultBinding { return structuredClone(this.#contents.binding); }
  context(): OwnerReviewContext { return this.#contents.context; }
  consume(account: AccountContext): { answers: ResultChoiceAnswer[]; payload: Buffer; peer: PeerResumePayload | undefined } {
    if (this.#consumed) throw new Error("choice_grant_consumed");
    this.#contents.validate(); if (account !== this.#contents.owner || account.signal.aborted) throw new Error("access.account_changed");
    this.#consumed = true;
    return { answers: structuredClone(this.#contents.answers), payload: Buffer.from(this.#contents.payload), peer: this.#contents.peer };
  }
  toJSON(): never { throw new Error("Submitted Choice authority must not be serialized"); }
  [inspect.custom](): string { return "SubmittedChoiceGrant { authority: [private] }"; }
}
const encode = (value: unknown) => Buffer.from(stringifyApplicationJson(value));
const digest = (value: Uint8Array) => `sha256:${createHash("sha256").update(value).digest("hex")}`;
const increment = (value: bigint) => { if (value < 1n || value >= 18446744073709551615n) throw new Error("result_revision"); return value + 1n; };
export interface ChoiceStoreLimits { max_results: number; max_total_payload_bytes: number; available_ttl_ms: number; terminal_ttl_ms: number }
export class ResultChoiceStore {
  readonly #accounts: AccountContextService;
  readonly #limits: ChoiceStoreLimits;
  readonly #records = new Map<string, ChoiceRecord>();
  #bytes = 0;
  #closed = false;
  readonly #budget: ResultStoreBudget;
  constructor(accounts: AccountContextService, limits: Partial<ChoiceStoreLimits> = {}, budget = new ResultStoreBudget()) { this.#accounts = accounts; this.#budget = budget; this.#limits = { max_results: 10000, max_total_payload_bytes: 64 * 1024 * 1024, available_ttl_ms: 86400000, terminal_ttl_ms: 259200000, ...limits }; if (Object.values(this.#limits).some(value => !Number.isSafeInteger(value) || value < 0)) throw new Error("result_store_limits"); }
  mint(input: ChoiceInput, account: AccountContext): string {
    this.#assertOpen(account); OwnerReviewContext.assertIssued(input.context); input.context.validateFrame(input.batch);
    if (input.peer !== undefined || input.context.call_digest !== null) input.context.validatePeer(input.batch, input.peer);
    const batch = executedBatch.parse(input.batch), questions = resultChoiceQuestions.parse(input.questions), completed = input.completed_sections_count ?? 0;
    if (!Number.isSafeInteger(completed) || completed < 0 || completed > input.presentation.sections.length) throw new Error("completed_sections");
    const presentation = (completed ? ownerBatchPresentationProjection : presentationProjection).parse(input.presentation);
    presentationProjection.parse({ ...presentation, sections: presentation.sections.slice(completed) });
    if (!(input.payload instanceof Uint8Array) || !input.payload.length && input.peer === undefined || input.payload.length > 16 * 1024) throw new Error("owner_action_payload");
    const payload = Buffer.from(input.payload), ref = `rr_${randomUUID().replaceAll("-", "")}`, view = { identity: `rv_${randomUUID().replaceAll("-", "")}`, kind: "choice" as const }, state = { kind: "available" as const };
    const pending = { kind: "choice", questions, payload_digest: digest(input.peer?.copyBytes() ?? payload) };
    const fingerprint = digest(encode({ result_ref: ref, view, origin: input.context.origin, batch, presentation, pending_action: pending, initial_state: state }));
    const bytes = encode([batch, presentation, pending, state]).length + payload.length + (input.peer?.copyBytes().length ?? 0);
    this.cleanup(); this.#capacity(bytes, true);
    const now = performance.now();
    this.#records.set(ref, { context: input.context, owner: account, binding: { result_ref: ref, view, origin: input.context.origin, fingerprint, revision: 1n }, locale: "en-US", presented: false, batch, presentation, completed, questions, payload, peer: input.peer, state, continuation: new ResultContinuation(), accessed: now, changed: now, bytes }); this.#budget.change(bytes, 1); this.#bytes += bytes;
    return ref;
  }
  present(reference: string, origin: string, account: AccountContext, locale: "en-US" | "zh-CN" = "en-US") {
    const record = this.#get(reference, account); if (origin !== record.binding.origin) throw new Error("origin_mismatch"); if (record.presented) throw new Error("result_already_presented"); if (locale !== "en-US" && locale !== "zh-CN") throw new Error("result_locale");
    const revision = increment(record.binding.revision); record.presented = true; record.locale = locale; record.binding.revision = revision; record.changed = record.accessed = performance.now(); return this.#snapshot(record);
  }
  read(binding: ResultBinding, account: AccountContext) { const record = this.#bound(binding, account); record.accessed = performance.now(); return this.#snapshot(record); }
  current(binding: ResultBinding, account: AccountContext) { const record = this.#bound(binding, account, true); if (!record.presented) throw new Error("result_not_presented"); record.accessed = performance.now(); return this.#snapshot(record); }
  requestEdit(binding: ResultBinding, account: AccountContext, rawInstruction: string) {
    if (!rawInstruction.isWellFormed() || /\p{Cc}/u.test(rawInstruction)) throw new Error("edit_instruction_control");
    const instruction = trim(rawInstruction);
    if (!instruction || Buffer.byteLength(instruction) > 2048) throw new Error("edit_instruction");
    const record = this.#bound(binding, account); this.#available(record);
    const terminal: TerminalResult = { kind: "resolved", code: "edit_requested", summary: "The user requested changes. The pending action was not executed." };
    const lead = record.locale === "zh-CN" ? "用户要求修改，原操作未执行。修改要求：" : "The user requested changes; the pending action was not executed. Requested changes:";
    const continuation = resultContinuationRequest.parse({ message: `${lead}\n\n${instruction}`, requires_model_context: false, context: { kind: "edit_requested" } });
    const revision = increment(record.binding.revision), bytes = encode([record.batch, record.presentation, terminal, continuation]).length;
    this.#capacity(bytes - record.bytes);
    record.continuation.apply(continuation); this.#replaceBytes(record, bytes);
    record.state = terminal; record.binding.revision = revision; record.payload = null; record.peer = undefined; record.questions = [];
    record.changed = record.accessed = performance.now();
    return this.#snapshot(record);
  }
  answer(binding: ResultBinding, account: AccountContext, input: unknown): SubmittedChoiceGrant {
    const record = this.#bound(binding, account); this.#available(record);
    const answers = validateChoiceAnswers(record.questions, input), revision = increment(record.binding.revision), payload = record.payload, peer = record.peer;
    if (!payload) throw new Error("choice_required");
    const nextState: ChoiceState = { kind: "submitted", action: { kind: "selected", answers } };
    const bytes = encode([record.batch, record.presentation, nextState, null]).length; this.#capacity(bytes - record.bytes);
    record.state = nextState; record.payload = null; record.peer = undefined; record.questions = []; record.binding.revision = revision; record.changed = record.accessed = performance.now(); this.#replaceBytes(record, bytes);
    const submitted = structuredClone(record.binding);
    return new SubmittedChoiceGrant(issuer, { owner: account, binding: submitted, context: record.context, answers, payload, peer, validate: () => { const current = this.#bound(submitted, account); if (current.state.kind !== "submitted") throw new Error("result_action_consumed"); } });
  }
  complete(binding: ResultBinding, account: AccountContext, input: ChoiceTerminalUpdate) {
    const record = this.#bound(binding, account); if (record.state.kind !== "submitted") throw new Error("terminal_requires_submitted");
    const batch = executedBatch.parse(input.batch), presentation = ownerBatchPresentationProjection.parse(input.presentation), terminal = terminalResult.parse(input.terminal), continuation = input.continuation === null ? null : resultContinuationRequest.parse(input.continuation);
    if (!isDeepStrictEqual(presentation.sections.slice(0, record.completed), record.presentation.sections.slice(0, record.completed))) throw new Error("completed_sections_mismatch");
    presentationProjection.parse({ ...presentation, sections: presentation.sections.slice(record.completed) });
    this.#preserveCompleted(record, batch);
    if (continuation !== null && (continuation.context.kind !== "choice" || !isDeepStrictEqual(continuation.context.answers, record.state.action.answers))) throw new Error("choice_continuation_answers");
    const revision = increment(record.binding.revision), bytes = encode([batch, presentation, terminal, continuation]).length; this.#capacity(bytes - record.bytes);
    record.continuation.apply(continuation); record.batch = batch; record.presentation = presentation; record.completed = presentation.sections.length; record.state = terminal; record.peer = undefined; record.binding.revision = revision; record.changed = record.accessed = performance.now(); this.#replaceBytes(record, bytes); return this.#snapshot(record);
  }
  cancel(binding: ResultBinding, account: AccountContext, request: ResultContinuationRequest | null = null) {
    const record = this.#bound(binding, account); this.#available(record);
    const requested = request === null ? null : resultContinuationRequest.parse(request); if (requested !== null && requested.context.kind !== "cancelled") throw new Error("cancel_continuation_context");
    const batch = structuredClone(record.batch), completed = record.completed > 0 || batch.outcomes.some(call => call.disposition.kind === "succeeded"), pending = batch.outcomes.some(call => call.disposition.kind === "not_run"), mixed = completed && pending, chinese = record.locale === "zh-CN";
    let summary = "The user cancelled this action.", continuation = requested;
    if (mixed) {
      if (batch.outcomes.length > 8) throw new Error("cancel_execution_batch");
      summary = chinese ? "已取消待执行的操作。已完成的调用及完整结果详情已保留。" : "The pending action was cancelled. Completed calls and full result details are retained.";
      for (const call of batch.outcomes) if (call.disposition.kind === "not_run") { call.disposition = { kind: "not_run", reason: "user_cancelled" }; call.summary = chinese ? "后续操作已取消，此调用未执行。" : "Not run because the remaining actions were cancelled."; }
      if (requested) continuation = resultContinuationRequest.parse({ message: chinese ? "待执行的操作及后续调用已取消，先前已完成的调用不应重复。完整结果详情仍保留在此结果中；后续上下文仅包含有限数量的资源引用。" : "The pending action and remaining calls were cancelled. Earlier completed calls must not be repeated. Full result details remain on this result; this continuation contains only a bounded selection of resource references.", requires_model_context: true, context: { kind: "execution", outcomes: batch.outcomes.map(call => ({ ...call, resource_refs: call.resource_refs.slice(0, 64).map(ref => ({ ...ref, label: null })) })) } });
    }
    const terminal: TerminalResult = { kind: "resolved", code: "user_cancelled", summary }, revision = increment(record.binding.revision), bytes = encode([batch, record.presentation, terminal, continuation]).length; this.#capacity(bytes - record.bytes);
    record.continuation.apply(continuation); record.batch = batch; record.state = terminal; record.payload = null; record.peer = undefined; record.questions = []; record.binding.revision = revision; record.changed = record.accessed = performance.now(); this.#replaceBytes(record, bytes); return this.#snapshot(record);
  }
  claimContinuation(binding: ResultBinding, account: AccountContext, revision: bigint, mode: "pending" | "retry_paused") { const record = this.#bound(binding, account); this.#terminal(record); return { binding: structuredClone(record.binding), ...record.continuation.claim(revision, mode) }; }
  completeContinuation(binding: ResultBinding, account: AccountContext, revision: bigint, lease: string, outcome: "delivered" | "paused" | "delivery_unknown") { const record = this.#bound(binding, account); this.#terminal(record); const continuation = record.continuation.complete(revision, lease, outcome); if (outcome === "delivered") this.#replaceBytes(record, encode([record.batch, record.presentation, record.state, null]).length); return { binding: structuredClone(record.binding), continuation }; }
  cleanup(): { removed: number; active: number; payload_bytes: number } { const now = performance.now(); let removed = 0; for (const [ref, record] of this.#records) if (record.state.kind === "available" && now - record.accessed >= this.#limits.available_ttl_ms || ["resolved", "failed"].includes(record.state.kind) && now - record.changed >= this.#limits.terminal_ttl_ms) { this.#records.delete(ref); this.#budget.change(-record.bytes, -1); this.#bytes -= record.bytes; removed++; } return { removed, active: this.#records.size, payload_bytes: this.#bytes }; }
  close(): void { if (this.#closed) return; this.#closed = true; this.#budget.change(-this.#bytes, -this.#records.size); this.#records.clear(); this.#bytes = 0; }
  #assertOpen(account: AccountContext) { if (this.#closed) throw new Error("result_store_closed"); this.#accounts.assertCurrent(account); }
  #get(reference: string, account: AccountContext): ChoiceRecord { this.#assertOpen(account); this.cleanup(); const record = this.#records.get(reference); if (!record) throw new Error("result_absent"); if (record.owner !== account) throw new Error("access.account_changed"); return record; }
  #bound(raw: ResultBinding, account: AccountContext, stale = false): ChoiceRecord { const binding = resultBinding.parse(raw), record = this.#get(binding.result_ref, account); if (!isDeepStrictEqual(binding.view, record.binding.view)) throw new Error("view_mismatch"); if (binding.origin !== record.binding.origin) throw new Error("origin_mismatch"); if (binding.fingerprint !== record.binding.fingerprint) throw new Error("fingerprint_mismatch"); if (binding.revision > record.binding.revision || !stale && binding.revision !== record.binding.revision) throw new Error("result_revision_conflict"); return record; }
  #available(record: ChoiceRecord) { if (!record.presented) throw new Error("result_not_presented"); if (record.state.kind !== "available") throw new Error("result_action_consumed"); if (!record.payload) throw new Error("choice_required"); }
  #terminal(record: ChoiceRecord) { if (!["resolved", "failed", "unknown"].includes(record.state.kind)) throw new Error("continuation_requires_terminal_result"); }
  #capacity(delta: number, row = false) { if (row && this.#records.size >= this.#limits.max_results) throw new Error("result_row_limit"); if (this.#bytes + delta > this.#limits.max_total_payload_bytes) throw new Error("result_payload_limit"); this.#budget.check(delta, row ? 1 : 0); }
  #replaceBytes(record: ChoiceRecord, bytes: number) { this.#budget.change(bytes - record.bytes); this.#bytes += bytes - record.bytes; record.bytes = bytes; }
  #preserveCompleted(record: ChoiceRecord, batch: ExecutedBatch) { if (batch.outcomes.length !== record.batch.outcomes.length) throw new Error("owner_frame_mismatch"); for (let i = 0; i < record.batch.outcomes.length; i++) { const previous = record.batch.outcomes[i]!, next = batch.outcomes[i]!; if (previous.call_id !== next.call_id || previous.operation_id !== next.operation_id || previous.disposition.kind !== "not_run" && !isDeepStrictEqual(previous, next)) throw new Error("completed_call_mismatch"); } }
  #snapshot(record: ChoiceRecord) { return structuredClone({ locale: record.locale, binding: record.binding, presented: record.presented, batch: record.batch, presentation: record.presentation, completed_sections_count: record.completed, pending_action: record.state.kind === "available" ? { kind: "choice" as const, questions: record.questions } : null, state: record.state, continuation: record.continuation.snapshot() }); }
}
