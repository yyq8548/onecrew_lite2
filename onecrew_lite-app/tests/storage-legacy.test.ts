import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { backupLegacyDatabase, inspectLegacyDatabase } from "../src/storage/legacy-sqlite.ts";

test("read-only inspection rejects unsupported versions and missing/corrupt files", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-", import.meta.url)));
  try {
    const path = join(directory, "creator.sqlite");
    const db = new DatabaseSync(path);
    for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) {
      db.exec(readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8"));
    }
    db.exec("INSERT INTO creators (id,display_name,created_at,updated_at) VALUES ('id','保留',1,1); PRAGMA user_version = 90");
    db.close();
    const before = await readFile(path);
    assert.equal(inspectLegacyDatabase(path, "creator").state, "upgrade-required");
    assert.equal(inspectLegacyDatabase(path, "creator").schemaValidated, true);
    assert.deepEqual(await readFile(path), before);
    assert.throws(() => inspectLegacyDatabase(path, "vault"), /Future/);
    const future = new DatabaseSync(path);
    future.exec("PRAGMA user_version = 93");
    future.close();
    assert.throws(() => inspectLegacyDatabase(path, "creator"), /Future/);
    assert.throws(() => inspectLegacyDatabase(join(directory, "missing"), "creator"));
    const corrupt = join(directory, "corrupt");
    await writeFile(corrupt, "invalid SQLite file");
    assert.throws(() => inspectLegacyDatabase(corrupt, "creator"));
  } finally {
    await rm(directory, { recursive: true });
  }
});

test("SQLite backup preserves WAL in a private directory and never overwrites files", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-", import.meta.url)));
  const path = join(directory, "vault.sqlite");
  const db = new DatabaseSync(path);
  try {
    db.exec("PRAGMA journal_mode = WAL");
    db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8"));
    db.exec("PRAGMA user_version = 4");
    db.prepare("INSERT INTO service_credentials (id,provider,label,encrypted_secret,created_at,updated_at) VALUES (?,?,?,?,?,?)")
      .run("id", "gmail", "test", Buffer.from("encrypted-secret-bytes"), "created", "updated");
    const destination = join(directory, "private backup", "backup.sqlite");
    const result = await backupLegacyDatabase(path, destination, "vault");
    assert.equal(result.inspection.version, 4);
    assert.equal(result.inspection.schemaValidated, true);
    const copied = new DatabaseSync(destination, { readOnly: true });
    try {
      assert.deepEqual(Buffer.from(copied.prepare("SELECT encrypted_secret FROM service_credentials").get()!.encrypted_secret as Uint8Array), Buffer.from("encrypted-secret-bytes"));
    } finally {
      copied.close();
    }
    const before = await readFile(destination);
    await assert.rejects(backupLegacyDatabase(path, destination, "vault"));
    assert.deepEqual(await readFile(destination), before);
    await assert.rejects(backupLegacyDatabase(path, path, "vault"), /differ/);
    db.exec("PRAGMA user_version = 3");
    assert.equal(inspectLegacyDatabase(path, "vault").state, "upgrade-required");
    db.exec("PRAGMA user_version = 1");
    assert.throws(() => inspectLegacyDatabase(path, "vault"), /Unsupported/);
  } finally {
    db.close();
    await rm(directory, { recursive: true });
  }
});
