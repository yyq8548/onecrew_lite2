import { randomUUID } from "node:crypto";
import type { DatabaseSync, SQLInputValue } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "@onecrew/runtime-contracts/json";
import { validateCreatorSchema90 } from "../storage/creator-schema.ts";
import { trim } from "./contracts.ts";
import { historyChannel, historyConversationListInput, historyCursor, historyExcerpt, historyPage, historyTriage, historyTriageListInput, historyTriageRecordInput, historyU64 } from "./history-contracts.ts";
import { historyActionStatus } from "./history-status.ts";

type Row = Record<string, unknown>;
const string = z.string(), nullable = string.nullable();
const platform = z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]);
const targetKind = z.enum(["user", "bot", "group", "channel", "supergroup", "creator", "business"]);
function triage(row: Row) {
  return historyTriage.parse({ ...row, email_id: row.email_id ?? undefined, message_id: row.message_id ?? undefined, suggested_reply: row.suggested_reply ?? undefined, acknowledged_at: row.acknowledged_at ?? undefined });
}
function account(row: Row) {
  return { id: string.parse(row.id), platform: platform.parse(row.platform), kind: z.enum(["profile", "service_credential"]).parse(row.kind), label: string.parse(row.label), active: z.union([z.literal(0n), z.literal(1n)]).parse(row.active) === 1n, availability: z.enum(["available", "unavailable"]).parse(row.availability), created_at: historyU64.parse(row.created_at), updated_at: historyU64.parse(row.updated_at) };
}
const triageColumns = "id,channel,email_id,message_id,intent,summary,suggested_reply,created_at,acknowledged_at";
export class CreatorHistoryRepository {
  readonly #db: DatabaseSync;
  constructor(db: DatabaseSync) { validateCreatorSchema90(db); this.#db = db; }
  #rows(sql: string, ...parameters: SQLInputValue[]): Row[] { const statement = this.#db.prepare(sql); statement.setReadBigInts(true); return statement.all(...parameters); }
  #row(sql: string, ...parameters: SQLInputValue[]): Row | undefined { return this.#rows(sql, ...parameters)[0]; }
  #snapshot<T>(write: boolean, operation: () => T): T {
    this.#db.exec(write ? "BEGIN IMMEDIATE" : "BEGIN DEFERRED");
    try { const value = operation(); this.#db.exec("COMMIT"); return value; }
    finally { if (this.#db.isTransaction) this.#db.exec("ROLLBACK"); }
  }
  actionStatus(key: string) { return this.#snapshot(false, () => historyActionStatus(this.#db, key)); }
  lastContact(creatorId: string, routeId: string) {
    return this.#snapshot(false, () => {
      if (!this.#row("SELECT id FROM creator_contact_routes WHERE id=? AND creator_id=?", routeId, creatorId)) throw new Error("contact route does not belong to the selected Creator");
      const row = this.#row(`SELECT account.id,account.platform,account.kind,account.label,account.active,account.availability,account.created_at,account.updated_at,conversation.channel,message.sent_at
        FROM creator_contact_routes route JOIN contact_locators locator ON locator.id=route.locator_id
        JOIN locator_resolutions resolution ON resolution.locator_id=route.locator_id
        JOIN resolved_targets target ON target.id=resolution.resolved_target_id AND target.account_ref_id=resolution.account_ref_id
        JOIN conversations conversation ON conversation.account_ref_id=target.account_ref_id AND conversation.resolved_target_id=target.id AND conversation.channel=locator.channel
        JOIN conversation_contact_routes conversation_owner ON conversation_owner.conversation_id=conversation.id AND conversation_owner.contact_route_id=route.id
        JOIN messages message ON message.conversation_id=conversation.id JOIN account_refs account ON account.id=conversation.account_ref_id
        WHERE route.id=? AND route.creator_id=? AND message.direction='outbound' AND message.status='sent' ORDER BY message.sent_at DESC,message.id ASC LIMIT 1`, routeId, creatorId);
      return row ? { contact_route_id: routeId, account_ref: account(row), channel: historyChannel.parse(row.channel), last_contact_at: historyU64.parse(row.sent_at) } : null;
    });
  }
  #avatar(creatorId: string): string | null {
    const keys = { x: ["profile_image_url"], instagram: ["profile_pic_url", "profile_pic_url_hd"], telegram: ["photo_url", "profile_photo_url"], youtube: ["thumbnail_url"], tiktok: ["profile_image"] };
    const rows = this.#rows(`SELECT account.platform,observation.public_json FROM platform_accounts account JOIN creator_account_links link ON link.platform_account_id=account.id
      LEFT JOIN platform_account_observations observation ON observation.id=(SELECT latest.id FROM platform_account_observations latest WHERE latest.platform_account_id=account.id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1)
      WHERE link.creator_id=? AND link.state!='rejected' ORDER BY account.created_at ASC,account.rowid ASC`, creatorId);
    for (const row of rows) {
      const kind = platform.parse(row.platform); if (row.public_json === null) continue;
      const data = parseApplicationJson(string.parse(row.public_json));
      if (data === null || typeof data !== "object" || Array.isArray(data)) continue;
      for (const key of ["avatar_url", ...keys[kind]]) { const value = Object.getOwnPropertyDescriptor(data, key)?.value; if (typeof value === "string" && trim(value)) return trim(value); }
    }
    return null;
  }
  #brief(conversationId: string) {
    const row = this.#row(`SELECT t.intent,t.summary,t.suggested_reply,t.created_at,t.acknowledged_at FROM reply_triage t JOIN messages m ON m.id=t.message_id WHERE m.conversation_id=? ORDER BY t.created_at DESC,t.id DESC LIMIT 1`, conversationId);
    return row ? { intent: historyTriageRecordInput.shape.intent.parse(row.intent), summary: string.parse(row.summary), ...(row.suggested_reply === null ? {} : { suggested_reply: string.parse(row.suggested_reply) }), created_at: historyU64.parse(row.created_at), acknowledged: row.acknowledged_at !== null } : null;
  }
  conversationList(raw: unknown) {
    const input = historyConversationListInput.parse(raw), query = input.query === null ? null : trim(trim(input.query).replace(/^@+/u, "")) || null;
    const context = `conversation_list|query=${query ?? ""}|status=${input.status ?? "any"}|channel=${input.channel ?? "any"}`, limit = Math.max(1, Math.min(100, input.limit)), [at, id] = historyCursor(input.cursor, context);
    return this.#snapshot(false, () => {
      const rows = this.#rows(`SELECT conversation.id,conversation.channel,conversation.provider_thread_id,conversation.account_ref_id,conversation.resolved_target_id,conversation.updated_at,
        account.label,route.id AS contact_route_id,route.creator_id,creator.display_name,last_message.direction,last_message.body,last_message.sent_at,peer_observation.handle
        FROM conversations conversation JOIN account_refs account ON account.id=conversation.account_ref_id
        JOIN creator_contact_routes route ON route.id=(SELECT link.contact_route_id FROM conversation_contact_routes link WHERE link.conversation_id=conversation.id ORDER BY link.created_at ASC,link.contact_route_id ASC LIMIT 1)
        JOIN creators creator ON creator.id=route.creator_id LEFT JOIN resolved_targets peer ON peer.id=conversation.resolved_target_id
        LEFT JOIN platform_account_observations peer_observation ON peer_observation.id=(SELECT latest.id FROM platform_account_observations latest WHERE latest.platform_account_id=peer.platform_account_id ORDER BY latest.observed_at DESC,latest.rowid DESC LIMIT 1)
        LEFT JOIN messages last_message ON last_message.id=(SELECT id FROM messages WHERE conversation_id=conversation.id ORDER BY sent_at DESC,rowid DESC LIMIT 1)
        WHERE (?1 IS NULL OR instr(lower(creator.display_name),lower(?1))>0 OR instr(lower(account.label),lower(?1))>0 OR (peer_observation.handle IS NOT NULL AND instr(lower(peer_observation.handle),lower(?1))>0))
        AND (?2 IS NULL OR conversation.channel=?2) AND (?3 IS NULL OR (?3='waiting_reply' AND last_message.direction='inbound') OR (?3='replied' AND last_message.direction='outbound'))
        AND (?4 IS NULL OR conversation.updated_at<?4 OR(conversation.updated_at=?4 AND conversation.id<?5)) ORDER BY conversation.updated_at DESC,conversation.id DESC LIMIT ?6`, query, input.channel, input.status, at, id, limit + 1);
      const values = rows.map(row => {
        const conversationId = string.parse(row.id), creatorId = string.parse(row.creator_id), avatar = this.#avatar(creatorId), brief = this.#brief(conversationId);
        return { conversation_id: conversationId, creator_id: creatorId, creator_display_name: string.parse(row.display_name), ...(avatar === null ? {} : { creator_avatar_url: avatar }), ...(row.handle === null ? {} : { peer_handle: string.parse(row.handle) }), channel: historyChannel.parse(row.channel), operator_account_ref_id: string.parse(row.account_ref_id), operator_account_label: string.parse(row.label), contact_route_id: string.parse(row.contact_route_id), resolved_target_id: string.parse(row.resolved_target_id), provider_thread_id: string.parse(row.provider_thread_id), ...(row.direction === null ? {} : { last_message: { direction: string.parse(row.direction), preview: Array.from(string.parse(row.body)).slice(0, 120).join(""), sent_at: historyU64.parse(row.sent_at) } }), ...(brief === null ? {} : { triage: brief }), updated_at: historyU64.parse(row.updated_at) };
      });
      return historyPage(values, limit, context, row => [row.updated_at, row.conversation_id]);
    });
  }
  conversationGet(conversationId: string) {
    return this.#snapshot(false, () => {
      const row = this.#row(`SELECT conversation.*,route.creator_id FROM conversations conversation LEFT JOIN creator_contact_routes route ON route.id=(SELECT link.contact_route_id FROM conversation_contact_routes link WHERE link.conversation_id=conversation.id ORDER BY link.created_at ASC,link.contact_route_id ASC LIMIT 1) WHERE conversation.id=?`, conversationId);
      if (!row) return null;
      if (row.creator_id === null) throw new Error("conversation is not linked to a contact route, so it has no Creator");
      const operator = this.#row("SELECT * FROM account_refs WHERE id=?", string.parse(row.account_ref_id)); if (!operator) throw new Error("account ref not found");
      const target = this.#row("SELECT * FROM resolved_targets WHERE id=?", string.parse(row.resolved_target_id)); if (!target) throw new Error("resolved target not found");
      const conversation = { id: conversationId, account_ref_id: string.parse(row.account_ref_id), resolved_target_id: string.parse(row.resolved_target_id), channel: historyChannel.parse(row.channel), provider_thread_id: string.parse(row.provider_thread_id), cursor: nullable.parse(row.cursor), unread_count: Number(z.bigint().min(0n).max(4294967295n).parse(row.unread_count)), created_at: historyU64.parse(row.created_at), updated_at: historyU64.parse(row.updated_at) };
      const messages = this.#rows("SELECT * FROM messages WHERE conversation_id=? ORDER BY sent_at ASC,id ASC", conversationId).map(message => ({ id: string.parse(message.id), conversation_id: conversationId, external_message_id: nullable.parse(message.external_message_id), sender_target_id: nullable.parse(message.sender_target_id), direction: string.parse(message.direction), body: string.parse(message.body), status: string.parse(message.status), public_data: parseApplicationJson(string.parse(message.public_json)), sent_at: historyU64.parse(message.sent_at) }));
      const pending = this.#rows(`SELECT message.id,message.body,message.sent_at FROM messages message LEFT JOIN reply_triage triage ON triage.message_id=message.id WHERE message.conversation_id=? AND message.direction='inbound' AND triage.id IS NULL ORDER BY message.sent_at DESC,message.rowid DESC LIMIT 20`, conversationId);
      const total = Number(z.bigint().min(0n).max(4294967295n).parse(this.#row("SELECT COUNT(*) AS n FROM messages message LEFT JOIN reply_triage triage ON triage.message_id=message.id WHERE message.conversation_id=? AND message.direction='inbound' AND triage.id IS NULL", conversationId)?.n));
      return { creator_id: string.parse(row.creator_id), operator_account: account(operator), target: { id: string.parse(target.id), account_ref_id: string.parse(target.account_ref_id), platform: platform.parse(target.platform), target_kind: targetKind.parse(target.target_kind), external_target_id: string.parse(target.external_target_id), platform_account_id: nullable.parse(target.platform_account_id), metadata: parseApplicationJson(string.parse(target.metadata_json)), created_at: historyU64.parse(target.created_at), updated_at: historyU64.parse(target.updated_at) }, conversation, messages, pending_triage: { items: pending.map(message => ({ channel: "dm" as const, message_id: string.parse(message.id), from: "", excerpt: historyExcerpt(string.parse(message.body)), occurred_at: historyU64.parse(message.sent_at) })), total } };
    });
  }
  #triageView(row: Row) {
    const value = triage(row);
    if (value.channel === "email") {
      if (!value.email_id) throw new Error("stored email triage has no email");
      const source = this.#row("SELECT e.thread_id,e.body_text,t.creator_id,c.display_name FROM emails e JOIN email_threads t ON t.id=e.thread_id LEFT JOIN creators c ON c.id=t.creator_id WHERE e.id=?", value.email_id);
      return { ...value, ...(source ? { email_thread_id: string.parse(source.thread_id), ...(source.creator_id === null ? {} : { creator_id: string.parse(source.creator_id) }), ...(source.display_name === null ? {} : { creator_name: string.parse(source.display_name) }) } : {}), excerpt: historyExcerpt(source?.body_text == null ? "" : string.parse(source.body_text)) };
    }
    if (!value.message_id) throw new Error("stored dm triage has no message");
    const source = this.#row(`SELECT m.conversation_id,m.body,route.creator_id,c.display_name FROM messages m LEFT JOIN creator_contact_routes route ON route.id=(SELECT link.contact_route_id FROM conversation_contact_routes link WHERE link.conversation_id=m.conversation_id ORDER BY link.created_at ASC,link.contact_route_id ASC LIMIT 1) LEFT JOIN creators c ON c.id=route.creator_id WHERE m.id=?`, value.message_id);
    return { ...value, ...(source ? { conversation_id: string.parse(source.conversation_id), ...(source.creator_id === null ? {} : { creator_id: string.parse(source.creator_id) }), ...(source.display_name === null ? {} : { creator_name: string.parse(source.display_name) }) } : {}), excerpt: historyExcerpt(source ? string.parse(source.body) : "") };
  }
  triageList(raw: unknown) {
    const input = historyTriageListInput.parse(raw), context = `reply_triage|acknowledged=${input.include_acknowledged}`, limit = Math.max(1, Math.min(100, input.limit)), [at, id] = historyCursor(input.cursor, context);
    return this.#snapshot(false, () => historyPage(this.#rows(`SELECT ${triageColumns} FROM reply_triage WHERE (?1 OR acknowledged_at IS NULL) AND (?2 IS NULL OR created_at<?2 OR(created_at=?2 AND id<?3)) ORDER BY created_at DESC,id DESC LIMIT ?4`, input.include_acknowledged ? 1 : 0, at, id, limit + 1).map(row => this.#triageView(row)), limit, context, row => [row.created_at, row.id]));
  }
  triageRecord(raw: unknown) {
    const input = historyTriageRecordInput.parse(raw), summary = trim(input.summary), suggestion = input.suggested_reply === null ? null : trim(input.suggested_reply) || null;
    if (!summary) throw new Error("reply triage summary is required");
    if ((input.email_id === null) === (input.message_id === null)) throw new Error("a reply triage names exactly one message: set email_id or message_id");
    return this.#snapshot(true, () => {
      const email = input.email_id !== null, source = this.#row(email ? "SELECT direction FROM emails WHERE id=?" : "SELECT direction FROM messages WHERE id=?", input.email_id ?? input.message_id!);
      if (!source) throw new Error(email ? "email not found" : "message not found");
      if (source.direction !== "inbound") throw new Error(email ? "only an inbound email can be triaged" : "only an inbound message can be triaged");
      const old = this.#row("SELECT id FROM reply_triage WHERE (email_id IS NOT NULL AND email_id=?1) OR(message_id IS NOT NULL AND message_id=?2)", input.email_id, input.message_id), id = old ? string.parse(old.id) : randomUUID();
      this.#db.prepare(`INSERT INTO reply_triage(${triageColumns}) VALUES(?,?,?,?,?,?,?,?,NULL) ON CONFLICT(id) DO UPDATE SET intent=excluded.intent,summary=excluded.summary,suggested_reply=excluded.suggested_reply,created_at=excluded.created_at,acknowledged_at=NULL`).run(id, email ? "email" : "dm", input.email_id, input.message_id, input.intent, summary, suggestion, Math.floor(Date.now() / 1000));
      return triage(this.#row(`SELECT ${triageColumns} FROM reply_triage WHERE id=?`, id)!);
    });
  }
  triageAcknowledge(id: string) {
    return this.#snapshot(true, () => {
      const row = this.#row(`SELECT ${triageColumns} FROM reply_triage WHERE id=?`, id); if (!row) throw new Error("reply triage not found");
      if (row.acknowledged_at !== null) return triage(row);
      this.#db.prepare("UPDATE reply_triage SET acknowledged_at=? WHERE id=?").run(Math.floor(Date.now() / 1000), id);
      return triage(this.#row(`SELECT ${triageColumns} FROM reply_triage WHERE id=?`, id)!);
    });
  }
}
