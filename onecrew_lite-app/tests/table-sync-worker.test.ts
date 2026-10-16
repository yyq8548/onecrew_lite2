import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { initializeNewInstallVault } from "../src/storage/new-install-vault.ts";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";
import { TableSyncClient } from "../src/sync/table-sync-client.ts";
import { TableSyncStore, tableSyncKey } from "../src/sync/table-sync-store.ts";
import { TableSyncWorker } from "../src/sync/table-sync-worker.ts";
import { createLibraryCaptureSource } from "../src/sync/library-capture.ts";
import { parseApplicationJson } from "../src/contracts/application-json.ts";

test("real empty Vault cannot enable remote sync; disabled local queue and missing Library binding remain explicit", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/table-sync-worker-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "worker-")), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const tables = await createEmbeddedTables(join(directory, "tables"), "http://127.0.0.1:34567");
  const vault = join(directory, "vault.sqlite"), key = join(directory, "masterkey");
  await initializeNewInstallVault(lease, vault, key);
  const identities = new UnifiedAccountService(vault, key, tables.tenants), tenant = await tables.tenants.resolve({ id: randomUUID(), email: "native-owner@example.test" });
  const context = identities.contexts.activate({ ...tenant, deviceId: "native-test-device", source: "access" });
  const databases = new AccountDatabases(lease, identities.contexts), store = new TableSyncStore(identities.contexts, databases), capture = createLibraryCaptureSource(identities.contexts, tables), gate = new DiscoveryAdmissionGate(identities.contexts);
  await databases.createNewAccount(context);
  const accountDirectory = await databases.accountDirectory(context), sqlite = new DatabaseSync(join(accountDirectory.directory, "creator.sqlite"), { readOnly: true });
  // 读取真实仓储中的当前任务状态；没有运行任务时也不代替服务端 consent。
  const worker = new TableSyncWorker(identities.contexts, store, new TableSyncClient(identities), gate, capture, () => { const count = sqlite.prepare("SELECT count(*) AS count FROM discovery_campaigns WHERE phase='active'").get()?.count; if (typeof count !== "number") throw new Error("Invalid native campaign count"); return count > 0; });
  try {
    await worker.registerResult(context, "campaign");
    await worker.configure(context, "campaign", false);
    await worker.searchSettled(context, "campaign");
    await worker.tick(context);
    await store.exclusive(context, async state => { const value = state.tables[tableSyncKey(context.principal.user.id, "campaign")]!; assert.equal(value.enabled, false); assert.equal(value.status, "disabled"); assert.equal(value.pending, null); });
    await assert.rejects(capture.capture(context, "campaign", { version_id: randomUUID(), source_instance_id: randomUUID(), consent_id: randomUUID(), capture_started_at: new Date().toISOString() }), /recommendation_table_missing/);
    await assert.rejects(worker.configure(context, "campaign", true));
    const saved = parseApplicationJson(await readFile(join(accountDirectory.directory, "dynamic-table-sync.json"))) as { tables: Record<string, { enabled: boolean }> };
    assert.equal(saved.tables[tableSyncKey(context.principal.user.id, "campaign")]!.enabled, false);
    worker.stopAcceptingWork();
    await assert.rejects(worker.registerResult(context, "another"), /stopping/);
  } finally { worker.stopAcceptingWork(); store.close(); identities.invalidate(); sqlite.close(); databases.close(); await tables.close(); await lease.close(); await rm(root, { recursive: true }); }
});
