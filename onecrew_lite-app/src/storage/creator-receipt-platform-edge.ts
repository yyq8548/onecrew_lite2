import type { DatabaseSync } from "node:sqlite";

const tables = ["discovery_profile_reads", "discovery_content_reads", "discovery_profile_identity_matches", "discovery_content_identity_bindings", "discovery_facebook_content_targets"];
const additions = new Map<number, string[]>([[66, ["instagram"]], [67, ["weibo"]], [68, ["bluesky"]], [69, ["zhihu"]], [70, ["hackernews"]], [71, ["pinterest"]], [73, ["pixiv", "linkedin", "douyin"]], [96, ["xiaohongshu"]]]);
const identifier = (name: string) => `"${name.replaceAll('"', '""')}"`;
const platformList = (values: string[]) => `('${values.join("','")}')`;

// 扩展 native receipt 平台；保留五张父子表、全部显式对象与历史行。
export function migrateCreatorReceiptPlatformEdge(db: DatabaseSync, version: number, reverse = false): void {
  const added = additions.get(version);
  if (!added) throw new Error(`Receipt platform migration ${version} is not registered`);
  const previous = [...additions].filter(([edge]) => edge < version).flatMap(([, platforms]) => platforms);
  const profiles = ["bilibili", "tiktok", "twitter", "reddit", "facebook", ...previous];
  const contents = ["bilibili", "twitter", "reddit", "facebook", ...previous];
  const definitions = tables.map((table) => {
    const row = db.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(table);
    if (!row || typeof row.sql !== "string") throw new Error(`Receipt predecessor ${table} is missing`);
    return row.sql;
  });
  for (const [index, before] of [[0, profiles], [1, contents]] as const) {
    const oldList = platformList(before);
    const newList = platformList([...before, ...added]);
    const from = reverse ? newList : oldList;
    const to = reverse ? oldList : newList;
    if (definitions[index]!.split(from).length !== 2) throw new Error("Receipt platform predecessor constraint differs");
    definitions[index] = definitions[index]!.replace(from, to);
  }
  const objects = tables.flatMap((table) => db.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").all(table).map((row) => String(row.sql)));
  db.exec("SAVEPOINT creator_receipt_platforms");
  let released = false;
  try {
    for (const table of tables) db.exec(`CREATE TEMP TABLE ${identifier(`receipt_copy_${table}`)} AS SELECT * FROM ${identifier(table)}`);
    for (const table of [...tables].reverse()) db.exec(`DROP TABLE ${identifier(table)}`);
    for (const [index, table] of tables.entries()) {
      db.exec(definitions[index]!);
      const copy = identifier(`receipt_copy_${table}`);
      db.exec(`INSERT INTO ${identifier(table)} SELECT * FROM ${copy}; DROP TABLE ${copy}`);
    }
    for (const sql of objects) db.exec(sql);
    db.exec("RELEASE creator_receipt_platforms");
    released = true;
  } finally {
    if (!released) db.exec("ROLLBACK TO creator_receipt_platforms; RELEASE creator_receipt_platforms");
  }
}
