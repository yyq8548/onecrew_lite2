import assert from "node:assert/strict";
import { createCipheriv, randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdtemp, open, readFile, rm, writeFile } from "node:fs/promises";
import { request } from "node:http";
import { connect } from "node:net";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import { createEmbeddedTables } from "onecrew_lite_tables/embedded";
import { AccessLifecycle } from "../src/identity/access-lifecycle.ts";
import { UnifiedAccountService } from "../src/identity/account-service.ts";
import { AccessVaultStore, PreparedAccessVaultWrite } from "../src/identity/access-vault-store.ts";
import { readStoredAccessCredential } from "../src/identity/legacy-access-credential.ts";
import { acquireInstanceLock } from "../src/service/instance-lock.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";
import type { VerifiedMigrationCredential } from "../src/identity/access-verification.ts";
import { classifyAccessHttpFailure } from "../src/identity/access-verification.ts";

test("Access HTTP failure classification keeps authoritative 401/installation404 distinct from protocol403", () => {
  assert.equal(classifyAccessHttpFailure(401, true), "non_active");
  assert.equal(classifyAccessHttpFailure(401, false), "non_active");
  assert.equal(classifyAccessHttpFailure(404, true), "non_active");
  for (const status of [400, 403, 404, 409, 422, 429]) assert.equal(classifyAccessHttpFailure(status, false), "protocol");
  for (const status of [500, 502, 503]) assert.equal(classifyAccessHttpFailure(status, true), "unavailable");
});

async function localGet(url: URL) {
  return new Promise<number>((resolve, reject) => {
    const outgoing = request(url, { method: "GET", headers: { connection: "close" } }, response => { response.resume(); response.on("end", () => resolve(response.statusCode!)); });
    outgoing.on("error", reject); outgoing.end();
  });
}

test("Access single-owner lifecycle uses real loopback and private Vault without enrolling a remote identity", { timeout: 300_000 }, async t => {
  const directory = await mkdtemp(fileURLToPath(new URL("../../.scratch/access-lifecycle-", import.meta.url)));
  const owner = await createPrivateDirectory(join(directory, "owner"));
  const vault = join(owner, "vault.sqlite"), masterKey = join(owner, "masterkey"), key = randomBytes(32);
  { await using file = await open(vault, "wx", 0o600); }
  { using db = new DatabaseSync(vault); db.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8")); db.exec("PRAGMA user_version=4"); }
  await writeFile(masterKey, key, { flag: "wx", mode: 0o600 });
  const lease = await acquireInstanceLock(owner);
  const tablesDirectory = await createPrivateDirectory(join(owner, "tables"));
  const tables = await createEmbeddedTables(tablesDirectory, "http://127.0.0.1:45987");
  const accounts = new UnifiedAccountService(vault, masterKey, tables.tenants);
  const options = { vaultPath: vault, masterKeyPath: masterKey, appVersion: "0.7.1", platform: "windows" as const, deviceLabel: "\ufeff 小凡的电脑 🖥️ \u3000" };
  const lifecycle = new AccessLifecycle(lease, options, accounts);
  try {
    await t.test("signed out status does not create an account or credential", async () => {
      assert.deepEqual(await lifecycle.status(), { state: "signed_out", user: null });
      assert.deepEqual(await readStoredAccessCredential(vault, masterKey), { state: "missing" });
    });
    await t.test("begin creates exact PKCE URL and cancel closes the native listener", async () => {
      const begun = await lifecycle.begin(), url = new URL(begun.authorizeUrl);
      assert.equal(url.origin, "https://access.onecrewai.io"); assert.equal(url.pathname, "/pairing/authorize");
      assert.deepEqual([...url.searchParams.keys()], ["app_version", "callback_port", "code_challenge", "code_challenge_method", "label", "platform", "state"]);
      assert.equal(url.searchParams.get("label"), "小凡的电脑 🖥️"); assert.equal(url.searchParams.get("code_challenge_method"), "S256");
      assert.match(url.searchParams.get("state")!, /^[A-Za-z0-9_-]{43}$/u); assert.match(url.searchParams.get("code_challenge")!, /^[A-Za-z0-9_-]{43}$/u);
      assert.notEqual(url.searchParams.get("state"), url.searchParams.get("code_challenge"));
      assert.deepEqual(await lifecycle.reopen(), begun);
      assert.deepEqual(lifecycle.pairingEvidence(), { flowId: begun.flowId, callbackReceived: false });
      assert.equal(lifecycle.completedPairingContext(begun.flowId), undefined);
      assert.equal((await lifecycle.status()).state, "authorizing");
      await assert.rejects(lifecycle.begin(), /already_pending/);
      assert.ok(!inspect(lifecycle).includes(url.searchParams.get("state")!)); assert.throws(() => JSON.stringify(lifecycle), /cannot be serialized/);
      await lifecycle.cancel();
      assert.equal((await lifecycle.status()).state, "signed_out");
      await assert.rejects(lifecycle.reopen(), /pairing_not_pending/);
      await assert.rejects(new Promise<void>((resolve, reject) => { const socket = connect(Number(url.searchParams.get("callback_port")), "127.0.0.1", () => { socket.destroy(); resolve(); }); socket.on("error", reject); }), /ECONNREFUSED/);
    });
    await t.test("mismatched state is rejected locally without consuming company registration", async () => {
      const begun = await lifecycle.begin(), authorize = new URL(begun.authorizeUrl);
      const callback = new URL(`http://127.0.0.1:${authorize.searchParams.get("callback_port")}/creator-sync/pairing-callback`);
      callback.search = new URLSearchParams({ code: `reacher_pairing_v1.${randomBytes(32).toString("base64url")}`, state: randomBytes(32).toString("base64url") }).toString();
      assert.equal(await localGet(callback), 400);
      await lifecycle.cancel();
      assert.deepEqual(await readStoredAccessCredential(vault, masterKey), { state: "missing" });
    });
    await t.test("duplicate callback fields and request bodies are refused by real HTTP", async () => {
      for (const suffix of ["&state=duplicate", "&unknown=value"]) {
        const begun = await lifecycle.begin(), authorize = new URL(begun.authorizeUrl);
        const callback = new URL(`http://127.0.0.1:${authorize.searchParams.get("callback_port")}/creator-sync/pairing-callback?code=reacher_pairing_v1.${randomBytes(32).toString("base64url")}&state=${authorize.searchParams.get("state")}${suffix}`);
        assert.equal(await localGet(callback), 400); await lifecycle.cancel();
      }
      const begun = await lifecycle.begin(), authorize = new URL(begun.authorizeUrl);
      const status = await new Promise<number>((resolve, reject) => {
        const outgoing = request({ host: "127.0.0.1", port: Number(authorize.searchParams.get("callback_port")), method: "POST", path: "/creator-sync/pairing-callback", headers: { "content-length": "1" } }, response => { response.resume(); response.on("end", () => resolve(response.statusCode!)); });
        outgoing.on("error", reject); outgoing.end("x");
      });
      assert.equal(status, 400); await lifecycle.cancel();
    });
    await t.test("invalid native labels and versions fail before opening a callback port", async () => {
      for (const changed of [{ deviceLabel: "x\u202ey" }, { deviceLabel: "😀".repeat(41) }, { appVersion: "0..7.1" }]) {
        const invalid = new AccessLifecycle(lease, { ...options, ...changed }, accounts);
        await assert.rejects(invalid.begin(), /invalid/); await invalid.close();
      }
    });
    await t.test("logout validates authentic encrypted legacy secret and preserves unrelated credentials", async () => {
      const credentialId = randomUUID(), bearer = `${credentialId}_${randomBytes(32).toString("base64url")}`;
      const metadata = { installation_id: randomUUID(), credential_id: credentialId, access_user_id: "local-format-only",
        access_device_id: `dev_${randomBytes(18).toString("base64url")}`, access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`,
        source_instance_id: randomUUID(), client_platform: "windows", app_version: "0.7.1", status: "active", revision: 1,
        created_at: 1, updated_at: 2, last_seen_at: null, revoked_at: null, protocol: "reacher.creator_sync", protocol_version: 1,
        capabilities: ["creator_snapshots:write_chunked_quarantine"], accepted_bundle: { format: "reacher.creator_bundle", format_version: 3,
          profile: "portable_public", max_snapshot_bytes: 33554432, max_chunk_bytes: 1048576, max_chunks: 128,
          max_namespace_staged_bytes: 104857600, max_access_user_staged_bytes: 419430400 } };
      const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce);
      const encrypted = Buffer.concat([nonce, cipher.update(JSON.stringify({ value: bearer })), cipher.final(), cipher.getAuthTag()]);
      { using db = new DatabaseSync(vault);
        db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,created_at,updated_at) VALUES(?,'creator_sync','creator-sync',?,?,'1','2')").run(randomUUID(), JSON.stringify(metadata), encrypted);
        db.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,created_at,updated_at) VALUES('history','email_resend_api_key','preserved','{}',?,'1','2')").run(randomBytes(40)); }
      await assert.rejects(lifecycle.begin(), /admission_validation_required/);
      const store = new AccessVaultStore(lease, vault, masterKey);
      const initial = await store.load();
      assert.equal(initial.state, "active");
      if (initial.state !== "active") throw new Error("Expected local encrypted credential");
      const oldFingerprint = initial.credential.fingerprint; initial.credential.dispose();
      { using db = new DatabaseSync(vault); db.prepare("UPDATE service_credentials SET metadata_json=? WHERE provider='creator_sync'").run(JSON.stringify({ ...metadata, revision: 2 })); }
      const beforeStaleCleanup = await readFile(vault);
      await assert.rejects(store.clear(oldFingerprint), /selected OneCrew account changed/);
      assert.deepEqual(await readFile(vault), beforeStaleCleanup);
      const current = await store.load();
      assert.equal(current.state, "active");
      if (current.state !== "active") throw new Error("Changed credential must be preserved");
      const fingerprint = current.credential.fingerprint; current.credential.dispose();
      await store.clear(fingerprint);
      await lifecycle.logout();
      assert.deepEqual(await lifecycle.status(), { state: "signed_out", user: null });
      assert.deepEqual(await readStoredAccessCredential(vault, masterKey), { state: "missing" });
      { using db = new DatabaseSync(vault); assert.equal(db.prepare("SELECT count(*) AS count FROM service_credentials WHERE id='history'").get()!.count, 1); }
      assert.ok(!(await readFile(vault)).includes(Buffer.from(bearer)));
    });
    await t.test("private store rejects an unissued proof without writing the Vault", async () => {
      const store = new AccessVaultStore(lease, vault, masterKey), before = await readFile(vault);
      await assert.rejects(store.save({} as VerifiedMigrationCredential, store.fingerprint()), /verified_credential_required/);
      await assert.rejects(store.refresh({} as VerifiedMigrationCredential), /verified_credential_required/);
      assert.deepEqual(await readFile(vault), before);
    });
    await t.test("prepared migration Vault writes require genuine proof and unchanged one-use storage authority", async () => {
      const store = new AccessVaultStore(lease, vault, masterKey), before = await readFile(vault);
      const prepared = await store.prepareMigrationSave(store.fingerprint());
      assert.throws(() => JSON.stringify(prepared), /cannot be serialized/);
      assert.throws(() => Reflect.construct(PreparedAccessVaultWrite, [Symbol(), store]), /unissued/);
      await assert.rejects(store.savePreparedMigration({} as VerifiedMigrationCredential, prepared), /verified_credential_required/);
      assert.deepEqual(await readFile(vault), before);
      prepared.consume(store);
      assert.throws(() => prepared.consume(store), /prepared_write_invalid/);
      const stale = await store.prepareMigrationSave(store.fingerprint());
      { using db = new DatabaseSync(vault); db.prepare("UPDATE service_credentials SET updated_at='3' WHERE id='history'").run(); }
      assert.throws(() => stale.consume(store), /selected OneCrew account changed/);
    });
  } finally { await lifecycle.close(); accounts.invalidate(); await tables.close(); key.fill(0); await lease.close(); await rm(directory, { recursive: true }); }
});
