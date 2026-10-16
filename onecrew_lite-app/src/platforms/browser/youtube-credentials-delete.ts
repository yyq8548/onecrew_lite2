import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { stringifyApplicationJson } from "../../contracts/application-json.ts";

export const youtubeCredentialDeleteInput = z.strictObject({ credential_id: z.string(), idempotency_key: z.string(), confirmed: z.boolean() });
const account = z.object({ id: z.string(), platform: z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]), kind: z.enum(["profile", "service_credential"]), label: z.string(), active: z.union([z.literal(0), z.literal(1)]).transform(value => value === 1), availability: z.enum(["available", "unavailable"]), created_at: creatorU64, updated_at: creatorU64 });
const result = z.object({ account_ref: account.extend({ active: z.boolean() }), replayed: z.boolean() });
const recovery = z.object({ account_ref_id: z.string() });
const record = z.object({ action: z.string(), request_fingerprint: z.string().nullable(), state: z.string(), result_json: z.string().nullable() });
type Result = z.infer<typeof result>;
export type YoutubeDeleteClaim = { state: "acquired" | "remote_verified"; account_ref_id: string } | { state: "completed"; result: Result };
const action = "service_credential_delete";
function key(raw: string) { const value = raw.trim(); if (!value || Buffer.byteLength(value) > 128) throw new Error("idempotency_key must contain 1 to 128 characters"); return value; }
function transaction(database: DatabaseSync) { if (!database.isTransaction) throw new Error("youtube_credential_delete_requires_transaction"); }
function loadAccount(database: DatabaseSync, id: string) {
  const statement = database.prepare("SELECT id,platform,kind,label,active,availability,created_at,updated_at FROM account_refs WHERE id=?"); statement.setReadBigInts(true);
  const row = statement.get(id); if (!row) throw new Error("account reference not found");
  return account.parse({ ...row, active: Number(row.active) });
}
function scoped(database: DatabaseSync, rawKey: string, id: string) {
  const value = record.parse(database.prepare("SELECT action,request_fingerprint,state,result_json FROM idempotency_records WHERE key=?").get(key(rawKey)));
  if (value.action !== action || value.request_fingerprint !== id) throw new Error("idempotency key was already used for another action or service credential");
  return value;
}
function audit(database: DatabaseSync, kind: string, id: string, payload: unknown, now: number) { database.prepare("INSERT INTO audit_events(id,kind,account_ref_id,payload_json,created_at) VALUES(?,?,?,?,?)").run(randomUUID(), kind, id, stringifyApplicationJson(payload), now); }
function decoded(value: z.infer<typeof record>): YoutubeDeleteClaim {
  if (value.result_json === null) throw new Error("service credential deletion is missing recovery state");
  if (value.state === "completed") return { state: "completed", result: { ...result.parse(parseStoredJson(value.result_json)), replayed: true } };
  const saved = recovery.parse(parseStoredJson(value.result_json));
  if (value.state === "in_progress") return { state: "acquired", ...saved };
  if (value.state === "remote_verified") return { state: "remote_verified", ...saved };
  throw new Error("service credential deletion has invalid state");
}

// 每一步由账号 Vault 的持久跨库事务单独提交，保持原删除恢复阶段。
export function beginYoutubeCredentialDelete(database: DatabaseSync, rawKey: string, id: string): YoutubeDeleteClaim {
  transaction(database); const operationKey = key(rawKey);
  if (database.prepare("SELECT key FROM idempotency_records WHERE key=?").get(operationKey)) return decoded(scoped(database, operationKey, id));
  const previous = loadAccount(database, id);
  const pending = database.prepare(`SELECT EXISTS(SELECT 1 FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1')
    AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
    AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version')=1
      AND json_type(result_json,'$.state.identity.creator_id')='text' AND json_type(result_json,'$.state.identity.operator_account_ref_id')='text'
      AND length(trim(json_extract(result_json,'$.state.identity.creator_id')))>0 AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id')))>0
      AND json_extract(result_json,'$.state.fingerprint')=request_fingerprint AND request_fingerprint GLOB 'sha256:v1:*'
      THEN json_extract(result_json,'$.state.identity.operator_account_ref_id')=? ELSE 1 END) AS pending`).get(id);
  if (pending?.pending !== 0) throw new Error("outreach_in_progress");
  if (previous.kind !== "service_credential") throw new Error("wrong_account_kind");
  const now = Math.floor(Date.now() / 1000);
  database.prepare("UPDATE account_refs SET active=0,availability='unavailable',updated_at=? WHERE id=?").run(now, id);
  database.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES(?,?,?,'in_progress',?,?,?)").run(operationKey, action, id, stringifyApplicationJson({ account_ref_id: id }), now, now);
  audit(database, "service_credential_delete_started", id, { previous_availability: previous.availability }, now);
  return { state: "acquired", account_ref_id: id };
}
export function markYoutubeCredentialDeleteVerified(database: DatabaseSync, rawKey: string, id: string): void {
  transaction(database); const value = scoped(database, rawKey, id); decoded(value);
  if (value.state === "in_progress") database.prepare("UPDATE idempotency_records SET state='remote_verified',updated_at=? WHERE key=? AND action=? AND state='in_progress'").run(Math.floor(Date.now() / 1000), key(rawKey), action);
}
export function completeYoutubeCredentialDelete(database: DatabaseSync, rawKey: string, id: string): Result {
  transaction(database); const claim = decoded(scoped(database, rawKey, id));
  if (claim.state === "completed") return claim.result;
  if (claim.state !== "remote_verified") throw new Error("service credential vault deletion is not verified");
  const now = Math.floor(Date.now() / 1000); database.prepare("UPDATE account_refs SET active=0,availability='unavailable',updated_at=? WHERE id=?").run(now, id);
  const output = { account_ref: loadAccount(database, id), replayed: false };
  audit(database, "service_credential_deleted", id, { account_ref_id: claim.account_ref_id, vault_credential_deleted: true }, now);
  const changed = database.prepare("UPDATE idempotency_records SET state='completed',result_json=?,updated_at=? WHERE key=? AND action=? AND state='remote_verified'").run(stringifyApplicationJson(output), now, key(rawKey), action);
  if (changed.changes !== 1) throw new Error("service credential deletion completion claim was lost");
  return output;
}
export function pendingYoutubeCredentialDeletes(database: DatabaseSync) {
  return database.prepare("SELECT key,result_json FROM idempotency_records WHERE action=? AND state IN ('in_progress','remote_verified') ORDER BY created_at ASC").all(action).map(value => { const row = z.object({ key: z.string(), result_json: z.string() }).parse(value); return { key: row.key, ...recovery.parse(parseStoredJson(row.result_json)) }; });
}
