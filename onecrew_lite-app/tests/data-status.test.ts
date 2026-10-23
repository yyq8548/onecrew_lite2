import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { inspectUnifiedData } from "../src/service/data-status.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { initializeNewInstallVault } from "../src/storage/new-install-vault.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("data status checks actual private Vault and account schemas without upgrading old data", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/data-status-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "status-"));
  const lease = await acquireInstanceLock(await createPrivateDirectory(join(root, "private")));
  const vaultPath = join(lease.directory, "vault.sqlite");
  const accounts = new AccountContextService();
  const databases = new AccountDatabases(lease, accounts);
  try {
    await initializeNewInstallVault(lease, vaultPath, join(lease.directory, "masterkey"));
    const options = { lease, generationRoot: lease.directory, vaultPath, originSource: "new-install", candidate: false };
    const fresh = await inspectUnifiedData(options);
    assert.equal(fresh.phase, "ready");
    assert.equal(fresh.preflight?.state, "fresh");
    assert.equal(fresh.preflight?.recoverable_backup, false);
    assert.equal((await inspectUnifiedData({ ...options, originSource: "legacy" })).preflight?.state, "blocked");
    const context = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
    await databases.ensureNewAccount(context);
    assert.equal((await inspectUnifiedData(options)).preflight?.state, "ready");
    const before = await readFile(vaultPath);
    using vault = new DatabaseSync(vaultPath);
    vault.exec("PRAGMA user_version=3");
    vault.exec("PRAGMA wal_checkpoint(TRUNCATE)");
    const oldBytes = await readFile(vaultPath);
    assert.notDeepEqual(oldBytes, before);
    const blocked = await inspectUnifiedData(options);
    assert.equal(blocked.phase, "failed");
    assert.equal(blocked.preflight?.state, "blocked");
    assert.equal(blocked.preflight?.blocked_store, "account");
    assert.equal(blocked.preflight?.targets[0]?.from_version, 3);
    assert.equal(vault.prepare("PRAGMA user_version").get()?.user_version, 3);
    assert.deepEqual(await readFile(vaultPath), oldBytes);
  } finally { databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});
