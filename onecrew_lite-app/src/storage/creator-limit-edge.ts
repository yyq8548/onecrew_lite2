import type { DatabaseSync } from "node:sqlite";

const quote = (name: string) => `"${name.replaceAll('"', '""')}"`;
const legacy = ["max_calls INTEGER NOT NULL CHECK(max_calls >= 1)", "used_calls >= 0 AND used_calls <= max_calls"];
const unlimited = ["max_calls INTEGER CHECK(max_calls IS NULL OR max_calls >= 1)", "used_calls >= 0 AND (max_calls IS NULL OR used_calls <= max_calls)"];

// 原 v64→65 按依赖顺序重建 discovery 表，保留 rowid、历史限额和全部数据。
export function migrateCreatorLimitEdge(db: DatabaseSync, reverse = false): void {
  const definitions = new Map(db.prepare("SELECT name,sql FROM sqlite_schema WHERE type='table' AND name GLOB 'discovery_*' ORDER BY name").all()
    .map((row) => [String(row.name), String(row.sql)]));
  let campaign = definitions.get("discovery_campaigns");
  if (!campaign) throw new Error("Creator campaign limit predecessor is missing");
  for (let index = 0; index < legacy.length; index++) {
    const from = (reverse ? unlimited : legacy)[index]!;
    const to = (reverse ? legacy : unlimited)[index]!;
    if (campaign.split(from).length !== 2) throw new Error("Creator campaign limit predecessor differs");
    campaign = campaign.replace(from, to);
  }
  definitions.set("discovery_campaigns", campaign);
  const dependencies = new Map([...definitions.keys()].map((name) => [name,
    new Set(db.prepare(`PRAGMA foreign_key_list(${quote(name)})`).all().map((row) => String(row.table))
      .filter((parent) => parent !== name && definitions.has(parent)))]));
  const ordered: string[] = [];
  while (dependencies.size) {
    const next = [...dependencies].find(([, parents]) => parents.size === 0)?.[0];
    if (!next) throw new Error("Creator campaign limit migration has cyclic dependencies");
    dependencies.delete(next);
    for (const parents of dependencies.values()) parents.delete(next);
    ordered.push(next);
  }
  const secondary = ordered.flatMap((name) => db.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name")
    .all(name).map((row) => String(row.sql)));
  for (const name of ordered) db.exec(`CREATE TEMP TABLE ${quote(`contact_upgrade_${name}`)} AS SELECT rowid AS preserved_rowid,* FROM ${quote(name)}`);
  for (const name of [...ordered].reverse()) db.exec(`DROP TABLE ${quote(name)}`);
  for (const name of ordered) {
    db.exec(definitions.get(name)!);
    const columns = db.prepare(`PRAGMA table_info(${quote(name)})`).all().map((row) => quote(String(row.name))).join(",");
    const saved = quote(`contact_upgrade_${name}`);
    db.exec(`INSERT INTO ${quote(name)}(rowid,${columns}) SELECT preserved_rowid,${columns} FROM ${saved} ORDER BY preserved_rowid; DROP TABLE ${saved}`);
  }
  for (const sql of secondary) db.exec(sql);
}
