import { readStorageSql } from "./sql-resource.ts";
import { DatabaseSync } from "node:sqlite";
import { cstVisitor, parse, show } from "sql-parser-cst";
import type { Node } from "sql-parser-cst";

const tables = ["platform_accounts", "account_refs", "observation_sources", "resolved_targets", "creator_candidate_snapshots", "contact_locators", "conversations"];
const quote = (name: string) => `"${name.replaceAll('"', '""')}"`;
const platforms = "('x','instagram','telegram','youtube','tiktok')";
const priorPlatforms = "('x','instagram','telegram','youtube')";
const channels = "'email','x','instagram','telegram','youtube','tiktok','website','link_hub'";
const priorChannels = "'email','x','instagram','telegram','youtube','website','link_hub'";

function predecessorSnapshot(sql: string): string {
  const fieldTree = parse("CREATE TABLE external_column(account_external_id TEXT NOT NULL CHECK(length(trim(account_external_id)) > 0))", { dialect: "sqlite", includeSpaces: true });
  let replacement: Extract<Node, { type: "column_definition" }> | undefined;
  cstVisitor({ column_definition: node => { replacement = node; } })(fieldTree);
  if (!replacement) throw new Error("Snapshot external identity definition is missing");
  const tree = parse(sql, { dialect: "sqlite", includeSpaces: true, includeNewlines: true });
  let changed = 0;
  cstVisitor({ column_definition: node => {
    if (node.name.name === "account_external_id") { Object.assign(node, replacement); changed++; }
  } })(tree);
  if (changed !== 1) throw new Error("Snapshot external identity predecessor differs");
  const serialized = show(tree);
  const from = "json_type(candidate_json, '$.account.external_id') IS CASE WHEN account_external_id IS NULL THEN 'null' ELSE 'text' END";
  const identity = "json_extract(candidate_json, '$.account.external_id') IS account_external_id";
  if (serialized.split(from).length !== 2 || serialized.split(identity).length !== 2) throw new Error("Snapshot external identity checks differ");
  return serialized.replace(from, "json_type(candidate_json, '$.account.external_id') = 'text'")
    .replace(identity, "json_extract(candidate_json, '$.account.external_id') = account_external_id");
}

function targetReference(): DatabaseSync {
  const db = new DatabaseSync(":memory:");
  for (const file of ["creator-schema90.sql", "creator-indexes90.sql", "creator-triggers90.sql", "creator-schema36-retired.sql"]) db.exec(readStorageSql(file));
  db.exec("DROP TABLE creator_candidate_snapshots; DROP TABLE creator_origins; DROP TABLE audit_events");
  for (const file of ["creator-schema35-campaigns.sql", "creator-schema35-provenance.sql"]) db.exec(readStorageSql(file));
  return db;
}

// 原 v32→33 的七张表重建；外层事务关闭 FK，恢复历史数据及显式索引。
export function migrateCreatorTikTokEdge(db: DatabaseSync, reverse = false): void {
  const reference = reverse ? undefined : targetReference();
  try {
    if (reverse) db.exec("DROP TABLE tiktok_one_account_bindings");
    for (const name of tables) {
      const source = reference ?? db;
      const row = source.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(name);
      if (!row || typeof row.sql !== "string") throw new Error(`TikTok predecessor ${name} is missing`);
      let sql = row.sql;
      if (name === "contact_locators" || name === "conversations") {
        if (!reverse) sql = sql.replace(`${channels},'wechat'`, channels);
        if (reverse) sql = sql.replace(channels, priorChannels);
      } else if (reverse) {
        sql = sql.replace(platforms, priorPlatforms);
        if (name === "creator_candidate_snapshots") sql = predecessorSnapshot(sql);
      }
      const objects = source.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").all(name);
      db.exec(`CREATE TEMP TABLE tiktok_upgrade_copy AS SELECT * FROM ${quote(name)}; DROP TABLE ${quote(name)}`);
      db.exec(sql);
      db.exec(`INSERT INTO ${quote(name)} SELECT * FROM tiktok_upgrade_copy; DROP TABLE tiktok_upgrade_copy`);
      for (const object of objects) db.exec(String(object.sql));
    }
    if (reference) db.exec(String(reference.prepare("SELECT sql FROM sqlite_schema WHERE name='tiktok_one_account_bindings'").get()!.sql));
  } finally { reference?.close(); }
}
