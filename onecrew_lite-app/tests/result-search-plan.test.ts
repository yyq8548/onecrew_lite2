import assert from "node:assert/strict";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { compileNativePlatformSearch, nativePlatformSearchCatalog } from "../src/discovery/native-search-contracts.ts";
import { OwnerConfirmationStore } from "../src/results/grants.ts";
import { OwnerReviewContext } from "../src/results/owner-context.ts";
import { SearchPlanAuthority, BoundSearchAuthorization } from "../src/results/search-plan.ts";

test("actual native search contracts confirm edited criteria and bind exact operation, account, connection and page", () => {
  const accounts = new AccountContextService();
  const principal = { user: { id: "search-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const };
  const account = accounts.activate(principal), store = new OwnerConfirmationStore(accounts), authority = new SearchPlanAuthority(accounts, compileNativePlatformSearch);
  const descriptor = nativePlatformSearchCatalog().find(value => value.operation_id === "youtube.channels.search")!;
  const scope = { operation_id: descriptor.operation_id, schema_revision: descriptor.schema_revision, connection: { kind: "service_credential" as const, toolkit: "youtube", credential_id: "selected-youtube-key" }, arguments: { query: "mechanical keyboards", count: 5 } };
  const draft = { description: "Mechanical keyboard reviewers", keywords: ["keyboards"], scopes: [scope] };
  const context = new OwnerReviewContext("mcp:full", "plan", "onecrew.search.plan.prepare");
  const frame = { outcomes: [{ call_id: "plan", operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "search_plan_confirmation_required" }, summary: "Search plan awaits user review. No search has run.", resource_refs: [] }] };
  try {
    assert.throws(() => authority.bind(undefined, scope, compileNativePlatformSearch(scope, "execute").request, account), /confirmation_required/);
    const ref = store.mint(authority.prepare(draft, context, account), frame, account);
    const initial = store.present(ref, context.origin, account, "zh-CN");
    assert.throws(() => store.reviseSearchPlan(initial.binding, account, { scopes: [] }));
    const revised = store.reviseSearchPlan(initial.binding, account, { description: "Independent mechanical keyboard reviewers", keywords: ["keyboards", "independent"] });
    assert.equal(revised.binding.fingerprint, initial.binding.fingerprint);
    assert.equal(revised.binding.view.identity, initial.binding.view.identity);
    assert.equal(revised.binding.revision, initial.binding.revision + 1n);
    assert.throws(() => store.submit(initial.binding, account), /revision_conflict/);
    const grant = store.submit(revised.binding, account);
    const confirmed = authority.confirm(grant, account, store, frame);
    assert.equal(confirmed.state.kind, "resolved");
    assert.equal(confirmed.pending_action, null);
    const notification = store.claimContinuation(confirmed.binding, account, confirmed.continuation.continuation_revision, "pending");
    assert.equal(notification.request.message, "已确认搜索计划，请按计划继续搜索。");
    assert.equal(notification.request.context.kind, "search_plan_confirmed");
    if (notification.request.context.kind !== "search_plan_confirmed") throw new Error("Expected actual confirmed plan continuation");
    assert.equal(notification.request.context.description, "Independent mechanical keyboard reviewers");
    assert.deepEqual(notification.request.context.keywords, ["keyboards", "independent"]);
    const reference = notification.request.context.confirmed_plan_ref;
    const compiled = compileNativePlatformSearch(scope, "execute");
    const bound = authority.bind(reference, scope, compiled.request, account);
    bound.validateRequest(compiled.operationType, compiled.request);
    assert.throws(() => JSON.stringify(bound), /must not be serialized/);
    assert.throws(() => new BoundSearchAuthorization(Symbol(), compiled.operationType, compiled.request, () => {}), /untrusted/);
    assert.throws(() => bound.validateRequest(Symbol("youtube.channels.search"), compiled.request), /scope_mismatch/);
    assert.throws(() => bound.validateRequest(compiled.operationType, { ...compiled.request as object, count: 6 }), /scope_mismatch/);
    assert.throws(() => authority.bind(reference, scope, { ...compiled.request as object, count: 6 }, account), /scope_mismatch/);
    assert.throws(() => authority.bind(reference, { ...scope, arguments: { ...scope.arguments, query: "different criteria" } }, compiled.request, account), /scope_mismatch/);
    const next = { ...scope, arguments: { ...scope.arguments, cursor: "actual-page-cursor" } }, nextCompiled = compileNativePlatformSearch(next, "execute");
    authority.bind(reference, next, nextCompiled.request, account).validateRequest(nextCompiled.operationType, nextCompiled.request);
    assert.throws(() => bound.validateRequest(compiled.operationType, nextCompiled.request), /scope_mismatch/);
    authority.cancel(reference, account);
    assert.throws(() => bound.validateRequest(compiled.operationType, compiled.request), /invalid_or_expired/);
    assert.throws(() => authority.confirm(grant, account, store, frame), /consumed/);
    accounts.activate(principal);
    assert.throws(() => authority.prepare(draft, context, account), /account changed/);
  } finally { authority.close(); store.close(); }
});

test("search plan rejects untyped scopes and keeps edit requests separate from execution", () => {
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const store = new OwnerConfirmationStore(accounts), authority = new SearchPlanAuthority(accounts, compileNativePlatformSearch);
  const entry = nativePlatformSearchCatalog().find(value => value.operation_id === "instagram.users.search")!;
  const scope = { operation_id: entry.operation_id, schema_revision: entry.schema_revision, connection: { kind: "operator_account" as const, toolkit: "instagram", operator_account_id: "selected-instagram-account" }, arguments: { query: "keyboards", count: 5 } };
  const draft = { description: "Keyboard reviewers", keywords: [], scopes: [scope] }, context = new OwnerReviewContext("mcp:full", "plan", "onecrew.search.plan.prepare");
  const frame = { outcomes: [{ call_id: "plan", operation_id: context.operation_id, disposition: { kind: "not_run" as const, reason: "confirmation_required" }, summary: "Review plan", resource_refs: [] }] };
  try {
    assert.throws(() => authority.prepare({ ...draft, scopes: [{ ...scope, schema_revision: "client-chosen" }] }, context, account), /schema_revision_mismatch/);
    assert.throws(() => authority.prepare({ ...draft, scopes: [scope, scope] }, context, account), /invalid_operation_scopes/);
    assert.throws(() => authority.prepare({ ...draft, scopes: [{ ...scope, arguments: { ...scope.arguments, search_plan_policy: "user_requested_direct" } }] }, context, account), /direct_policy/);
    const ref = store.mint(authority.prepare(draft, context, account), frame, account), presented = store.present(ref, context.origin, account, "zh-CN");
    assert.throws(() => store.requestEdit(presented.binding, account, "change\ncriteria"), /instruction_control/);
    const edited = store.requestEdit(presented.binding, account, "  增加独立测评要求  ");
    assert.equal(edited.pending_action, null);
    const notification = store.claimContinuation(edited.binding, account, edited.continuation.continuation_revision, "pending");
    assert.equal(notification.request.message, "用户要求修改，原操作未执行。修改要求：\n\n增加独立测评要求");
    assert.equal(notification.request.context.kind, "edit_requested");
    assert.equal(notification.request.requires_model_context, false);
    assert.throws(() => store.submit(edited.binding, account), /consumed/);
    const directScope = { ...scope, arguments: { ...scope.arguments, search_plan_policy: "user_requested_direct" } }, compiled = compileNativePlatformSearch(directScope, "execute");
    const direct = authority.bindDirect(directScope, compiled.request, account);
    direct.validateRequest(compiled.operationType, compiled.request);
    assert.throws(() => authority.bindDirect(scope, compiled.request, account), /scope_mismatch/);
  } finally { authority.close(); store.close(); }
});
