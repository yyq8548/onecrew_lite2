import { readStorageSql } from "./sql-resource.ts";
import { open } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { cstVisitor, parse } from "sql-parser-cst";
import { verifyPrivateDirectory, verifyPrivateFile } from "./private-backup.ts";
import { migrateCreatorWebSourceEdge } from "./creator-web-source-edges.ts";
import { migrateCreatorCandidateSourceEdge } from "./creator-candidate-edge.ts";
import { migrateCreatorReceiptPlatformEdge } from "./creator-receipt-platform-edge.ts";
import { migrateCreatorLimitEdge } from "./creator-limit-edge.ts";
import { migrateCreatorFacebookContentEdge } from "./creator-facebook-content-edge.ts";
import { migrateCreatorTikTokEdge } from "./creator-tiktok-edge.ts";
import { migrateCreatorTaskOriginEdge } from "./creator-task-origin-edge.ts";
import { migrateCreatorPublicContactEdge } from "./creator-public-contact-edge.ts";
import { migrateCreatorWebProfileEdge } from "./creator-web-profile-edge.ts";
import { migrateCreatorInstagramDomEdge } from "./creator-instagram-dom-edge.ts";

const schemaSql = () => ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql"]
  .map(readStorageSql).join("\n");
const asciiLower = (value: string) => value.replace(/[A-Z]/g, (character) => character.toLowerCase());
type CreatorSchemaStamp = { userVersion: number; schemaVersion: number };
const validatedConnections = new WeakMap<DatabaseSync, CreatorSchemaStamp>();
function creatorSchemaStamp(db: DatabaseSync): CreatorSchemaStamp {
  return { userVersion: Number(db.prepare("PRAGMA user_version").get()!.user_version), schemaVersion: Number(db.prepare("PRAGMA schema_version").get()!.schema_version) };
}

export function assertValidatedCreatorConnection(db: DatabaseSync): void {
  const current = creatorSchemaStamp(db), validated = validatedConnections.get(db);
  if (validated?.userVersion === current.userVersion && validated.schemaVersion === current.schemaVersion) return;
  validateCreatorSchema(db);
}

// 使用成熟 SQLite CST 解析器比较结构，保留带引号的标识符与字面值。
export function normalizeCreatorSchemaSql(sql: string): string {
  const tree = parse(sql, { dialect: "sqlite", includeSpaces: false, includeNewlines: false, includeComments: false, includeRange: false });
  cstVisitor({
    keyword: (node) => { node.text = asciiLower(node.text); },
    identifier: (node) => {
      if (!["'", '"', "`", "["].includes(node.text[0]!)) {
        node.text = asciiLower(node.text);
        node.name = asciiLower(node.name);
      }
    },
  })(tree);
  return JSON.stringify(tree);
}

function quoteIdentifier(value: string): string { return `"${value.replaceAll('"', '""')}"`; }

export function creatorSchemaSnapshot(db: DatabaseSync) {
  const tables = db.prepare("SELECT name FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
    .all().map((row) => String(row.name));
  return {
    tables: tables.map((name) => ({
      name,
      columns: db.prepare(`PRAGMA table_info(${quoteIdentifier(name)})`).all(),
      foreignKeys: db.prepare(`PRAGMA foreign_key_list(${quoteIdentifier(name)})`).all(),
      indexes: db.prepare(`PRAGMA index_list(${quoteIdentifier(name)})`).all().map((row) => ({
        name: row.name, unique: row.unique, origin: row.origin, partial: row.partial,
        columns: db.prepare(`PRAGMA index_xinfo(${quoteIdentifier(String(row.name))})`).all()
          .map((column) => ({ ...column, coll: asciiLower(String(column.coll)) })),
      })).sort((left, right) => String(left.name).localeCompare(String(right.name))),
    })),
    objects: db.prepare("SELECT type, name, sql FROM sqlite_schema WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type, name")
      .all().map((row) => ({ type: row.type, name: row.name, sql: normalizeCreatorSchemaSql(String(row.sql)) })),
  };
}

function validateStoredTags(db: DatabaseSync): void {
  for (const row of db.prepare("SELECT tags_json FROM creators").all()) {
    if (typeof row.tags_json !== "string") throw new Error("Stored Creator tags are invalid");
    const tags: unknown = JSON.parse(row.tags_json);
    if (!Array.isArray(tags) || tags.length > 3) throw new Error("Stored Creator tags are invalid");
    const seen = new Set<string>();
    for (const tag of tags) {
      if (typeof tag !== "string" || !tag.isWellFormed() || /\p{Cc}/u.test(tag)) throw new Error("Stored Creator tags are invalid");
      const normalized = tag.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
      if (normalized !== tag || [...tag].length < 1 || [...tag].length > 12 || seen.has(tag)) throw new Error("Stored Creator tags are not normalized");
      seen.add(tag);
    }
  }
}

export function validateCreatorSchema90(db: DatabaseSync): void {
  validateCreatorSchemaVersion(db, 90);
}
export function validateCreatorSchema(db: DatabaseSync): void {
  validatedConnections.delete(db);
  const version = Number(db.prepare("PRAGMA user_version").get()!.user_version);
  if (version !== 90 && version !== 91 && version !== 92 && version !== 93 && version !== 94 && version !== 95 && version !== 96 && version !== 97) throw new Error(`Creator current schema is unsupported: ${version}`);
  validateCreatorSchemaVersion(db, version);
}

export function rebuildCreatorTable(db: DatabaseSync, table: string, from: string, to: string): void {
  rebuildCreatorTableChanges(db, table, [[from, to]]);
}

export function rebuildCreatorTableChanges(db: DatabaseSync, table: string, replacements: ReadonlyArray<readonly [string, string]>): void {
  const row = db.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(table);
  if (!row || typeof row.sql !== "string") throw new Error(`Creator ${table} predecessor table is missing`);
  let sql = row.sql;
  for (const [from, to] of replacements) {
    if (sql.split(from).length !== 2) throw new Error(`Creator ${table} predecessor constraint differs`);
    sql = sql.replace(from, to);
  }
  const secondary = db.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").all(table);
  const quoted = quoteIdentifier(table);
  db.exec(`CREATE TEMP TABLE creator_edge_copy AS SELECT * FROM ${quoted}; DROP TABLE ${quoted};`);
  db.exec(sql);
  db.exec(`INSERT INTO ${quoted} SELECT * FROM creator_edge_copy; DROP TABLE creator_edge_copy;`);
  for (const object of secondary) db.exec(String(object.sql));
}

export function validateCreatorSchemaVersion(db: DatabaseSync, expectedVersion: number): void {
  validatedConnections.delete(db);
  if (!Number.isInteger(expectedVersion) || expectedVersion < 27 || expectedVersion > 97) throw new Error(`Creator version ${expectedVersion} frozen validation is not implemented`);
  const version = Number(db.prepare("PRAGMA user_version").get()!.user_version);
  if (version !== expectedVersion) throw new Error(`Creator schema validation requires version ${expectedVersion}; found ${version}`);
  const checks = db.prepare("PRAGMA integrity_check").all();
  if (checks.length !== 1 || checks[0]!.integrity_check !== "ok" || db.prepare("PRAGMA foreign_key_check").all().length !== 0) throw new Error("Creator integrity check failed");
  const reference = new DatabaseSync(":memory:");
  try {
    reference.exec(schemaSql());
    if (expectedVersion >= 91) migrateCreatorPublicContactEdge(reference);
    if (expectedVersion >= 92) reference.exec(readStorageSql("creator-91-to-92.sql"));
    if (expectedVersion >= 93) { reference.exec(readStorageSql("creator-92-to-93.sql")); migrateCreatorWebProfileEdge(reference); }
    if (expectedVersion >= 94) reference.exec(readStorageSql("creator-93-to-94.sql"));
    if (expectedVersion >= 95) reference.exec(readStorageSql("creator-94-to-95.sql"));
    if (expectedVersion >= 96) migrateCreatorInstagramDomEdge(reference);
    if (expectedVersion >= 97) migrateCreatorReceiptPlatformEdge(reference, 96);
    if (expectedVersion < 90) reference.exec("DROP TABLE discovery_query_rounds");
    if (expectedVersion < 89) rebuildCreatorTable(reference, "discovery_target_increases", "new_target_count != previous_target_count", "new_target_count > previous_target_count");
    if (expectedVersion < 88) {
      for (const table of ["contact_locators", "conversations"]) rebuildCreatorTable(reference, table,
        "'email','x','instagram','telegram','youtube','tiktok','website','link_hub','wechat'",
        "'email','x','instagram','telegram','youtube','tiktok','website','link_hub'");
    }
    if (expectedVersion < 87) reference.exec("DROP TABLE gmail_send_attempts; DROP TABLE gmail_bulk_items; DROP TABLE gmail_bulk_batches");
    if (expectedVersion < 86) rebuildCreatorTable(reference, "mailbox_refs", "'resend','smtp','imap','gmail'", "'resend','smtp','imap'");
    if (expectedVersion < 85) reference.exec("DROP TABLE discovery_search_positions");
    if (expectedVersion < 84) rebuildCreatorTable(reference, "discovery_contact_observations",
      "'email','telegram','hidden_email','social_profile','whatsapp','wechat','wechat_official')",
      "'email','telegram','hidden_email','social_profile')");
    if (expectedVersion < 83) migrateCreatorWebSourceEdge(reference, 82, true);
    if (expectedVersion < 82) migrateCreatorWebSourceEdge(reference, 81, true);
    if (expectedVersion < 81) reference.exec("DROP TABLE discovery_facebook_original_attempts");
    if (expectedVersion < 80) reference.exec("DROP TRIGGER discovery_facebook_native_profile_insert; DROP TRIGGER discovery_facebook_native_profile_update");
    if (expectedVersion < 79) reference.exec("DROP TABLE discovery_facebook_identity_reads");
    if (expectedVersion < 78) migrateCreatorWebSourceEdge(reference, 77, true);
    if (expectedVersion < 77) migrateCreatorCandidateSourceEdge(reference, true);
    if (expectedVersion < 76) reference.exec("DROP TABLE discovery_web_source_reads");
    if (expectedVersion < 75) reference.exec("DROP TABLE discovery_douyin_author_reads");
    if (expectedVersion < 74) migrateCreatorReceiptPlatformEdge(reference, 73, true);
    if (expectedVersion < 73) rebuildCreatorTable(reference, "discovery_contact_observations", "'email','telegram','hidden_email','social_profile')", "'email','telegram')");
    for (const edge of [71, 70, 69, 68, 67, 66]) if (expectedVersion <= edge) migrateCreatorReceiptPlatformEdge(reference, edge, true);
    if (expectedVersion < 66) reference.exec("DROP TABLE discovery_contact_lookups");
    if (expectedVersion < 65) {
      reference.exec("DROP TABLE discovery_contact_observations; DROP TABLE discovery_recommendation_policies");
      migrateCreatorLimitEdge(reference, true);
    }
    if (expectedVersion < 64) {
      reference.exec("DROP TABLE discovery_facebook_content_targets");
      migrateCreatorFacebookContentEdge(reference, true);
    }
    if (expectedVersion < 63) migrateCreatorFacebookContentEdge(reference, true, 62);
    if (expectedVersion < 62) migrateCreatorFacebookContentEdge(reference, true, 61);
    if (expectedVersion < 61) reference.exec("ALTER TABLE discovery_language_samples DROP COLUMN sample_format_version");
    if (expectedVersion < 60) reference.exec("DROP TABLE discovery_manual_approvals");
    if (expectedVersion < 59) reference.exec("DROP TABLE discovery_language_samples");
    if (expectedVersion < 57) reference.exec("DROP TABLE discovery_library_identity_versions");
    if (expectedVersion < 56) reference.exec("DROP TABLE discovery_identity_reviews");
    if (expectedVersion < 55) reference.exec("DROP TABLE discovery_target_increases");
    if (expectedVersion < 54) rebuildCreatorTableChanges(reference, "discovery_evidence_reviews", [
      ["coalesce(json_extract(material_json,'$.format_version'),0) IN (1,2)", "json_extract(material_json,'$.format_version') IS 1"],
      ["REFERENCES discovery_search_hits(id)", "REFERENCES discovery_video_identity_bindings(search_hit_id)"],
    ]);
    if (expectedVersion < 53) reference.exec("DROP TABLE discovery_budget_increases");
    if (expectedVersion < 52) reference.exec("DROP TABLE discovery_video_read_failures");
    if (expectedVersion < 51) reference.exec("DROP TABLE discovery_evidence_reviews");
    if (expectedVersion < 50) reference.exec("DROP TABLE discovery_library_source_versions; DROP TABLE discovery_video_identity_bindings");
    if (expectedVersion < 49) reference.exec("DROP TABLE discovery_content_identity_bindings");
    if (expectedVersion < 48) reference.exec("DROP TABLE discovery_profile_identity_matches");
    if (expectedVersion < 47) reference.exec("DROP TABLE discovery_identity_reads");
    if (expectedVersion < 46) rebuildCreatorTable(reference, "discovery_content_reads", "platform in('bilibili','twitter')", "platform='bilibili'");
    if (expectedVersion < 45) rebuildCreatorTable(reference, "discovery_profile_reads", "('bilibili','tiktok','twitter')", "('bilibili','tiktok')");
    if (expectedVersion < 44) reference.exec("DROP TABLE discovery_content_reads");
    if (expectedVersion < 43) reference.exec("DROP TABLE discovery_profile_reads");
    if (expectedVersion < 42) reference.exec("DROP TABLE discovery_search_authors");
    if (expectedVersion < 41) reference.exec("DROP TABLE discovery_public_page_reads");
    if (expectedVersion < 40) reference.exec("DROP TABLE discovery_transcript_reads");
    if (expectedVersion < 39) reference.exec("DROP TABLE discovery_video_completions; DROP TABLE discovery_search_completions");
    if (expectedVersion < 38) reference.exec("DROP TABLE discovery_pending_decisions; DROP TABLE discovery_search_hits; DROP TABLE discovery_candidates; DROP TABLE discovery_platform_runs; DROP TABLE discovery_campaign_revisions; DROP TABLE discovery_campaigns");
    if (expectedVersion < 37) reference.exec(readStorageSql("creator-schema36-retired.sql"));
    if (expectedVersion < 36) {
      reference.exec("DROP TABLE creator_candidate_snapshots; DROP TABLE creator_origins; DROP TABLE audit_events");
      for (const file of ["creator-schema35-campaigns.sql", "creator-schema35-provenance.sql"]) reference.exec(readStorageSql(file));
    }
    if (expectedVersion < 35) reference.exec("ALTER TABLE creators DROP COLUMN tags_json");
    if (expectedVersion < 34) {
      reference.exec("DROP TABLE reply_triage");
      reference.exec(readStorageSql("creator-schema33-tasks.sql"));
    }
    if (expectedVersion < 33) migrateCreatorTikTokEdge(reference, true);
    if (expectedVersion < 32) {
      reference.exec("DROP TABLE mailbox_identity_mappings; DROP TABLE email_auth_connections");
      rebuildCreatorTable(reference, "mailbox_refs", "provider IN ('resend','smtp','imap')", "provider = 'resend'");
    }
    if (expectedVersion < 31) migrateCreatorTaskOriginEdge(reference, true);
    if (expectedVersion < 30) reference.exec("DROP INDEX creator_contact_routes_one_per_locator");
    if (expectedVersion < 29) reference.exec("DROP TABLE mailbox_sync_incremental_continuations; DROP INDEX creator_origins_by_creator_recency; DROP INDEX locator_observations_by_locator_recency; DROP INDEX locator_observations_by_creator_source; DROP INDEX locator_observations_by_platform_account_source; DROP INDEX touchpoints_by_creator_recency; DROP INDEX email_threads_by_creator");
    if (expectedVersion < 28) reference.exec("DROP INDEX creators_by_group; ALTER TABLE creators DROP COLUMN group_id; DROP TABLE creator_groups");
    if (JSON.stringify(creatorSchemaSnapshot(db)) !== JSON.stringify(creatorSchemaSnapshot(reference))) throw new Error(`Creator schema${version} frozen structure differs`);
  } finally { reference.close(); }
  if (expectedVersion >= 35) validateStoredTags(db);
  if (db.prepare("SELECT thread.id FROM email_threads thread WHERE thread.archived_at IS NOT NULL AND NOT EXISTS (SELECT 1 FROM emails email WHERE email.thread_id = thread.id AND email.direction = 'inbound') LIMIT 1").get()) throw new Error("Email archive marker has no inbound message");
  if (db.prepare("SELECT locator_id FROM creator_contact_routes GROUP BY locator_id HAVING COUNT(DISTINCT creator_id) > 1 LIMIT 1").get()) throw new Error("Contact locator belongs to multiple Creators");
  if (expectedVersion >= 90) validatedConnections.set(db, creatorSchemaStamp(db));
}

export async function initializeNewCreatorDatabase(path: string): Promise<void> {
  const absolute = resolve(path);
  await verifyPrivateDirectory(dirname(absolute));
  const reservation = await open(absolute, "wx", 0o600);
  await reservation.close();
  await verifyPrivateFile(absolute);
  const db = new DatabaseSync(absolute);
  let committed = false;
  try {
    db.exec("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; BEGIN IMMEDIATE");
    db.exec(schemaSql());
    migrateCreatorPublicContactEdge(db);
    db.exec(readStorageSql("creator-91-to-92.sql"));
    db.exec(readStorageSql("creator-92-to-93.sql"));
    migrateCreatorWebProfileEdge(db);
    db.exec(readStorageSql("creator-93-to-94.sql"));
    db.exec(readStorageSql("creator-94-to-95.sql"));
    migrateCreatorInstagramDomEdge(db);
    migrateCreatorReceiptPlatformEdge(db, 96);
    db.exec("PRAGMA user_version = 97");
    validateCreatorSchema(db);
    await verifyPrivateFile(absolute);
    db.exec("COMMIT");
    committed = true;
  } finally {
    try { if (!committed && db.isTransaction) db.exec("ROLLBACK"); }
    finally { db.close(); }
  }
}
