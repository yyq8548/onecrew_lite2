import assert from "node:assert/strict";
import { createCipheriv, randomBytes } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { backupLegacyDatabase } from "../src/storage/legacy-sqlite.ts";
import { readVaultSecret } from "../src/storage/vault-reader.ts";
import { upgradeVaultStagingCopy, validateVaultSchema } from "../src/storage/vault-schema.ts";

const schema = readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8");

for (const version of [2, 3] as const) {
  test(`vault v${version} staging upgrades preserve encrypted bytes, IDs and health fields`, async () => {
    const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-upgrade-", import.meta.url)));
    const key = randomBytes(32);
    const plaintext = Buffer.from(JSON.stringify({ value: randomBytes(30).toString("hex") }));
    try {
      const nonce = randomBytes(12);
      const cipher = createCipheriv("aes-256-gcm", key, nonce);
      const envelope = Buffer.concat([nonce, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]);
      const sourcePath = join(directory, "original.sqlite");
      const original = new DatabaseSync(sourcePath);
      original.exec(schema);
      original.exec(`PRAGMA user_version = ${version}`);
      original.prepare("INSERT INTO service_credentials (id,provider,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_used_at,last_error,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
        .run("old-id", "resend", "old-label", '{"region":"us","nested":{"safe":"value"}}', envelope, 1, 123, 45, "previous failure", "created", "updated");
      const beforeRow = original.prepare("SELECT * FROM service_credentials").get()!;
      original.close();
      const originalBytes = await readFile(sourcePath);
      const stagingPath = join(directory, "private staging", "vault.sqlite");
      await backupLegacyDatabase(sourcePath, stagingPath, "vault");
      assert.deepEqual(await upgradeVaultStagingCopy({ sourcePath, stagingPath }), { fromVersion: version, toVersion: 4 });
      const staged = new DatabaseSync(stagingPath, { readOnly: true });
      try {
        assert.equal(validateVaultSchema(staged), 4);
        assert.deepEqual({ ...staged.prepare("SELECT * FROM service_credentials").get() }, { ...beforeRow, provider: "email_resend_api_key" });
      } finally { staged.close(); }
      const restored = readVaultSecret(stagingPath, key, "service_credentials", "old-id");
      assert.deepEqual(restored, plaintext);
      restored.fill(0);
      assert.deepEqual(await readFile(sourcePath), originalBytes);
      assert.deepEqual(await upgradeVaultStagingCopy({ sourcePath, stagingPath }), { fromVersion: 4, toVersion: 4 });
      await assert.rejects(upgradeVaultStagingCopy({ sourcePath, stagingPath: sourcePath }), /distinct/);
    } finally {
      key.fill(0);
      plaintext.fill(0);
      await rm(directory, { recursive: true });
    }
  });
}

test("vault rejects future versions and modified constraints, indexes, or extra objects", () => {
  for (const mutation of [
    "PRAGMA user_version = 5",
    "DROP INDEX profiles_platform_provider_account_id",
    "CREATE TABLE extra (id TEXT)",
    "CREATE TRIGGER unexpected AFTER INSERT ON profiles BEGIN SELECT 1; END",
  ]) {
    const db = new DatabaseSync(":memory:");
    try {
      db.exec(schema);
      db.exec("PRAGMA user_version = 4");
      db.exec(mutation);
      assert.throws(() => validateVaultSchema(db), /Unsupported|identity check/);
    } finally { db.close(); }
  }
  const weakened = new DatabaseSync(":memory:");
  try {
    weakened.exec(schema.replace(" CHECK(length(trim(provider_account_id)) > 0)", ""));
    weakened.exec("PRAGMA user_version = 4");
    assert.throws(() => validateVaultSchema(weakened), /identity check/);
  } finally { weakened.close(); }
  const changedLiteral = new DatabaseSync(":memory:");
  try {
    changedLiteral.exec(schema.replace("DEFAULT '{}'", "DEFAULT '{ }'"));
    changedLiteral.exec("PRAGMA user_version = 4");
    assert.throws(() => validateVaultSchema(changedLiteral), /identity check/);
  } finally { changedLiteral.close(); }
});

for (const failure of ["conflict", "invalid-json", "metadata-secret", "metadata-array", "numeric-overflow", "invalid-unicode"] as const) {
  test(`vault staging ${failure} failure preserves original and staging bytes`, async () => {
    const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/storage-upgrade-fail-", import.meta.url)));
    try {
      const sourcePath = join(directory, "original.sqlite");
      const db = new DatabaseSync(sourcePath);
      db.exec(schema);
      db.exec("PRAGMA user_version = 2");
      const metadata = failure === "invalid-json" ? "{" : failure === "metadata-secret" ? '{"nested":{"api_key":"synthetic"}}' : failure === "metadata-array" ? "[]" : failure === "numeric-overflow" ? '{"number":1e400}' : failure === "invalid-unicode" ? '{"value":"\\ud800"}' : "{}";
      const insert = db.prepare("INSERT INTO service_credentials (id,provider,label,metadata_json,encrypted_secret,created_at,updated_at) VALUES (?,?,?,?,?,?,?)");
      insert.run("old", "resend", "same-label", metadata, randomBytes(40), "created", "updated");
      if (failure === "conflict") insert.run("typed", "email_resend_api_key", "same-label", "{}", randomBytes(40), "created", "updated");
      db.close();
      const sourceBytes = await readFile(sourcePath);
      const stagingPath = join(directory, "private staging", "vault.sqlite");
      await backupLegacyDatabase(sourcePath, stagingPath, "vault");
      const stagingBytes = await readFile(stagingPath);
      await assert.rejects(upgradeVaultStagingCopy({ sourcePath, stagingPath }));
      assert.deepEqual(await readFile(stagingPath), stagingBytes);
      assert.deepEqual(await readFile(sourcePath), sourceBytes);
      const unchanged = new DatabaseSync(stagingPath, { readOnly: true });
      try { assert.equal(validateVaultSchema(unchanged), 2); }
      finally { unchanged.close(); }
    } finally { await rm(directory, { recursive: true }); }
  });
}
