import { randomUUID } from "node:crypto";
import type { DatabaseSync } from "node:sqlite";
import { z } from "zod";
import { creatorU64 } from "@onecrew/runtime-contracts/creators";
import { parseStoredJson } from "../../contracts/stored-json.ts";
import { stringifyApplicationJson } from "../../contracts/application-json.ts";

const action = "operator_account_disconnect";
const account = z.object({ id: z.string(), platform: z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]), kind: z.enum(["profile", "service_credential"]), label: z.string(), active: z.boolean(), availability: z.enum(["available", "unavailable"]), created_at: creatorU64, updated_at: creatorU64 });
const resultSchema = z.object({ account_ref: account, replayed: z.boolean() });
const recovery = z.object({ account_ref_id: z.string() });
const recordSchema = z.object({ action: z.string(), request_fingerprint: z.string().nullable(), state: z.string(), result_json: z.string().nullable() });
export type OperatorDisconnectResult = z.infer<typeof resultSchema>;
export type OperatorDisconnectClaim = { state: "acquired" | "remote_started" | "remote_verified"; account_ref_id: string } | { state: "completed"; result: OperatorDisconnectResult };
function validateKey(raw: string) { const value = raw.trim(); if (!value || Buffer.byteLength(value) > 128) throw new Error("idempotency_key must contain 1 to 128 characters"); return value; }

// 调用方使用账号 Vault 的真实跨库事务；每个持久阶段单独提交。
export class OperatorDisconnectRepository {
  readonly #db: DatabaseSync;
  constructor(database: DatabaseSync) { this.#db = database; }
  #transaction() { if (!this.#db.isTransaction) throw new Error("operator_disconnect_requires_transaction"); }
  account(id: string) {
    const query = this.#db.prepare("SELECT id,platform,kind,label,active,availability,created_at,updated_at FROM account_refs WHERE id=?"); query.setReadBigInts(true); const value = query.get(id);
    if (!value) throw new Error("operator account was not found"); return account.parse({ ...value, active: z.bigint().parse(value.active) !== 0n });
  }
  #record(key: string, id: string) {
    const value = recordSchema.parse(this.#db.prepare("SELECT action,request_fingerprint,state,result_json FROM idempotency_records WHERE key=?").get(validateKey(key)));
    if (value.action !== action || value.request_fingerprint !== id) throw new Error("idempotency_key was already used for a different disconnect intent");
    if (value.result_json === null) throw new Error("operator account disconnect is missing recovery state"); return { ...value, result_json: value.result_json };
  }
  #audit(kind: string, id: string, payload: unknown, now: number) { this.#db.prepare("INSERT INTO audit_events(id,kind,account_ref_id,payload_json,created_at) VALUES(?,?,?,?,?)").run(randomUUID(), kind, id, stringifyApplicationJson(payload), now); }
  begin(rawKey: string, id: string): OperatorDisconnectClaim {
    this.#transaction(); const key = validateKey(rawKey);
    if (this.#db.prepare("SELECT key FROM idempotency_records WHERE key=?").get(key)) {
      const value = this.#record(key, id);
      if (value.state === "completed") return { state: "completed", result: { ...resultSchema.parse(parseStoredJson(value.result_json)), replayed: true } };
      const saved = recovery.parse(parseStoredJson(value.result_json));
      if (value.state === "in_progress") return { state: "acquired", ...saved };
      if (value.state === "remote_started" || value.state === "remote_verified") return { state: value.state, ...saved };
      if (value.state === "failed") throw new Error("operator account disconnect previously failed to delete the vault profile");
      throw new Error("operator account disconnect has invalid state");
    }
    const previous = this.account(id);
    const pending = this.#db.prepare(`SELECT EXISTS(SELECT 1 FROM idempotency_records WHERE action IN ('relationship_update_v1','content_reply_send_v1','direct_message_send_v1') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
      AND CASE WHEN json_valid(result_json) AND json_extract(result_json,'$.version')=1 AND json_type(result_json,'$.state.identity.creator_id')='text' AND json_type(result_json,'$.state.identity.operator_account_ref_id')='text'
        AND length(trim(json_extract(result_json,'$.state.identity.creator_id')))>0 AND length(trim(json_extract(result_json,'$.state.identity.operator_account_ref_id')))>0 AND json_extract(result_json,'$.state.fingerprint')=request_fingerprint AND request_fingerprint GLOB 'sha256:v1:*'
        THEN json_extract(result_json,'$.state.identity.operator_account_ref_id')=? ELSE 1 END) AS pending`).get(id);
    if (pending?.pending !== 0) throw new Error("outreach_in_progress");
    if (previous.kind !== "profile") throw new Error("operator account was not found");
    const now = Math.floor(Date.now() / 1000); this.#db.prepare("UPDATE account_refs SET active=0,updated_at=? WHERE id=?").run(now, id);
    this.#db.prepare("INSERT INTO idempotency_records(key,action,request_fingerprint,state,result_json,created_at,updated_at) VALUES(?,?,?,'in_progress',?,?,?)").run(key, action, id, stringifyApplicationJson({ account_ref_id: id }), now, now);
    this.#audit("operator_account_disconnect_started", id, { previous_availability: previous.availability }, now); return { state: "acquired", account_ref_id: id };
  }
  #transition(rawKey: string, id: string, from: string, to: string, allowed: readonly string[]) {
    this.#transaction(); const key = validateKey(rawKey), value = this.#record(key, id); recovery.parse(parseStoredJson(value.result_json));
    if (value.state !== from) { if (!allowed.includes(value.state)) throw new Error("operator account disconnect has invalid state"); return; }
    const now = Math.floor(Date.now() / 1000), changed = this.#db.prepare("UPDATE idempotency_records SET state=?,updated_at=? WHERE key=? AND action=? AND state=?").run(to, now, key, action, from);
    if (changed.changes !== 1) throw new Error("operator account disconnect claim was lost");
    if (to === "failed") this.#audit("operator_account_disconnect_failed", id, { reason: "vault_profile_not_deleted" }, now);
  }
  markStarted(key: string, id: string) { this.#transition(key, id, "in_progress", "remote_started", ["remote_started", "remote_verified", "completed"]); }
  markVerified(key: string, id: string) { this.#transition(key, id, "remote_started", "remote_verified", ["remote_verified", "completed"]); }
  markFailed(key: string, id: string) { this.#transition(key, id, "remote_started", "failed", ["failed"]); }
  complete(rawKey: string, id: string): OperatorDisconnectResult {
    this.#transaction(); const key = validateKey(rawKey), value = this.#record(key, id);
    if (value.state === "completed") return { ...resultSchema.parse(parseStoredJson(value.result_json)), replayed: true };
    if (value.state !== "remote_verified") throw new Error("operator account disconnect vault deletion is not verified");
    const saved = recovery.parse(parseStoredJson(value.result_json)), now = Math.floor(Date.now() / 1000);
    this.#db.prepare("UPDATE account_refs SET active=0,availability='unavailable',updated_at=? WHERE id=?").run(now, id);
    const output = { account_ref: this.account(id), replayed: false };
    this.#audit("operator_account_disconnected", id, { account_ref_id: saved.account_ref_id, vault_profile_deleted: true }, now);
    const changed = this.#db.prepare("UPDATE idempotency_records SET state='completed',result_json=?,updated_at=? WHERE key=? AND action=? AND state='remote_verified'").run(stringifyApplicationJson(output), now, key, action);
    if (changed.changes !== 1) throw new Error("operator account disconnect completion claim was lost"); return output;
  }
  pending() { return this.#db.prepare("SELECT key,result_json FROM idempotency_records WHERE action=? AND state IN ('in_progress','remote_started','remote_verified') ORDER BY created_at ASC").all(action).map(value => { const row = z.object({ key: z.string(), result_json: z.string() }).parse(value); return { key: row.key, ...recovery.parse(parseStoredJson(row.result_json)) }; }); }
}
