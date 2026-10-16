import type { DatabaseSync } from "node:sqlite";
import { LosslessNumber, stringify } from "lossless-json";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { legacyCreatorU64Schema as u64 } from "./creator-retire-edge.ts";
import { legacySqliteText } from "./sqlite-value.ts";

const object = z.record(z.string(), z.unknown());
const platform = z.enum(["x", "instagram", "telegram", "youtube", "tiktok"]);
const stage = z.enum(["prospect", "engaged", "collaborating", "archived"]);
const confidence = z.enum(["exact", "asserted", "possible"]);
const channel = z.enum(["email", "wechat", "x", "instagram", "telegram", "youtube", "tiktok", "website", "link_hub"]);
const sourceKind = z.enum(["initial_platform_account", "profile_bio", "profile_website", "profile_location", "video_description", "channel_description", "link_hub", "manual_business_email", "community_administrator", "manual_entry", "contact_route_resolution", "creator_import"]);
const timestamps = { created_at: u64, updated_at: u64 };
const optionalText = z.string().nullish();
const contact = z.object({
  route: z.object({ id: z.string(), creator_id: z.string(), locator_id: z.string(), confidence, ...timestamps }),
  locator: z.object({ id: z.string(), channel, kind: z.enum(["email_address", "handle", "url", "invite_link", "channel_id"]), normalized_value: z.string(), display_value: z.string(), ...timestamps }),
  observations: z.array(z.object({
    observation: z.object({ id: z.string(), locator_id: z.string(), source_id: z.string(), creator_id: optionalText, platform_account_id: optionalText, observed_value: z.string(), observed_at: u64 }),
    source: z.object({ id: z.string(), kind: sourceKind, platform: platform.nullish(), source_url: optionalText, external_ref: optionalText, account_ref_id: optionalText, metadata: z.unknown().refine(value => value !== undefined), observed_at: u64 }),
  })),
  allowed_actions: z.array(z.enum(["update", "delete"])),
});
const creator = z.object({ id: z.string(), display_name: z.string(), tags: z.array(z.string()), notes: optionalText,
  score: u64.refine(value => value <= 100n).nullish(), assessment: optionalText,
  follower_counts: z.partialRecord(platform, u64), ...timestamps, archived_at: u64.nullish(), pinned_at: u64.nullish() });
const manualCreator = z.object({ creator_id: z.string(), display_name: z.string(), tags: z.array(z.string()), stage, created_at: u64, contact_routes: z.array(contact) });
const batch = z.strictObject({ outcomes: z.array(z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("accepted"), input_index: u64, creator: manualCreator }),
  z.strictObject({ kind: z.literal("rejected"), input_index: u64, message: z.string() }),
])), replayed: z.boolean() });
const impactFields = ["creator_inputs", "creators_created", "creators_merged_by_public_identity", "creator_scores_filled", "follower_counts_filled", "platform_accounts_created", "claimed_links_created", "confirmed_links_created", "rejected_links_created", "links_promoted_to_confirmed", "profile_observations_created", "contact_routes_created", "creators_unchanged", "creator_tags_filled"];
const legacyFields = ["creator_inputs", "creators_created", "creators_merged_by_id", "creators_merged_by_confirmed_account", "creator_scores_filled", "follower_counts_filled", "platform_accounts_created", "claimed_links_created", "confirmed_links_created", "links_promoted_to_confirmed", "profile_observations_created", "origins_created", "creators_unchanged", "creator_tags_filled"];
const terminal = z.discriminatedUnion("status", [
  z.strictObject({ status: z.literal("cancelled"), operation_id: z.string() }),
  z.strictObject({ status: z.literal("expired"), operation_id: z.string(), next_action: z.string() }),
  z.strictObject({ status: z.literal("stale"), operation_id: z.string(), next_action: z.string() }),
]);

function evolveCreator(value: Record<string, unknown>): void {
  if ("entity_kind" in value) z.enum(["person", "team", "brand"]).parse(value.entity_kind);
  for (const field of ["id", "display_name", "notes", "created_at", "updated_at", "archived_at"]) if (!(field in value)) throw new Error("Historical Creator snapshot is missing a base field");
  if ("tags" in value) throw new Error("Historical Creator save replay already contains tags");
  const quality = ["score", "assessment", "follower_counts"].filter(field => field in value).length;
  if (quality === 0 && !("pinned_at" in value)) Object.assign(value, { score: null, assessment: null, follower_counts: {} });
  else if (quality !== 3) throw new Error("Historical Creator snapshot has incomplete quality fields");
  if (!("pinned_at" in value)) value.pinned_at = null;
  value.tags = [];
  creator.parse(value);
}

// 原 v34→35 的历史 replay 与 import impact 检查，验证完成后再执行 ALTER。
export function migrateCreatorTagsEdge(db: DatabaseSync): void {
  const updates: { key: string; payload: string }[] = [];
  for (const row of db.prepare("SELECT key,action,result_json FROM idempotency_records WHERE action IN ('creator_candidate_save_confirmation','creator_batch_create') AND state='completed'").all()) {
    if (typeof row.result_json !== "string") throw new Error("Historical Creator replay is missing JSON");
    const payload = object.parse(parseStoredJson(row.result_json));
    if (row.action === "creator_candidate_save_confirmation") {
      const result = object.parse(payload.result);
      const record = object.parse(result.creator);
      const snapshot = object.parse(record.creator);
      evolveCreator(snapshot);
      record.creator = snapshot;
      result.creator = record;
      payload.result = result;
    } else {
      if (!Array.isArray(payload.outcomes)) throw new Error("Historical Creator batch replay is missing outcomes");
      const outcomes = payload.outcomes.map(value => object.parse(value));
      const indexed = outcomes.filter(value => "input_index" in value).length;
      if (indexed !== 0 && indexed !== outcomes.length) throw new Error("Historical Creator batch mixes indexed outcomes");
      for (const [index, outcome] of outcomes.entries()) {
        if (indexed === 0) outcome.input_index = new LosslessNumber(String(index));
        if (outcome.kind === "accepted") {
          const created = object.parse(outcome.creator);
          if ("tags" in created) throw new Error("Historical Creator batch already contains tags");
          created.tags = [];
          outcome.creator = created;
        }
      }
      payload.outcomes = outcomes;
      batch.parse(payload);
    }
    const encoded = stringify(payload);
    if (encoded === undefined) throw new Error("Historical Creator replay cannot be serialized");
    updates.push({ key: legacySqliteText(row.key), payload: encoded });
  }
  for (const row of db.prepare("SELECT impact_json,status,result_json FROM creator_import_operations UNION ALL SELECT impact_json,NULL,NULL FROM creator_import_receipts").all()) {
    if (row.status !== null) legacySqliteText(row.status);
    if (row.result_json !== null) legacySqliteText(row.result_json);
    const impact = object.parse(parseStoredJson(legacySqliteText(row.impact_json)));
    if ("creator_tags_filled" in impact && u64.parse(impact.creator_tags_filled) !== 0n) throw new Error("Historical import impact already contains tag changes");
    impact.creator_tags_filled = new LosslessNumber("0");
    const legacy = ["creators_merged_by_id", "creators_merged_by_confirmed_account", "origins_created"].some(field => field in impact);
    const fields = legacy ? legacyFields : impactFields;
    if (Object.keys(impact).length !== fields.length || fields.some(field => !(field in impact))) throw new Error("Historical import impact fields differ");
    for (const field of fields) u64.parse(impact[field]);
    if (legacy) {
      if (row.status !== "completed" || typeof row.result_json !== "string") throw new Error("Legacy import impact requires a terminal result without a receipt");
      terminal.parse(parseStoredJson(row.result_json));
    }
  }
  db.exec("ALTER TABLE creators ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(tags_json) AND json_type(tags_json) = 'array' AND json_array_length(tags_json) <= 3)");
  for (const update of updates) db.prepare("UPDATE idempotency_records SET result_json=? WHERE key=?").run(update.payload, update.key);
  db.exec("UPDATE creator_import_operations SET impact_json=json_set(impact_json,'$.creator_tags_filled',0); UPDATE creator_import_receipts SET impact_json=json_set(impact_json,'$.creator_tags_filled',0); UPDATE creator_import_operations SET result_json=json_set(result_json,'$.receipt.impact.creator_tags_filled',0) WHERE json_type(result_json,'$.receipt.impact')='object'");
}
