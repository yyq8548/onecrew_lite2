import assert from "node:assert/strict";
import { chmod, copyFile, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { createEmbeddedTables, TableDataError } from "onecrew_lite_tables/embedded";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { createDiscoveryLibraryProjection } from "../src/sync/discovery-library.ts";
import { createLibraryCaptureSource } from "../src/sync/library-capture.ts";
import { validateTableSyncArchive } from "../src/sync/table-sync-integrity.ts";
import { randomUUID } from "node:crypto";

for (const fixture of ["discovery-identity/merged.sqlite", "discovery-contacts/native-before.sqlite", "discovery-contacts/native-after.sqlite", ...(process.env.ONECREW_QUERY_FEEDBACK_DATABASE ? ["current-account"] : [])]) test(`real SQLite ${fixture} publishes into embedded Library, replays exact row IDs and rejects an old account`, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/discovery-library-embedded/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "projection-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory);
  const tables = await createEmbeddedTables(await createPrivateDirectory(join(directory, "tables")), "http://127.0.0.1:33333");
  const accounts = new AccountContextService();
  const tenant = await tables.tenants.resolve({ id: "operator", email: "operator@example.com" });
  const context = accounts.activate({ user: tenant.user, tenantId: tenant.tenantId, baseId: tenant.baseId, deviceId: "test-device", source: "access" });
  const databases = new AccountDatabases(lease, accounts);
  try {
    await databases.createNewAccount(context);
    const directories = (await readdir(directory)).filter(name => /^account-[a-f0-9]{64}$/u.test(name));
    assert.equal(directories.length, 1);
    const path = join(directory, directories[0]!, "creator.sqlite");
    await copyFile(fixture === "current-account" ? process.env.ONECREW_QUERY_FEEDBACK_DATABASE! : fileURLToPath(new URL(`./fixtures/${fixture}`, import.meta.url)), path);
    if (process.platform !== "win32") await chmod(path, 0o600);
    using original = new DatabaseSync(path, { readOnly: true });
    const plan = fixture === "current-account" ? original.prepare("SELECT id,current_revision FROM discovery_campaigns WHERE id=?").get("b29b55fa-1131-4d00-9d92-ecad85d3f8f7")! : original.prepare("SELECT id,current_revision FROM discovery_campaigns ORDER BY id LIMIT 1").get()!;
    assert.equal(typeof plan.id, "string"); assert.equal(typeof plan.current_revision, "number");
    let stateReads = 0, identityReads = 0;
    // 仅记录真实 embedded 方法的调用次数，所有数据和校验仍由实际实现返回。
    const observed = { ...tables, data: { ...tables.data,
      async getDiscoveryState(...args: Parameters<typeof tables.data.getDiscoveryState>) { stateReads++; return tables.data.getDiscoveryState(...args); },
      async getDiscoveryIdentity(...args: Parameters<typeof tables.data.getDiscoveryIdentity>) { identityReads++; return tables.data.getDiscoveryIdentity(...args); },
    } };
    const projection = createDiscoveryLibraryProjection(accounts, databases, observed, "http://127.0.0.1:33333");
    await projection.readiness(context);
    const result = await projection.publish(context, plan.id as string, plan.current_revision as number);
    assert.ok(result.total_hits > 0); assert.equal(result.inserted_rows, result.total_hits);
    assert.equal(result.web_url, `http://127.0.0.1:33333/library/${result.table_id}`);
    const before = await tables.data.getDiscoveryState(tenant, { table_id: result.table_id, campaign_id: plan.id as string, limit: 100 });
    const identity = await tables.data.getDiscoveryIdentity(tenant, { table_id: result.table_id, campaign_id: plan.id as string });
    assert.equal(identity.state, "current");
    if (fixture.startsWith("discovery-identity/")) assert.ok(identity.projection!.groups.some(group => group.members.length > 1));
    const beforeReuse = stateReads, beforeIdentity = identityReads;
    assert.deepEqual(await projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true }), { ...result, inserted_rows: 0 });
    assert.equal(stateReads, beforeReuse);
    assert.equal(identityReads, beforeIdentity + 1);
    const repeated = await projection.publish(context, plan.id as string, plan.current_revision as number);
    assert.equal(repeated.inserted_rows, 0); assert.equal(repeated.table_id, result.table_id);
    assert.ok(stateReads > beforeReuse, "手动默认同步继续读取完整分页状态");
    assert.deepEqual(await tables.data.getDiscoveryState(tenant, { table_id: result.table_id, campaign_id: plan.id as string, limit: 100 }), before);
    assert.deepEqual(await tables.data.getDiscoveryIdentity(tenant, { table_id: result.table_id, campaign_id: plan.id as string }), identity);
    const capture = await createLibraryCaptureSource(accounts, tables).capture(context, plan.id as string, { version_id: randomUUID(), source_instance_id: randomUUID(), consent_id: randomUUID(), capture_started_at: new Date().toISOString() });
    assert.equal(capture.table_id, result.table_id);
    assert.match(capture.content_hash, /^[a-f0-9]{64}$/u);
    await validateTableSyncArchive(capture.archive, BigInt(capture.record_count));
    const owned = await databases.discoveryResolver().resolve(context);
    if (fixture === "discovery-identity/merged.sqlite") {
      using changed = new DatabaseSync(path);
      const candidate = changed.prepare("SELECT id FROM discovery_candidates WHERE discovery_campaign_id=? AND criteria_revision=? AND verdict='qualified' LIMIT 1").get(plan.id as string, plan.current_revision as number)!;
      const material = owned.repository.readLibraryMaterial(plan.id as string, plan.current_revision as number);
      changed.prepare("UPDATE discovery_candidates SET verdict='rejected' WHERE id=?").run(String(candidate.id));
      const rejected = owned.repository.readLibraryMaterial(plan.id as string, plan.current_revision as number), beforeRejection = stateReads;
      assert.notEqual(rejected.sources.snapshot_hash, material.sources.snapshot_hash);
      await projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true });
      assert.ok(stateReads > beforeRejection);
      const rejectedRows = await tables.data.getDiscoveryState(tenant, { table_id: result.table_id, campaign_id: plan.id as string, limit: 100 });
      assert.ok(rejectedRows.rows.some(row => row.candidate_id === candidate.id && row.verdict === "rejected"));
      const binding = changed.prepare("SELECT * FROM discovery_video_identity_bindings WHERE discovery_campaign_id=? LIMIT 1").get(plan.id as string)!;
      changed.prepare("DELETE FROM discovery_video_identity_bindings WHERE search_hit_id=?").run(String(binding.search_hit_id));
      const unbound = owned.repository.readLibraryMaterial(plan.id as string, plan.current_revision as number), beforeBinding = stateReads;
      assert.notEqual(unbound.sources.snapshot_hash, rejected.sources.snapshot_hash);
      await assert.rejects(projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true }), (error: unknown) => error instanceof Error && error.cause instanceof TableDataError && error.cause.code === "data.projection_identity_conflict");
      assert.ok(stateReads > beforeBinding);
      changed.prepare("INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at) VALUES(?,?,?,?,?,?)").run(String(binding.search_hit_id), String(binding.discovery_campaign_id), Number(binding.criteria_revision), String(binding.candidate_id), String(binding.native_author_id), Number(binding.created_at));
      await projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true });
    }
    if (fixture === "discovery-contacts/native-after.sqlite") {
      const material = owned.repository.readLibraryMaterial(plan.id as string, plan.current_revision as number);
      assert.ok(material.sources.rows.some(row => row.contacts?.length));
      using changed = new DatabaseSync(path);
      assert.ok(changed.prepare("DELETE FROM discovery_contact_observations WHERE discovery_campaign_id=? AND criteria_revision=?").run(plan.id as string, plan.current_revision as number).changes > 0);
      const withoutContacts = owned.repository.readLibraryMaterial(plan.id as string, plan.current_revision as number), beforeContacts = stateReads;
      assert.notEqual(withoutContacts.sources.snapshot_hash, material.sources.snapshot_hash);
      await projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true });
      assert.ok(stateReads > beforeContacts);
      const saved = await tables.data.getDiscoveryState(tenant, { table_id: result.table_id, campaign_id: plan.id as string, limit: 100 });
      for (const row of withoutContacts.sources.rows) assert.deepEqual(saved.rows.find(item => item.search_hit_id === row.hit.id)!.contacts, row.contacts ?? []);
    }
    const latest = await tables.data.getDiscoveryState(tenant, { table_id: result.table_id, campaign_id: plan.id as string, limit: 100 });
    await tables.data.deleteRows(tenant, result.table_id, [latest.rows[0]!.row_id]);
    assert.equal((await tables.data.getDiscoveryIdentity(tenant, { table_id: result.table_id, campaign_id: plan.id as string })).state, "sources_changed");
    const beforeDamage = stateReads;
    await assert.rejects(projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true }), { code: "library_row_missing" });
    assert.ok(stateReads > beforeDamage);
    await assert.rejects(projection.publish(context, plan.id as string, plan.current_revision as number), { code: "library_row_missing" });
    accounts.logout();
    await assert.rejects(projection.publish(context, plan.id as string, plan.current_revision as number), { code: "access.account_changed" });
    await assert.rejects(projection.publish(context, plan.id as string, plan.current_revision as number, { reuse_unchanged: true }), { code: "access.account_changed" });
  } finally { databases.close(); await tables.close(); await lease.close(); await rm(root, { recursive: true }); }
});
