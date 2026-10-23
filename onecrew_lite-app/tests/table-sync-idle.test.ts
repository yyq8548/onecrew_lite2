import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { setTimeout } from "node:timers/promises";
import { channel } from "node:diagnostics_channel";
import test from "node:test";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { initializeNewInstallVault } from "../src/storage/new-install-vault.ts";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { OperationRegistry } from "../src/application/operation-registry.ts";
import { DiscoveryAdmissionGate } from "../src/discovery/admission-gate.ts";
import { createTableSyncModule } from "../src/sync/table-sync-module.ts";
import { createLibraryCaptureSource } from "../src/sync/library-capture.ts";

test("idle sync timer does not refresh identity or invalidate a local session without queued uploads", { timeout: 90_000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/table-sync-idle-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "idle-")), directory = await createPrivateDirectory(join(root, "private"));
  const lease = await acquireInstanceLock(directory), vault = join(directory, "vault.sqlite"), key = join(directory, "masterkey");
  await initializeNewInstallVault(lease, vault, key);
  const tables = await createEmbeddedTables(join(directory, "tables"), "http://127.0.0.1:34567");
  const identities = new UnifiedAccountService(vault, key, tables.tenants);
  const tenant = await tables.tenants.resolve({ id: randomUUID(), email: "idle-owner@example.test" });
  const context = identities.contexts.activate({ ...tenant, deviceId: "idle-device", source: "access" });
  const databases = new AccountDatabases(lease, identities.contexts);
  await databases.createNewAccount(context);
  let requests = 0;
  const observe = () => { requests++; }, traffic = channel("undici:request:create");
  traffic.subscribe(observe);
  const sync = createTableSyncModule(identities, databases, new OperationRegistry(identities.contexts), new DiscoveryAdmissionGate(identities.contexts), createLibraryCaptureSource(identities.contexts, tables), () => false);
  try {
    await setTimeout(32_000);
    await sync.close();
    identities.contexts.assertCurrent(context);
    assert.equal(requests, 0);
    assert.deepEqual(sync.status(), { running: false, failed: false });
    // An actual operation still cannot use this empty Vault as authenticated authority.
    await assert.rejects(identities.refresh(), /access.login_required/);
    assert.throws(() => identities.contexts.assertCurrent(context), /account changed/i);
  } finally {
    traffic.unsubscribe(observe);
    await sync.close(); identities.invalidate(); databases.close(); await tables.close(); await lease.close();
    await rm(root, { recursive: true });
  }
});
