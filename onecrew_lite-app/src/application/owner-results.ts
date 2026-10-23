import { z } from "zod";
import { AccountContextService, type AccountContext } from "../identity/account-context.ts";
import type { CreatorRepositoryResolver } from "../creators/operations.ts";
import { prepareCreatorBatch, executeSubmittedCreatorBatch, intoCreatorBatchOwnerCompletion } from "../creators/batch-review.ts";
import { prepareCreatorCandidateSave, executeSubmittedCreatorSave } from "../creators/candidate-review.ts";
import { parseCreatorSaveCandidates, creatorSaveCandidateInput, type CreatorSaveProfileResolver } from "../creators/candidate-input.ts";
import { McpSessions } from "../service/mcp-sessions.ts";
import { ResultChoiceStore } from "../results/choice-store.ts";
import { prepareChoice, submitChoice } from "../results/choice-flow.ts";
import { OwnerConfirmationStore, type OwnerCallCompletion, type PreparedOwnerReview, type SubmittedOwnerGrant } from "../results/grants.ts";
import { ResultStoreBudget } from "../results/budget.ts";
import { SearchPlanAuthority } from "../results/search-plan.ts";
import { compileNativePlatformSearch } from "../discovery/native-search-contracts.ts";
import { searchPlanScope } from "../discovery/search-plan-contracts.ts";
import { resultBinding, type ResultBinding, type ExecutedBatch, type PresentationProjection, type ExecutedCallOutcome } from "../results/contracts.ts";
import { wireResultInput } from "@onecrew/runtime-contracts/result-wire";
import { randomUUID } from "node:crypto";
import { mcpExecuteInput, type McpExecutionCall, type McpExecutionOutcome, type McpExecutionResult } from "../service/mcp-contracts.ts";
import type { McpCatalogue } from "../service/mcp-catalogue.ts";
import type { McpRegisteredHandler } from "../service/mcp-handlers.ts";
import type { McpOperationEntry } from "../service/mcp-catalogue.ts";
import { PreparedMcpOperationAction } from "../service/mcp-operation-review.ts";
import { mcpCompletedSections, mcpOutputResourceRefs } from "../service/mcp-projection.ts";
import { OwnerReviewContext, type PeerResumePayload } from "../results/owner-context.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { batchInput } from "../creators/batch.ts";
import { McpJsonSchemaValidator } from "../service/mcp-json-schema.ts";
import { discoveryExecutionFailureCode } from "../discovery/execution-failure.ts";

// GitHub 0.7.1 production registry 的实际输入合同版本；不接受客户端自行选择版本。
export const ownerOperationRevisions = Object.freeze({
  "onecrew.ask_user_question": "sha256:a17d39e7a6a1c0fec437881f2173723ab45d4a908ae4054d613d588be1801bf4",
  "onecrew.creator.batch.create": "sha256:578657ef95e5c21853c3de13de067ae881d30ea12c36034df9240aa653dcca9e",
  "onecrew.creator.candidate.batch.save": "sha256:faa7daf3b0f11db938a6bec4acc219d7b31df3e767a5e2ade4fab8350c06d1b3",
  "onecrew.creator.candidate.save": "sha256:1d5026e973be4e70a949879727e4acc7f8817460679ba6ce4f80261ab593d29e",
  "onecrew.search.plan.prepare": "sha256:07139fc30a0b083637bed8135a719160578eb26a487a3f51c7858e5d48595bf0",
});
const entryId = z.string().min(1).max(128).regex(/^[a-z0-9._-]+$/u);
const callSchema = z.object({ call_id: entryId, operation_id: z.enum(Object.keys(ownerOperationRevisions) as [keyof typeof ownerOperationRevisions, ...(keyof typeof ownerOperationRevisions)[]]), schema_revision: z.string(), authority: z.object({ kind: z.literal("direct") }).strict(), connection: z.null().default(null), arguments: z.record(z.string(), z.unknown()) }).strict();
interface Reference { kind: "owner" | "choice"; account: AccountContext; retainUntil: number }
type ProfileResolverFactory = (account: AccountContext) => Promise<CreatorSaveProfileResolver>;
interface ExecutionCall { call: McpExecutionCall; operation: McpOperationEntry; handler: McpRegisteredHandler | undefined; context: OwnerReviewContext; review?: PreparedOwnerReview }
interface ExecutionChain {
  batchId: string; account: AccountContext; calls: ExecutionCall[]; peer: PeerResumePayload | undefined;
  frame: ExecutedBatch; sections: PresentationProjection["sections"]; outcomes: McpExecutionOutcome[];
  index: number; stopped: boolean; reference: string | undefined; binding: ResultBinding | undefined;
  projectionFailed: boolean;
}

export class OwnerResults {
  readonly #accounts: AccountContextService;
  readonly #sessions: McpSessions;
  readonly #repositories: CreatorRepositoryResolver;
  readonly #profiles: ProfileResolverFactory | undefined;
  readonly #changed: ((account: AccountContext) => void) | undefined;
  readonly #owners: OwnerConfirmationStore;
  readonly #choices: ResultChoiceStore;
  readonly #search: SearchPlanAuthority;
  readonly #inputs = new McpJsonSchemaValidator();
  readonly #references = new Map<string, Reference>();
  readonly #chains = new Map<string, ExecutionChain>();
  #closed = false;
  authorizeNativeSearch(account: AccountContext, rawScope: unknown) {
    if (this.#closed) throw new Error("result_store_closed");
    this.#accounts.assertCurrent(account);
    const scope = searchPlanScope.parse(structuredClone(rawScope)), compiled = compileNativePlatformSearch(scope, "execute");
    const authorization = scope.arguments.search_plan_policy === "user_requested_direct"
      ? this.#search.bindDirect(scope, compiled.request, account)
      : this.#search.bind(z.string().nullish().parse(scope.arguments.confirmed_plan_ref) ?? undefined, scope, compiled.request, account);
    return Object.freeze({ operationType: compiled.operationType, request: structuredClone(compiled.request), authorization });
  }
  constructor(accounts: AccountContextService, sessions: McpSessions, repositories: CreatorRepositoryResolver, profiles?: ProfileResolverFactory, changed?: (account: AccountContext) => void) {
    this.#accounts = accounts; this.#sessions = sessions; this.#repositories = repositories; this.#profiles = profiles; this.#changed = changed;
    const budget = new ResultStoreBudget();
    this.#owners = new OwnerConfirmationStore(accounts, budget); this.#choices = new ResultChoiceStore(accounts, {}, budget); this.#search = new SearchPlanAuthority(accounts, compileNativePlatformSearch);
  }
  async #repository(account: AccountContext) {
    this.#accounts.assertCurrent(account);
    const value = await this.#repositories.resolve(account);
    this.#accounts.assertCurrent(account);
    if (value.owner.accountId !== account.principal.user.id || value.owner.tenantId !== account.principal.tenantId || value.owner.baseId !== account.principal.baseId) throw new Error("Creator result repository owner mismatch");
    return value.repository;
  }
  #cleanup(): void {
    this.#owners.cleanup(); this.#choices.cleanup();
    for (const [reference, value] of this.#references) if (value.account.signal.aborted || value.retainUntil <= performance.now()) { this.#references.delete(reference); this.#chains.delete(reference); }
  }
  #remember(reference: string, kind: Reference["kind"], account: AccountContext, terminal = false): void {
    this.#references.set(reference, { kind, account, retainUntil: performance.now() + (terminal ? 72 : 24) * 60 * 60_000 });
  }
  #bound(session: string, input: ResultBinding) {
    if (this.#closed) throw new Error("result_store_closed");
    const binding = resultBinding.parse(input), account = this.#sessions.assertResultOrigin(session, binding.origin), reference = this.#references.get(binding.result_ref);
    if (!reference || reference.account !== account) throw new Error("result_absent");
    return { binding, account, reference };
  }
  async execute(session: string, raw: unknown, catalogue: McpCatalogue, handlers: readonly McpRegisteredHandler[]): Promise<McpExecutionResult> {
    if (this.#closed) throw new Error("result_store_closed");
    this.#cleanup();
    const input = mcpExecuteInput.parse(structuredClone(raw)), selected = this.#sessions.authorize(session), contexts = this.#sessions.reviewContexts(session, input.calls);
    const calls = input.calls.map((call, index): ExecutionCall => {
      const operation = catalogue.operation(call.operation_id, selected.surface);
      if (!operation) throw new Error("operation_unavailable");
      if (call.schema_revision !== operation.input_revision) throw new Error("schema_revision_mismatch");
      this.#inputs.validateOperation(operation.metadata.id, operation.input_revision, operation.input_schema, call.arguments);
      const handler = handlers.find(value => value.operation_id === call.operation_id);
      if (handler) handler.validate(call);
      else if (!(call.operation_id in ownerOperationRevisions)) throw new Error("mcp_handler_unavailable");
      if (operation.connection.kind === "none" && call.connection !== null) throw new Error("operation_connection_mismatch");
      if (operation.connection.kind !== "none" && !handler?.validatesConnection) throw new Error("mcp_verified_connection_handler_unavailable");
      if (handler?.requiresSearchPlan) this.authorizeNativeSearch(selected.account, { operation_id: call.operation_id, schema_revision: call.schema_revision, connection: call.connection, arguments: call.arguments });
      if (call.operation_id === "onecrew.creator.batch.create") batchInput.parse(call.arguments);
      if (call.operation_id === "onecrew.creator.candidate.batch.save") parseCreatorSaveCandidates(z.strictObject({ candidates: z.array(creatorSaveCandidateInput).min(1).max(50) }).parse(call.arguments).candidates);
      if (call.operation_id === "onecrew.creator.candidate.save") parseCreatorSaveCandidates([call.arguments]);
      const entry: ExecutionCall = { call, operation, handler, context: contexts.contexts[index]! };
      if (call.operation_id === "onecrew.creator.batch.create") entry.review = prepareCreatorBatch(call.arguments, entry.context);
      else if (handler && operation.approval === "attended_prompt") entry.review = PreparedMcpOperationAction.prepare(call, operation, handler, entry.context);
      return entry;
    });
    const singleton = calls.find(value => ["onecrew.ask_user_question", "onecrew.search.plan.prepare"].includes(value.call.operation_id));
    if (singleton) {
      if (calls.length !== 1) throw new Error("operation_requires_single_call_batch");
      const prepared = await this.prepare(session, singleton.call);
      return { batch_id: randomUUID(), outcomes: [{ status: "input_required", call_id: singleton.call.call_id, operation_id: singleton.call.operation_id, input_kind: singleton.call.operation_id === "onecrew.ask_user_question" ? "choice" : "confirmation" }], result_ref: prepared.result_ref, presentation_failure: null };
    }
    const required = calls.some(value => value.operation.approval === "attended_prompt" || !value.handler);
    if ((required || input.presentation === "requested") && calls.some(value => value.operation.secret !== "none" || value.operation.sensitive_input_paths.length)) throw new Error("presentation_unavailable");
    const chain: ExecutionChain = { batchId: randomUUID(), account: selected.account, calls, peer: contexts.peer,
      frame: { outcomes: calls.map(({ call }) => ({ call_id: call.call_id, operation_id: call.operation_id, disposition: { kind: "not_run", reason: "not_invoked" }, summary: "The operation has not run.", resource_refs: [] })) },
      sections: [], outcomes: [], index: 0, stopped: false, reference: undefined, binding: undefined, projectionFailed: false };
    await this.#continueExecution(chain, undefined, required || input.presentation === "requested");
    const outcomes = [...chain.outcomes];
    if (!chain.stopped && chain.index < calls.length) {
      const pending = calls[chain.index]!;
      outcomes.push({ status: "input_required", call_id: pending.call.call_id, operation_id: pending.call.operation_id, input_kind: "confirmation" });
      for (const later of calls.slice(chain.index + 1)) outcomes.push({ status: "skipped", call_id: later.call.call_id, operation_id: later.call.operation_id, reason: "earlier_call_requires_attention" });
    }
    if (chain.stopped) for (const later of calls.slice(chain.index)) outcomes.push({ status: "skipped", call_id: later.call.call_id, operation_id: later.call.operation_id, reason: "earlier_call_requires_attention" });
    return { batch_id: chain.batchId, outcomes, result_ref: chain.reference ?? null, presentation_failure: null };
  }
  async #reviewCall(entry: ExecutionCall, account: AccountContext): Promise<PreparedOwnerReview | OwnerCallCompletion> {
    const { call, operation, handler, context } = entry;
    if (entry.review) return entry.review;
    if (call.operation_id === "onecrew.creator.batch.create") return prepareCreatorBatch(call.arguments, context);
    if (call.operation_id === "onecrew.creator.candidate.batch.save" || call.operation_id === "onecrew.creator.candidate.save") {
      if (!this.#profiles) throw new Error("creator_profile_resolution_unavailable");
      const candidates = call.operation_id === "onecrew.creator.candidate.batch.save" ? parseCreatorSaveCandidates(z.strictObject({ candidates: z.array(creatorSaveCandidateInput).min(1).max(50) }).parse(call.arguments).candidates) : parseCreatorSaveCandidates([call.arguments]);
      const prepared = await prepareCreatorCandidateSave(candidates, context, await this.#repository(account), await this.#profiles(account));
      this.#accounts.assertCurrent(account);
      return prepared.kind === "review" ? prepared.review : prepared.completion;
    }
    if (!handler) throw new Error("mcp_handler_unavailable");
    return PreparedMcpOperationAction.prepare(call, operation, handler, context);
  }
  #recordCompletion(chain: ExecutionChain, entry: ExecutionCall, completion: OwnerCallCompletion, beforeInvocation = false): void {
    if (completion.outcome.call_id !== entry.call.call_id || completion.outcome.operation_id !== entry.call.operation_id) throw new Error("mcp_completion_call_mismatch");
    chain.frame.outcomes[chain.index] = structuredClone(completion.outcome);
    chain.sections.push(...structuredClone(completion.presentation.sections));
    if (completion.terminal.code === "presentation_failed") chain.projectionFailed = true;
    const disposition = completion.outcome.disposition;
    if (disposition.kind === "failed" || disposition.kind === "unknown") chain.outcomes.push({ status: disposition.kind, call_id: entry.call.call_id, operation_id: entry.call.operation_id, failure: { code: disposition.code, phase: beforeInvocation ? "before_invocation" : "after_invocation", retry_safe: beforeInvocation, effect: beforeInvocation ? "not_invoked" : disposition.kind === "unknown" ? "unknown" : "known_no_effect", recovery: null } });
    else chain.outcomes.push({ status: "completed", call_id: entry.call.call_id, operation_id: entry.call.operation_id, result: completion.output });
    chain.stopped = !completion.continue_batch || disposition.kind !== "succeeded";
    chain.index++;
  }
  #ordinaryCompletion(entry: ExecutionCall, output: unknown, batchSize: number, present = true): OwnerCallCompletion {
    const summary = `${entry.operation.metadata.title} completed.`, outcome: ExecutedCallOutcome = { call_id: entry.call.call_id, operation_id: entry.call.operation_id, disposition: { kind: "succeeded" }, summary, resource_refs: mcpOutputResourceRefs(output) };
    let sections: PresentationProjection["sections"];
    try { sections = present ? mcpCompletedSections(output, entry.operation, batchSize) : []; }
    catch (error) {
      if (!(error instanceof Error) || !error.message.startsWith("projection_")) throw error;
      const failure = "The owner finished, but this result could not be shown safely. Do not rerun the completed operation; use its execution receipt.";
      return { output, continue_batch: true, outcome, presentation: { kind: entry.operation.view_kind, title: "Execution result unavailable", summary: failure, sections: [{ kind: "notice", heading: "Presentation failed", body: failure, tone: "critical" }] }, terminal: { kind: "failed", code: "presentation_failed", summary: failure, retryable: false }, continuation: null };
    }
    return { output, continue_batch: true, outcome, presentation: { kind: entry.operation.view_kind, title: "Execution result", summary, sections }, terminal: { kind: "resolved", code: "execution_completed", summary }, continuation: null };
  }
  #failedCompletion(entry: ExecutionCall, unknown: boolean, before: boolean, error?: unknown): OwnerCallCompletion {
    const classified = discoveryExecutionFailureCode(error);
    unknown = !before && (unknown || classified.endsWith("_outcome_unknown") || classified === "opencli_cancel_pending");
    const kind = unknown ? "unknown" as const : "failed" as const, code = classified !== "execution_failed" ? classified : unknown ? "mcp_operation_outcome_unknown" : "mcp_operation_failed", summary = unknown ? "The operation outcome could not be determined. Inspect its existing receipt before retrying." : before ? "The operation failed before invocation." : "The operation did not complete.";
    const outcome: ExecutedCallOutcome = { call_id: entry.call.call_id, operation_id: entry.call.operation_id, disposition: { kind, code }, summary, resource_refs: [] };
    return { output: null, continue_batch: false, outcome, presentation: { kind: entry.operation.view_kind, title: "Execution result", summary, sections: [{ kind: "notice", heading: null, body: summary, tone: unknown ? "warning" : "critical" }] }, terminal: unknown ? { kind: "unknown", code, summary } : { kind: "failed", code, summary, retryable: false }, continuation: null };
  }
  async #invokeCall(entry: ExecutionCall, account: AccountContext, batchSize: number, present: boolean): Promise<OwnerCallCompletion> {
    let output: unknown;
    try { output = await entry.handler!.invoke(entry.call, account); }
    catch (error) {
      this.#accounts.assertCurrent(account);
      return this.#failedCompletion(entry, entry.operation.effect !== "read_local" && entry.operation.effect !== "read_open_world", false, error);
    }
    this.#accounts.assertCurrent(account);
    return this.#ordinaryCompletion(entry, output, batchSize, present);
  }
  async #continueExecution(chain: ExecutionChain, grant?: SubmittedOwnerGrant, present = true): Promise<void> {
    const account = chain.account;
    if (grant) {
      const entry = chain.calls[chain.index]!;
      if (grant.context() !== entry.context) throw new Error("mcp_batch_authority_mismatch");
      this.#accounts.assertCurrent(account);
      let completion: OwnerCallCompletion, changed = false;
      if (entry.call.operation_id === "onecrew.creator.batch.create") {
        const result = executeSubmittedCreatorBatch(grant, account, await this.#repository(account));
        completion = intoCreatorBatchOwnerCompletion(result, entry.context); changed = !result.replayed && result.outcomes.some(row => row.kind === "accepted");
      } else if (["onecrew.creator.candidate.batch.save", "onecrew.creator.candidate.save"].includes(entry.call.operation_id)) {
        const result = executeSubmittedCreatorSave(grant, account, await this.#repository(account)); completion = result.completion; changed = result.changed;
      } else completion = this.#ordinaryCompletion(entry, await PreparedMcpOperationAction.execute(grant, account), chain.calls.length);
      this.#accounts.assertCurrent(account);
      this.#recordCompletion(chain, entry, completion);
      if (changed) this.#changed?.(account);
      chain.binding = grant.binding();
    }
    while (!chain.stopped && chain.index < chain.calls.length) {
      this.#accounts.assertCurrent(account);
      const entry = chain.calls[chain.index]!;
      entry.handler?.validate(entry.call);
      if (!entry.handler || entry.operation.approval === "attended_prompt") {
        let prepared: PreparedOwnerReview | OwnerCallCompletion;
        try { prepared = await this.#reviewCall(entry, account); }
        catch (error) { this.#accounts.assertCurrent(account); this.#recordCompletion(chain, entry, this.#failedCompletion(entry, false, true, error), true); break; }
        if (!("action" in prepared)) { this.#recordCompletion(chain, entry, prepared); continue; }
        chain.frame.outcomes[chain.index] = { ...chain.frame.outcomes[chain.index]!, disposition: { kind: "not_run", reason: "confirmation_required" }, summary: "The operation awaits user review. It has not run." };
        if (chain.reference === undefined) {
          const review = { ...prepared, presentation: { ...prepared.presentation, sections: [...chain.sections, ...prepared.presentation.sections] } };
          chain.reference = this.#owners.mint(review, chain.frame, account, chain.peer);
          this.#remember(chain.reference, "owner", account); this.#chains.set(chain.reference, chain);
        } else {
          if (!chain.binding) throw new Error("mcp_batch_binding_missing");
          this.#owners.advance(chain.binding, account, prepared, chain.frame, chain.sections, chain.peer);
        }
        return;
      }
      this.#recordCompletion(chain, entry, await this.#invokeCall(entry, account, chain.calls.length, present));
    }
    if (!present) return;
    const unknown = chain.frame.outcomes.find(outcome => outcome.disposition.kind === "unknown"), failed = chain.frame.outcomes.find(outcome => outcome.disposition.kind === "failed");
    const summary = unknown ? "Execution stopped after an effect became uncertain." : chain.stopped ? "Execution stopped. Recorded execution outcomes are retained." : chain.projectionFailed ? "The owners finished, but some results could not be shown safely. Do not rerun completed operations; use their execution receipts." : "Batch execution completed.";
    const terminal = unknown?.disposition.kind === "unknown" ? { kind: "unknown" as const, code: unknown.disposition.code, summary } : failed?.disposition.kind === "failed" ? { kind: "failed" as const, code: failed.disposition.code, summary, retryable: false } : chain.projectionFailed ? { kind: "failed" as const, code: "presentation_failed", summary, retryable: false } : { kind: "resolved" as const, code: "execution_completed", summary };
    const presentation: PresentationProjection = { kind: "batch_result", title: "Execution result", summary, sections: chain.sections.length ? chain.sections : [{ kind: "items", heading: "Operations", items: chain.frame.outcomes.map(outcome => ({ title: outcome.operation_id, description: outcome.summary })) }] };
    const continuation = { message: summary, requires_model_context: true, context: { kind: "execution", outcomes: chain.frame.outcomes } };
    if (chain.reference === undefined) {
      const context = chain.calls[0]!.context, completion: OwnerCallCompletion = { output: null, continue_batch: true, outcome: chain.frame.outcomes[0]!, presentation, terminal, continuation };
      chain.reference = this.#owners.mintCompleted(context, completion, chain.frame, account); this.#remember(chain.reference, "owner", account, true);
    } else {
      if (!chain.binding) throw new Error("mcp_batch_binding_missing");
      this.#owners.complete(chain.binding, account, { batch: chain.frame, presentation, terminal, continuation });
      this.#chains.delete(chain.reference);
      this.#references.get(chain.reference)!.retainUntil = performance.now() + 259200000;
    }
  }
  async prepare(session: string, raw: unknown): Promise<{ result_ref: string }> {
    if (this.#closed) throw new Error("result_store_closed");
    this.#cleanup();
    const call = callSchema.parse(raw), peer = this.#sessions.reviewContexts(session, [call]), account = peer.account, context = peer.contexts[0]!;
    if (call.schema_revision !== ownerOperationRevisions[call.operation_id]) throw new Error("schema_revision_mismatch");
    const permitted = call.operation_id === "onecrew.ask_user_question" ? ["full", "compact"] : ["full", "compact", "creators"];
    if (!permitted.includes(peer.surface)) throw new Error("operation_surface_denied");
    if (this.#references.size >= 10000) throw new Error("result_row_limit");
    if (call.operation_id === "onecrew.ask_user_question") {
      const reference = this.#choices.mint(prepareChoice(call.arguments, context), account); this.#remember(reference, "choice", account); return { result_ref: reference };
    }
    const frame: ExecutedBatch = { outcomes: [{ call_id: call.call_id, operation_id: call.operation_id, disposition: { kind: "not_run", reason: call.operation_id === "onecrew.search.plan.prepare" ? "search_plan_confirmation_required" : "confirmation_required" }, summary: "The operation awaits user review. It has not run.", resource_refs: [] }] };
    if (call.operation_id === "onecrew.creator.batch.create" || call.operation_id === "onecrew.search.plan.prepare") {
      const review = call.operation_id === "onecrew.creator.batch.create" ? prepareCreatorBatch(call.arguments, context) : this.#search.prepare(call.arguments, context, account);
      const reference = this.#owners.mint(review, frame, account); this.#remember(reference, "owner", account); return { result_ref: reference };
    }
    if (!this.#profiles) throw new Error("creator_profile_resolution_unavailable");
    const candidates = call.operation_id === "onecrew.creator.candidate.batch.save" ? parseCreatorSaveCandidates(z.object({ candidates: z.array(creatorSaveCandidateInput).min(1).max(50) }).strict().parse(call.arguments).candidates) : parseCreatorSaveCandidates([call.arguments]);
    const repository = await this.#repository(account), profiles = await this.#profiles(account);
    this.#accounts.assertCurrent(account);
    const prepared = await prepareCreatorCandidateSave(candidates, context, repository, profiles);
    this.#accounts.assertCurrent(account);
    const reference = prepared.kind === "review" ? this.#owners.mint(prepared.review, frame, account) : this.#owners.mintCompleted(context, prepared.completion, { outcomes: [prepared.completion.outcome] }, account);
    this.#remember(reference, "owner", account, prepared.kind === "noop"); return { result_ref: reference };
  }
  present(session: string, reference: string, locale: "en-US" | "zh-CN" = "en-US") {
    if (this.#closed) throw new Error("result_store_closed");
    const selected = this.#sessions.authorize(session), stored = this.#references.get(reference);
    if (!stored || stored.account !== selected.account) throw new Error("result_absent");
    return stored.kind === "choice" ? this.#choices.present(reference, selected.origin, selected.account, locale) : this.#owners.present(reference, selected.origin, selected.account, locale);
  }
  async state(session: string, raw: unknown) {
    const input = wireResultInput.parse(raw), selected = this.#bound(session, resultBinding.parse("binding" in input ? input.binding : input.input.binding)), { binding, account, reference } = selected;
    const store = reference.kind === "choice" ? this.#choices : this.#owners;
    if (input.action === "claim_continuation") return { kind: "claimed_continuation", claimed: store.claimContinuation(binding, account, BigInt(input.input.expected_continuation_revision), input.input.mode) };
    if (input.action === "complete_continuation") return { kind: "completed_continuation", completed: store.completeContinuation(binding, account, BigInt(input.input.expected_continuation_revision), input.input.lease_id, input.input.outcome) };
    if (input.action === "read_current") return { kind: "snapshot", snapshot: store.current(binding, account) };
    if (input.action === "request_edit") { const snapshot = store.requestEdit(binding, account, input.input.instruction); this.#chains.delete(binding.result_ref); reference.retainUntil = performance.now() + 259200000; return { kind: "snapshot", snapshot }; }
    if (input.action === "cancel") { const snapshot = store.cancel(binding, account, { message: "Cancelled.", context: { kind: "cancelled" }, requires_model_context: false }); this.#chains.delete(binding.result_ref); reference.retainUntil = performance.now() + 259200000; return { kind: "snapshot", snapshot }; }
    if (reference.kind === "choice") {
      if (input.action !== "select") throw new Error("choice_selection_required");
      const snapshot = submitChoice(this.#choices, binding, account, input.answers); reference.retainUntil = performance.now() + 259200000; return { kind: "snapshot", snapshot };
    }
    if (input.action === "revise_search_plan") return { kind: "snapshot", snapshot: this.#owners.reviseSearchPlan(binding, account, input.patch) };
    if (input.action !== "confirm" && input.action !== "confirm_creator_save_batch") throw new Error("owner_confirmation_required");
    const current = this.#owners.read(binding, account), isSave = current.pending_action?.kind === "creator_save_batch";
    if (isSave !== (input.action === "confirm_creator_save_batch")) throw new Error("owner_review_selection_mismatch");
    const grant = this.#owners.submit(binding, account, input.action === "confirm_creator_save_batch" ? input.selections : undefined);
    reference.retainUntil = Infinity;
    const chain = this.#chains.get(binding.result_ref);
    if (chain) {
      let settled = false;
      using completionGuard = { [Symbol.dispose]: () => {
        if (settled || account.signal.aborted) return;
        const index = Math.min(chain.index, chain.calls.length - 1), entry = chain.calls[index]!;
        const unknown = entry.operation.effect !== "read_local" && entry.operation.effect !== "read_open_world", kind = unknown ? "unknown" as const : "failed" as const;
        const summary = unknown ? "An execution outcome is uncertain. Inspect the retained receipt before retrying." : "Execution stopped. Recorded execution outcomes are retained.";
        if (chain.frame.outcomes[index]!.disposition.kind === "not_run") chain.frame.outcomes[index] = { ...chain.frame.outcomes[index]!, disposition: { kind, code: "mcp_execution_interrupted" }, summary };
        this.#owners.complete(grant.binding(), account, { batch: chain.frame, presentation: { kind: "batch_result", title: "Execution result", summary, sections: [...chain.sections, { kind: "notice", heading: null, body: summary, tone: unknown ? "warning" : "critical" }] }, terminal: unknown ? { kind: "unknown", code: "mcp_execution_interrupted", summary } : { kind: "failed", code: "mcp_execution_interrupted", summary, retryable: false }, continuation: { message: summary, requires_model_context: true, context: { kind: "execution", outcomes: chain.frame.outcomes } } });
        this.#chains.delete(binding.result_ref); reference.retainUntil = unknown ? Infinity : performance.now() + 259200000;
      } };
      await this.#continueExecution(chain, grant);
      settled = true;
      return { kind: "snapshot", snapshot: this.#owners.current(grant.binding(), account) };
    }
    let completed = false, dispatched = false;
    using guard = { [Symbol.dispose]: () => {
      if (completed || account.signal.aborted) return;
      const kind = dispatched ? "unknown" as const : "failed" as const, code = dispatched ? "owner_execution_unknown" : "owner_execution_not_started", summary = dispatched ? "The execution result could not be determined. Inspect the existing receipt before retrying." : "The operation was not executed.";
      this.#owners.complete(grant.binding(), account, { batch: { outcomes: current.batch.outcomes.map(call => call.call_id === grant.context().call_id ? { ...call, disposition: { kind, code }, summary } : call) }, presentation: current.presentation, terminal: kind === "unknown" ? { kind, code, summary } : { kind, code, summary, retryable: false }, continuation: null });
      reference.retainUntil = kind === "unknown" ? Infinity : performance.now() + 259200000;
    } };
    if (current.pending_action?.kind === "search_plan") { const snapshot = this.#search.confirm(grant, account, this.#owners, current.batch); completed = true; reference.retainUntil = performance.now() + 259200000; return { kind: "snapshot", snapshot }; }
    const repository = await this.#repository(account);
    this.#accounts.assertCurrent(account); dispatched = true;
    let completion: OwnerCallCompletion, changed: boolean;
    if (isSave) { const result = executeSubmittedCreatorSave(grant, account, repository); completion = result.completion; changed = result.changed; }
    else { const result = executeSubmittedCreatorBatch(grant, account, repository); completion = intoCreatorBatchOwnerCompletion(result, grant.context()); changed = !result.replayed && result.outcomes.some(row => row.kind === "accepted"); }
    const snapshot = this.#owners.complete(grant.binding(), account, { batch: { outcomes: [completion.outcome] }, presentation: completion.presentation, terminal: completion.terminal, continuation: completion.continuation });
    completed = true; reference.retainUntil = snapshot.state.kind === "unknown" ? Infinity : performance.now() + 259200000;
    if (changed) this.#changed?.(account);
    return { kind: "snapshot", snapshot };
  }
  close(): void { if (this.#closed) return; this.#closed = true; this.#search.close(); this.#choices.close(); this.#owners.close(); this.#references.clear(); this.#chains.clear(); }
}
