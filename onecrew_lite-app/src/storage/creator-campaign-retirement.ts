import { readStorageSql } from "./sql-resource.ts";
import { DatabaseSync } from "node:sqlite";
import { isDeepStrictEqual } from "node:util";
import { isLosslessNumber, stringify } from "lossless-json";
import { z } from "zod";
import { parseStoredJson } from "../contracts/stored-json.ts";
import { legacyCreatorU64Schema } from "./creator-retire-edge.ts";
import { legacySqliteText } from "./sqlite-value.ts";

const community = z.strictObject({ external_id: z.string(), title: z.string(), handle: z.string().nullable().default(null), target_kind: z.enum(["channel", "supergroup"]) });
const source = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("campaign_community"), campaign_id: z.string(), operator_account_ref_id: z.string() }),
  z.strictObject({ kind: z.literal("creator_contact_route"), source_creator_id: z.string(), source_contact_route_id: z.string().nullable().default(null), operator_account_ref_id: z.string(), source_resolved_target_id: z.string() }),
]);
const candidateOrigin = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("campaign_discovery"), discovery_id: z.string(), campaign_id: z.string(), account_ref_id: z.string(), query: z.string(), search_kind: z.string() }),
  z.strictObject({ kind: z.literal("ad_hoc_search"), discovery_id: z.string(), account_ref_id: z.string(), query: z.string(), search_kind: z.string() }),
  z.strictObject({ kind: z.literal("community_administrator"), discovery_id: z.string(), role: z.enum(["owner", "administrator"]), community, source }),
]);
const creatorOrigin = z.discriminatedUnion("kind", [
  z.strictObject({ kind: z.literal("campaign_discovery"), id: z.string(), discovery_id: z.string(), campaign_id: z.string(), creator_id: z.string(), source_id: z.string(), discovered_at: legacyCreatorU64Schema }),
  z.strictObject({ kind: z.literal("ad_hoc_search"), id: z.string(), discovery_id: z.string(), account_ref_id: z.string(), creator_id: z.string(), source_id: z.string(), discovered_at: legacyCreatorU64Schema }),
]);
const accountsSchema = z.array(z.object({ platform: z.string(), target_kind: z.string(), external_id: z.string() }));
type Accounts = z.output<typeof accountsSchema>;
const quote = (name: string) => `"${name.replaceAll('"', '""')}"`;
const objectSchema = z.record(z.string(), z.unknown());
const encode = (value: unknown) => {
  const encoded = stringify(value);
  if (encoded === undefined) throw new Error("Legacy provenance cannot be serialized");
  return encoded;
};

function convertSource(value: z.output<typeof source>) {
  return value.kind === "campaign_community" ? { kind: "operator_account", operator_account_ref_id: value.operator_account_ref_id } : value;
}

function convertCandidate(value: z.output<typeof candidateOrigin>) {
  return value.kind === "community_administrator"
    ? { kind: "community_administrator", discovery_id: value.discovery_id, role: value.role, community: value.community, source: convertSource(value.source) }
    : { kind: "platform_search", discovery_id: value.discovery_id, account_ref_id: value.account_ref_id, query: value.query, search_kind: value.search_kind };
}

function recoverOperator(db: DatabaseSync, discovery: string, accounts: Accounts): string | undefined {
  const operators = new Set<string>();
  const query = db.prepare("SELECT DISTINCT account_ref_id FROM creator_candidate_snapshots WHERE discovery_id=? AND origin_kind='campaign_discovery' AND account_platform=? AND account_target_kind=? AND account_external_id=?");
  for (const account of accounts) for (const row of query.all(discovery, account.platform, account.target_kind, account.external_id)) operators.add(legacySqliteText(row.account_ref_id));
  if (operators.size > 1) throw new Error("Campaign search origin has ambiguous operator accounts");
  return [...operators][0];
}

function rewriteRetiredObject(db: DatabaseSync, value: Record<string, unknown>, accounts: Accounts): { value: unknown } | { removed: true } | undefined {
  if (value.kind === "campaign_discovery" || value.kind === "ad_hoc_search") {
    const creator = creatorOrigin.safeParse(value);
    if (creator.success) {
      const old = creator.data;
      const operator = old.kind === "ad_hoc_search" ? old.account_ref_id : recoverOperator(db, old.discovery_id, accounts);
      return operator === undefined ? { removed: true } : { value: { kind: "platform_search", id: old.id, discovery_id: old.discovery_id, account_ref_id: operator, creator_id: old.creator_id, source_id: old.source_id, discovered_at: old.discovered_at } };
    }
    return { value: convertCandidate(candidateOrigin.parse(value)) };
  }
  if (value.kind === "campaign_community") return { value: convertSource(source.parse(value)) };
  return undefined;
}

function rewriteReplay(db: DatabaseSync, value: unknown, accounts: Accounts): { value: unknown; changed: boolean } {
  if (value === null || typeof value !== "object" || isLosslessNumber(value)) return { value, changed: false };
  if (Array.isArray(value)) {
    const output: unknown[] = [];
    let changed = false;
    for (const item of value) {
      const retired = item !== null && typeof item === "object" && !Array.isArray(item) && !isLosslessNumber(item)
        ? rewriteRetiredObject(db, item as Record<string, unknown>, accounts) : undefined;
      if (retired) {
        changed = true;
        if ("value" in retired) output.push(retired.value);
      } else {
        const rewritten = rewriteReplay(db, item, accounts);
        output.push(rewritten.value);
        changed ||= rewritten.changed;
      }
    }
    return { value: output, changed };
  }
  const object = objectSchema.parse(value);
  const retired = rewriteRetiredObject(db, object, accounts);
  if (retired) {
    if ("removed" in retired) throw new Error("Replay campaign origin outside a list has no operator");
    return { value: retired.value, changed: true };
  }
  const recordAccounts = "origins" in object && "platform_accounts" in object ? accountsSchema.parse(object.platform_accounts) : [];
  const inherited = recordAccounts.length ? recordAccounts : accounts;
  let changed = false;
  for (const [key, child] of Object.entries(object)) {
    const rewritten = rewriteReplay(db, child, inherited);
    object[key] = rewritten.value;
    changed ||= rewritten.changed;
  }
  return { value: object, changed };
}

function targetDefinitions() {
  const reference = new DatabaseSync(":memory:");
  try {
    for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql", "creator-schema36-retired.sql"]) reference.exec(readStorageSql(file));
    return ["creator_candidate_snapshots", "creator_origins", "audit_events"].map((name) => ({ name,
      sql: String(reference.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(name)!.sql),
      columns: reference.prepare(`PRAGMA table_info(${quote(name)})`).all().map((row) => quote(String(row.name))).join(","),
      secondary: reference.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").all(name).map((row) => String(row.sql)),
    }));
  } finally { reference.close(); }
}

// 原 v35→36 先完整生成转换计划；歧义、碰撞或旧 JSON 形状错误直接中止。
export function migrateCreatorCampaignRetirement(db: DatabaseSync): void {
  const snapshots = db.prepare("SELECT discovery_id,candidate_id,origin_kind,source_kind,origin_json,candidate_json FROM creator_candidate_snapshots WHERE origin_kind IN ('campaign_discovery','ad_hoc_search') OR source_kind='campaign_community' ORDER BY discovery_id,candidate_id").all().map((row) => {
    const origin = candidateOrigin.parse(parseStoredJson(legacySqliteText(row.origin_json)));
    const candidate = objectSchema.parse(parseStoredJson(legacySqliteText(row.candidate_json)));
    if (!isDeepStrictEqual(origin, candidateOrigin.parse(candidate.origin))) throw new Error("Candidate snapshot origin differs from its payload");
    const sourceKind = origin.kind === "community_administrator" ? origin.source.kind : origin.kind;
    if (row.origin_kind !== origin.kind || row.source_kind !== sourceKind) throw new Error("Candidate snapshot provenance kinds differ");
    const converted = convertCandidate(origin);
    const kind = origin.kind === "community_administrator" ? "community_administrator" : "platform_search";
    return { discovery: legacySqliteText(row.discovery_id), candidate: legacySqliteText(row.candidate_id), kind,
      sourceKind: origin.kind !== "community_administrator" ? "platform_search" : origin.source.kind === "campaign_community" ? "operator_community" : "creator_contact_route",
      origin: encode(converted), payload: encode({ ...candidate, origin: converted }) };
  });
  const searchKeys = new Set<string>();
  const communityKeys = new Set<string>();
  const origins = db.prepare("SELECT id,kind,creator_id,discovery_id,operator_account_ref_id,community_external_id FROM creator_origins WHERE kind IN ('campaign_discovery','ad_hoc_search') OR (kind='community_administrator' AND campaign_id IS NOT NULL) ORDER BY id").all().map((row) => {
    if (typeof row.discovery_id !== "string") throw new Error("Legacy search origin has no discovery id");
    let operator = row.operator_account_ref_id;
    if (row.kind === "campaign_discovery") {
      const accounts = accountsSchema.parse(db.prepare("SELECT account.platform,account.target_kind,account.external_id FROM creator_account_links link JOIN platform_accounts account ON account.id=link.platform_account_id WHERE link.creator_id=? ORDER BY account.id").all(legacySqliteText(row.creator_id)));
      operator = recoverOperator(db, row.discovery_id, accounts) ?? null;
      if (operator === null) return { id: legacySqliteText(row.id), removed: true as const };
    }
    if (typeof operator !== "string") throw new Error("Legacy origin has no operator account");
    const isCommunity = row.kind === "community_administrator";
    if (isCommunity && typeof row.community_external_id !== "string") throw new Error("Legacy community origin has no community identity");
    const creatorId = legacySqliteText(row.creator_id);
    const key = JSON.stringify(isCommunity ? [operator, row.community_external_id, row.discovery_id, creatorId] : [operator, row.discovery_id, creatorId]);
    const seen = isCommunity ? communityKeys : searchKeys;
    if (seen.has(key)) throw new Error("Converted legacy origins collide");
    seen.add(key);
    return { id: legacySqliteText(row.id), removed: false as const, kind: isCommunity ? "community_administrator" : "platform_search", operator: row.kind === "campaign_discovery" ? operator : null };
  });
  const replays = db.prepare("SELECT key,result_json FROM idempotency_records WHERE result_json IS NOT NULL AND action NOT IN ('campaign_create','campaign_template_create') ORDER BY key").all().flatMap((row) => {
    const raw = legacySqliteText(row.result_json);
    if (!["campaign_discovery", "ad_hoc_search", "campaign_community"].some((kind) => raw.includes(`"${kind}"`))) return [];
    const rewritten = rewriteReplay(db, parseStoredJson(raw), []);
    return rewritten.changed ? [{ key: legacySqliteText(row.key), payload: encode(rewritten.value) }] : [];
  });
  const definitions = targetDefinitions();
  db.exec("CREATE TABLE creator_candidate_snapshots_v36_copy AS SELECT * FROM creator_candidate_snapshots; CREATE TABLE creator_origins_v36_copy AS SELECT * FROM creator_origins");
  const expectOne = (changed: number | bigint) => { if (changed !== 1 && changed !== 1n) throw new Error("Legacy provenance rewrite affected an unexpected row count"); };
  for (const snapshot of snapshots) expectOne(db.prepare("UPDATE creator_candidate_snapshots_v36_copy SET origin_kind=?,source_kind=?,campaign_id=NULL,origin_json=?,candidate_json=? WHERE discovery_id=? AND candidate_id=?")
    .run(snapshot.kind, snapshot.sourceKind, snapshot.origin, snapshot.payload, snapshot.discovery, snapshot.candidate).changes);
  for (const origin of origins) expectOne(origin.removed
    ? db.prepare("DELETE FROM creator_origins_v36_copy WHERE id=?").run(origin.id).changes
    : db.prepare("UPDATE creator_origins_v36_copy SET kind=?,campaign_id=NULL,operator_account_ref_id=COALESCE(?,operator_account_ref_id) WHERE id=?").run(origin.kind, origin.operator, origin.id).changes);
  db.exec("CREATE TABLE audit_events_v36_copy AS SELECT * FROM audit_events");
  for (const definition of definitions) {
    db.exec(`DROP TABLE ${quote(definition.name)}`);
    db.exec(definition.sql);
    db.exec(`INSERT INTO ${quote(definition.name)}(${definition.columns}) SELECT ${definition.columns} FROM ${quote(`${definition.name}_v36_copy`)}; DROP TABLE ${quote(`${definition.name}_v36_copy`)}`);
    for (const sql of definition.secondary) db.exec(sql);
  }
  db.exec("DROP TABLE campaign_account_bindings; DROP TABLE campaign_delete_operations; DROP TABLE campaign_template_delete_operations; DROP TABLE campaign_templates; DROP TABLE campaigns; DELETE FROM idempotency_records WHERE action IN ('campaign_create','campaign_template_create')");
  for (const replay of replays) expectOne(db.prepare("UPDATE idempotency_records SET result_json=? WHERE key=?").run(replay.payload, replay.key).changes);
}
