import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { validateCreatorSchema90 } from "../storage/creator-schema.ts";
import { bulkIdempotencyKey, mailFingerprint } from "./gmail-bulk.ts";
import type { BulkBatch, BulkItem } from "./gmail-bulk.ts";
import { gmailMessageId } from "./gmail-mime.ts";
import type { GmailBulkMessage, VerifiedGmailSendReceipt } from "./gmail-mime.ts";

const string = z.string().refine(value => value.isWellFormed());
const receiptSchema = z.object({ providerMessageId: string.min(1), internetMessageId: string.min(1), from: string.min(1), to: string.min(1), subject: string, body: string, occurredAt: z.number().int().nonnegative().safe(), headers: z.record(string, string) }).strict();
const commitSchema = z.object({ recipient: string, preparation: z.object({ mailbox: z.object({ id: string, email_address: string }).passthrough(), to: string, confirmation_required: z.boolean(), confirmation_reason: string }), snapshot: z.object({ mailbox_ref_id: string, provider_thread_key: string, subject: string, messages: z.array(z.object({ provider_message_id: string, internet_message_id: string, from: string, to: z.array(string).length(1), subject: string, text: string, occurred_at: z.number().int().nonnegative().safe(), headers: z.record(string, string) }).passthrough()).length(1) }).passthrough(), verified_provider_message_id: string, idempotency_key: string, request_fingerprint: string });
export function stagedOriginalGmailReceipt(value: string): VerifiedGmailSendReceipt {
  const commit = commitSchema.parse(JSON.parse(value));
  const message = commit.snapshot.messages[0]!;
  z.object({ creator_id: z.null(), contact_route_id: z.null() }).parse(commit.snapshot);
  z.object({ direction: z.literal("outbound"), origin: z.literal("confirmed_send"), html: z.null(), in_reply_to: z.null(), cc: z.array(z.never()), bcc: z.array(z.never()), received_for: z.array(z.never()), reply_to: z.array(z.never()), references: z.array(z.never()), attachments: z.array(z.never()), delivery_status: z.literal("sent") }).parse(message);
  if (commit.snapshot.mailbox_ref_id !== commit.preparation.mailbox.id || commit.recipient !== commit.preparation.to || message.to[0] !== commit.recipient || message.provider_message_id !== commit.verified_provider_message_id || message.internet_message_id !== gmailMessageId(commit.preparation.mailbox.email_address, commit.idempotency_key)) throw new Error("gmail_staged_receipt_context_mismatch");
  return receiptSchema.parse({ providerMessageId: message.provider_message_id, internetMessageId: message.internet_message_id, from: message.from, to: message.to[0]!, subject: message.subject, body: message.text, occurredAt: message.occurred_at, headers: message.headers });
}

// 独立提交原 remote_verified 合同，之后本地历史事务失败也能无网络恢复回执。
export function stageOriginalGmailReceipt(db: DatabaseSync, batch: BulkBatch, item: BulkItem, input: VerifiedGmailSendReceipt): void {
  const receipt = receiptSchema.parse(input);
  const key = bulkIdempotencyKey(item);
  const row = db.prepare("SELECT state,result_json,request_fingerprint FROM idempotency_records WHERE key=? AND action='manual_email_send' AND mailbox_ref_id=?").get(key, batch.mailbox_ref_id);
  if (!row || !["remote_started", "remote_accepted"].includes(string.parse(row.state))) throw new Error("gmail_verified_receipt_has_no_original_claim");
  const recovery = z.object({ context: z.object({ preparation: z.unknown() }), provider_thread_key: string, mailbox: z.object({ email_address: string }), request_fingerprint: string, provider_message_id: string.optional() }).parse(JSON.parse(string.parse(row.result_json)));
  if (recovery.provider_message_id !== undefined && recovery.provider_message_id !== receipt.providerMessageId) throw new Error("gmail_original_receipt_provider_mismatch");
  if (receipt.internetMessageId !== gmailMessageId(recovery.mailbox.email_address, key) || receipt.to !== item.recipient || receipt.subject !== batch.subject || receipt.body.replace(/\r\n/gu, "\n").replace(/\n$/u, "") !== batch.body.replace(/\r\n/gu, "\n").replace(/\n$/u, "") || recovery.request_fingerprint !== row.request_fingerprint) throw new Error("gmail_original_verified_receipt_mismatch");
  const snapshot = { mailbox_ref_id: batch.mailbox_ref_id, creator_id: null, contact_route_id: null, provider_thread_key: recovery.provider_thread_key, subject: batch.subject, messages: [{ provider_message_id: receipt.providerMessageId, internet_message_id: receipt.internetMessageId, direction: "outbound", from: receipt.from, to: [receipt.to], cc: [], bcc: [], received_for: [], reply_to: [], in_reply_to: null, references: [], subject: receipt.subject, text: receipt.body, html: null, delivery_status: "sent", headers: receipt.headers, occurred_at: receipt.occurredAt, origin: "confirmed_send", attachments: [] }] };
  const commit = commitSchema.parse({ recipient: item.recipient, preparation: recovery.context.preparation, snapshot, verified_provider_message_id: receipt.providerMessageId, idempotency_key: key, request_fingerprint: recovery.request_fingerprint });
  if (db.prepare("UPDATE idempotency_records SET state='remote_verified',result_json=?,updated_at=? WHERE key=? AND state=? AND request_fingerprint=?").run(JSON.stringify(commit), Math.floor(Date.now() / 1000), key, string.parse(row.state), string.parse(row.request_fingerprint)).changes !== 1) throw new Error("gmail_verified_receipt_stage_mismatch");
}
export function reserveOriginalGmailSend(db: DatabaseSync, batch: BulkBatch, item: BulkItem, message: GmailBulkMessage): boolean {
  validateCreatorSchema90(db);
  if (!db.isTransaction) throw new Error("Original Gmail send claim requires the durable attempt transaction");
  if (item.state !== "attempting" || message.idempotencyKey !== bulkIdempotencyKey(item) || message.to !== item.recipient || message.subject !== batch.subject || message.body !== batch.body || mailFingerprint(message.from, message.to, message.subject, message.body) !== item.fingerprint) throw new Error("gmail_bulk_frozen_claim_mismatch");
  if (db.prepare("SELECT 1 FROM gmail_send_attempts WHERE idempotency_key=?").get(message.idempotencyKey) || db.prepare("SELECT 1 FROM idempotency_records WHERE key=?").get(message.idempotencyKey)) return false;
  const mailbox = db.prepare("SELECT * FROM mailbox_refs WHERE id=?").get(batch.mailbox_ref_id);
  if (!mailbox || mailbox.provider !== "gmail" || mailbox.email_address !== message.from || mailbox.available !== 1) throw new Error("gmail_mailbox_unavailable");
  const { capabilities_json, last_error: _lastError, available, ...fields } = mailbox;
  const snapshot = { ...fields, capabilities: z.array(string).parse(JSON.parse(string.parse(capabilities_json))), available: available === 1, last_error: null };
  const preparation = { mailbox: snapshot, to: message.to, confirmation_required: true, confirmation_reason: "sending email is an external write" };
  const fingerprint = JSON.stringify({ kind: "manual", mailbox_ref_id: batch.mailbox_ref_id, to: message.to, subject: message.subject, text: message.body, html: null });
  const request = { to: [message.to], cc: [], bcc: [], reply_to: [], in_reply_to: null, references: [], subject: message.subject, text: message.body, html: null, idempotency_key: message.idempotencyKey };
  const recovery = { action: "manual_email_send", request_fingerprint: fingerprint, canonical_request_fingerprint: JSON.stringify({ ...request, idempotency_key: "" }), mailbox: snapshot, expected_recipient: message.to, request, provider_thread_key: `manual:${batch.mailbox_ref_id}:${message.to}:send:${message.idempotencyKey}`, in_reply_to: null, references: [], context: { kind: "manual", preparation } };
  const now = Math.floor(Date.now() / 1000);
  db.prepare("INSERT INTO gmail_send_attempts VALUES (?)").run(message.idempotencyKey);
  db.prepare("INSERT INTO idempotency_records(key,action,mailbox_ref_id,contact_route_id,request_fingerprint,state,result_json,created_at,updated_at) VALUES (?,'manual_email_send',?,NULL,?,'remote_started',?,?,?)").run(message.idempotencyKey, batch.mailbox_ref_id, fingerprint, JSON.stringify(recovery), now, now);
  return true;
}

// 调用方把历史、审核记录、幂等结果和 bulk 回执放在同一个 SQLite 事务中。
export function settleOriginalGmailSend(db: DatabaseSync, batch: BulkBatch, item: BulkItem, receipt?: VerifiedGmailSendReceipt): boolean {
  if (!db.isTransaction) throw new Error("Original Gmail receipt requires the bulk result transaction");
  if (!db.prepare("SELECT 1 FROM sqlite_schema WHERE type='table' AND name='idempotency_records'").get()) return false;
  const key = bulkIdempotencyKey(item);
  const claim = db.prepare("SELECT action,mailbox_ref_id,request_fingerprint,state,result_json FROM idempotency_records WHERE key=?").get(key);
  if (!claim) return false;
  if (claim.action !== "manual_email_send" || claim.mailbox_ref_id !== batch.mailbox_ref_id || !["remote_started", "remote_accepted", "remote_verified"].includes(string.parse(claim.state))) throw new Error("gmail_original_receipt_claim_mismatch");
  const persisted = JSON.parse(string.parse(claim.result_json)) as unknown;
  const commit = claim.state === "remote_verified" ? commitSchema.parse(persisted) : null;
  const stagedReceipt = commit ? stagedOriginalGmailReceipt(string.parse(claim.result_json)) : undefined;
  if (receipt && stagedReceipt && JSON.stringify(receiptSchema.parse(receipt)) !== JSON.stringify(stagedReceipt)) throw new Error("gmail_verified_receipt_changed_after_stage");
  const normalized = commit ? { action: "manual_email_send", request_fingerprint: commit.request_fingerprint, mailbox: commit.preparation.mailbox, expected_recipient: commit.recipient, provider_thread_key: commit.snapshot.provider_thread_key, request: { subject: commit.snapshot.subject, text: batch.body, html: null, idempotency_key: commit.idempotency_key }, context: { kind: "manual", preparation: commit.preparation } } : persisted;
  const recovery = z.object({ action: z.literal("manual_email_send"), request_fingerprint: string, mailbox: z.object({ id: string, email_address: string }).passthrough(), expected_recipient: string, provider_thread_key: string, request: z.object({ subject: string, text: string, html: z.null(), idempotency_key: string }).passthrough(), context: z.object({ kind: z.literal("manual"), preparation: z.object({ mailbox: z.unknown(), to: string, confirmation_required: z.boolean(), confirmation_reason: string }) }) }).passthrough().parse(normalized);
  if (recovery.mailbox.id !== batch.mailbox_ref_id || recovery.expected_recipient !== item.recipient || recovery.request.idempotency_key !== key || recovery.request.subject !== batch.subject || recovery.request.text !== batch.body || recovery.request_fingerprint !== claim.request_fingerprint) throw new Error("gmail_original_receipt_context_mismatch");
  const now = Math.floor(Date.now() / 1000);
  if (item.state === "unknown") return true;
  if (claim.state === "remote_verified" && item.state !== "sent") throw new Error("gmail_verified_receipt_cannot_be_downgraded");
  if (typeof recovery.provider_message_id === "string" && item.provider_message_id !== recovery.provider_message_id && item.state !== "failed") throw new Error("gmail_original_receipt_provider_mismatch");
  if (item.state === "failed") {
    if (db.prepare("UPDATE idempotency_records SET state='failed',result_json=?,updated_at=? WHERE key=? AND state=?").run(JSON.stringify({ kind: "remote_write_rejected", message: item.detail ?? "gmail_send_failed", remote_receipt: null }), now, key, string.parse(claim.state)).changes !== 1) throw new Error("gmail_original_failure_commit_mismatch");
    return true;
  }
  if (!item.provider_message_id) throw new Error("gmail_original_receipt_missing_provider_id");
  if (item.state === "accepted") {
    if (db.prepare("UPDATE idempotency_records SET state='remote_accepted',result_json=?,updated_at=? WHERE key=? AND state=?").run(JSON.stringify({ ...recovery, provider_message_id: item.provider_message_id }), now, key, string.parse(claim.state)).changes !== 1) throw new Error("gmail_original_acceptance_commit_mismatch");
    return true;
  }
  receipt = receipt ? receiptSchema.parse(receipt) : stagedReceipt;
  if (!receipt || receipt.providerMessageId !== item.provider_message_id || receipt.internetMessageId !== gmailMessageId(recovery.mailbox.email_address, key) || receipt.to !== item.recipient || receipt.subject !== batch.subject || receipt.body.replace(/\r\n/gu, "\n").replace(/\n$/u, "") !== batch.body.replace(/\r\n/gu, "\n").replace(/\n$/u, "")) throw new Error("gmail_original_verified_receipt_mismatch");
  const existing = db.prepare("SELECT id,creator_id,contact_route_id,created_at,active_folder,archived_at,deleted_at FROM email_threads WHERE mailbox_ref_id=? AND provider_thread_key=?").get(batch.mailbox_ref_id, recovery.provider_thread_key);
  if (existing && (existing.creator_id !== null || existing.contact_route_id !== null)) throw new Error("manual email snapshot must not be bound to a Creator");
  const threadId = existing ? string.parse(existing.id) : randomUUID();
  db.prepare("INSERT INTO email_threads(id,mailbox_ref_id,creator_id,contact_route_id,provider_thread_key,subject,unread_count,active_folder,archived_at,deleted_at,created_at,updated_at) VALUES (?,?,NULL,NULL,?,?,0,'sent',NULL,NULL,?,?) ON CONFLICT(mailbox_ref_id,provider_thread_key) DO UPDATE SET subject=excluded.subject,updated_at=excluded.updated_at").run(threadId, batch.mailbox_ref_id, recovery.provider_thread_key, batch.subject, now, now);
  const prior = db.prepare("SELECT id FROM emails WHERE thread_id=? AND provider_message_id=?").get(threadId, receipt.providerMessageId);
  const emailId = prior ? string.parse(prior.id) : randomUUID();
  db.prepare("INSERT INTO emails(id,thread_id,provider_message_id,internet_message_id,direction,subject,body_text,body_html,in_reply_to,references_json,delivery_status,headers_json,occurred_at,origin) VALUES (?,?,?,?,'outbound',?,?,NULL,NULL,'[]','sent',?,?,'confirmed_send') ON CONFLICT(thread_id,provider_message_id) DO UPDATE SET internet_message_id=excluded.internet_message_id,direction='outbound',subject=excluded.subject,body_text=excluded.body_text,body_html=NULL,in_reply_to=NULL,references_json='[]',delivery_status='sent',headers_json=excluded.headers_json,occurred_at=excluded.occurred_at,origin='confirmed_send'").run(emailId, threadId, receipt.providerMessageId, receipt.internetMessageId, receipt.subject, receipt.body, JSON.stringify(receipt.headers), receipt.occurredAt);
  db.prepare("DELETE FROM email_recipients WHERE email_id=?").run(emailId);
  for (const [kind, address] of [["from", receipt.from], ["to", receipt.to]]) db.prepare("INSERT INTO email_recipients(email_id,recipient_kind,address,locator_id) VALUES (?,?,?,NULL)").run(emailId, kind!, address!);
  db.prepare("DELETE FROM email_attachments WHERE email_id=?").run(emailId);
  db.prepare("INSERT INTO email_provider_identities(mailbox_ref_id,provider_message_id,email_id,direction,detail_complete,updated_at) VALUES (?,?,?,'outbound',1,?) ON CONFLICT(mailbox_ref_id,provider_message_id) DO UPDATE SET email_id=excluded.email_id,direction='outbound',detail_complete=1,updated_at=excluded.updated_at").run(batch.mailbox_ref_id, receipt.providerMessageId, emailId, now);
  const audit = { id: randomUUID(), kind: "manual_email_sent", creator_id: null, account_ref_id: null, resolved_target_id: null, mailbox_ref_id: batch.mailbox_ref_id, contact_locator_id: null, email_id: emailId, payload: { provider_message_id: receipt.providerMessageId, provider_thread_key: recovery.provider_thread_key, recipient: item.recipient, verified: true }, created_at: now };
  db.prepare("INSERT INTO audit_events(id,kind,creator_id,account_ref_id,resolved_target_id,mailbox_ref_id,contact_locator_id,email_id,payload_json,created_at) VALUES (?,'manual_email_sent',NULL,NULL,NULL,?,NULL,?,?,?)").run(audit.id, batch.mailbox_ref_id, emailId, JSON.stringify(audit.payload), now);
  db.prepare("UPDATE email_threads SET unread_count=(SELECT COUNT(*) FROM emails WHERE thread_id=? AND direction='inbound' AND delivery_status='unread') WHERE id=?").run(threadId, threadId);
  const storedThread = db.prepare("SELECT unread_count,active_folder,archived_at,deleted_at,created_at FROM email_threads WHERE id=?").get(threadId)!;
  const hasInbound = db.prepare("SELECT 1 FROM emails WHERE thread_id=? AND direction='inbound' LIMIT 1").get(threadId) !== undefined;
  const thread = { id: threadId, mailbox_ref_id: batch.mailbox_ref_id, creator_id: null, contact_route_id: null, provider_thread_key: recovery.provider_thread_key, subject: batch.subject, unread_count: storedThread.unread_count, folder: storedThread.deleted_at !== null ? "deleted" : storedThread.archived_at !== null && hasInbound ? "archive" : storedThread.active_folder, created_at: storedThread.created_at, updated_at: now };
  const email = { id: emailId, thread_id: threadId, provider_message_id: receipt.providerMessageId, internet_message_id: receipt.internetMessageId, direction: "outbound", from: receipt.from, to: [receipt.to], cc: [], reply_to: [], in_reply_to: null, references: [], subject: receipt.subject, text: receipt.body, html: null, delivery_status: "sent", occurred_at: receipt.occurredAt };
  const result = { preparation: recovery.context.preparation, thread, email, audit_event: audit, replayed: false };
  if (db.prepare("UPDATE idempotency_records SET state='completed',result_json=?,updated_at=? WHERE key=? AND state=? AND request_fingerprint=?").run(JSON.stringify(result), now, key, string.parse(claim.state), string.parse(claim.request_fingerprint)).changes !== 1) throw new Error("gmail_original_history_commit_mismatch");
  return true;
}
