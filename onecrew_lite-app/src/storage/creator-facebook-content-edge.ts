import type { DatabaseSync } from "node:sqlite";

export function migrateCreatorFacebookContentEdge(db: DatabaseSync, reverse = false, version: 61 | 62 | 63 = 63): void {
  const tables = version === 63 ? ["discovery_content_reads", "discovery_content_identity_bindings"]
    : version === 62 ? ["discovery_profile_reads", "discovery_profile_identity_matches", "discovery_content_identity_bindings"]
    : ["discovery_profile_reads", "discovery_content_reads", "discovery_profile_identity_matches", "discovery_content_identity_bindings"];
  const changes = version === 63 ? [[0, "platform IN ('bilibili','twitter','reddit')", "platform IN ('bilibili','twitter','reddit','facebook')"]] as const
    : version === 62 ? [[0, "platform IN ('bilibili','tiktok','twitter','reddit')", "platform IN ('bilibili','tiktok','twitter','reddit','facebook')"]] as const
    : [[0, "platform IN ('bilibili','tiktok','twitter')", "platform IN ('bilibili','tiktok','twitter','reddit')"], [1, "platform IN ('bilibili','twitter')", "platform IN ('bilibili','twitter','reddit')"]] as const;
  const definitions = tables.map((table) => {
    const row = db.prepare("SELECT sql FROM sqlite_schema WHERE type='table' AND name=?").get(table);
    if (!row || typeof row.sql !== "string") throw new Error("Facebook content predecessor is missing");
    return row.sql;
  });
  for (const [index, before, after] of changes) {
    const oldList = before.slice("platform IN ".length);
    const newList = after.slice("platform IN ".length);
    const from = reverse ? newList : oldList;
    if (definitions[index]!.split(from).length !== 2) throw new Error("Native receipt predecessor constraint differs");
    definitions[index] = definitions[index]!.replace(from, reverse ? oldList : newList);
  }
  for (const table of tables) db.exec(`CREATE TEMP TABLE facebook_content_upgrade_${table} AS SELECT * FROM ${table}`);
  for (const table of [...tables].reverse()) db.exec(`DROP TABLE ${table}`);
  for (const [index, table] of tables.entries()) {
    db.exec(definitions[index]!);
    db.exec(`INSERT INTO ${table} SELECT * FROM facebook_content_upgrade_${table}; DROP TABLE facebook_content_upgrade_${table}`);
  }
}
