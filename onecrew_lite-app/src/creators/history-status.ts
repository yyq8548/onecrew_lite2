import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { parseApplicationJson } from "@onecrew/runtime-contracts/json";
import { trim } from "./contracts.ts";
import { historyU64 } from "./history-contracts.ts";

const operation = z.enum(["relationship_update", "content_reply_send", "direct_message_send"]);
const text = z.string(), optional = text.nullable();
const identity = z.strictObject({ operation, creator_id: text, operator_account_ref_id: text, platform: z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]), target_platform_account_id: text, target_kind: z.enum(["user", "bot", "group", "channel", "supergroup", "creator", "business"]), external_target_id: text, contact_route_id: optional, resolved_target_id: optional, conversation: z.strictObject({ conversation_id: text, provider_thread_id: text }).nullable(), content_id: optional, reply_to_id: optional, relationship_action: z.enum(["follow", "unfollow"]).nullable(), allow_unfollowed_recipient: z.boolean() });
const receipt = z.discriminatedUnion("kind", [z.strictObject({ kind: z.literal("relationship") }), z.strictObject({ kind: z.literal("content_reply"), external_reply_id: text }), z.strictObject({ kind: z.literal("direct_message"), external_message_id: text, provider_thread_id: optional })]);
const relationship = z.enum(["yes", "no", "unknown"]);
const verification = z.discriminatedUnion("kind", [z.strictObject({ kind: z.literal("relationship"), following: relationship, can_message: relationship, checked_at: historyU64 }), z.strictObject({ kind: z.literal("content_reply"), external_reply_id: text }), z.strictObject({ kind: z.literal("direct_message"), external_message_id: text, provider_thread_id: text, sent_at: historyU64 })]);
const record = z.discriminatedUnion("kind", [z.strictObject({ kind: z.literal("relationship"), relationship_id: text, can_message_id: text, touchpoint_id: text, audit_event_id: text }), z.strictObject({ kind: z.literal("content_reply"), external_reply_id: text, touchpoint_id: text, audit_event_id: text }), z.strictObject({ kind: z.literal("direct_message"), conversation_id: text, message_id: text, touchpoint_id: text, audit_event_id: text })]);
const stateSchema = z.strictObject({ operation, identity, fingerprint: text.regex(/^sha256:v1:[0-9a-f]{64}$/u), claimed_at: historyU64, phase: z.enum(["remote_started", "remote_accepted", "record_pending", "completed", "refused"]), receipt: receipt.nullable(), verification: verification.nullable(), record: record.nullable() });
const phases = { remote_started: "remote_started", remote_accepted: "remote_accepted", record_pending: "remote_verified", completed: "completed", refused: "failed" };
function assert(value: boolean): asserts value { if (!value) throw new Error("Creator outreach storage is invalid"); }
export function historyActionStatus(db: DatabaseSync, key: string) {
  const statement = db.prepare("SELECT action,request_fingerprint,state,result_json,contact_route_id,created_at,CASE WHEN json_valid(result_json) THEN json_extract(result_json,'$.kind') END AS marker FROM idempotency_records WHERE key=?"); statement.setReadBigInts(true);
  const row = statement.get(key); if (!row) return null;
  if (operation.safeParse(row.action).success) return { kind: "retired" as const, operation: operation.parse(row.action), phase: row.marker === "retired_action_unresolved" ? "retired_action_unresolved" : row.state === "completed" ? "completed" : row.state === "failed" ? "failed" : "retired_action_unresolved", marked: row.marker === "retired_action_unresolved" };
  const expected = operation.parse(z.string().parse(row.action).replace(/_v1$/u, ""));
  assert(row.action === `${expected}_v1`);
  const payload = text.parse(row.result_json); assert(Buffer.byteLength(payload) <= 64 * 1024);
  const envelope = z.strictObject({ version: z.union([z.literal(1), z.literal(1n)]), state: stateSchema }).parse(parseApplicationJson(payload)), state = envelope.state, owner = state.identity;
  assert(state.operation === expected && owner.operation === expected && row.request_fingerprint === state.fingerprint && row.state === phases[state.phase] && historyU64.parse(row.created_at) === state.claimed_at && row.contact_route_id === owner.contact_route_id);
  assert([owner.creator_id, owner.operator_account_ref_id, owner.target_platform_account_id, owner.external_target_id].every(value => trim(value).length > 0));
  const route = owner.contact_route_id !== null && !!trim(owner.contact_route_id) && owner.resolved_target_id !== null && !!trim(owner.resolved_target_id);
  assert(owner.reply_to_id === null || !!trim(owner.reply_to_id));
  assert(owner.conversation === null || !!trim(owner.conversation.conversation_id) && !!trim(owner.conversation.provider_thread_id));
  if (expected === "relationship_update") assert(route && owner.relationship_action !== null && owner.content_id === null && owner.reply_to_id === null && owner.conversation === null && !owner.allow_unfollowed_recipient && ["x", "instagram"].includes(owner.platform));
  if (expected === "content_reply_send") assert(!route && owner.contact_route_id === null && owner.resolved_target_id === null && owner.relationship_action === null && owner.content_id !== null && !!trim(owner.content_id) && owner.conversation === null && !owner.allow_unfollowed_recipient && owner.platform === "x" && owner.target_kind === "user");
  if (expected === "direct_message_send") assert(route && owner.relationship_action === null && owner.content_id === null && owner.reply_to_id === null && ["x", "instagram", "telegram"].includes(owner.platform));
  const has = [state.receipt !== null, state.verification !== null, state.record !== null], required = state.phase === "remote_accepted" ? [true, false, false] : state.phase === "record_pending" ? [true, true, false] : state.phase === "completed" ? [true, true, true] : [false, false, false];
  assert(has.every((value, index) => value === required[index]));
  const kind = expected === "relationship_update" ? "relationship" : expected === "content_reply_send" ? "content_reply" : "direct_message";
  if (state.receipt) {
    assert(state.receipt.kind === kind);
    if (state.receipt.kind === "content_reply") assert(!!trim(state.receipt.external_reply_id));
    if (state.receipt.kind === "direct_message") { assert(!!trim(state.receipt.external_message_id) && (state.receipt.provider_thread_id === null || !!trim(state.receipt.provider_thread_id))); if (owner.conversation && state.receipt.provider_thread_id !== null) assert(state.receipt.provider_thread_id === owner.conversation.provider_thread_id); }
  }
  if (state.verification) {
    assert(state.verification.kind === kind);
    if (state.verification.kind === "relationship") assert(state.verification.following === (owner.relationship_action === "follow" ? "yes" : "no"));
    if (state.verification.kind === "content_reply") assert(state.receipt?.kind === "content_reply" && state.receipt.external_reply_id === state.verification.external_reply_id && !!trim(state.verification.external_reply_id));
    if (state.verification.kind === "direct_message") assert(state.receipt?.kind === "direct_message" && state.receipt.external_message_id === state.verification.external_message_id && !!trim(state.verification.external_message_id) && !!trim(state.verification.provider_thread_id) && (state.receipt.provider_thread_id === null || state.receipt.provider_thread_id === state.verification.provider_thread_id) && (owner.conversation === null || owner.conversation.provider_thread_id === state.verification.provider_thread_id));
  }
  if (state.record) {
    assert(state.record.kind === kind);
    assert(Object.entries(state.record).every(([key, value]) => key === "kind" || !!value));
    if (state.record.kind === "content_reply") assert(state.verification?.kind === "content_reply" && state.record.external_reply_id === state.verification.external_reply_id);
  }
  return { kind: "current" as const, state };
}
