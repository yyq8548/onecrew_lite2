import type { DatabaseSync } from "node:sqlite";
import { isLosslessNumber, stringify } from "lossless-json";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { legacySqliteText } from "./sqlite-value.ts";

const maximum = 18446744073709551615n;
const count = z.custom<unknown>().transform((value) => {
  if (!isLosslessNumber(value) || value.value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value.value)) throw new Error("Legacy route impact requires unsigned integer tokens within u64");
  const integer = BigInt(value.value);
  if (integer > maximum) throw new Error("Legacy route impact exceeds u64");
  return integer;
});
export { count as legacyCreatorU64Schema };
const impactSchema = z.strictObject({
  contact_routes: count, dependencies: count, email_threads: count, email_messages: count,
  email_recipients: count, email_drafts: count, direct_message_conversations: count,
  direct_messages: count, outreach_touchpoints: count, audit_events: count,
  idempotency_records: count, target_resolutions: count, identity_conflicts: count,
  observations: count, contact_locators: count, observation_sources: count,
  resolved_targets: count, candidate_snapshots_detached: count, creator_origins_detached: count,
  local_records_total: count,
});
const resultSchema = z.discriminatedUnion("status", [
  z.strictObject({ status: z.literal("deleted"), operation_id: z.string(), creator_id: z.string(), contact_route_id: z.string(), deleted: impactSchema, remote_not_deleted: z.boolean() }),
  z.strictObject({ status: z.literal("stale"), operation_id: z.string(), reason: z.literal("plan_changed"), next_tool: z.string() }),
  z.strictObject({ status: z.literal("expired"), operation_id: z.string(), next_tool: z.string() }),
]);
type Impact = z.output<typeof impactSchema>;

function rewriteImpact(impact: Impact): Omit<Impact, "candidate_snapshots_detached"> {
  let total = 0n;
  for (const [key, value] of Object.entries(impact)) if (key !== "local_records_total") {
    total += value;
    if (total > maximum) throw new Error("Legacy route impact total exceeds u64");
  }
  if (total !== impact.local_records_total) throw new Error("Legacy route impact total differs");
  const { candidate_snapshots_detached, ...rewritten } = impact;
  return { ...rewritten, local_records_total: total - candidate_snapshots_detached };
}

// 原 v36→37：完整检查冻结的 route-delete JSON，再执行退休数据转换。
export function migrateCreatorRetiredStorageEdge(db: DatabaseSync): void {
  const operations = db.prepare("SELECT id,creator_id,contact_route_id,impact_json,result_json FROM creator_contact_route_delete_operations ORDER BY id").all().map((row) => {
    legacySqliteText(row.creator_id);
    legacySqliteText(row.contact_route_id);
    const old = impactSchema.parse(parseStoredJson(legacySqliteText(row.impact_json)));
    const impact = stringify(rewriteImpact(old))!;
    let result = row.result_json === null ? null : legacySqliteText(row.result_json);
    if (result !== null) {
      const frozen = resultSchema.parse(parseStoredJson(result));
      if (frozen.operation_id !== row.id) throw new Error("Legacy route result operation identity differs");
      if (frozen.status === "deleted") {
        if (frozen.creator_id !== row.creator_id || frozen.contact_route_id !== row.contact_route_id ||
            Object.entries(old).some(([key, value]) => frozen.deleted[key as keyof Impact] !== value)) throw new Error("Legacy route deleted impact differs");
        result = stringify({ ...frozen, deleted: rewriteImpact(frozen.deleted) })!;
      }
    }
    return { id: legacySqliteText(row.id), impact, result };
  });
  for (const operation of operations) db.prepare("UPDATE creator_contact_route_delete_operations SET impact_json=?,result_json=? WHERE id=?").run(operation.impact, operation.result, operation.id);
  const claims = db.prepare("SELECT key,action,state,result_json FROM idempotency_records WHERE action IN ('relationship_update','content_reply_send','direct_message_send') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified') ORDER BY key").all();
  for (const claim of claims) db.prepare("UPDATE idempotency_records SET state='failed',result_json=?,updated_at=? WHERE key=?")
    .run(JSON.stringify({ kind: "retired_action_unresolved", action: legacySqliteText(claim.action), state_before: legacySqliteText(claim.state), recovery: claim.result_json === null ? null : legacySqliteText(claim.result_json) }), Math.floor(Date.now() / 1000), legacySqliteText(claim.key));
  db.exec("DELETE FROM idempotency_records WHERE action IN ('creator_candidate_save_confirmation','creator_contact_confirmation'); DROP TABLE creator_candidate_snapshots; DROP TABLE tiktok_one_account_bindings");
}
