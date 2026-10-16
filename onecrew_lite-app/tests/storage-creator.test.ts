import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { homedir } from "node:os";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { creatorSchemaSnapshot, initializeNewCreatorDatabase, normalizeCreatorSchemaSql, validateCreatorSchema90 } from "../src/storage/creator-schema.ts";
import { inspectLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

const schema = ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]
  .map((file) => readFileSync(new URL(`../src/storage/${file}`, import.meta.url), "utf8")).join("\n");

test("new Creator SQLite matches the original Rust constructor across every schema object", async () => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-creator-", import.meta.url)));
  try {
    const cargo = process.env.CARGO ?? join(homedir(), ".cargo", "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
    await promisify(execFile)(cargo, ["test", "--locked", "-p", "onecrew-creator-runtime", "export_schema90_node_compatibility_fixture", "--", "--nocapture"], {
      cwd: fileURLToPath(new URL("../../onecrew_lite-rust", import.meta.url)), windowsHide: true, timeout: 600_000, maxBuffer: 1024 * 1024,
      env: { ...process.env, CARGO_INCREMENTAL: "0", ONECREW_SCHEMA_ORACLE_DIRECTORY: directory },
    });
    const privatePath = await createPrivateDirectory(join(directory, "private"));
    const path = join(privatePath, "creator.sqlite");
    await initializeNewCreatorDatabase(path);
    const created = new DatabaseSync(path, { readOnly: true });
    const original = new DatabaseSync(join(directory, "creator-schema90-oracle.sqlite"), { readOnly: true });
    try {
      validateCreatorSchema90(original);
      assert.deepEqual(creatorSchemaSnapshot(created), creatorSchemaSnapshot(original));
      const snapshot = creatorSchemaSnapshot(created);
      assert.equal(snapshot.tables.length, 79);
      assert.equal(snapshot.tables.reduce((count, table) => count + table.columns.length, 0), 661);
      assert.equal(snapshot.tables.reduce((count, table) => count + table.foreignKeys.length, 0), 196);
      assert.equal(snapshot.tables.reduce((count, table) => count + table.indexes.length, 0), 143);
      assert.equal(snapshot.objects.filter((object) => object.type === "index").length, 44);
      assert.equal(snapshot.objects.filter((object) => object.type === "trigger").length, 13);
      assert.equal(inspectLegacyDatabase(path, "creator").schemaValidated, true);
    } finally { created.close(); original.close(); }
    const before = await readFile(path);
    await assert.rejects(initializeNewCreatorDatabase(path), { code: "EEXIST" });
    assert.deepEqual(await readFile(path), before);
    const predecessorPath = join(directory, "creator-schema89-oracle.sqlite");
    const previousBytes = await readFile(predecessorPath);
    const previousInspection = inspectLegacyDatabase(predecessorPath, "creator");
    assert.equal(previousInspection.version, 89);
    assert.equal(previousInspection.state, "upgrade-required");
    assert.equal(previousInspection.schemaValidated, false);
    await assert.rejects(initializeNewCreatorDatabase(predecessorPath));
    assert.deepEqual(await readFile(predecessorPath), previousBytes);
  } finally { await rm(directory, { recursive: true }); }
});

test("Creator preserves the Rust archive marker rule beyond structural validation", () => {
  const db = new DatabaseSync(":memory:");
  try {
    db.exec(schema);
    db.exec("PRAGMA user_version=90; PRAGMA foreign_keys=ON");
    db.exec("INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at) VALUES('mailbox','gmail','test','test@example.com','[]',1,1,1)");
    db.exec("INSERT INTO email_threads(id,mailbox_ref_id,provider_thread_key,subject,active_folder,archived_at,created_at,updated_at) VALUES('thread','mailbox','provider-thread','subject','sent',1,1,1)");
    assert.throws(() => validateCreatorSchema90(db), /archive marker/);
    db.exec("INSERT INTO emails(id,thread_id,provider_message_id,direction,subject,delivery_status,occurred_at,origin) VALUES('email','thread','provider-message','inbound','subject','received',1,'provider_import')");
    validateCreatorSchema90(db);
  } finally { db.close(); }
});

test("Creator CST normalization ignores structural spacing and preserves quoted values", () => {
  assert.equal(normalizeCreatorSchemaSql("CREATE TABLE t(x TEXT CHECK ( x = 'a b' ))"), normalizeCreatorSchemaSql("create table t ( x text CHECK(x='a b'))"));
  assert.notEqual(normalizeCreatorSchemaSql("CREATE TABLE t(x TEXT CHECK(x='a b'))"), normalizeCreatorSchemaSql("CREATE TABLE t(x TEXT CHECK(x='ab'))"));
  assert.notEqual(normalizeCreatorSchemaSql("CREATE TABLE t(x TEXT CHECK(x='Codex CLI'))"), normalizeCreatorSchemaSql("CREATE TABLE t(x TEXT CHECK(x='codex cli'))"));
  assert.notEqual(normalizeCreatorSchemaSql('CREATE TABLE "Name"(x TEXT)'), normalizeCreatorSchemaSql('CREATE TABLE "name"(x TEXT)'));
});

test("Creator schema rejects unknown objects, weakened constraints, and invalid stored tags", () => {
  for (const mutation of ["CREATE TABLE unexpected(id TEXT)", "DROP INDEX creators_by_group", "DROP TRIGGER prevent_shared_creator_contact_locator_insert", "ALTER TABLE creators ADD COLUMN campaign_id TEXT", "PRAGMA user_version=91"]) {
    const db = new DatabaseSync(":memory:");
    try {
      db.exec(schema);
      db.exec("PRAGMA user_version=90");
      db.exec(mutation);
      assert.throws(() => validateCreatorSchema90(db));
    } finally { db.close(); }
  }
  for (const tags of ['[" duplicate","duplicate"]', '["same","same"]', '["1234567890123"]', '["line\\nfeed"]', '[1]']) {
    const db = new DatabaseSync(":memory:");
    try {
      db.exec(schema);
      db.exec("PRAGMA user_version=90");
      db.prepare("INSERT INTO creators(id,display_name,created_at,updated_at,tags_json) VALUES('id','test',1,1,?)").run(tags);
      assert.throws(() => validateCreatorSchema90(db), /tags/);
    } finally { db.close(); }
  }
  const weak = new DatabaseSync(":memory:");
  try {
    weak.exec(schema.replace("revision integer not null check(revision >= 1)", "revision integer not null"));
    weak.exec("PRAGMA user_version=90");
    assert.throws(() => validateCreatorSchema90(weak), /structure differs/);
  } finally { weak.close(); }
});
