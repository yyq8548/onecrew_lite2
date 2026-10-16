import assert from "node:assert/strict";
import { createCipheriv, randomBytes, randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { mkdir, mkdtemp, open, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { fileURLToPath } from "node:url";
import { inspect } from "node:util";
import test from "node:test";
import { parseStoredJson } from "../src/contracts/stored-json.ts";
import { creatorSyncLimits, readStoredAccessCredential } from "../src/identity/legacy-access-credential.ts";
import { AccessInstallationVerifier } from "../src/identity/access-verification.ts";
import { createPrivateDirectory } from "../src/storage/private-backup.ts";

test("stored metadata rejects duplicate keys, rounded integers and invalid Unicode", () => {
  assert.deepEqual(parseStoredJson('{"revision":1}', true), { revision: 1 });
  for (const text of ['{"revision":1,"revision":2}', '{"revision":1.0}', '{"revision":-0}',
    '{"revision":9007199254740990.1}', '{"revision":9007199254740993}', '{"id":"\\ud800"}']) {
    assert.throws(() => parseStoredJson(text, true));
  }
});

test("real encrypted Vault credentials preserve local lifecycle without claiming remote verification", async () => {
  const scratch = fileURLToPath(new URL("../../.scratch/access-credential-tests/", import.meta.url));
  await mkdir(scratch, { recursive: true });
  const root = await mkdtemp(join(scratch, "credential-"));
  const directory = await createPrivateDirectory(join(root, "private"));
  const vaultPath = join(directory, "vault.sqlite3");
  const keyPath = join(directory, "master.key");
  const key = randomBytes(32);
  const reservation = await open(vaultPath, "wx", 0o600);
  await reservation.close();
  await writeFile(keyPath, key, { flag: "wx", mode: 0o600 });
  const database = new DatabaseSync(vaultPath);
  try {
    database.exec(readFileSync(new URL("../src/storage/vault-schema.sql", import.meta.url), "utf8"));
    database.exec("PRAGMA user_version=4");
    assert.deepEqual(await readStoredAccessCredential(vaultPath, keyPath), { state: "missing" });
    const credentialId = randomUUID();
    const id = randomUUID();
    const metadata = {
      installation_id: randomUUID(), credential_id: credentialId, access_user_id: "user-中文", access_device_id: `dev_${randomBytes(18).toString("base64url")}`,
      source_instance_id: randomUUID(), access_pairing_id: `pair_${randomBytes(18).toString("base64url")}`, client_platform: "windows", app_version: "0.7.1",
      status: "active", revision: 1, created_at: 1, updated_at: 2, last_seen_at: null, revoked_at: null,
      protocol: "reacher.creator_sync", protocol_version: 1, capabilities: ["creator_snapshots:write_chunked_quarantine"],
      accepted_bundle: { format: "reacher.creator_bundle", format_version: 3, profile: "portable_public", ...creatorSyncLimits },
    };
    const bearer = `${credentialId}_${randomBytes(32).toString("base64url")}`;
    const nonce = randomBytes(12);
    const cipher = createCipheriv("aes-256-gcm", key, nonce);
    const encrypted = Buffer.concat([nonce, cipher.update(JSON.stringify({ value: bearer })), cipher.final(), cipher.getAuthTag()]);
    database.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,created_at,updated_at) VALUES (?,'creator_sync','creator-sync',?,?,0,'1','2')")
      .run(id, JSON.stringify(metadata), encrypted);
    const state = await readStoredAccessCredential(vaultPath, keyPath);
    assert.equal(state.state, "active");
    if (state.state !== "active") throw new Error("Expected a locally active credential");
    assert.equal(state.credential.metadata.access_user_id, "user-中文");
    assert.equal(state.credential.exposeBearer(), bearer);
    assert.equal(inspect(state).includes(bearer), false);
    assert.throws(() => JSON.stringify(state), /cannot be serialized/);
    const cancelled = new AbortController();
    const reason = new Error("Identity verification cancelled before networking");
    cancelled.abort(reason);
    await assert.rejects(new AccessInstallationVerifier().verify(state.credential, cancelled.signal), error => error === reason);
    state.credential.dispose();
    assert.throws(() => state.credential.exposeBearer(), /disposed/);
    database.prepare("UPDATE service_credentials SET metadata_json=? WHERE id=?").run(JSON.stringify({ ...metadata, protocol_version: 2 }), id);
    await assert.rejects(readStoredAccessCredential(vaultPath, keyPath));
    database.prepare("UPDATE service_credentials SET metadata_json=? WHERE id=?").run(JSON.stringify({ ...metadata, accepted_bundle: { ...metadata.accepted_bundle, max_chunk_bytes: 512 } }), id);
    await assert.rejects(readStoredAccessCredential(vaultPath, keyPath));
    database.prepare("UPDATE service_credentials SET metadata_json=?,disabled=1 WHERE id=?")
      .run(JSON.stringify({ ...metadata, status: "revoked", revoked_at: 3 }), id);
    assert.equal((await readStoredAccessCredential(vaultPath, keyPath)).state, "inactive");
    database.prepare("UPDATE service_credentials SET disabled=0 WHERE id=?").run(id);
    await assert.rejects(readStoredAccessCredential(vaultPath, keyPath), /lifecycle/);
  } finally {
    key.fill(0);
    database.close();
    await rm(root, { recursive: true });
  }
});
