import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { bulkIdempotencyKey, controlBulk, prepareBulk } from "./gmail-bulk.ts";
import type { BulkBatch, BulkControl, BulkItem, BulkMailbox, BulkRecipient, PrepareBulkInput } from "./gmail-bulk.ts";
import { reserveOriginalGmailSend, settleOriginalGmailSend, stageOriginalGmailReceipt, stagedOriginalGmailReceipt } from "./gmail-send-history.ts";
import type { GmailBulkMessage, VerifiedGmailSendReceipt } from "./gmail-mime.ts";

const itemSchema = z.object({
  id: z.string(), recipient: z.string(), source: z.string(), fingerprint: z.string(),
  state: z.enum(["pending", "attempting", "accepted", "sent", "failed", "unknown", "cancelled"]),
  provider_message_id: z.string().nullable(), detail: z.string().nullable(),
});
const batchSchema = z.object({
  id: z.string(), mailbox_ref_id: z.string(), subject: z.string(), body: z.string(),
  state: z.enum(["draft", "running", "paused", "cancelled", "completed"]), created_at: z.number().int().safe(),
});

const expectedColumns = {
  gmail_bulk_batches: ["id:TEXT:0:1", "mailbox_ref_id:TEXT:1:0", "subject:TEXT:1:0", "body:TEXT:1:0", "state:TEXT:1:0", "created_at:INTEGER:1:0"],
  gmail_bulk_items: ["id:TEXT:0:1", "batch_id:TEXT:1:0", "recipient:TEXT:1:0", "source:TEXT:1:0", "fingerprint:TEXT:1:0", "state:TEXT:1:0", "provider_message_id:TEXT:0:0", "detail:TEXT:0:0"],
  gmail_send_attempts: ["idempotency_key:TEXT:0:1"],
} as const;

function validateSchema(db: DatabaseSync): void {
  if (![90, 91, 92, 93, 94, 95, 96, 97].includes(Number(db.prepare("PRAGMA user_version").get()!.user_version))) throw new Error("Gmail bulk storage requires a supported Creator schema");
  if (db.prepare("PRAGMA foreign_keys").get()!.foreign_keys !== 1) throw new Error("Gmail bulk storage requires foreign key enforcement");
  for (const [table, expected] of Object.entries(expectedColumns)) {
    const actual = db.prepare(`PRAGMA table_info(${table})`).all().map(row => `${row.name}:${row.type}:${row.notnull}:${row.pk}`);
    if (actual.join("|") !== expected.join("|")) throw new Error(`Incompatible ${table} columns`);
  }
  const indexes = db.prepare("PRAGMA index_list(gmail_bulk_items)").all();
  for (const [name, columns] of [["gmail_bulk_pending", ["batch_id", "state"]], ["gmail_bulk_duplicates", ["fingerprint", "state"]]] as const) {
    if (!indexes.some(row => row.name === name && row.unique === 0 && row.partial === 0)) throw new Error(`Missing Gmail bulk index ${name}`);
    if (db.prepare(`PRAGMA index_info(${name})`).all().map(row => row.name).join("|") !== columns.join("|")) throw new Error(`Incompatible Gmail bulk index ${name}`);
  }
  const unique = indexes.some(index => index.unique === 1 && index.origin === "u" && index.partial === 0 &&
    db.prepare("SELECT name FROM pragma_index_info(?) ORDER BY seqno").all(String(index.name)).map(row => row.name).join("|") === "batch_id|recipient");
  if (!unique) throw new Error("Missing Gmail bulk recipient uniqueness constraint");
  for (const [table, from, target] of [["gmail_bulk_batches", "mailbox_ref_id", "mailbox_refs"], ["gmail_bulk_items", "batch_id", "gmail_bulk_batches"]] as const) {
    const foreignKeys = db.prepare(`PRAGMA foreign_key_list(${table})`).all();
    if (foreignKeys.length !== 1 || !foreignKeys.some(row => row.from === from && row.table === target && row.to === "id" && row.on_delete === "NO ACTION" && row.on_update === "NO ACTION")) throw new Error(`Incompatible ${table} foreign key`);
  }
  for (const [table, states] of [["gmail_bulk_batches", "'draft','running','paused','cancelled','completed'"], ["gmail_bulk_items", "'pending','attempting','accepted','sent','failed','unknown','cancelled'"]] as const) {
    const sql = db.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(table)?.sql;
    if (typeof sql !== "string" || !sql.replace(/\s/gu, "").includes(`CHECK(stateIN(${states}))`)) throw new Error(`Incompatible ${table} state constraint`);
  }
}

function assertDurableConnection(db: DatabaseSync): void {
  if (db.prepare("PRAGMA journal_mode").get()!.journal_mode !== "wal") throw new Error("Gmail bulk writes require a file-backed WAL journal");
  const synchronous = db.prepare("PRAGMA synchronous").get()!.synchronous;
  if (synchronous !== 2 && synchronous !== 3) throw new Error("Gmail bulk writes require synchronous FULL or EXTRA");
}

// 仅操作调用方注入的已验证数据库，不创建表、不执行迁移、不持有网络能力。
export class GmailBulkRepository {
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync) {
    validateSchema(db);
    this.#db = db;
  }

  #transaction<T>(body: () => T, readOnly = false): T {
    if (!readOnly) assertDurableConnection(this.#db);
    this.#db.exec(readOnly ? "BEGIN DEFERRED" : "BEGIN IMMEDIATE");
    let committed = false;
    try {
      const result = body();
      this.#db.exec("COMMIT");
      committed = true;
      return result;
    } finally {
      if (!committed) this.#db.exec("ROLLBACK");
    }
  }

  #load(id: string): BulkBatch {
    const row = this.#db.prepare("SELECT id,mailbox_ref_id,subject,body,state,created_at FROM gmail_bulk_batches WHERE id=?").get(id);
    if (!row) throw new Error("bulk batch missing");
    const batch = batchSchema.parse(row);
    const items = this.#db.prepare("SELECT id,recipient,source,fingerprint,state,provider_message_id,detail FROM gmail_bulk_items WHERE batch_id=? ORDER BY rowid").all(id).map(row => itemSchema.parse(row));
    return { ...batch, items };
  }

  create(input: PrepareBulkInput): BulkBatch {
    return this.#transaction(() => {
      const row = this.#db.prepare("SELECT id,provider,email_address,available,capabilities_json FROM mailbox_refs WHERE id=?").get(input.mailbox_ref_id);
      if (!row) throw new Error("mailbox reference missing");
      const stored = z.object({ id: z.string(), provider: z.string(), email_address: z.string(), available: z.union([z.literal(0), z.literal(1)]), capabilities_json: z.string() }).parse(row);
      const mailbox: BulkMailbox = { ...stored, available: stored.available === 1, capabilities: z.array(z.string()).parse(JSON.parse(stored.capabilities_json)) };
      const prepared = prepareBulk(input, mailbox);
      const id = randomUUID();
      this.#db.prepare("INSERT INTO gmail_bulk_batches VALUES (?,?,?,?,'draft',?)").run(id, prepared.mailbox_ref_id, prepared.subject, prepared.body, Math.floor(Date.now() / 1000));
      for (const item of prepared.items) {
        const duplicate = this.#db.prepare("SELECT EXISTS(SELECT 1 FROM gmail_bulk_items WHERE fingerprint=? AND state IN ('attempting','accepted','sent','unknown')) AS found").get(item.fingerprint)!.found === 1;
        this.#db.prepare("INSERT INTO gmail_bulk_items VALUES (?,?,?,?,?,'pending',NULL,?)").run(randomUUID(), id, item.recipient, item.source, item.fingerprint, duplicate ? "Identical mail was already attempted; this recipient will be skipped" : null);
      }
      return this.#load(id);
    });
  }

  list(): BulkBatch[] {
    return this.#transaction(() => this.#db.prepare("SELECT id FROM gmail_bulk_batches ORDER BY created_at DESC,rowid DESC LIMIT 100").all().map(row => this.#load(z.string().parse(row.id))), true);
  }

  get(id: string): BulkBatch { return this.#transaction(() => this.#load(id), true); }

  pauseIfRunning(id: string): boolean {
    return this.#transaction(() => this.#db.prepare("UPDATE gmail_bulk_batches SET state='paused' WHERE id=? AND state='running'").run(id).changes === 1);
  }

  recipients(): BulkRecipient[] {
    const sql = "SELECT l.normalized_value AS email,c.display_name || ' · ' || r.confidence AS source FROM contact_locators l JOIN creator_contact_routes r ON r.locator_id=l.id JOIN creators c ON c.id=r.creator_id WHERE l.channel='email' AND l.kind='email_address' AND c.archived_at IS NULL UNION SELECT o.normalized_value,o.source_url || ' · ' || o.attribution FROM discovery_contact_observations o JOIN discovery_campaigns c ON c.id=o.discovery_campaign_id WHERE o.kind='email' AND o.criteria_revision=c.current_revision ORDER BY 1,2 LIMIT 500";
    return this.#db.prepare(sql).all().map(row => z.object({ email: z.string(), source: z.string() }).parse(row));
  }

  control(id: string, action: BulkControl): void {
    this.#transaction(() => {
      const original = this.#load(id);
      const next = controlBulk(original, action);
      if (next === original) return;
      this.#db.prepare("UPDATE gmail_bulk_batches SET state=? WHERE id=?").run(next.state, id);
      if (next.state === "cancelled") this.#db.prepare("UPDATE gmail_bulk_items SET state='cancelled' WHERE batch_id=? AND state='pending'").run(id);
    });
  }

  claim(): { batch: BulkBatch; item: BulkItem } | null {
    return this.#transaction(() => {
      const next = this.#db.prepare("SELECT b.id AS batch_id,i.id AS item_id FROM gmail_bulk_batches b JOIN gmail_bulk_items i ON i.batch_id=b.id WHERE b.state='running' AND i.state='pending' ORDER BY b.created_at,i.rowid LIMIT 1").get();
      if (!next) return null;
      const batch = this.#load(z.string().parse(next.batch_id));
      const item = batch.items.find(item => item.id === next.item_id);
      if (!item) throw new Error("bulk item missing");
      const duplicate = this.#db.prepare("SELECT EXISTS(SELECT 1 FROM gmail_bulk_items WHERE fingerprint=? AND id<>? AND state IN ('attempting','accepted','sent','unknown')) AS found").get(item.fingerprint, item.id)!.found === 1;
      const claimed: BulkItem = { ...item, state: duplicate ? "failed" : "attempting", detail: duplicate ? "Identical mail was already attempted; duplicate blocked" : null };
      const updated = this.#db.prepare("UPDATE gmail_bulk_items SET state=?,detail=? WHERE id=? AND batch_id=? AND state='pending'").run(claimed.state, claimed.detail, claimed.id, batch.id);
      if (updated.changes !== 1) throw new Error("Gmail bulk claim did not update exactly one pending item");
      return { batch, item: claimed };
    });
  }

  finish(batchId: string, item: BulkItem): void {
    itemSchema.parse(item);
    if (item.state !== "accepted" && item.state !== "sent" && item.state !== "failed" && item.state !== "unknown") throw new Error("Gmail bulk finish requires a terminal send result");
    if (item.state === "sent" && !item.provider_message_id) throw new Error("Sent Gmail bulk result requires a provider message id");
    this.#transaction(() => {
      const batch = this.#load(batchId);
      const prior = batch.items.find(stored => stored.id === item.id);
      if (!prior) throw new Error("Gmail bulk result does not belong to this batch");
      if (prior.recipient !== item.recipient || prior.source !== item.source || prior.fingerprint !== item.fingerprint) throw new Error("Gmail bulk result identity does not match its stored item");
      if (prior.state !== "attempting" && !(prior.state === "failed" && item.state === "failed" && prior.provider_message_id === item.provider_message_id && prior.detail === item.detail)) throw new Error("Gmail bulk result has no active attempt or duplicate rejection");
      if ((item.state === "sent" || item.state === "accepted") && !this.#db.prepare("SELECT idempotency_key FROM gmail_send_attempts WHERE idempotency_key=?").get(bulkIdempotencyKey(item))) throw new Error("Gmail accepted result has no durable original send attempt");
      settleOriginalGmailSend(this.#db, batch, item);
      const updated = this.#db.prepare("UPDATE gmail_bulk_items SET state=?,provider_message_id=?,detail=? WHERE id=? AND batch_id=? AND state=?").run(item.state, item.provider_message_id, item.detail, item.id, batch.id, prior.state);
      if (updated.changes !== 1) throw new Error("Gmail bulk finish did not update exactly one item");
      this.#db.prepare("UPDATE gmail_bulk_batches SET state='completed' WHERE id=? AND state='running' AND NOT EXISTS (SELECT 1 FROM gmail_bulk_items i WHERE i.batch_id=gmail_bulk_batches.id AND i.state IN ('pending','attempting'))").run(batch.id);
    });
  }

  // 仅供已开始的原发送调用提交晚到回执，不触发发送，也不依赖当前页面身份。
  // 调用方必须持有原账号仓储及原批次关联；unknown 永远不能变回待发送状态。
  reconcileReceipt(batchId: string, item: BulkItem, attemptKey: string, allowActiveAttempt = false, receipt?: VerifiedGmailSendReceipt): void {
    itemSchema.parse(item);
    if (item.state !== "sent" && item.state !== "accepted" && item.state !== "failed") throw new Error("Gmail receipt reconciliation requires a settled provider effect");
    if ((item.state === "sent" || item.state === "accepted") && !item.provider_message_id) throw new Error("Gmail accepted receipt requires a provider message id");
    if (attemptKey !== bulkIdempotencyKey(item)) throw new Error("Gmail receipt attempt key does not match its item");
    this.#transaction(() => {
      const batch = this.#load(batchId);
      const prior = batch.items.find(stored => stored.id === item.id);
      if (!prior || (prior.state !== "unknown" && !(allowActiveAttempt && (prior.state === "attempting" || prior.state === "accepted" && item.state === "sent" && receipt !== undefined)))) throw new Error("Gmail receipt requires an unknown item or permitted original attempt in the original batch");
      if (prior.recipient !== item.recipient || prior.source !== item.source || prior.fingerprint !== item.fingerprint) throw new Error("Gmail receipt identity does not match its stored item");
      if (!this.#db.prepare("SELECT idempotency_key FROM gmail_send_attempts WHERE idempotency_key=?").get(attemptKey)) throw new Error("Gmail receipt has no durable original send attempt");
      settleOriginalGmailSend(this.#db, batch, item, receipt);
      const updated = this.#db.prepare("UPDATE gmail_bulk_items SET state=?,provider_message_id=?,detail=? WHERE id=? AND batch_id=? AND state=?").run(item.state, item.provider_message_id, item.detail, item.id, batch.id, prior.state);
      if (updated.changes !== 1) throw new Error("Gmail receipt did not update exactly one unknown item");
      this.#db.prepare("UPDATE gmail_bulk_batches SET state='completed' WHERE id=? AND state='running' AND NOT EXISTS (SELECT 1 FROM gmail_bulk_items i WHERE i.batch_id=gmail_bulk_batches.id AND i.state IN ('pending','attempting'))").run(batch.id);
    });
  }

  interrupt(): void {
    this.#transaction(() => {
      this.#db.exec("UPDATE gmail_bulk_items SET state='unknown',detail='Service interrupted after reservation; automatic resend disabled' WHERE state='attempting'");
      this.#db.exec("UPDATE gmail_bulk_batches SET state='paused' WHERE state='running'");
    });
  }

  // 成功提交此标记后才允许调用外部发送；返回 false 必须按 unknown 处理。
  reserveAttempt(key: string): boolean {
    if (this.#db.isTransaction) throw new Error("Gmail attempt marker requires an independent durable commit");
    assertDurableConnection(this.#db);
    return this.#db.prepare("INSERT OR IGNORE INTO gmail_send_attempts(idempotency_key) VALUES (?)").run(key).changes === 1;
  }
  reserveSendAttempt(batchId: string, item: BulkItem, message: GmailBulkMessage): boolean {
    return this.#transaction(() => {
      const batch = this.#load(batchId);
      const prior = batch.items.find(candidate => candidate.id === item.id);
      if (!prior || prior.state !== "attempting" || prior.recipient !== item.recipient || prior.fingerprint !== item.fingerprint || prior.source !== item.source) throw new Error("Gmail original send claim no longer matches its reserved item");
      return reserveOriginalGmailSend(this.#db, batch, prior, message);
    });
  }
  stageVerifiedReceipt(batchId: string, item: BulkItem, receipt: VerifiedGmailSendReceipt): void {
    this.#transaction(() => {
      const batch = this.#load(batchId);
      const prior = batch.items.find(candidate => candidate.id === item.id);
      if (!prior || !["attempting", "unknown", "accepted"].includes(prior.state) || prior.fingerprint !== item.fingerprint || prior.recipient !== item.recipient || prior.source !== item.source || !this.#db.prepare("SELECT 1 FROM gmail_send_attempts WHERE idempotency_key=?").get(bulkIdempotencyKey(item))) throw new Error("Gmail verified receipt has no original reserved item");
      stageOriginalGmailReceipt(this.#db, batch, prior, receipt);
    });
  }
  recordProviderAcceptance(batchId: string, item: BulkItem, providerMessageId: string): void {
    this.#transaction(() => {
      const batch = this.#load(batchId);
      const prior = batch.items.find(candidate => candidate.id === item.id);
      if (!prior || !["attempting", "unknown"].includes(prior.state) || prior.fingerprint !== item.fingerprint || prior.recipient !== item.recipient || prior.source !== item.source || !this.#db.prepare("SELECT 1 FROM gmail_send_attempts WHERE idempotency_key=?").get(bulkIdempotencyKey(item))) throw new Error("Gmail acceptance has no original reserved item");
      if (!settleOriginalGmailSend(this.#db, batch, { ...prior, state: "accepted", provider_message_id: providerMessageId })) throw new Error("Gmail acceptance has no original durable claim");
    });
  }
  recoverVerifiedReceipts(): number {
    if (!this.#db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='idempotency_records'").get()) throw new Error("Original Gmail recovery requires full Creator schema 90");
    const rows = this.#db.prepare("SELECT b.id AS batch_id,i.id AS item_id,r.result_json FROM gmail_bulk_batches b JOIN gmail_bulk_items i ON i.batch_id=b.id JOIN idempotency_records r ON r.key='gmail-bulk-'||i.id AND r.action='manual_email_send' AND r.mailbox_ref_id=b.mailbox_ref_id WHERE r.state='remote_verified' AND i.state IN ('attempting','unknown','accepted') ORDER BY b.created_at,b.id,i.rowid").all();
    for (const row of rows) {
      const receipt = stagedOriginalGmailReceipt(z.string().parse(row.result_json));
      const batch = this.get(z.string().parse(row.batch_id));
      const item = batch.items.find(candidate => candidate.id === row.item_id);
      if (!item) throw new Error("Gmail staged receipt item disappeared");
      this.reconcileReceipt(batch.id, { ...item, state: "sent", provider_message_id: receipt.providerMessageId, detail: null }, bulkIdempotencyKey(item), true, receipt);
    }
    return rows.length;
  }
}
