import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { copyFile, mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import test from "node:test";
import { OperatorDisconnectRepository } from "../src/platforms/browser/operator-disconnect-repository.ts";

test("原schema90真实断开四阶段保留availability直到验证、原历史、幂等及失败边界", async () => {
  await mkdir(resolve(".scratch"), { recursive: true }); const directory = await mkdtemp(resolve(".scratch", "operator-disconnect-")), path = join(directory, "creator.sqlite");
  await copyFile(new URL("./fixtures/discovery-identity/baseline.sqlite", import.meta.url), path);
  const db = new DatabaseSync(path), repository = new OperatorDisconnectRepository(db), id = randomUUID(), other = randomUUID();
  const commit = <T>(operation: () => T) => { db.exec("BEGIN IMMEDIATE"); try { const value = operation(); db.exec("COMMIT"); return value; } finally { if (db.isTransaction) db.exec("ROLLBACK"); } };
  try {
    for (const value of [id, other]) db.prepare("INSERT INTO account_refs(id,platform,kind,label,active,availability,created_at,updated_at) VALUES(?,'x','profile','@operator',1,'available',1,1)").run(value);
    assert.throws(() => repository.begin("disconnect", id), /requires_transaction/);
    const first = commit(() => repository.begin("disconnect", id)); assert.equal(first.state, "acquired");
    assert.equal(repository.account(id).active, false); assert.equal(repository.account(id).availability, "available"); assert.equal(repository.account(other).active, true);
    assert.deepEqual(repository.pending(), [{ key: "disconnect", account_ref_id: id }]);
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => repository.begin("disconnect", other), /different disconnect intent/); db.exec("ROLLBACK");
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => repository.complete("disconnect", id), /not verified/); db.exec("ROLLBACK");
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => repository.markVerified("disconnect", id), /invalid state/); db.exec("ROLLBACK");
    commit(() => repository.markStarted("disconnect", id)); assert.equal(commit(() => repository.begin("disconnect", id)).state, "remote_started");
    commit(() => repository.markStarted("disconnect", id)); commit(() => repository.markVerified("disconnect", id));
    assert.equal(commit(() => repository.begin("disconnect", id)).state, "remote_verified");
    const completed = commit(() => repository.complete("disconnect", id)); assert.equal(completed.replayed, false); assert.equal(completed.account_ref.active, false); assert.equal(completed.account_ref.availability, "unavailable");
    assert.equal(commit(() => repository.complete("disconnect", id)).replayed, true); assert.equal(commit(() => repository.begin("disconnect", id)).state, "completed");
    assert.deepEqual(repository.pending(), []); assert.equal(db.prepare("SELECT COUNT(*) AS n FROM audit_events WHERE account_ref_id=?").get(id)?.n, 2);
    commit(() => repository.begin("failed", other)); commit(() => repository.markStarted("failed", other)); commit(() => repository.markFailed("failed", other)); commit(() => repository.markFailed("failed", other));
    assert.deepEqual(repository.pending(), []); assert.equal(repository.account(other).availability, "available");
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => repository.begin("failed", other), /previously failed/); db.exec("ROLLBACK");
    assert.equal(db.prepare("SELECT COUNT(*) AS n FROM audit_events WHERE account_ref_id=?").get(other)?.n, 2);
    db.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES('historical','direct_message_send_v1','sha256:v1:old','remote_started','malformed',1,1)").run();
    db.exec("BEGIN IMMEDIATE"); assert.throws(() => repository.begin("blocked", other), /outreach_in_progress/); db.exec("ROLLBACK");
    assert.equal(db.prepare("SELECT state FROM idempotency_records WHERE key='historical'").get()?.state, "remote_started");
  } finally { db.close(); }
});
