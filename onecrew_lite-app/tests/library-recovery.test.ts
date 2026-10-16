import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, realpath, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { prepareLibraryRecovery, prepareLibraryStaging } from "../src/migration/library-recovery.ts";
import { readLibraryMigrationAccount } from "../src/identity/library-migration-account.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("real PGlite recovery refuses its active writer and preserves an isolated reopenable copy", { timeout: 300000 }, async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/library-recovery-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "recovery-"));
  const source = await realpath(await createPrivateDirectory(join(directory, "old-library")));
  const destination = await createPrivateDirectory(join(directory, "new-owner"));
  const lease = await acquireInstanceLock(destination);
  const tables = await createEmbeddedTables(source, "http://127.0.0.1:45987");
  let sourceClosed = false;
  try {
    const tenant = await tables.tenants.resolve({ id: "stored-table-owner", email: "owner@example.com" });
    await assert.rejects(prepareLibraryRecovery(lease, source), /library_database_busy/);
    await tables.close(); sourceClosed = true;
    const recovery = await prepareLibraryRecovery(lease, source);
    assert.equal(recovery.activated, false);
    assert.equal(recovery.state, "library_recovery_ready");
    assert.ok(recovery.files.some(file => file.path === "PG_VERSION"));
    const journal = JSON.parse(await readFile(join(recovery.directory, "journal.json"), "utf8"));
    assert.equal(journal.state, recovery.state);
    const staging = await prepareLibraryStaging(lease, recovery);
    const snapshot = await readLibraryMigrationAccount(lease, staging.directory);
    assert.equal(snapshot.snapshot.state, "missing");
    assert.equal(snapshot.credential, undefined);
    assert.deepEqual(snapshot.knownAccountIds, []);
    assert.deepEqual(snapshot.tenants, [{ id: tenant.tenantId, owner_user_id: "stored-table-owner", base_id: tenant.baseId, space_id: snapshot.tenants[0]!.space_id }]);
    const reopened = await createEmbeddedTables(source, "http://127.0.0.1:45987");
    try { assert.equal((await reopened.tenants.resolve({ id: "stored-table-owner", email: "owner@example.com" })).baseId, tenant.baseId); }
    finally { await reopened.close(); }
  } finally {
    if (!sourceClosed) await tables.close();
    await lease.close();
    await rm(directory, { recursive: true });
  }
});
