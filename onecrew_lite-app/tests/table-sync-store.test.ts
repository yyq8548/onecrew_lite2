import assert from "node:assert/strict";
import { appendFile, mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { AccountContextService } from "../src/identity/account-context.ts";
import { AccountDatabases } from "../src/identity/account-databases.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import { registerTableSyncResult, resumeTableSyncConsent, TableSyncStore, tableSyncKey, tableSyncQueued } from "../src/sync/table-sync-store.ts";

test("real private account queue reopens exact u64, serializes checkpoints, detects external edits and rejects stale epochs", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/table-sync-store-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "queue-")), directory = await createPrivateDirectory(join(root, "private")), lease = await acquireInstanceLock(directory);
  const accounts = new AccountContextService(), account = accounts.activate({ user: { id: "owner", email: "owner@example.com" }, tenantId: "tenant", baseId: "base", deviceId: "device", source: "access" });
  const databases = new AccountDatabases(lease, accounts), store = new TableSyncStore(accounts, databases);
  try {
    await databases.createNewAccount(account);
    const key = tableSyncKey("owner", "campaign");
    let expiredCheckpoint: (() => Promise<void>) | undefined;
    await store.exclusive(account, async (state, checkpoint) => {
      assert.equal(registerTableSyncResult(state, "owner", "campaign"), true);
      assert.equal(registerTableSyncResult(state, "owner", "campaign"), false);
      const value = state.tables[key]!;
      value.sequence = 18446744073709551615n;
      assert.equal(tableSyncQueued(value, 0n), true);
      await checkpoint(); expiredCheckpoint = checkpoint;
    });
    await assert.rejects(expiredCheckpoint!, /checkpoint_expired/);
    await Promise.all(Array.from({ length: 3 }, () => store.exclusive(account, async (state, checkpoint) => { assert.equal(state.tables[key]!.sequence, 18446744073709551615n); state.tables[key]!.failures++; await checkpoint(); })));
    await store.exclusive(account, async (state, checkpoint) => {
      const value = state.tables[key]!;
      assert.equal(value.failures, 3);
      for (const status of ["synced", "up_to_date"]) { value.status = status; assert.equal(tableSyncQueued(value, 0n), false); assert.equal(registerTableSyncResult(state, "owner", "campaign"), true); }
      value.enabled = false; value.status = "consent_revoked";
      assert.equal(registerTableSyncResult(state, "owner", "campaign"), false);
      resumeTableSyncConsent(state, "other"); assert.equal(value.enabled, false);
      resumeTableSyncConsent(state, "owner"); assert.equal(value.enabled, true);
      await checkpoint();
    });
    const accountDirectory = await databases.accountDirectory(account), path = join(accountDirectory.directory, "dynamic-table-sync.json");
    assert.match(await readFile(path, "utf8"), /18446744073709551615/);
    await assert.rejects(store.exclusive(account, async (_state, checkpoint) => { await appendFile(path, " "); await checkpoint(); }), /store_changed/);
    const reopened = new TableSyncStore(accounts, databases);
    try {
      await reopened.exclusive(account, async state => { assert.equal(state.tables[key]!.sequence, 18446744073709551615n); });
      await assert.rejects(reopened.exclusive(account, async (state, checkpoint) => { state.tables[key]!.user_id = "other"; await checkpoint(); }), /account_changed/);
      accounts.logout();
      await assert.rejects(reopened.exclusive(account, async () => undefined), /selected OneCrew account changed/);
    } finally { reopened.close(); }
  } finally { store.close(); databases.close(); await lease.close(); await rm(root, { recursive: true }); }
});
