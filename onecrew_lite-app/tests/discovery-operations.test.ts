import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { setTimeout as wait } from "node:timers/promises";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { registerDiscoveryReadOperations, registerDiscoveryDraftOperations, registerDiscoveryWriteOperations } from "../src/discovery/operations.ts";
import { DiscoveryDraftRuntime } from "../src/discovery/draft-runtime.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("八项Discovery实际注册读取使用唯一账号SQLite，严格旧Web包装并阻止账号切换和读取路径注入", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-operations-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory), accounts = new AccountContextService(), databases = new AccountDatabases(lease, accounts), registry = new OperationRegistry(accounts);
  const principal = { user: { id: "operations-owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" as const }, context = accounts.activate(principal);
  try {
    await databases.createNewAccount(context); registerDiscoveryReadOperations(registry, accounts, databases.discoveryResolver());
    const execute = (name: string, input: unknown) => registry.execute(name, input, context.principal.user.id, context.epoch);
    assert.equal(registry.names().length, 12); assert.deepEqual(await execute("onecrew_discovery_campaigns_recent", {}), []);
    registerDiscoveryWriteOperations(registry, accounts, databases.discoveryResolver());
    const drafts = new DiscoveryDraftRuntime(accounts, databases.discoveryResolver()); registerDiscoveryDraftOperations(registry, drafts);
    async function settledDraft(id: string) {
      for (let attempt = 0; attempt < 100; attempt++) {
        const progress = await execute("onecrew_discovery_draft_progress_get", { id }) as { phase: string; error_code: string | null } | null;
        if (progress && ["finished", "failed"].includes(progress.phase)) return progress;
        await wait(10);
      }
      throw new Error("draft_did_not_settle");
    }
    const repository = (await databases.discoveryResolver().resolve(context)).repository;
    repository.create({ id: "actual-local-plan", brief: "找鞋子作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["shoe review"] }], target_count: 2 }); repository.approve("actual-local-plan", 1);
    assert.equal((await execute("onecrew_discovery_campaign_get", { id: "actual-local-plan" }) as { id: string }).id, "actual-local-plan");
    assert.equal((await execute("onecrew_discovery_campaigns_recent", {}) as unknown[]).length, 1);
    const writeInput = { id: "registered-write", brief: "键盘作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["keyboard review"] }], target_count: 2 };
    assert.equal((await execute("onecrew_discovery_campaign_create", { input: writeInput }) as { phase: string }).phase, "draft");
    assert.equal((await execute("onecrew_discovery_campaign_approve", { id: writeInput.id, expectedRevision: 1 }) as { phase: string }).phase, "active");
    assert.equal(registry.names().includes("onecrew_discovery_budget_increase"), false);
    const targetInput = { campaign_id: writeInput.id, expected_revision: 1, expected_target_count: 2, new_target_count: 1, decision_id: "registered-target" };
    assert.equal((await execute("onecrew_discovery_target_increase", { input: targetInput }) as { target_count: number }).target_count, 1);
    assert.equal((await execute("onecrew_discovery_target_increase", { input: targetInput }) as { target_count: number }).target_count, 1);
    for (const name of ["onecrew_discovery_candidates_list", "onecrew_discovery_search_hits_list"]) assert.deepEqual(await execute(name, { input: { campaign_id: "actual-local-plan", after_id: null, limit: 100 } }), []);
    assert.deepEqual(await execute("onecrew_discovery_candidates_count", { id: "actual-local-plan" }), { public_page_errors: 0, qualified: 0, pending: 0, rejected: 0 });
    assert.equal(await execute("onecrew_discovery_review_get", { input: { campaign_id: "actual-local-plan", request_id: "not-yet-reviewed" } }), null);
    await assert.rejects(execute("onecrew_discovery_profile_get", { input: { campaign_id: "actual-local-plan", search_hit_id: "missing" } }), /read_missing/);
    await assert.rejects(execute("onecrew_discovery_review_material_get", { input: { campaign_id: "actual-local-plan", expected_revision: 1, search_hit_id: "missing" } }));
    await assert.rejects(execute("onecrew_discovery_campaign_get", { id: "actual-local-plan", accountId: "another-owner", databasePath: "other.sqlite" }));
    repository.create({ id: "existing-draft", brief: "找鞋子作者", opencli_version: "1.8.8", criteria: { region: "" }, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["shoe review"] }], target_count: 2 });
    const draftInput = { id: "existing-draft", brief: "找鞋子作者", profile_ref: "work", opencli_version: "1.8.8", target_count: 2, scope_overrides: [] };
    assert.equal((await execute("onecrew_discovery_draft_prepare", { input: draftInput }) as { id: string }).id, "existing-draft");
    assert.equal((await settledDraft("existing-draft")).phase, "finished");
    await execute("onecrew_discovery_draft_prepare", { input: { ...draftInput, brief: "不同请求" } });
    assert.equal((await settledDraft("existing-draft")).error_code, "discovery_draft_conflict");
    await execute("onecrew_discovery_draft_prepare", { input: { ...draftInput, id: "requires-dependencies" } });
    assert.equal((await settledDraft("requires-dependencies")).error_code, "discovery_draft_browser_unavailable");
    assert.equal(repository.findPlan("requires-dependencies"), null); assert.equal(repository.getPlan("actual-local-plan").used_calls, 0);
    assert.equal((await execute("onecrew_discovery_draft_progress_get", { id: "requires-dependencies" }) as { error_code: string }).error_code, "discovery_draft_browser_unavailable");
    await drafts.close();
    const next = accounts.activate({ ...principal, user: { id: "different-owner", email: "different@example.com" } }); await databases.createNewAccount(next);
    await assert.rejects(execute("onecrew_discovery_campaign_get", { id: "actual-local-plan" }), /account changed/i);
    assert.deepEqual(await registry.execute("onecrew_discovery_campaigns_recent", {}, next.principal.user.id, next.epoch), []);
    await assert.rejects(registry.execute("onecrew_discovery_campaign_get", { id: "actual-local-plan" }, next.principal.user.id, next.epoch), /plan_missing/);
  } finally { databases.close(); await lease.close(); }
});
