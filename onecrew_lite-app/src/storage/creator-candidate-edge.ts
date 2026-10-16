import type { DatabaseSync } from "node:sqlite";
import { cstVisitor, parse, show } from "sql-parser-cst";
import type { Node } from "sql-parser-cst";

const identifier = (name: string) => `"${name.replaceAll('"', '""')}"`;
const columns = "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at";

function candidateDefinition(sql: string, reverse: boolean): string {
  const tree = parse(sql, { dialect: "sqlite", includeSpaces: true, includeComments: true, includeNewlines: true });
  let field: Extract<Node, { type: "column_definition" }> | undefined;
  cstVisitor({ column_definition: (node) => { field = node; } })(parse("CREATE TABLE candidate_column(source_platform TEXT NOT NULL CHECK(source_platform=platform OR (source_platform IN ('google','yahoo') AND platform IN ('youtube','tiktok','twitter','bilibili','reddit','weibo','zhihu','instagram','facebook','pinterest','linkedin','douyin','pixiv'))))", { dialect: "sqlite", includeSpaces: true }));
  if (!field) throw new Error("Candidate source column definition is missing");
  let changedColumns = 0;
  let changedKeys = 0;
  cstVisitor({
    create_table_stmt: (node) => {
      if (!node.columns) throw new Error("Candidate predecessor columns are missing");
      const items = node.columns.expr.items;
      const existing = items.findIndex((item) => item.type === "column_definition" && item.name.name === "source_platform");
      if (reverse) {
        if (existing < 0) throw new Error("Candidate predecessor source_platform is missing");
        items.splice(existing, 1);
      } else {
        if (existing >= 0) throw new Error("Candidate predecessor already has source_platform");
        const anchor = items.findIndex((item) => item.type === "column_definition" && item.name.name === "updated_at");
        if (anchor < 0) throw new Error("Candidate predecessor updated_at is missing");
        items.splice(anchor + 1, 0, field!);
      }
      changedColumns++;
    },
    constraint_foreign_key: (node) => {
      const names = node.columns.expr.items.map((item) => item.name);
      const expected = reverse ? "source_platform" : "platform";
      if (names.join(",") === `discovery_campaign_id,${expected},source_profile_ref`) {
        const column = node.columns.expr.items[1]!;
        column.name = reverse ? "platform" : "source_platform";
        column.text = column.name;
        changedKeys++;
      }
    },
  })(tree);
  if (changedColumns !== 1 || changedKeys !== 1) throw new Error("Candidate source predecessor structure differs");
  return show(tree);
}

// 原 v76→77：按 FK 后代闭包重建，先恢复所有行，再恢复索引和触发器。
export function migrateCreatorCandidateSourceEdge(db: DatabaseSync, reverse = false): void {
  const definitions = new Map(db.prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").all().map((row) => [String(row.name), String(row.sql)]));
  const parents = new Map([...definitions.keys()].map((name) => [name, new Set(db.prepare(`PRAGMA foreign_key_list(${identifier(name)})`).all().map((row) => String(row.table)))]));
  const affected = new Set(["discovery_candidates"]);
  let previous = 0;
  while (previous !== affected.size) {
    previous = affected.size;
    for (const [name, relations] of parents) if ([...relations].some((parent) => affected.has(parent))) affected.add(name);
  }
  const order: string[] = [];
  while (order.length < affected.size) {
    const next = [...affected].sort().find((name) => !order.includes(name) && [...parents.get(name)!].every((parent) => parent === name || !affected.has(parent) || order.includes(parent)));
    if (!next) throw new Error("Candidate source migration has cyclic dependencies");
    order.push(next);
  }
  const candidate = definitions.get("discovery_candidates");
  if (!candidate) throw new Error("Candidate predecessor is absent");
  definitions.set("discovery_candidates", candidateDefinition(candidate, reverse));
  const objects = order.flatMap((name) => db.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").all(name).map((row) => String(row.sql)));
  db.exec("SAVEPOINT creator_candidate_source");
  let released = false;
  try {
    for (const name of order) db.exec(`CREATE TEMP TABLE ${identifier(`candidate_copy_${name}`)} AS SELECT * FROM ${identifier(name)}`);
    for (const name of [...order].reverse()) db.exec(`DROP TABLE ${identifier(name)}`);
    for (const name of order) {
      db.exec(definitions.get(name)!);
      const saved = identifier(`candidate_copy_${name}`);
      const restore = name === "discovery_candidates" ? reverse
        ? `INSERT INTO discovery_candidates(${columns}) SELECT ${columns} FROM ${saved}`
        : `INSERT INTO discovery_candidates(${columns},source_platform) SELECT ${columns},platform FROM ${saved}`
        : `INSERT INTO ${identifier(name)} SELECT * FROM ${saved}`;
      db.exec(`${restore}; DROP TABLE ${saved}`);
    }
    for (const sql of objects) db.exec(sql);
    db.exec("RELEASE creator_candidate_source");
    released = true;
  } finally {
    if (!released) db.exec("ROLLBACK TO creator_candidate_source; RELEASE creator_candidate_source");
  }
}
