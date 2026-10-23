import { createHash, randomUUID } from "node:crypto";
import { inspect } from "node:util";
import type { AccountContext } from "../identity/account-context.ts";
import { AccountContextService } from "../identity/account-context.ts";
import { searchPlanEditorDraft, SEARCH_PLAN_MAX_ACTIVE, SEARCH_PLAN_TTL_SECONDS, type SearchPlanEditorDraft, type SearchPlanScope } from "../discovery/search-plan-contracts.ts";
import { stringifyApplicationJson } from "../contracts/application-json.ts";
import { OwnerReviewContext } from "./owner-context.ts";
import { OwnerConfirmationStore, SubmittedOwnerGrant, type OwnerReviewAuthority, type PreparedOwnerReview } from "./grants.ts";
import { searchPlanProjection } from "./search-plan-projection.ts";
import type { ExecutedBatch } from "./contracts.ts";

export interface CompiledSearchScope { readonly token: string; readonly operationType: symbol; readonly request: unknown }
export type SearchScopeCompiler = (scope: SearchPlanScope, purpose: "initial" | "execute", account: AccountContext) => CompiledSearchScope;
const actionIssuer = Symbol("search-plan-action"), requestIssuer = Symbol("bound-search-request");

class SearchPlanAction implements OwnerReviewAuthority {
  readonly kind = "search_plan";
  readonly deadline: number;
  readonly #draft: SearchPlanEditorDraft;
  constructor(issuer: symbol, draft: SearchPlanEditorDraft, deadline: number) {
    if (issuer !== actionIssuer) throw new Error("search_plan_untrusted");
    this.#draft = searchPlanEditorDraft.parse(draft); this.deadline = deadline; Object.freeze(this);
  }
  encodedPayload(): Buffer { return Buffer.from(stringifyApplicationJson(this.#draft)); }
  revise(draft: SearchPlanEditorDraft): SearchPlanAction {
    if (stringifyApplicationJson(draft.scopes) !== stringifyApplicationJson(this.#draft.scopes)) throw new Error("search_plan_scope_mismatch");
    return new SearchPlanAction(actionIssuer, draft, this.deadline);
  }
  draft(): SearchPlanEditorDraft { return structuredClone(this.#draft); }
  toJSON(): never { throw new Error("Search plan authority must not be serialized"); }
  [inspect.custom](): string { return "SearchPlanAction [private]"; }
}
interface Approval { account: AccountContext; tokens: ReadonlySet<string>; expires: number }
function requestDigest(request: unknown): string { return createHash("sha256").update(stringifyApplicationJson(request)).digest("hex"); }

export class BoundSearchAuthorization {
  readonly #operationType: symbol;
  readonly #digest: string;
  readonly #validate: () => void;
  constructor(issuer: symbol, operationType: symbol, request: unknown, validate: () => void) {
    if (issuer !== requestIssuer) throw new Error("search_authorization_untrusted");
    this.#operationType = operationType; this.#digest = requestDigest(request); this.#validate = validate; Object.freeze(this);
  }
  validateRequest(operationType: symbol, request: unknown): void {
    if (operationType !== this.#operationType || requestDigest(request) !== this.#digest) throw new Error("search_plan_scope_mismatch");
    this.#validate();
  }
  toJSON(): never { throw new Error("Search authorization must not be serialized"); }
  [inspect.custom](): string { return "BoundSearchAuthorization [private]"; }
}

// 确认权限仅存在于当前进程，读取和翻页不会延长有效期。
export class SearchPlanAuthority {
  readonly #accounts: AccountContextService;
  readonly #compile: SearchScopeCompiler;
  readonly #approvals = new Map<string, Approval>();
  #closed = false;
  constructor(accounts: AccountContextService, compiler: SearchScopeCompiler) { this.#accounts = accounts; this.#compile = compiler; }
  #assert(account: AccountContext): void { if (this.#closed) throw new Error("search_plan_approval_unavailable"); this.#accounts.assertCurrent(account); }
  #compilePlan(draft: SearchPlanEditorDraft, account: AccountContext): string[] {
    this.#assert(account);
    const tokens = draft.scopes.map(scope => this.#compile(scope, "initial", account).token);
    if (tokens.some(token => !/^sha256:[0-9a-f]{64}$/u.test(token)) || new Set(tokens).size !== tokens.length) throw new Error("invalid_operation_scopes");
    return tokens;
  }
  prepare(raw: unknown, context: OwnerReviewContext, account: AccountContext): PreparedOwnerReview {
    if (context.operation_id !== "onecrew.search.plan.prepare") throw new Error("search_plan_operation_mismatch");
    const draft = searchPlanEditorDraft.parse(raw); this.#compilePlan(draft, account);
    return { context, action: new SearchPlanAction(actionIssuer, draft, performance.now() + 86_400_000), draft, presentation: searchPlanProjection(draft) };
  }
  confirm(grant: SubmittedOwnerGrant, account: AccountContext, store: OwnerConfirmationStore, frame: ExecutedBatch) {
    this.#assert(account);
    const action = grant.consume("search_plan", account);
    if (!(action instanceof SearchPlanAction)) throw new Error("search_plan_untrusted");
    const draft = action.draft(), tokens = this.#compilePlan(draft, account);
    if (frame.outcomes.length !== 1 || frame.outcomes[0]!.call_id !== grant.context().call_id || frame.outcomes[0]!.operation_id !== "onecrew.search.plan.prepare" || frame.outcomes[0]!.disposition.kind !== "not_run") throw new Error("search_plan_snapshot_mismatch");
    const now = performance.now();
    for (const [ref, approval] of this.#approvals) if (approval.expires <= now || approval.account.signal.aborted) this.#approvals.delete(ref);
    if (this.#approvals.size >= SEARCH_PLAN_MAX_ACTIVE) throw new Error("too_many_active_search_plans");
    const reference = randomUUID();
    this.#approvals.set(reference, { account, tokens: new Set(tokens), expires: now + SEARCH_PLAN_TTL_SECONDS * 1000 });
    let committed = false;
    using revoke = { [Symbol.dispose]: () => { if (!committed) this.#approvals.delete(reference); } };
    const summary = `Search plan confirmed: ${reference}. Use this reference for the approved searches and evaluate results against the final acceptance criteria. The confirmed_plan_ref expires 60 minutes after confirmation; until then approved searches page freely with next_cursor, and changed criteria need a new confirmed plan.`;
    const presentation = searchPlanProjection(draft); presentation.summary = summary;
    const batch = structuredClone(frame); batch.outcomes[0]!.disposition = { kind: "succeeded" }; batch.outcomes[0]!.summary = summary;
    const result = store.complete(grant.binding(), account, { batch, presentation, terminal: { kind: "resolved", code: "search_plan_confirmed", summary }, continuation: { message: grant.locale() === "zh-CN" ? "已确认搜索计划，请按计划继续搜索。" : "Search plan confirmed. Continue with the approved searches.", requires_model_context: true, context: { kind: "search_plan_confirmed", confirmed_plan_ref: reference, description: draft.description, keywords: draft.keywords } } });
    committed = true;
    return result;
  }
  bind(reference: string | undefined, scope: SearchPlanScope, request: unknown, account: AccountContext): BoundSearchAuthorization {
    this.#assert(account);
    if (reference === undefined) throw new Error("search_plan_confirmation_required");
    const compiled = this.#compile(scope, "execute", account);
    if (requestDigest(request) !== requestDigest(compiled.request)) throw new Error("search_plan_scope_mismatch");
    const validate = () => {
      this.#assert(account);
      const approval = this.#approvals.get(reference);
      if (!approval || approval.expires <= performance.now() || approval.account !== account) throw new Error("search_plan_invalid_or_expired");
      if (!approval.tokens.has(compiled.token)) throw new Error("search_plan_scope_mismatch");
    };
    validate();
    return new BoundSearchAuthorization(requestIssuer, compiled.operationType, request, validate);
  }
  bindDirect(scope: SearchPlanScope, request: unknown, account: AccountContext): BoundSearchAuthorization {
    this.#assert(account);
    if (scope.arguments.search_plan_policy !== "user_requested_direct" || scope.arguments.confirmed_plan_ref != null) throw new Error("search_plan_scope_mismatch");
    const compiled = this.#compile(scope, "execute", account);
    if (requestDigest(request) !== requestDigest(compiled.request)) throw new Error("search_plan_scope_mismatch");
    return new BoundSearchAuthorization(requestIssuer, compiled.operationType, request, () => this.#assert(account));
  }
  cancel(reference: string, account: AccountContext): void {
    this.#assert(account);
    const approval = this.#approvals.get(reference);
    if (approval && approval.account !== account) throw new Error("access.account_changed");
    this.#approvals.delete(reference);
  }
  close(): void { this.#closed = true; this.#approvals.clear(); }
}
