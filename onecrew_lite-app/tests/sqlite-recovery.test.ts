import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { prepareSqliteRecovery } from "../src/migration/sqlite-recovery.ts";
import { fingerprintSqlite } from "../src/migration/sqlite-fingerprint.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("private recovery records real consistent Creator/Vault backups without activation or source changes", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/recovery-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "recovery-"));
  const source = await createPrivateDirectory(join(directory, "old"));
  const owner = await createPrivateDirectory(join(directory, "new"));
  const creator = join(source, "creator.sqlite");
  const vault = join(source, "vault.sqlite");
  await initializeNewCreatorDatabase(creator);
  const db = new DatabaseSync(vault);
  db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8"));
  db.exec("PRAGMA user_version=4");
  db.close();
  const before = { creator: fingerprintSqlite(creator), vault: fingerprintSqlite(vault) };
  const lease = await acquireInstanceLock(owner);
  try {
    const result = await prepareSqliteRecovery(lease, { creator, vault });
    assert.equal(result.activated, false);
    assert.equal(result.state, "sqlite_recovery_ready");
    assert.deepEqual(result.fingerprints, before);
    assert.deepEqual(fingerprintSqlite(creator), before.creator);
    assert.deepEqual(fingerprintSqlite(vault), before.vault);
    const journal = JSON.parse(await readFile(join(result.directory, "journal.json"), "utf8"));
    assert.equal(journal.state, "sqlite_recovery_ready");
    assert.equal(journal.activated, false);
    assert.equal(journal.creator.inspection.version, 96);
    const future = new DatabaseSync(vault);
    future.exec("PRAGMA user_version=5");
    future.close();
    await assert.rejects(prepareSqliteRecovery(lease, { creator, vault }), /Future vault/);
    const failures = (await readdir(owner)).filter(name => name.startsWith("recovery-") && name !== `recovery-${result.id}`);
    assert.equal(failures.length, 1);
    const failed = JSON.parse(await readFile(join(owner, failures[0]!, "journal.json"), "utf8"));
    assert.equal(failed.state, "failed");
    assert.equal(failed.activated, false);
    assert.equal(fingerprintSqlite(vault).version, 5);
    await lease.close();
    await assert.rejects(prepareSqliteRecovery(lease, { creator, vault }), /released/);
  } finally {
    await lease.close();
    await rm(directory, { recursive: true });
  }
});
