import assert from "node:assert/strict";
import { createCipheriv, randomBytes, randomUUID } from "node:crypto";
import { createServer } from "node:net";
import { once } from "node:events";
import { readFileSync } from "node:fs";
import { mkdtemp, open, readFile, readdir, rm, unlink, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { openDatabase } from "../../onecrew_lite-library/dist/database.js";
import { createCredentialStore } from "../../onecrew_lite-library/dist/modules/access/store.js";
import { AccountDatabases, type PreparedCreatorAdoption } from "../src/identity/account-databases.ts";
import type { VerifiedMigrationCredential } from "../src/identity/access-verification.ts";
import { prepareActivationCandidate, prepareActivationPublication, readActivationCandidate, readActiveGeneration, commitActivation } from "../src/migration/activation.ts";
import { reviewMigrationAccounts } from "../src/migration/account-review.ts";
import { prepareSqliteRecovery } from "../src/migration/sqlite-recovery.ts";
import { prepareSqliteStaging } from "../src/migration/sqlite-staging.ts";
import { prepareLibraryRecovery, prepareLibraryStaging } from "../src/migration/library-recovery.ts";
import { LegacyQuiescenceLease } from "../src/migration/legacy-quiescence.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { loadOrCreateServiceProfile } from "../src/service/profile.ts";
import { initializeNewCreatorDatabase } from "../src/storage/creator-schema.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("real SQLite/PGlite activation candidates preserve sources and require genuine authority before cutover", { timeout: 900_000 }, async t => {
  const reservation = createServer();
  reservation.listen(0, "127.0.0.1");
  await once(reservation, "listening");
  const address = reservation.address();
  assert.ok(address && typeof address !== "string");
  const originalPort = address.port;
  await new Promise<void>((resolve, reject) => reservation.close(error => error ? reject(error) : resolve()));
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/activation-", import.meta.url)));
  const old = await createPrivateDirectory(join(directory, "旧实例")), owner = await createPrivateDirectory(join(directory, "统一实例"));
  const creator = join(old, "creator.sqlite"), vault = join(old, "vault.sqlite"), masterKey = join(old, "masterkey");
  const key = randomBytes(32), userId = "local-format-owner", credentialId = randomUUID(), bearer = `${credentialId}_${randomBytes(32).toString("base64url")}`;
  const metadata = { installation_id: randomUUID(), credential_id: credentialId, access_user_id: userId,
    access_device_id: `dev_${randomBytes(18).toString("base64url")}`, access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`,
    source_instance_id: randomUUID(), client_platform: "windows" as const, app_version: "0.7.1", status: "active" as const, revision: 1,
    created_at: 1, updated_at: 2, last_seen_at: null, revoked_at: null, protocol: "reacher.creator_sync", protocol_version: 1,
    capabilities: ["creator_snapshots:write_chunked_quarantine"], accepted_bundle: { format: "reacher.creator_bundle", format_version: 3,
      profile: "portable_public", max_snapshot_bytes: 33554432, max_chunk_bytes: 1048576, max_chunks: 128,
      max_namespace_staged_bytes: 104857600, max_access_user_staged_bytes: 419430400 } };
  await initializeNewCreatorDatabase(creator);
  { using db = new DatabaseSync(creator); db.exec("INSERT INTO creators(id,display_name,notes,created_at,updated_at) VALUES('history','历史作者','保留',1,2)"); }
  { await using file = await open(vault, "wx", 0o600); }
  { using db = new DatabaseSync(vault);
    db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8")); db.exec("PRAGMA user_version=4");
    const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce), envelope = Buffer.concat([nonce, cipher.update(JSON.stringify({ value: bearer })), cipher.final(), cipher.getAuthTag()]);
    db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,created_at,updated_at) VALUES(?,'creator_sync','creator-sync',?,?,'1','2')").run(randomUUID(), JSON.stringify(metadata), envelope); }
  await writeFile(masterKey, key, { flag: "wx", mode: 0o600 });
  await writeFile(join(old, ".onecrew_lite-service-owner.lock"), "", { flag: "wx", mode: 0o600 });
  let lease = await acquireInstanceLock(owner);
  try {
    await loadOrCreateServiceProfile(lease, originalPort);
    const sqlite = await prepareSqliteStaging(lease, { recovery: await prepareSqliteRecovery(lease, { creator, vault }), sources: { creator, vault, masterKey } });
    const libraryDirectory = await createPrivateDirectory(join(directory, "旧表格"));
    const tables = await createEmbeddedTables(libraryDirectory, "http://127.0.0.1:45987");
    const tenant = await tables.tenants.resolve({ id: userId, email: "owner@example.com" });
    await tables.tenants.resolve({ id: "other-history", email: "history@example.com" });
    await tables.close();
    await writeFile(join(libraryDirectory, "server.lock"), "", { flag: "wx", mode: 0o600 });
    { const db = await openDatabase(libraryDirectory);
      try { const { protocol: _protocol, protocol_version: _version, capabilities: _capabilities, accepted_bundle: _bundle, ...installation } = metadata;
        await createCredentialStore(db.db).save({ bearer, installation }); }
      finally { await db.close(); } }
    const library = { state: "staged" as const, receipt: await prepareLibraryStaging(lease, await prepareLibraryRecovery(lease, libraryDirectory)) };
    const sources = { sqlite, library }, signal = new AbortController().signal;
    const reviewed = await reviewMigrationAccounts(lease, { ...sources, signal });
    const input = { sources, legacyServiceDirectory: old, sourcePaths: { creator, vault, masterKey, libraryDirectory }, expectedReview: reviewed.review,
      owner: { accountId: userId, tenantId: tenant.tenantId, baseId: tenant.baseId }, expectedCurrent: null, signal };
    const original = { creator: await readFile(creator), vault: await readFile(vault), masterKey: await readFile(masterKey) };
    const candidate = await prepareActivationCandidate(lease, input);
    await t.test("candidate copies both real engines and preserves the original port and data", async () => {
      assert.equal(candidate.activated, false); assert.equal(await readActiveGeneration(lease), null);
      assert.equal((await readActivationCandidate(lease, candidate.id)).candidate.id, candidate.id);
      assert.deepEqual(await readFile(creator), original.creator); assert.deepEqual(await readFile(vault), original.vault); assert.deepEqual(await readFile(masterKey), original.masterKey);
      assert.equal((await loadOrCreateServiceProfile(lease)).port, originalPort);
      using db = new DatabaseSync(candidate.creator, { readOnly: true }); assert.equal(db.prepare("SELECT display_name FROM creators WHERE id='history'").get()!.display_name, "历史作者");
    });
    await t.test("restart reopens an unactivated candidate without inventing a current generation", async () => {
      await lease.close(); lease = await acquireInstanceLock(owner);
      assert.equal((await readActivationCandidate(lease, candidate.id)).candidate.libraryFingerprint, candidate.libraryFingerprint);
      assert.equal(await readActiveGeneration(lease), null);
    });
    await t.test("real legacy locks cannot substitute for an online-issued Access proof", async () => {
      await using quiescence = await LegacyQuiescenceLease.acquire(lease, { serviceDirectory: old, sources: input.sourcePaths });
      await assert.rejects(commitActivation(lease, { candidate, proof: {} as VerifiedMigrationCredential, quiescence, expectedCurrent: null, signal }), /verified_credential_required/);
      assert.equal(await readActiveGeneration(lease), null);
      assert.deepEqual(await readFile(vault), original.vault);
      await assert.rejects(AccountDatabases.adoptPrepared(lease, {} as VerifiedMigrationCredential, {} as PreparedCreatorAdoption), /Verified migration credential required/);
      assert.ok(!(await readdir(join(candidate.creator, ".."))).includes("account.json"));
    });
    await t.test("publication validates actual candidate health before proof and detects later file changes", async () => {
      await using quiescence = await LegacyQuiescenceLease.acquire(lease, { serviceDirectory: old, sources: input.sourcePaths });
      const prepared = await prepareActivationPublication(lease, { candidate, quiescence, signal,
        webAssetsDirectory: fileURLToPath(new URL("../../onecrew_lite-rust/apps/web/dist", import.meta.url)) });
      assert.equal(prepared.health.resource_identity.profile_port, originalPort);
      assert.equal(prepared.health.resource_identity.kind, "source_development");
      assert.equal(prepared.assertPrepared(lease, quiescence).candidate.id, candidate.id);
      assert.throws(() => JSON.stringify(prepared), /cannot be serialized/);
      assert.equal(await readActiveGeneration(lease), null);
      const extra = join(candidate.directory, "unreviewed-file");
      await writeFile(extra, "changed", { flag: "wx", mode: 0o600 });
      assert.throws(() => prepared.assertPrepared(lease, quiescence), /publication_data_changed/);
      await unlink(extra);
    });
    await t.test("unknown Creator or Library tenant ownership blocks before creating another generation", async () => {
      const before = await readdir(owner);
      await assert.rejects(prepareActivationCandidate(lease, { ...input, owner: { ...input.owner, accountId: "other-owner" } }), /creator_owner_mapping_required/);
      await assert.rejects(prepareActivationCandidate(lease, { ...input, owner: { ...input.owner, baseId: "unmapped-base" } }), /library_owner_mapping_required/);
      assert.deepEqual(await readdir(owner), before);
    });
    await t.test("stale current-pointer CAS rejects before writing a generation", async () => {
      const before = await readdir(owner);
      await assert.rejects(prepareActivationCandidate(lease, { ...input, expectedCurrent: "0".repeat(64) }), /current_pointer_changed/);
      assert.deepEqual(await readdir(owner), before);
    });
    await t.test("key corruption and failed journal state cannot pass candidate readback", async () => {
      const bytes = await readFile(candidate.masterKey);
      await writeFile(candidate.masterKey, randomBytes(32));
      await assert.rejects(readActivationCandidate(lease, candidate.id), /candidate_key_changed/);
      await writeFile(candidate.masterKey, bytes);
      const journal = join(candidate.directory, "journal.json"), saved = await readFile(journal);
      await writeFile(journal, JSON.stringify({ id: candidate.id, state: "failed", activated: false }));
      await assert.rejects(readActivationCandidate(lease, candidate.id), /candidate_journal_changed/);
      await writeFile(journal, saved);
    });
    await t.test("a path-traversal current pointer is rejected without selecting arbitrary data", async () => {
      const path = join(owner, "current.json");
      await writeFile(path, JSON.stringify({ format: 1, generationId: "../other", manifestSha256: "0".repeat(64) }), { flag: "wx", mode: 0o600 });
      await assert.rejects(readActiveGeneration(lease));
      await unlink(path);
      assert.equal(await readActiveGeneration(lease), null);
    });
    await t.test("private current metadata rejects duplicate fields including identical values", async () => {
      const path = join(owner, "current.json");
      await writeFile(path, '{"format":1,"format":1,"generationId":"' + candidate.id + '","manifestSha256":"' + "0".repeat(64) + '"}', { flag: "wx", mode: 0o600 });
      await assert.rejects(readActiveGeneration(lease));
      await unlink(path);
      assert.equal(await readActiveGeneration(lease), null);
    });
    await t.test("cancellation preserves its reason and creates no target", async () => {
      const before = await readdir(owner), controller = new AbortController(), reason = new Error("cancel isolated activation"); controller.abort(reason);
      await assert.rejects(prepareActivationCandidate(lease, { ...input, signal: controller.signal }), error => error === reason);
      assert.deepEqual(await readdir(owner), before);
    });
    await t.test("changed original SQLite data invalidates the recorded migration source", async () => {
      { using db = new DatabaseSync(creator); db.exec("INSERT INTO creators(id,display_name,notes,created_at,updated_at) VALUES('newer','后来数据','不得丢弃',3,3)"); }
      await assert.rejects(prepareActivationCandidate(lease, input), /original_source_changed/);
      using staged = new DatabaseSync(candidate.creator, { readOnly: true }); assert.equal(staged.prepare("SELECT COUNT(*) AS count FROM creators").get()!.count, 1);
      using source = new DatabaseSync(creator, { readOnly: true }); assert.equal(source.prepare("SELECT COUNT(*) AS count FROM creators").get()!.count, 2);
      assert.equal(await readActiveGeneration(lease), null);
    });
  } finally { key.fill(0); await lease.close(); await rm(directory, { recursive: true }); }
});
