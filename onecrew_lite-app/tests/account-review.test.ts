import assert from "node:assert/strict";
import { createCipheriv, randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp, open, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { openDatabase } from "../../onecrew_lite-library/dist/database.js";
import { createCredentialStore } from "../../onecrew_lite-library/dist/modules/access/store.js";
import { reviewMigrationAccounts, verifyMigrationAccount } from "../src/migration/account-review.ts";
import { prepareLibraryRecovery, prepareLibraryStaging } from "../src/migration/library-recovery.ts";
import { prepareSqliteRecovery } from "../src/migration/sqlite-recovery.ts";
import { prepareSqliteStaging } from "../src/migration/sqlite-staging.ts";
import { fingerprintSqlite } from "../src/migration/sqlite-fingerprint.ts";
import { writeMigrationJournal } from "../src/migration/journal.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

function localLibraryCredential(userId: string) {
  const credentialId = randomUUID();
  return { bearer: `${credentialId}_${randomBytes(32).toString("base64url")}`, installation: {
    installation_id: randomUUID(), credential_id: credentialId, access_user_id: userId,
    access_device_id: `dev_${randomBytes(18).toString("base64url")}`, access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`,
    source_instance_id: randomUUID(), client_platform: "windows" as const, app_version: "0.1.4", status: "active" as const,
    revision: 1, created_at: 1, updated_at: 1, last_seen_at: null, revoked_at: null,
  } };
}

test("migration account review uses real encrypted Vault and isolated PGlite without issuing offline proof", { timeout: 900_000 }, async t => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/account-review-", import.meta.url)));
  const old = await createPrivateDirectory(join(directory, "旧数据"));
  const owner = await createPrivateDirectory(join(directory, "新实例"));
  const creator = join(old, "creator.sqlite"), vault = join(old, "vault.sqlite"), masterKey = join(old, "masterkey");
  await initializeNewCreatorDatabase(creator);
  const key = randomBytes(32);
  await writeFile(masterKey, key, { flag: "wx", mode: 0o600 });
  { await using reserved = await open(vault, "wx", 0o600); }
  { using db = new DatabaseSync(vault); db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8")); db.exec("PRAGMA user_version=4"); }
  const lease = await acquireInstanceLock(owner);
  try {
    let sqlite = await prepareSqliteStaging(lease, { recovery: await prepareSqliteRecovery(lease, { creator, vault }), sources: { creator, vault, masterKey } });
    const sourceLibrary = await createPrivateDirectory(join(directory, "old-library"));
    const tables = await createEmbeddedTables(sourceLibrary, "http://127.0.0.1:45987");
    const tenant = await tables.tenants.resolve({ id: "history-owner", email: "history@example.com" });
    await tables.close();
    const libraryReceipt = await prepareLibraryStaging(lease, await prepareLibraryRecovery(lease, sourceLibrary));
    const signal = new AbortController().signal;
    const input = { sqlite, library: { state: "staged" as const, receipt: libraryReceipt }, signal };
    async function setMain(userId: string | null, status: "active" | "revoked" = "active") {
      using db = new DatabaseSync(sqlite.vault.path);
      db.exec("DELETE FROM service_credentials");
      let bearer: string | undefined;
      if (userId) {
        const credentialId = randomUUID();
        const metadata = { installation_id: randomUUID(), credential_id: credentialId, access_user_id: userId, access_device_id: `dev_${randomBytes(18).toString("base64url")}`,
          source_instance_id: randomUUID(), access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`, client_platform: "windows", app_version: "0.7.1", status,
          revision: 1, created_at: 1, updated_at: 2, last_seen_at: null, revoked_at: status === "revoked" ? 3 : null,
          protocol: "reacher.creator_sync", protocol_version: 1, capabilities: ["creator_snapshots:write_chunked_quarantine"],
          accepted_bundle: { format: "reacher.creator_bundle", format_version: 3, profile: "portable_public", max_snapshot_bytes: 32 * 1024 * 1024,
            max_chunk_bytes: 1024 * 1024, max_chunks: 128, max_namespace_staged_bytes: 100 * 1024 * 1024, max_access_user_staged_bytes: 400 * 1024 * 1024 } };
        bearer = `${credentialId}_${randomBytes(32).toString("base64url")}`;
        const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce);
        const encrypted = Buffer.concat([nonce, cipher.update(JSON.stringify({ value: bearer })), cipher.final(), cipher.getAuthTag()]);
        db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,created_at,updated_at) VALUES(?,'creator_sync','creator-sync',?,?,?,'1','2')")
          .run(randomUUID(), JSON.stringify(metadata), encrypted, status === "active" ? 0 : 1);
      }
      sqlite = { ...sqlite, fingerprints: { ...sqlite.fingerprints, vault: fingerprintSqlite(sqlite.vault.path) } };
      input.sqlite = sqlite;
      await writeMigrationJournal(sqlite.directory, sqlite);
      return bearer;
    }
    async function setLibrary(userId: string | null) {
      const db = await openDatabase(libraryReceipt.directory);
      try {
        const store = createCredentialStore(db.db);
        await store.clear();
        if (userId) { const credential = localLibraryCredential(userId); await store.save(credential); return credential.bearer; }
      } finally { await db.close(); }
    }
    await t.test("same account keeps all tenant/history ownership and exposes no bearer", async () => {
      const mainBearer = await setMain("same-owner"), libraryBearer = await setLibrary("same-owner");
      const result = await reviewMigrationAccounts(lease, input);
      assert.equal(result.state, "verify_existing");
      assert.equal(result.decision.state, "verify_existing");
      assert.deepEqual(result.preserved.tenants.map(value => [value.owner_user_id, value.base_id]), [["history-owner", tenant.baseId]]);
      assert.ok(result.preserved.knownLibraryAccountIds.includes("same-owner"));
      for (const text of [JSON.stringify(result), inspect(result)]) {
        assert.ok(!text.includes(mainBearer!)); assert.ok(!text.includes(libraryBearer!));
      }
    });
    await t.test("different identities require explicit unchanged-source choice", async () => {
      await setMain("main-owner"); await setLibrary("library-owner");
      const result = await reviewMigrationAccounts(lease, input);
      assert.equal(result.state, "choice_required");
      await assert.rejects(verifyMigrationAccount(lease, { ...input, expectedReview: result.review }), /choice_required/);
      const main = result.review.candidates[0]!;
      const chosen = { ...input, choice: { source: "main" as const, fingerprint: main.fingerprint! } };
      const selected = await reviewMigrationAccounts(lease, chosen);
      assert.equal(selected.state, "verify_existing");
      await assert.rejects(verifyMigrationAccount(lease, { ...chosen, expectedReview: result.review }), /selected OneCrew account changed/);
    });
    await t.test("signed out remains signed out and retains historical Library accounts", async () => {
      await setMain(null); await setLibrary(null);
      const result = await reviewMigrationAccounts(lease, input);
      assert.equal(result.state, "signed_out");
      assert.ok(result.preserved.knownLibraryAccountIds.includes("library-owner"));
      await assert.rejects(verifyMigrationAccount(lease, { ...input, expectedReview: result.review }), /signed_out/);
    });
    await t.test("inactive credentials request reauthorization and cannot verify", async () => {
      await setMain("inactive-owner", "revoked");
      const result = await reviewMigrationAccounts(lease, input);
      assert.equal(result.state, "reauthorize");
      await assert.rejects(verifyMigrationAccount(lease, { ...input, expectedReview: result.review }), /reauthorize/);
    });
    await t.test("explicit absent Library must really be absent", async () => {
      const absent = { ...input, library: { state: "absent" as const, sourceDirectory: join(directory, "never-created") } };
      const result = await reviewMigrationAccounts(lease, absent);
      assert.equal(result.review.candidates[1]!.state, "missing");
      await assert.rejects(reviewMigrationAccounts(lease, { ...absent, library: { state: "absent", sourceDirectory: sourceLibrary } }), /source_exists/);
    });
    await t.test("changed main ciphertext/metadata cannot reuse a prior review", async () => {
      await setMain("before-owner");
      const result = await reviewMigrationAccounts(lease, input);
      await setMain("after-owner");
      await assert.rejects(verifyMigrationAccount(lease, { ...input, expectedReview: result.review }), /selected OneCrew account changed/);
    });
    await t.test("changed Library active identity cannot reuse a prior review", async () => {
      await setLibrary("before-library");
      const result = await reviewMigrationAccounts(lease, input);
      await setLibrary("after-library");
      await assert.rejects(verifyMigrationAccount(lease, { ...input, expectedReview: result.review }), /selected OneCrew account changed/);
    });
    await t.test("modified SQLite data with unchanged receipt is rejected", async () => {
      using db = new DatabaseSync(sqlite.creator.path);
      db.exec("INSERT INTO creators(id,display_name,notes,created_at,updated_at) VALUES('changed','changed','',1,1)");
      await assert.rejects(reviewMigrationAccounts(lease, input), /selected OneCrew account changed/);
      db.exec("DELETE FROM creators WHERE id='changed'");
    });
    await t.test("modified staging journal is rejected", async () => {
      const journal = join(libraryReceipt.directory, "journal.json"), original = await readFile(journal);
      await writeMigrationJournal(libraryReceipt.directory, { ...libraryReceipt, activated: true });
      await assert.rejects(reviewMigrationAccounts(lease, input), /selected OneCrew account changed/);
      await writeFile(journal, original);
    });
    await t.test("cancelled verification rejects original cancellation before any request", async () => {
      const result = await reviewMigrationAccounts(lease, input);
      const controller = new AbortController(), reason = new Error("cancel account migration"); controller.abort(reason);
      await assert.rejects(verifyMigrationAccount(lease, { ...input, signal: controller.signal, expectedReview: result.review }), error => error === reason);
    });
    await t.test("malformed secret failures reveal no decrypted bearer", async () => {
      const bearer = await setMain("private-owner");
      using db = new DatabaseSync(sqlite.vault.path);
      db.prepare("UPDATE service_credentials SET metadata_json=?").run(JSON.stringify({ private: bearer }));
      sqlite = { ...sqlite, fingerprints: { ...sqlite.fingerprints, vault: fingerprintSqlite(sqlite.vault.path) } }; input.sqlite = sqlite;
      await writeMigrationJournal(sqlite.directory, sqlite);
      await assert.rejects(reviewMigrationAccounts(lease, input), error => error instanceof Error && error.message === "migration.accounts.invalid_main_credential" && !inspect(error).includes(bearer!));
    });
    await t.test("a validly shaped identity cannot reflect its decrypted bearer into review output", async () => {
      const bearer = await setMain("private-owner");
      { using db = new DatabaseSync(sqlite.vault.path);
        const row = db.prepare("SELECT metadata_json FROM service_credentials WHERE provider='creator_sync'").get()!;
        const metadata = JSON.parse(String(row.metadata_json));
        db.prepare("UPDATE service_credentials SET metadata_json=? WHERE provider='creator_sync'").run(JSON.stringify({ ...metadata, access_user_id: bearer })); }
      sqlite = { ...sqlite, fingerprints: { ...sqlite.fingerprints, vault: fingerprintSqlite(sqlite.vault.path) } }; input.sqlite = sqlite;
      await writeMigrationJournal(sqlite.directory, sqlite);
      await assert.rejects(reviewMigrationAccounts(lease, input), error => error instanceof Error && error.message === "migration.accounts.invalid_main_credential" && !inspect(error).includes(bearer!));
    });
  } finally { key.fill(0); await lease.close(); await rm(directory, { recursive: true }); }
});
