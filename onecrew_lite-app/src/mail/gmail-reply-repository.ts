import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { MailHistoryRepository } from "./mail-history.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";

const text = z.string().refine(value => value.isWellFormed());
export const gmailReceivedMessageSchema = z.object({
  providerMessageId: text.regex(/^[a-f\d]{1,128}$/iu), internetMessageId: text.nullable(),
  from: text.min(1), to: z.array(text), cc: z.array(text), bcc: z.array(text), replyTo: z.array(text),
  receivedFor: z.array(text), subject: text, bodyText: text.nullable(), bodyHtml: text.nullable(),
  inReplyTo: text.nullable(), references: z.array(text), headers: z.record(text, text),
  occurredAt: z.number().int().nonnegative().safe(),
}).strict();
export type GmailReceivedMessage = z.infer<typeof gmailReceivedMessageSchema>;

// Gmail 只导入本地 confirmed_send 的回复；此仓储不读取整个收件箱。
export class GmailReplyRepository {
  readonly owner: Readonly<GmailBulkRepositoryOwner>;
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync, owner: GmailBulkRepositoryOwner) {
    this.owner = new MailHistoryRepository(db, owner).owner;
    this.#db = db;
  }
  #write<T>(operation: () => T): T {
    const sync = this.#db.prepare("PRAGMA synchronous").get()!.synchronous;
    if (this.#db.prepare("PRAGMA journal_mode").get()!.journal_mode !== "wal" || (sync !== 2 && sync !== 3)) throw new Error("Gmail reply import requires durable WAL storage");
    this.#db.exec("BEGIN IMMEDIATE");
    let committed = false;
    try { const result = operation(); this.#db.exec("COMMIT"); committed = true; return result; }
    finally { if (!committed && this.#db.isTransaction) this.#db.exec("ROLLBACK"); }
  }
  #mailbox(mailboxId: string): string {
    const row = this.#db.prepare("SELECT provider,email_address,available,capabilities_json FROM mailbox_refs WHERE id=?").get(mailboxId);
    if (!row || row.provider !== "gmail" || row.available !== 1 || !z.array(text).parse(JSON.parse(text.parse(row.capabilities_json))).includes("inbox")) throw new Error("gmail_mailbox_unavailable");
    return text.min(1).parse(row.email_address);
  }
  seeds(mailboxId: string): { threadId: string; providerMessageId: string }[] {
    this.#mailbox(mailboxId);
    return this.#db.prepare("SELECT e.thread_id AS threadId,MIN(e.provider_message_id) AS providerMessageId FROM emails e JOIN email_threads t ON t.id=e.thread_id WHERE t.mailbox_ref_id=? AND e.direction='outbound' AND e.origin='confirmed_send' AND e.provider_message_id!='' GROUP BY e.thread_id ORDER BY e.thread_id").all(mailboxId).map(row => z.object({ threadId: text.min(1), providerMessageId: text.min(1) }).parse(row));
  }
  complete(mailboxId: string, ids: readonly string[]): Set<string> {
    this.#mailbox(mailboxId);
    const query = this.#db.prepare("SELECT 1 FROM email_provider_identities WHERE mailbox_ref_id=? AND provider_message_id=? AND detail_complete=1");
    return new Set(ids.filter(id => query.get(mailboxId, id) !== undefined));
  }
  commitReplies(mailboxId: string, threadId: string, input: readonly GmailReceivedMessage[]): string[] {
    const messages = input.map(message => gmailReceivedMessageSchema.parse(message));
    return this.#write(() => {
      const mailboxAddress = this.#mailbox(mailboxId);
      const thread = this.#db.prepare("SELECT mailbox_ref_id,creator_id,contact_route_id FROM email_threads WHERE id=?").get(threadId);
      if (!thread || thread.mailbox_ref_id !== mailboxId) throw new Error("reply_mailbox_mismatch");
      if ((thread.creator_id === null) !== (thread.contact_route_id === null)) throw new Error("email thread ownership requires both Creator and contact route or neither");
      if (thread.creator_id !== null) {
        const route = this.#db.prepare("SELECT creator_id FROM creator_contact_routes WHERE id=?").get(text.parse(thread.contact_route_id));
        if (!route || route.creator_id !== thread.creator_id) throw new Error("email snapshot contact route belongs to a different Creator");
      }
      const timestamp = Math.floor(Date.now() / 1000);
      for (const message of messages) {
        if (!message.receivedFor.includes(mailboxAddress)) throw new Error("received email was not addressed to the selected mailbox");
        const existing = this.#db.prepare("SELECT id FROM emails WHERE thread_id=? AND provider_message_id=?").get(threadId, message.providerMessageId);
        const id = existing ? text.parse(existing.id) : randomUUID();
        this.#db.prepare(`INSERT INTO emails (id,thread_id,provider_message_id,internet_message_id,direction,subject,body_text,body_html,in_reply_to,references_json,delivery_status,headers_json,occurred_at,origin)
          VALUES (?,?,?,?,'inbound',?,?,?,?,?,'unread',?,?,'provider_import')
          ON CONFLICT(thread_id,provider_message_id) DO UPDATE SET internet_message_id=excluded.internet_message_id,direction=excluded.direction,subject=excluded.subject,body_text=excluded.body_text,body_html=excluded.body_html,in_reply_to=excluded.in_reply_to,references_json=excluded.references_json,
          delivery_status=CASE WHEN emails.delivery_status='read' AND excluded.delivery_status='unread' THEN emails.delivery_status ELSE excluded.delivery_status END,
          headers_json=excluded.headers_json,occurred_at=excluded.occurred_at,origin=CASE WHEN emails.origin='confirmed_send' THEN emails.origin ELSE excluded.origin END`).run(id, threadId, message.providerMessageId, message.internetMessageId, message.subject, message.bodyText, message.bodyHtml, message.inReplyTo, JSON.stringify(message.references), JSON.stringify(message.headers), message.occurredAt);
        this.#db.prepare("DELETE FROM email_recipients WHERE email_id=?").run(id);
        const recipients = { from: [message.from], to: message.to, cc: message.cc, bcc: message.bcc, received_for: message.receivedFor, reply_to: message.replyTo };
        for (const [kind, addresses] of Object.entries(recipients)) for (const address of addresses) this.#db.prepare("INSERT INTO email_recipients(email_id,recipient_kind,address,locator_id) VALUES (?,?,?,NULL)").run(id, kind, address);
        this.#db.prepare("DELETE FROM email_attachments WHERE email_id=?").run(id);
        this.#db.prepare("INSERT INTO email_provider_identities(mailbox_ref_id,provider_message_id,email_id,direction,detail_complete,updated_at) VALUES (?,?,?,'inbound',1,?) ON CONFLICT(mailbox_ref_id,provider_message_id) DO UPDATE SET email_id=excluded.email_id,direction=excluded.direction,detail_complete=1,updated_at=excluded.updated_at").run(mailboxId, message.providerMessageId, id, timestamp);
      }
      if (messages.length > 0) {
        this.#db.prepare("UPDATE email_threads SET unread_count=(SELECT COUNT(*) FROM emails WHERE thread_id=? AND direction='inbound' AND delivery_status='unread'),active_folder='inbox',updated_at=? WHERE id=?").run(threadId, timestamp, threadId);
        // 原页提交首次建立状态；尚有缺失详情时不得推进完成边界。
        this.#db.prepare("INSERT INTO mailbox_sync_state(mailbox_ref_id,direction,newest_provider_message_id,newest_occurred_at,oldest_backfill_cursor,backfill_complete,last_page_at,last_full_sync_at,failure_json,updated_at) VALUES (?,'inbound',NULL,NULL,NULL,0,?,NULL,NULL,?) ON CONFLICT(mailbox_ref_id,direction) DO UPDATE SET last_page_at=excluded.last_page_at,failure_json=NULL,updated_at=excluded.updated_at").run(mailboxId, timestamp, timestamp);
      }
      return messages.length === 0 ? [] : [threadId];
    });
  }
  completeCheckpoint(mailboxId: string): void {
    this.#write(() => {
      this.#mailbox(mailboxId);
      const current = this.#db.prepare("SELECT newest_provider_message_id,newest_occurred_at,oldest_backfill_cursor,backfill_complete,last_full_sync_at,failure_json FROM mailbox_sync_state WHERE mailbox_ref_id=? AND direction='inbound'").get(mailboxId);
      if (!current || current.newest_provider_message_id !== null || current.newest_occurred_at !== null || current.oldest_backfill_cursor !== null || current.backfill_complete !== 1 || current.last_full_sync_at === null || current.failure_json !== null) {
        const timestamp = Math.floor(Date.now() / 1000);
        this.#db.prepare("INSERT INTO mailbox_sync_state(mailbox_ref_id,direction,newest_provider_message_id,newest_occurred_at,oldest_backfill_cursor,backfill_complete,last_page_at,last_full_sync_at,failure_json,updated_at) VALUES (?,'inbound',NULL,NULL,NULL,1,?,?,NULL,?) ON CONFLICT(mailbox_ref_id,direction) DO UPDATE SET newest_provider_message_id=NULL,newest_occurred_at=NULL,oldest_backfill_cursor=NULL,backfill_complete=1,last_page_at=excluded.last_page_at,last_full_sync_at=excluded.last_page_at,failure_json=NULL,updated_at=excluded.updated_at").run(mailboxId, timestamp, timestamp, timestamp);
      }
      this.#db.prepare("DELETE FROM mailbox_sync_incremental_continuations WHERE mailbox_ref_id=? AND direction='inbound'").run(mailboxId);
    });
  }
  recordFailure(mailboxId: string, code: string): void {
    const allowed = new Set(["gmail_read_timeout", "gmail_read_transport_failed", "gmail_read_response_too_large", "gmail_read_response_invalid_json", "gmail_authorization_required", "gmail_mailbox_unavailable", "gmail_account_mismatch", "gmail_connection_missing", "gmail_reply_sync_stopped"]);
    const safe = allowed.has(code) || /^gmail_read_http_\d{3}(?:_(?:retry_after|rateLimitExceeded|userRateLimitExceeded|dailyLimitExceeded|domainPolicy|insufficientPermissions|forbidden))?$/u.test(code) ? code : "mailbox_sync_failed";
    this.#write(() => {
      const timestamp = Math.floor(Date.now() / 1000);
      const failure = JSON.stringify({ error_class: "sync_run_failed", message: safe, missing_provider_message_ids: [] });
      this.#db.prepare("INSERT INTO mailbox_sync_state(mailbox_ref_id,direction,newest_provider_message_id,newest_occurred_at,oldest_backfill_cursor,backfill_complete,last_page_at,last_full_sync_at,failure_json,updated_at) VALUES (?,'inbound',NULL,NULL,NULL,0,?,NULL,?,?) ON CONFLICT(mailbox_ref_id,direction) DO UPDATE SET failure_json=excluded.failure_json,updated_at=excluded.updated_at").run(mailboxId, timestamp, failure, timestamp);
    });
  }
}
