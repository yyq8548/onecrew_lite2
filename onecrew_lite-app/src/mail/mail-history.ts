import { createHash } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { validateCreatorSchema } from "../storage/creator-schema.ts";
import type { GmailBulkRepositoryOwner } from "./gmail-bulk-account-binding.ts";

const string = z.string().refine(value => value.isWellFormed());
const number = z.number().int().nonnegative().safe();
const nullable = string.nullable();
const direction = z.enum(["inbound", "outbound"]);
const timestampFields = { created_at: number, updated_at: number };
const mailboxSchema = z.object({ mailbox_ref_id: string, provider: z.enum(["resend", "gmail"]), label: string, email_address: string, display_name: nullable, capabilities_json: string, available: z.union([z.literal(0), z.literal(1)]), last_error: nullable, ...timestampFields });
const threadSchema = z.object({ id: string, mailbox_ref_id: string, creator_id: nullable, contact_route_id: nullable, provider_thread_key: string, subject: string, unread_count: number.max(4294967295), active_folder: z.enum(["inbox", "sent"]), archived_at: number.nullable(), deleted_at: number.nullable(), has_inbound: z.union([z.literal(0), z.literal(1)]), ...timestampFields });
const messageSchema = z.object({ id: string, thread_id: string, provider_message_id: string, internet_message_id: nullable, direction, subject: string, text: nullable, html: nullable, in_reply_to: nullable, references_json: string, headers_json: string, delivery_status: string, occurred_at: number });
const templateSchema = z.object({ id: string, name: string, subject: string, text: string, html: nullable, variables_json: string, revision: number.min(1), ...timestampFields });
const draftSchema = z.object({ id: string, mailbox_ref_id: string, to: string, creator_id: nullable, contact_route_id: nullable, reply_to_email_id: nullable, subject: string, text: string, html: nullable, template_id: nullable, template_variables_json: string, state: z.enum(["draft", "trashed"]), pending_send_idempotency_key: nullable, revision: number.min(1), ...timestampFields });
export const historyPageInput = z.object({ limit: z.number().int().min(0).max(4294967295).nullish(), cursor: string.nullish() }).strict();
export const historyThreadListInput = historyPageInput.extend({ mailbox_ref_id: string.nullish(), folder: z.enum(["all", "inbox", "sent", "drafts", "archive", "deleted"]).default("all"), sort: z.enum(["newest", "oldest"]) });
export const historyDraftListInput = historyPageInput.extend({ mailbox_ref_id: string.nullish(), state: z.enum(["draft", "trashed"]).nullish() });
export type HistoryPageInput = z.infer<typeof historyPageInput>;
const trim = (value: string) => value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
const preview = (value: string) => [...trim(value)].slice(0, 200).join("");
const effective = (limit: number | null | undefined, initial = 25, max = 100) => Math.min(max, Math.max(1, limit ?? initial));
export interface HistoryPage<T> { items: T[]; next_cursor?: string }

export class MailHistoryRepository {
  readonly #db: DatabaseSync;
  readonly owner: Readonly<GmailBulkRepositoryOwner>;
  readonly #scope: string;
  constructor(db: DatabaseSync, owner: GmailBulkRepositoryOwner) {
    validateCreatorSchema(db);
    this.owner = Object.freeze(z.object({ accountId: string.min(1), tenantId: string.min(1), baseId: string.min(1) }).strict().parse(owner));
    this.#scope = createHash("sha256").update(JSON.stringify([this.owner.accountId, this.owner.tenantId, this.owner.baseId])).digest("hex");
    this.#db = db;
  }
  #read<T>(operation: () => T): T {
    this.#db.exec("BEGIN DEFERRED");
    let committed = false;
    try { const value = operation(); this.#db.exec("COMMIT"); committed = true; return value; }
    finally { if (!committed && this.#db.isTransaction) this.#db.exec("ROLLBACK"); }
  }
  #position(cursor: string | null | undefined, context: string): [number | null, string | null] {
    if (cursor == null) return [null, null];
    const encoded = trim(cursor);
    if (!/^[A-Za-z0-9_-]+$/u.test(encoded)) throw new Error("page cursor is not valid for this query");
    const bytes = Buffer.from(encoded, "base64url");
    if (bytes.toString("base64url") !== encoded) throw new Error("page cursor is not valid for this query");
    const payload = z.object({ v: z.literal(1), context: z.literal(context), position: string, scope: z.literal(this.#scope) }).parse(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)));
    const parts = payload.position.split("|");
    if (parts.length !== 2 || !/^\+?\d+$/u.test(parts[0]!)) throw new Error("page cursor is not valid for this query");
    return [number.parse(Number(parts[0])), parts[1]!];
  }
  #page<T>(rows: T[], limit: number, context: string, key: (row: T) => [number, string]): HistoryPage<T> {
    const items = rows.slice(0, limit);
    if (rows.length <= limit) return { items };
    const [at, id] = key(items.at(-1)!);
    return { items, next_cursor: Buffer.from(JSON.stringify({ v: 1, context, position: `${at}|${id}`, scope: this.#scope })).toString("base64url") };
  }
  #mailbox(id: string) {
    const row = this.#db.prepare("SELECT id AS mailbox_ref_id,provider,label,email_address,display_name,capabilities_json,available,last_error,created_at,updated_at FROM mailbox_refs WHERE id=?").get(id);
    if (!row) throw new Error("mailbox reference not found");
    const { capabilities_json, last_error, available, ...detail } = mailboxSchema.parse(row);
    return { ...detail, capabilities: z.array(z.enum(["send", "sent_readback", "inbox", "reply"])).parse(JSON.parse(capabilities_json)), available: available === 1, issues: [...(available === 0 ? ["unavailable"] : []), ...(last_error !== null ? ["recorded_failure"] : [])] };
  }
  mailboxes() { return this.#read(() => this.#db.prepare("SELECT id FROM mailbox_refs ORDER BY available DESC,label COLLATE NOCASE,id").all().map(row => this.#mailbox(string.parse(row.id)))); }
  mailbox(id: string) { return this.#read(() => this.#mailbox(id)); }
  mailboxGet(id: string) { return this.#read(() => this.#db.prepare("SELECT id FROM mailbox_refs WHERE id=?").get(id) ? { status: "found" as const, detail: this.#mailbox(id) } : { status: "not_found" as const, mailbox_ref_id: id }); }
  #thread(id: string) {
    const row = this.#db.prepare("SELECT t.*,EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=t.id AND e.direction='inbound') AS has_inbound FROM email_threads t WHERE t.id=?").get(id);
    if (!row) throw new Error("email thread not found");
    const { active_folder, archived_at, deleted_at, has_inbound, ...thread } = threadSchema.parse(row);
    return { ...thread, folder: deleted_at !== null ? "deleted" : archived_at !== null && has_inbound === 1 ? "archive" : active_folder };
  }
  #creatorName(id: string | null): string | null {
    if (id === null) return null;
    const row = this.#db.prepare("SELECT display_name FROM creators WHERE id=?").get(id);
    if (!row) throw new Error("email thread Creator is missing");
    return string.parse(row.display_name);
  }
  #message(id: string) {
    const row = this.#db.prepare("SELECT id,thread_id,provider_message_id,internet_message_id,direction,subject,body_text AS text,body_html AS html,in_reply_to,references_json,headers_json,delivery_status,occurred_at FROM emails WHERE id=?").get(id);
    if (!row) throw new Error("email message not found");
    const { references_json, headers_json, ...message } = messageSchema.parse(row);
    z.json().parse(JSON.parse(headers_json));
    const recipients = this.#db.prepare("SELECT recipient_kind,address FROM email_recipients WHERE email_id=? ORDER BY recipient_kind,address").all(id).map(row => z.object({ recipient_kind: z.enum(["from", "to", "cc", "bcc", "received_for", "reply_to"]), address: string }).parse(row));
    const addresses = (kind: string) => recipients.filter(row => row.recipient_kind === kind).map(row => row.address);
    return { ...message, from: addresses("from")[0] ?? "", to: addresses("to"), cc: addresses("cc"), reply_to: addresses("reply_to"), references: z.array(string).parse(JSON.parse(references_json)) };
  }
  message(id: string) { return this.#read(() => this.#message(id)); }
  folders(mailbox: string | null = null) {
    return this.#read(() => {
      const count = (predicate: string) => number.parse(this.#db.prepare(`SELECT COUNT(*) AS n FROM email_threads WHERE (? IS NULL OR mailbox_ref_id=?) AND (${predicate})`).get(mailbox, mailbox)!.n);
      const drafts = (state: string) => number.parse(this.#db.prepare("SELECT COUNT(*) AS n FROM email_drafts WHERE (? IS NULL OR mailbox_ref_id=?) AND state=?").get(mailbox, mailbox, state)!.n);
      const inbound = "EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=email_threads.id AND e.direction='inbound')";
      return [
        { folder: "all", count: count("deleted_at IS NULL") },
        { folder: "inbox", count: count(`archived_at IS NULL AND deleted_at IS NULL AND ${inbound}`) },
        { folder: "sent", count: count("deleted_at IS NULL AND EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=email_threads.id AND e.direction='outbound')") },
        { folder: "drafts", count: drafts("draft") },
        { folder: "archive", count: count(`archived_at IS NOT NULL AND deleted_at IS NULL AND ${inbound}`) },
        { folder: "deleted", count: count("deleted_at IS NOT NULL") + drafts("trashed") },
      ];
    });
  }
  threads(input: z.infer<typeof historyThreadListInput>) {
    const parsed = historyThreadListInput.parse(input);
    const mailbox = parsed.mailbox_ref_id ?? null;
    const context = `email_thread_list|mailbox=${mailbox ?? "<all>"}|folder=${parsed.folder}|sort=${parsed.sort}`;
    const [at, lastId] = this.#position(parsed.cursor, context);
    const limit = effective(parsed.limit);
    const comparison = parsed.sort === "newest" ? "<" : ">";
    const order = parsed.sort === "newest" ? "DESC" : "ASC";
    return this.#read(() => {
      const rows = this.#db.prepare(`WITH thread_activity AS (SELECT t.id,COALESCE(MAX(e.occurred_at),t.created_at) AS latest_message_at FROM email_threads t LEFT JOIN emails e ON e.thread_id=t.id WHERE (? IS NULL OR t.mailbox_ref_id=?) AND ((?='all' AND t.deleted_at IS NULL) OR (?='inbox' AND t.archived_at IS NULL AND t.deleted_at IS NULL AND EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=t.id AND e.direction='inbound')) OR (?='sent' AND t.deleted_at IS NULL AND EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=t.id AND e.direction='outbound')) OR (?='archive' AND t.archived_at IS NOT NULL AND t.deleted_at IS NULL AND EXISTS(SELECT 1 FROM emails e WHERE e.thread_id=t.id AND e.direction='inbound')) OR (?='deleted' AND t.deleted_at IS NOT NULL)) GROUP BY t.id) SELECT id FROM thread_activity WHERE (? IS NULL OR latest_message_at ${comparison} ? OR (latest_message_at=? AND id ${comparison} ?)) ORDER BY latest_message_at ${order},id ${order} LIMIT ?`).all(mailbox, mailbox, parsed.folder, parsed.folder, parsed.folder, parsed.folder, parsed.folder, at, at, at, lastId, limit + 1);
      const summaries = rows.map(row => {
        const thread = this.#thread(string.parse(row.id));
        const latestId = this.#db.prepare("SELECT id FROM emails WHERE thread_id=? ORDER BY occurred_at DESC,id DESC LIMIT 1").get(thread.id);
        const message = latestId ? this.#message(string.parse(latestId.id)) : null;
        const latest = message === null ? null : { id: message.id, direction: message.direction, from: message.from, to: message.to, reply_to: message.reply_to, subject: message.subject, preview_text: message.text === null ? null : preview(message.text), has_html: message.html !== null, delivery_status: message.delivery_status, occurred_at: message.occurred_at };
        const triageRow = this.#db.prepare("SELECT t.intent,t.summary,t.suggested_reply,t.created_at,t.acknowledged_at FROM reply_triage t JOIN emails e ON e.id=t.email_id WHERE e.thread_id=? ORDER BY t.created_at DESC,t.id DESC LIMIT 1").get(thread.id);
        const triage = triageRow ? z.object({ intent: z.enum(["interested", "question", "negotiating", "declined", "irrelevant", "other"]), summary: string, suggested_reply: nullable, created_at: number, acknowledged_at: number.nullable() }).parse(triageRow) : null;
        return { thread, latest_message: latest, message_count: number.parse(this.#db.prepare("SELECT COUNT(*) AS n FROM emails WHERE thread_id=?").get(thread.id)!.n), has_outbound: this.#db.prepare("SELECT 1 FROM emails WHERE thread_id=? AND direction='outbound' LIMIT 1").get(thread.id) !== undefined, creator_display_name: this.#creatorName(thread.creator_id), ...(triage === null ? {} : { triage: { intent: triage.intent, summary: triage.summary, ...(triage.suggested_reply === null ? {} : { suggested_reply: triage.suggested_reply }), created_at: triage.created_at, acknowledged: triage.acknowledged_at !== null } }) };
      });
      return this.#page(summaries, limit, context, row => [row.latest_message?.occurred_at ?? row.thread.created_at, row.thread.id]);
    });
  }
  thread(input: { thread_id: string; limit?: number | null | undefined; cursor?: string | null | undefined }) {
    const context = `email_thread_messages|thread=${input.thread_id}`;
    const [at, id] = this.#position(input.cursor, context);
    const limit = effective(input.limit, 50, 200);
    return this.#read(() => {
      const thread = this.#thread(input.thread_id);
      const messages = this.#db.prepare("SELECT id FROM emails WHERE thread_id=? AND (? IS NULL OR occurred_at<? OR (occurred_at=? AND id<?)) ORDER BY occurred_at DESC,id DESC LIMIT ?").all(thread.id, at, at, at, id, limit + 1).map(row => this.#message(string.parse(row.id)));
      const page = this.#page(messages, limit, context, row => [row.occurred_at, row.id]);
      return { mailbox: this.#mailbox(thread.mailbox_ref_id), thread, messages: page.items, ...(page.next_cursor ? { next_message_cursor: page.next_cursor } : {}), creator_display_name: this.#creatorName(thread.creator_id) };
    });
  }
  #template(id: string) {
    const row = this.#db.prepare("SELECT id,name,subject_template AS subject,text_template AS text,html_template AS html,variables_json,revision,created_at,updated_at FROM email_templates WHERE id=?").get(id);
    if (!row) throw new Error("email template not found");
    const { variables_json, ...template } = templateSchema.parse(row);
    return { ...template, variables: z.array(string).parse(JSON.parse(variables_json)) };
  }
  template(id: string) { return this.#read(() => this.#template(id)); }
  #draft(id: string) {
    const row = this.#db.prepare("SELECT id,mailbox_ref_id,to_address AS `to`,creator_id,contact_route_id,reply_to_email_id,subject,body_text AS text,body_html AS html,template_id,template_variables_json,state,sending_idempotency_key AS pending_send_idempotency_key,revision,created_at,updated_at FROM email_drafts WHERE id=?").get(id);
    if (!row) throw new Error("email draft not found");
    const { template_variables_json, ...draft } = draftSchema.parse(row);
    return { ...draft, sending: draft.pending_send_idempotency_key !== null, template_variables: z.record(string, string).parse(JSON.parse(template_variables_json)) };
  }
  draft(id: string) { return this.#read(() => this.#draft(id)); }
  templates(input: HistoryPageInput = {}) {
    const parsed = historyPageInput.parse(input);
    const context = "email_template_list";
    const [at, id] = this.#position(parsed.cursor, context);
    const limit = effective(parsed.limit);
    return this.#read(() => {
      const rows = this.#db.prepare("SELECT id FROM email_templates WHERE (? IS NULL OR updated_at<? OR (updated_at=? AND id<?)) ORDER BY updated_at DESC,id DESC LIMIT ?").all(at, at, at, id, limit + 1).map(row => {
        const { text: _text, html: _html, ...summary } = this.#template(string.parse(row.id));
        return summary;
      });
      return this.#page(rows, limit, context, row => [row.updated_at, row.id]);
    });
  }
  drafts(input: z.infer<typeof historyDraftListInput>) {
    const parsed = historyDraftListInput.parse(input);
    const mailbox = parsed.mailbox_ref_id ?? null;
    const state = parsed.state ?? null;
    const context = `email_draft_list|mailbox=${mailbox ?? "<all>"}|state=${state ?? ""}`;
    const [at, id] = this.#position(parsed.cursor, context);
    const limit = effective(parsed.limit);
    return this.#read(() => {
      const rows = this.#db.prepare("SELECT id FROM email_drafts WHERE (? IS NULL OR mailbox_ref_id=?) AND (? IS NULL OR state=?) AND (? IS NULL OR updated_at<? OR (updated_at=? AND id<?)) ORDER BY updated_at DESC,id DESC LIMIT ?").all(mailbox, mailbox, state, state, at, at, at, id, limit + 1).map(row => {
        const draft = this.#draft(string.parse(row.id));
        return { id: draft.id, mailbox_ref_id: draft.mailbox_ref_id, to: draft.to, creator_id: draft.creator_id, contact_route_id: draft.contact_route_id, subject: draft.subject, preview_text: preview(draft.text), has_html: draft.html !== null, state: draft.state, sending: draft.sending, revision: draft.revision, created_at: draft.created_at, updated_at: draft.updated_at };
      });
      return this.#page(rows, limit, context, row => [row.updated_at, row.id]);
    });
  }
  syncStatus(mailbox: string) {
    return this.#read(() => {
      const state = (dir: string) => {
        const row = this.#db.prepare("SELECT backfill_complete,last_page_at,last_full_sync_at,failure_json FROM mailbox_sync_state WHERE mailbox_ref_id=? AND direction=?").get(mailbox, dir);
        if (!row) return null;
        const parsed = z.object({ backfill_complete: z.union([z.literal(0), z.literal(1)]), last_page_at: number.nullable(), last_full_sync_at: number.nullable(), failure_json: nullable }).parse(row);
        return { backfill_complete: parsed.backfill_complete === 1, last_page_at: parsed.last_page_at, last_full_sync_at: parsed.last_full_sync_at, failure: parsed.failure_json === null ? null : z.object({ missing_provider_message_ids: z.array(string), error_class: string, message: string }).parse(JSON.parse(parsed.failure_json)) };
      };
      return { mailbox: this.#mailbox(mailbox), running: false, progress: null, inbound: state("inbound"), outbound: state("outbound") };
    });
  }
  syncReadback(mailbox: string) {
    return this.#read(() => {
      const state = (dir: string) => {
        const row = this.#db.prepare("SELECT backfill_complete,last_page_at,last_full_sync_at,failure_json FROM mailbox_sync_state WHERE mailbox_ref_id=? AND direction=?").get(mailbox, dir);
        if (!row) return null;
        const parsed = z.object({ backfill_complete: z.union([z.literal(0), z.literal(1)]), last_page_at: number.nullable(), last_full_sync_at: number.nullable(), failure_json: nullable }).parse(row);
        return { backfill_complete: parsed.backfill_complete === 1, last_page_at: parsed.last_page_at, last_full_sync_at: parsed.last_full_sync_at, failure: parsed.failure_json === null ? null : z.object({ missing_provider_message_ids: z.array(string), error_class: string, message: string }).parse(JSON.parse(parsed.failure_json)) };
      };
      const from = "FROM emails email JOIN email_threads thread ON thread.id=email.thread_id LEFT JOIN reply_triage triage ON triage.email_id=email.id WHERE thread.mailbox_ref_id=? AND email.direction='inbound' AND triage.id IS NULL";
      const total = number.max(4294967295).parse(this.#db.prepare(`SELECT COUNT(*) AS total ${from}`).get(mailbox)!.total);
      const items = this.#db.prepare(`SELECT email.id,thread.creator_id,email.subject,email.body_text,email.occurred_at,(SELECT address FROM email_recipients WHERE email_id=email.id AND recipient_kind='from' LIMIT 1) AS sender ${from} ORDER BY email.occurred_at DESC,email.rowid DESC LIMIT 20`).all(mailbox).map(row => {
        const parsed = z.object({ id: string, creator_id: nullable, subject: string, body_text: nullable, occurred_at: number, sender: nullable }).parse(row);
        const body = (parsed.body_text ?? "").split(/\p{White_Space}+/u).filter(Boolean).join(" ");
        const excerpt = [...body].length <= 160 ? body : `${[...body].slice(0, 160).join("")}…`;
        return { channel: "email" as const, email_id: parsed.id, ...(parsed.creator_id === null ? {} : { creator_id: parsed.creator_id }), from: parsed.sender ?? "", subject: trim(parsed.subject) ? parsed.subject : null, excerpt, occurred_at: parsed.occurred_at };
      });
      return { mailbox: this.#mailbox(mailbox), inbound: state("inbound"), outbound: state("outbound"), pending_triage: { items, total } };
    });
  }
  outboundHistory(mailbox: string | null, input: HistoryPageInput = {}) {
    const parsed = historyPageInput.parse(input);
    const context = `email_outbound_history|mailbox=${mailbox ?? "<all>"}`;
    const [at, id] = this.#position(parsed.cursor, context);
    const limit = effective(parsed.limit);
    return this.#read(() => {
      const rows = this.#db.prepare("SELECT key,action,mailbox_ref_id,state,created_at,updated_at FROM idempotency_records WHERE action IN ('email_send','manual_email_send','email_draft_send','email_batch_send') AND (? IS NULL OR mailbox_ref_id=?) AND (? IS NULL OR updated_at<? OR (updated_at=? AND key<?)) ORDER BY updated_at DESC,key DESC LIMIT ?").all(mailbox, mailbox, at, at, at, id, limit + 1).map(row => z.object({ key: string, action: string, mailbox_ref_id: nullable, state: z.enum(["in_progress", "remote_started", "remote_accepted", "remote_verified", "completed", "failed"]), ...timestampFields }).parse(row));
      return this.#page(rows, limit, context, row => [row.updated_at, row.key]);
    });
  }
}
