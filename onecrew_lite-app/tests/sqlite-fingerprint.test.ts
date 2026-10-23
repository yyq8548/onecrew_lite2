import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync, backup } from "node:sqlite";
import { fileURLToPath } from "node:url";
import test from "node:test";
import { fingerprintSqlite } from "../src/migration/sqlite-fingerprint.ts";

test("logical fingerprints preserve SQLite types, integer precision, raw text and duplicate rows across real backups", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/fingerprint-tests", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const directory = await mkdtemp(join(scratch, "database-"));
  const source = join(directory, "source.sqlite");
  const copy = join(directory, "copy.sqlite");
  const db = new DatabaseSync(source);
  try {
    db.exec("PRAGMA journal_mode=WAL; PRAGMA user_version=90; CREATE TABLE records(value); INSERT INTO records VALUES(9223372036854775807),(9223372036854775806),(NULL),(''),(x''),(CAST(x'80' AS TEXT)),('重复'),('重复')");
    await backup(db, copy);
    const original = fingerprintSqlite(source);
    assert.deepEqual(fingerprintSqlite(copy), original);
    assert.equal(original.tables[0]!.rows, 8);
    db.exec("INSERT INTO records VALUES(1.0)");
    const withReal = fingerprintSqlite(source);
    db.exec("UPDATE records SET value=1.0000000000000002 WHERE typeof(value)='real'");
    assert.notDeepEqual(fingerprintSqlite(source), withReal);
    db.exec("UPDATE records SET value=9223372036854775806 WHERE typeof(value)='integer' AND value=9223372036854775807");
    assert.notDeepEqual(fingerprintSqlite(source), original);
    db.exec("DELETE FROM records WHERE typeof(value)='text' AND CAST(value AS BLOB)=x'80'; INSERT INTO records VALUES(CAST(x'81' AS TEXT))");
    assert.notDeepEqual(fingerprintSqlite(source).tables, fingerprintSqlite(copy).tables);
    db.exec("CREATE INDEX records_value ON records(value)");
    assert.notEqual(fingerprintSqlite(source).schemaSha256, original.schemaSha256);
  } finally {
    db.close();
    await rm(directory, { recursive: true });
  }
});
