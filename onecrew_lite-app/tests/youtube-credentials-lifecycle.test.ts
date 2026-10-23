import assert from "node:assert/strict";
import { randomBytes, randomUUID } from "node:crypto";
import { copyFile, mkdir, mkdtemp } from "node:fs/promises";
import { join, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { beginYoutubeCredentialDelete, markYoutubeCredentialDeleteVerified, completeYoutubeCredentialDelete, pendingYoutubeCredentialDeletes } from "../src/platforms/browser/youtube-credentials-delete.ts";
import { validateYoutubeCredentialKey, youtubeCredentialConnectInput } from "../src/platforms/browser/youtube-credentials-validation.ts";
import { renameYoutubeCredentialInTransaction, recordYoutubeCredentialProbeInTransaction } from "../src/platforms/browser/youtube-credentials.ts";
import { readStorageSql } from "../src/storage/sql-resource.ts";
import { encryptOwnerVaultSecret } from "../src/platforms/browser/owner-vault-store.ts";
import { decryptVaultEnvelope } from "../src/storage/vault-reader.ts";

test("原schema90真实SQLite保留YouTube删除阶段、审计、幂等冲突与历史未完成操作保护", async () => {
  await mkdir(resolve(".scratch"), { recursive: true }); const directory = await mkdtemp(resolve(".scratch", "youtube-lifecycle-")), path = join(directory, "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path); const db = new DatabaseSync(path), id = randomUUID();
  const commit = <T>(work: () => T): T => { db.exec("BEGIN IMMEDIATE"); try { const result = work(); db.exec("COMMIT"); return result; } finally { if (db.isTransaction) db.exec("ROLLBACK"); } };
  try {
    db.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'youtube','service_credential','key',1,'available',1,1)").run(id);
    assert.throws(() => beginYoutubeCredentialDelete(db, "delete", id), /requires_transaction/);
    const first = commit(() => beginYoutubeCredentialDelete(db, "delete", id)); assert.equal(first.state, "acquired");
    assert.equal(db.prepare("SELECT active FROM account_refs WHERE id=?").get(id)?.active, 0);
    assert.deepEqual(pendingYoutubeCredentialDeletes(db), [{ key: "delete", account_ref_id: id }]);
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => completeYoutubeCredentialDelete(db, "delete", id), /not verified/); db.exec("ROLLBACK");
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => beginYoutubeCredentialDelete(db, "delete", randomUUID()), /another action or service credential/); db.exec("ROLLBACK");
    commit(() => markYoutubeCredentialDeleteVerified(db, "delete", id));
    assert.equal(commit(() => beginYoutubeCredentialDelete(db, "delete", id)).state, "remote_verified");
    const completed = commit(() => completeYoutubeCredentialDelete(db, "delete", id)); assert.equal(completed.replayed, false); assert.equal(completed.account_ref.availability, "unavailable");
    const replay = commit(() => beginYoutubeCredentialDelete(db, "delete", id)); assert.equal(replay.state, "completed"); if (replay.state === "completed") assert.equal(replay.result.replayed, true);
    assert.equal(db.prepare("SELECT COUNT(*) AS count FROM audit_events WHERE account_ref_id=?").get(id)?.count, 2); assert.deepEqual(pendingYoutubeCredentialDeletes(db), []);
    db.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES('historical','direct_message_send_v1','sha256:v1:old','remote_started','malformed',1,1)").run();
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => beginYoutubeCredentialDelete(db, "new-delete", id), /outreach_in_progress/); db.exec("ROLLBACK");
    assert.equal(db.prepare("SELECT state FROM idempotency_records WHERE key='historical'").get()?.state, "remote_started");
  } finally { db.close(); }
});
test("候选空Key在网络与Vault访问前明确失败，秘密输入合同拒绝未知字段", async () => {
  await assert.rejects(validateYoutubeCredentialKey("   ", new AbortController().signal, async () => { throw new Error("should_not_be_called"); }), /请输入 YouTube API Key/);
  assert.equal(youtubeCredentialConnectInput.safeParse({ label: "key", api_key: "value", confirmed: true }).success, false);
});
test("真实双SQLite改名保留稳定ID、加密Key、冷却与历史状态并拒绝重名", async () => {
  await mkdir(resolve(".scratch"), { recursive: true }); const directory = await mkdtemp(resolve(".scratch", "youtube-rename-")), path = join(directory, "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  const creator = new DatabaseSync(path), vault = new DatabaseSync(":memory:"), key = randomBytes(32), plaintext = randomBytes(64), encrypted = encryptOwnerVaultSecret(key, plaintext), id = randomUUID();
  try {
    vault.exec(readStorageSql("vault-schema.sql"));
    vault.prepare("INSERT INTO service_credentials(id,provider,label,metadata_json,encrypted_secret,disabled,cooldown_until,last_error,created_at,updated_at) VALUES(?,'youtube','before',?, ?,1,9999999999,'recorded failure','1','2')").run(id, JSON.stringify({ api: "youtube_data_api_v3", project: "project" }), encrypted);
    creator.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'youtube','service_credential','before',0,'unavailable',1,2)").run(id);
    assert.throws(() => renameYoutubeCredentialInTransaction(vault, creator, id, id, "after"), /requires_transaction/);
    vault.exec("BEGIN IMMEDIATE"); creator.exec("BEGIN IMMEDIATE");
    const result = renameYoutubeCredentialInTransaction(vault, creator, id, id, "after"); vault.exec("COMMIT"); creator.exec("COMMIT");
    assert.equal(result.id, id); assert.deepEqual(Buffer.from(result.encrypted_secret), encrypted); assert.equal(result.disabled, 1); assert.equal(BigInt(result.cooldown_until!), 9999999999n); assert.equal(result.last_error, "recorded failure");
    const clear = decryptVaultEnvelope(key, result.encrypted_secret); assert.deepEqual(clear, plaintext); clear.fill(0);
    assert.deepEqual(creator.prepare("SELECT label,active,availability FROM account_refs WHERE id=?").get(id), Object.assign(Object.create(null), { label: "after", active: 0, availability: "unavailable" }));
    vault.prepare("INSERT INTO service_credentials(id,provider,label,encrypted_secret,created_at,updated_at) VALUES(?,'youtube','occupied',?,'1','2')").run(randomUUID(), encrypted);
    vault.exec("BEGIN IMMEDIATE"); creator.exec("BEGIN IMMEDIATE"); assert.throws(() => renameYoutubeCredentialInTransaction(vault, creator, id, id, "occupied"), /already exists/); vault.exec("ROLLBACK"); creator.exec("ROLLBACK");
    assert.equal(vault.prepare("SELECT label FROM service_credentials WHERE id=?").get(id)?.label, "after");
    vault.exec("BEGIN IMMEDIATE"); const success = recordYoutubeCredentialProbeInTransaction(vault, id, null, 100); vault.exec("COMMIT");
    assert.equal(success.disabled, 1); assert.equal(BigInt(success.cooldown_until!), 9999999999n); assert.equal(success.last_error, "recorded failure");
    vault.exec("BEGIN IMMEDIATE"); const quota = recordYoutubeCredentialProbeInTransaction(vault, id, "quota_exceeded", 200); vault.exec("COMMIT");
    assert.equal(quota.disabled, 0); assert.equal(BigInt(quota.cooldown_until!), 1100n); assert.equal(quota.last_error, "YouTube API quota is exhausted");
    vault.exec("BEGIN IMMEDIATE"); const expired = recordYoutubeCredentialProbeInTransaction(vault, id, null, 1200); vault.exec("COMMIT"); assert.equal(expired.cooldown_until, null); assert.equal(expired.last_error, null);
  } finally { key.fill(0); plaintext.fill(0); creator.close(); vault.close(); }
});
