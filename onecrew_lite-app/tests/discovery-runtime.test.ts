import assert from "node:assert/strict";
import { chmod, copyFile, mkdtemp, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { createDiscoveryLibraryProjection } from "../src/sync/discovery-library.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";
import { DiscoveryExecutionSupervisor } from "../src/discovery/execution-supervisor.ts";
import { createDiscoveryRuntime } from "../src/discovery/runtime.ts";

test("完整注册器使用真实SQLite/PGlite：缺浏览器不领取，严格包装，空历史不伪造执行状态", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-runtime-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const tables = await createEmbeddedTables(await createPrivateDirectory(join(directory, "tables")), "http://127.0.0.1:33333"), accounts = new AccountContextService(), tenant = await tables.tenants.resolve({ id: "runtime-owner", email: "owner@example.com" });
  const context = accounts.activate({ user: tenant.user, tenantId: tenant.tenantId, baseId: tenant.baseId, deviceId: "device", source: "access" }), databases = new AccountDatabases(lease, accounts), supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts));
  let started = 0;
  const runtime = createDiscoveryRuntime({ accounts, repositories: databases.discoveryResolver(), directories: databases, supervisor, library: createDiscoveryLibraryProjection(accounts, databases, tables, "http://127.0.0.1:33333"), onStarted: () => { started++; } });
  try {
    await databases.createNewAccount(context); const registry = new OperationRegistry(accounts); runtime.registerOperations(registry);
    const execute = (name: string, input: unknown) => registry.execute(name, input, context.principal.user.id, context.epoch);
    assert.equal(registry.names().length, 31); assert.equal(await execute("onecrew_discovery_execution_get", {}), null);
    const repository = (await databases.discoveryResolver().resolve(context)).repository;
    repository.create({ id: "runtime-plan", brief: "找鞋子作者", opencli_version: "1.8.8", criteria: {}, scopes: [{ platform: "youtube", profile_ref: "work", keywords: ["shoe review"] }], target_count: 2, max_calls: 2 }); repository.approve("runtime-plan", 1);
    const input = { id: "runtime-plan", expectedRevision: 1, profileRef: "work", limit: 10 };
    await assert.rejects(execute("onecrew_discovery_youtube_search_once", input), /browser_unavailable/);
    await assert.rejects(execute("onecrew_discovery_search_once", { ...input, platform: "youtube", executable: "untrusted" }));
    await assert.rejects(execute("onecrew_discovery_youtube_verify_hit_once", { id: input.id, expectedRevision: 1, profileRef: "work", searchHitId: "unread-hit" }), /browser_unavailable/);
    const advanced = await execute("onecrew_discovery_advance", { input: { campaign_id: input.id, expected_revision: 1, excluded_platforms: ["youtube"] } }) as { step: { kind: string }; campaign: { used_calls: number } };
    assert.equal(advanced.step.kind, "search_exhausted"); assert.equal(advanced.campaign.used_calls, 0);
    await assert.rejects(execute("onecrew_discovery_evidence_collect", { input: { campaign_id: input.id, expected_revision: 2, search_hit_id: "missing", profile_ref: "work", kind: "content" } }), /evidence_approval_changed/);
    assert.equal(repository.getPlan(input.id).used_calls, 0); assert.equal(runtime.blocksBackendSync(), false);
    await assert.rejects(execute("onecrew_discovery_execution_start", { input: { campaign_id: input.id, expected_revision: 1, profile_ref: "work", excluded_platforms: [] } }), /dependency_unavailable/);
    assert.equal(started, 0); assert.equal(repository.getPlan(input.id).used_calls, 0);
    const start = { input: { campaign_id: input.id, expected_revision: 1, profile_ref: "work", excluded_platforms: ["youtube"] } };
    await Promise.all([execute("onecrew_discovery_execution_start", start), execute("onecrew_discovery_execution_start", start)]);
    assert.equal(started, 1);
    await supervisor.waitForIdle(context);
    await runtime.close();
    const status = await execute("onecrew_discovery_execution_get", {}) as { stage: string; sync_error: string; sync_pending: boolean };
    assert.equal(status.stage, "needs_evidence"); assert.equal(status.sync_error, "library_no_search_hits"); assert.equal(status.sync_pending, true);
    assert.equal(runtime.blocksBackendSync(), false); assert.equal(repository.getPlan(input.id).used_calls, 0);
    const activity = await runtime.workspaceActivity(context);
    assert.equal(activity?.campaign_id, input.id); assert.equal(runtime.workspaceHistory(context).length, 1);
  } finally { await runtime.close(); databases.close(); await tables.close(); await lease.close(); await rm(root, { recursive: true }); }
});

test("真实调度器排空原Rust已保存材料并发布实际Library，终止后保存同一工作台状态", async () => {
  const root = await mkdtemp(fileURLToPath(new URL("../../.scratch/discovery-runtime-publish-", import.meta.url))), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const tables = await createEmbeddedTables(await createPrivateDirectory(join(directory, "tables")), "http://127.0.0.1:33333"), accounts = new AccountContextService(), tenant = await tables.tenants.resolve({ id: "scheduler-owner", email: "owner@example.com" });
  const context = accounts.activate({ user: tenant.user, tenantId: tenant.tenantId, baseId: tenant.baseId, deviceId: "device", source: "access" }), databases = new AccountDatabases(lease, accounts), supervisor = new DiscoveryExecutionSupervisor(accounts, new DiscoveryAdmissionGate(accounts));
  const runtime = createDiscoveryRuntime({ accounts, repositories: databases.discoveryResolver(), directories: databases, supervisor, library: createDiscoveryLibraryProjection(accounts, databases, tables, "http://127.0.0.1:33333") });
  try {
    await databases.createNewAccount(context);
    const folders = (await readdir(directory)).filter(name => /^account-[a-f0-9]{64}$/u.test(name)); assert.equal(folders.length, 1);
    await copyFile(fileURLToPath(new URL("./fixtures/discovery-video-hydration/after.sqlite", import.meta.url)), join(directory, folders[0]!, "creator.sqlite"));
    if (process.platform !== "win32") await chmod(join(directory, folders[0]!, "creator.sqlite"), 0o600);
    const repository = (await databases.discoveryResolver().resolve(context)).repository, plan = repository.getPlan("video-hydration"), before = repository.countCandidates(plan.id);
    const registry = new OperationRegistry(accounts); runtime.registerOperations(registry);
    await registry.execute("onecrew_discovery_execution_start", { input: { campaign_id: plan.id, expected_revision: plan.revision, profile_ref: plan.scopes[0]!.profile_ref, excluded_platforms: plan.scopes.map(scope => scope.platform) } }, context.principal.user.id, context.epoch);
    await supervisor.waitForIdle(context);
    await runtime.close();
    const status = supervisor.snapshot(context)!; assert.equal(status.stage, "needs_evidence"); assert.equal(status.sync_error, null); assert.equal(status.sync_pending, false); assert.ok(status.library);
    assert.equal(repository.getPlan(plan.id).used_calls, plan.used_calls); assert.deepEqual(repository.countCandidates(plan.id), before);
    const state = await tables.data.getDiscoveryState(tenant, { table_id: status.library.table_id, campaign_id: plan.id, limit: 100 });
    assert.ok(state); assert.ok(status.library.total_hits > 0);
    const activity = await runtime.workspaceActivity(context); assert.equal(activity?.campaign_id, plan.id); assert.equal(activity?.library_sync_pending, false); assert.equal(runtime.workspaceHistory(context).length, 1);
  } finally { await runtime.close(); databases.close(); await tables.close(); await lease.close(); await rm(root, { recursive: true }); }
});
