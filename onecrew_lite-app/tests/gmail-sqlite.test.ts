import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { isMainThread, parentPort, Worker, workerData } from "node:worker_threads";
import { GmailBulkRepository } from "../src/mail/gmail-bulk-repository.ts";
import { GmailBulkWorker, GmailBulkWorkerNotReadyError } from "../src/mail/gmail-bulk-worker.ts";
import { AccountContextService } from "../src/identity/account-context.ts";
import { bulkIdempotencyKey, recordBulkResult } from "../src/mail/gmail-bulk.ts";
import type { BulkMailbox, PrepareBulkInput } from "../src/mail/gmail-bulk.ts";

const mailbox: BulkMailbox = { id: "mailbox", provider: "gmail", email_address: "sender@example.com", available: true, capabilities: ["send", "sent_readback"] };
function request(email = "recipient@example.com"): PrepareBulkInput { return { mailbox_ref_id: "mailbox", subject: "你好", body: "正文", recipients: [{ email, source: "Creator" }] }; }
function connect(path: string): DatabaseSync {
  const db = new DatabaseSync(path);
  db.exec("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000");
  return db;
}

function fixture(): { db: DatabaseSync; path: string; repository: GmailBulkRepository } {
  const root = new URL("../../", import.meta.url);
  const scratch = new URL(".scratch/gmail-repository/", root);
  mkdirSync(scratch, { recursive: true });
  const directory = mkdtempSync(resolve(fileURLToPath(scratch), "case-"));
  const path = resolve(directory, "creator.sqlite3");
  const db = connect(path);
  for (const name of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]) db.exec(readFileSync(new URL(`../src/storage/${name}`, import.meta.url), "utf8"));
  db.exec("PRAGMA user_version=90; PRAGMA journal_mode=WAL");
  db.prepare("INSERT INTO mailbox_refs VALUES (?,'gmail','Gmail',?,NULL,?,1,NULL,0,0)").run(mailbox.id, mailbox.email_address, JSON.stringify(mailbox.capabilities));
  return { db, path, repository: new GmailBulkRepository(db) };
}

if (!isMainThread) {
  const data = workerData as { path: string; gate: SharedArrayBuffer };
  const db = connect(data.path);
  try {
    const repository = new GmailBulkRepository(db);
    const gate = new Int32Array(data.gate);
    Atomics.add(gate, 0, 1);
    Atomics.notify(gate, 0);
    Atomics.wait(gate, 1, 0);
    const claimed = repository.claim();
    parentPort!.postMessage(claimed?.item.id ?? null);
  } finally {
    db.close();
  }
} else {
  test("原始 Gmail 子schema：草稿、事务领取、永久标记与发送结果持久保存", () => {
    const { db, path, repository } = fixture();
    try {
      const batch = repository.create(request());
      assert.equal(repository.claim(), null);
      repository.control(batch.id, "confirm");
      const claim = repository.claim()!;
      assert.equal(claim.item.state, "attempting");
      const key = bulkIdempotencyKey(claim.item);
      db.exec("BEGIN");
      assert.throws(() => repository.reserveAttempt(key), /independent durable commit/);
      db.exec("ROLLBACK");
      assert.equal(repository.reserveAttempt(key), true);
      const other = connect(path);
      try {
        const second = new GmailBulkRepository(other);
        assert.equal(second.reserveAttempt(key), false);
        assert.equal(second.claim(), null);
      } finally { other.close(); }
      repository.finish(batch.id, recordBulkResult(claim.item, { kind: "success", provider_message_id: "gmail-id" }));
      assert.equal(repository.list()[0]?.state, "completed");
      assert.equal(repository.list()[0]?.items[0]?.provider_message_id, "gmail-id");
      assert.throws(() => repository.control(batch.id, "resume"), /invalid/);
    } finally { db.close(); }
  });

  test("两个真实连接同步竞争只领取同一待发送项一次", async () => {
    const { db, path, repository } = fixture();
    try {
      const batch = repository.create(request());
      repository.control(batch.id, "confirm");
      const gate = new SharedArrayBuffer(8);
      const view = new Int32Array(gate);
      const workers = [1, 2].map(() => new Worker(new URL(import.meta.url), { workerData: { path, gate } }));
      const results = workers.map(worker => new Promise<string | null>((resolve, reject) => {
        worker.once("message", resolve);
        worker.once("error", reject);
      }));
      const exits = workers.map(worker => new Promise<void>((resolve, reject) => {
        worker.once("exit", code => code === 0 ? resolve() : reject(new Error(`claim worker exited ${code}`)));
      }));
      while (Atomics.load(view, 0) !== 2) {
        const ready = Atomics.load(view, 0);
        if (ready !== 2) await Atomics.waitAsync(view, 0, ready, 5000).value;
      }
      Atomics.store(view, 1, 1);
      Atomics.notify(view, 1, 2);
      const claimed = await Promise.all(results);
      await Promise.all(exits);
      assert.deepEqual(claimed.filter(id => id !== null), [batch.items[0]!.id]);
    } finally { db.close(); }
  });

  test("领取后关闭重开按崩溃恢复，unknown 不重发，剩余 pending 显式恢复", () => {
    const first = fixture();
    const request_ = request();
    request_.recipients = [...request_.recipients, { email: "remaining@example.com", source: "Discovery" }];
    const batch = first.repository.create(request_);
    first.repository.control(batch.id, "confirm");
    const claim = first.repository.claim()!;
    assert.equal(first.repository.reserveAttempt(bulkIdempotencyKey(claim.item)), true);
    first.db.close();
    const db = connect(first.path);
    try {
      const repository = new GmailBulkRepository(db);
      repository.interrupt();
      const restored = repository.list()[0]!;
      assert.equal(restored.state, "paused");
      assert.deepEqual(restored.items.map(item => item.state), ["unknown", "pending"]);
      assert.equal(repository.claim(), null);
      assert.equal(repository.reserveAttempt(bulkIdempotencyKey(claim.item)), false);
      repository.control(batch.id, "resume");
      assert.equal(repository.claim()?.item.recipient, "remaining@example.com");
      assert.equal(repository.claim(), null);
      // 原仓储规则禁止晚到结果覆盖中断后 unknown。
      assert.throws(() => repository.finish(batch.id, recordBulkResult(claim.item, { kind: "success", provider_message_id: "late" })), /no active attempt/);
      assert.equal(repository.list()[0]?.items[0]?.state, "unknown");
    } finally { db.close(); }
  });

  test("两个预先存在的草稿仍在领取时查重，后建草稿显示历史尝试提示", () => {
    const { db, repository } = fixture();
    try {
      const first = repository.create(request());
      const second = repository.create(request());
      repository.control(first.id, "confirm");
      repository.control(second.id, "confirm");
      const firstClaim = repository.claim()!;
      const secondClaim = repository.claim()!;
      assert.equal(firstClaim.item.state, "attempting");
      assert.equal(secondClaim.item.state, "failed");
      assert.match(secondClaim.item.detail!, /duplicate blocked/);
      repository.finish(secondClaim.batch.id, secondClaim.item);
      const later = repository.create(request());
      assert.equal(later.items[0]?.state, "pending");
      assert.match(later.items[0]!.detail!, /already attempted/);
      repository.control(later.id, "confirm");
      assert.equal(repository.claim()?.item.state, "failed");
    } finally { db.close(); }
  });

  test("暂停取消与失败效果符合原存储状态，不存在旧发送恢复接口", () => {
    const { db, repository } = fixture();
    try {
      const batch = repository.create(request());
      repository.control(batch.id, "confirm");
      repository.control(batch.id, "pause");
      assert.equal(repository.claim(), null);
      repository.control(batch.id, "resume");
      const claim = repository.claim()!;
      repository.finish(batch.id, recordBulkResult(claim.item, { kind: "failure", effect: "unknown", message: "response missing" }));
      assert.equal(repository.list()[0]?.state, "completed");
      assert.equal(repository.list()[0]?.items[0]?.state, "unknown");
      const cancelled = repository.create(request("different@example.com"));
      repository.control(cancelled.id, "cancel");
      const saved = repository.list().find(batch => batch.id === cancelled.id)!;
      assert.equal(saved.state, "cancelled");
      assert.equal(saved.items[0]?.state, "cancelled");
      assert.deepEqual(repository.recipients(), []);
    } finally { db.close(); }
  });

  test("缺少 Creator 表时明确失败，不构造收件人", () => {
    const { db, repository } = fixture();
    try {
      db.exec("DROP TABLE creators");
      assert.throws(() => repository.recipients(), /no such table/);
    } finally { db.close(); }
  });

  test("列、索引、唯一约束、状态约束和schema版本必须满足原SQL", () => {
    const { db, repository } = fixture();
    try {
      const batch = repository.create(request());
      const existing = batch.items[0]!;
      assert.throws(() => db.prepare("INSERT INTO gmail_bulk_items VALUES ('duplicate',?,?,?,?,'pending',NULL,NULL)").run(batch.id, existing.recipient, existing.source, existing.fingerprint), /UNIQUE/);
      assert.throws(() => db.prepare("UPDATE gmail_bulk_items SET state='retryable' WHERE id=?").run(existing.id), /CHECK/);
      assert.throws(() => db.prepare("DELETE FROM mailbox_refs WHERE id=?").run(mailbox.id), /FOREIGN KEY/);
      db.prepare("UPDATE mailbox_refs SET available=0 WHERE id=?").run(mailbox.id);
      assert.throws(() => repository.create(request()), /unavailable/);
      assert.equal(repository.list().length, 1);
      db.exec("DROP INDEX gmail_bulk_duplicates");
      assert.throws(() => new GmailBulkRepository(db), /index/);
      db.exec("CREATE INDEX gmail_bulk_duplicates ON gmail_bulk_items(fingerprint,state); PRAGMA user_version=89");
      assert.throws(() => new GmailBulkRepository(db), /schema 90/);
      db.exec("PRAGMA user_version=90; ALTER TABLE gmail_bulk_items ADD COLUMN extra TEXT");
      assert.throws(() => new GmailBulkRepository(db), /columns/);
    } finally { db.close(); }
  });

  test("finish 拒绝非终态、错误归属与已结算记录，不把未知结果恢复待发送", () => {
    const { db, repository } = fixture();
    try {
      const first = repository.create(request());
      const second = repository.create(request("other@example.com"));
      repository.control(first.id, "confirm");
      const claim = repository.claim()!;
      assert.throws(() => repository.finish(first.id, claim.item), /terminal/);
      assert.throws(() => repository.finish(first.id, { ...claim.item, state: "pending" }), /terminal/);
      const result = recordBulkResult(claim.item, { kind: "success", provider_message_id: "sent-id" });
      assert.throws(() => repository.finish(second.id, result), /belong/);
      assert.throws(() => repository.finish(first.id, { ...result, fingerprint: "changed" }), /identity/);
      assert.throws(() => repository.finish(first.id, { ...result, provider_message_id: null }), /provider message/);
      assert.throws(() => repository.finish(first.id, result), /durable original/);
      repository.reserveAttempt(bulkIdempotencyKey(claim.item));
      repository.finish(first.id, result);
      assert.throws(() => repository.finish(first.id, result), /no active attempt/);
      assert.equal(repository.list().find(batch => batch.id === first.id)?.items[0]?.provider_message_id, "sent-id");
    } finally { db.close(); }
  });

  test("晚到真实回执关联原批次和永久尝试键，只核对未知效果并保存原账号仓储", () => {
    const { db, repository } = fixture();
    try {
      const batch = repository.create(request());
      repository.control(batch.id, "confirm");
      const claim = repository.claim()!;
      const key = bulkIdempotencyKey(claim.item);
      repository.reserveAttempt(key);
      repository.interrupt();
      const receipt = recordBulkResult(claim.item, { kind: "success", provider_message_id: "late-receipt" });
      assert.throws(() => repository.reconcileReceipt(batch.id, receipt, "wrong-key"), /key/);
      assert.throws(() => repository.reconcileReceipt(batch.id, { ...receipt, state: "pending" }, key), /settled/);
      assert.throws(() => repository.reconcileReceipt(batch.id, { ...receipt, recipient: "another@example.com" }, key), /identity/);
      repository.reconcileReceipt(batch.id, receipt, key);
      const saved = repository.list()[0]!;
      assert.equal(saved.state, "paused");
      assert.equal(saved.items[0]?.provider_message_id, "late-receipt");
      assert.equal(saved.items[0]?.state, "sent");
      assert.equal(repository.reserveAttempt(key), false);
      assert.throws(() => repository.reconcileReceipt(batch.id, receipt, key), /unknown/);
      repository.control(batch.id, "resume");
      assert.equal(repository.claim(), null);
      assert.equal(repository.list()[0]?.state, "completed");
    } finally { db.close(); }
  });

  test("弱持久化配置明确拒绝领取和尝试标记，读取仍可使用", () => {
    const { db, repository } = fixture();
    try {
      const batch = repository.create(request());
      repository.control(batch.id, "confirm");
      db.exec("PRAGMA synchronous=NORMAL");
      assert.throws(() => repository.claim(), /FULL or EXTRA/);
      assert.throws(() => repository.reserveAttempt("not-saved"), /FULL or EXTRA/);
      assert.equal(repository.list()[0]?.items[0]?.state, "pending");
      db.exec("PRAGMA synchronous=FULL; PRAGMA journal_mode=DELETE");
      assert.throws(() => repository.claim(), /WAL/);
      db.exec("PRAGMA journal_mode=WAL");
      assert.equal(repository.claim()?.item.state, "attempting");
    } finally { db.close(); }
  });

  test("没有生产AccessProvider和归属resolver时明确未就绪，不领取真实SQLite待发批次", async () => {
    const { db, repository } = fixture();
    try {
      const batch = repository.create(request());
      repository.control(batch.id, "confirm");
      const worker = new GmailBulkWorker(new AccountContextService());
      await assert.rejects(worker.tick(new AbortController().signal), GmailBulkWorkerNotReadyError);
      assert.equal(repository.get(batch.id).items[0]?.state, "pending");
      assert.equal(db.prepare("SELECT count(*) AS count FROM gmail_send_attempts").get()!.count, 0);
    } finally { db.close(); }
  });
}
