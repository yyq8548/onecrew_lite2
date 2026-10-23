import type { DatabaseSync } from "node:sqlite";

// 原 v30→31 只退休 envoy/dispatcher，保留 external 运行和回复业务事实。
export function migrateCreatorTaskOriginEdge(db: DatabaseSync, reverse = false): void {
  const tables = ["task_runs", "task_attention", "reply_triage"];
  const definitions = tables.map(name => {
    const row = db.prepare("SELECT sql FROM sqlite_schema WHERE name=? AND type='table'").get(name);
    if (!row || typeof row.sql !== "string") throw new Error("Task origin predecessor table is missing");
    return row.sql;
  });
  for (const [before, after] of [["origin IN ('envoy','dispatcher','external')", "origin IN ('external')"], ["CHECK(origin = 'envoy' OR chat_session_id IS NULL)", "CHECK(chat_session_id IS NULL)"]]) {
    const from = reverse ? after! : before!;
    if (definitions[0]!.split(from).length !== 2) throw new Error("Task origin predecessor constraint differs");
    definitions[0] = definitions[0]!.replace(from, reverse ? before! : after!);
  }
  if (!reverse) db.exec("DELETE FROM task_attention WHERE task_run_id IN (SELECT id FROM task_runs WHERE origin IN ('envoy','dispatcher')); UPDATE reply_triage SET task_run_id=NULL WHERE task_run_id IN (SELECT id FROM task_runs WHERE origin IN ('envoy','dispatcher')); DELETE FROM task_runs WHERE origin IN ('envoy','dispatcher')");
  const secondary = tables.map(name => db.prepare("SELECT sql FROM sqlite_schema WHERE tbl_name=? AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY name").all(name).map(row => String(row.sql)));
  for (const name of tables) db.exec(`CREATE TEMP TABLE origin_upgrade_${name} AS SELECT * FROM ${name}`);
  for (const name of [...tables].reverse()) db.exec(`DROP TABLE ${name}`);
  for (const [index, name] of tables.entries()) {
    db.exec(definitions[index]!);
    db.exec(`INSERT INTO ${name} SELECT * FROM origin_upgrade_${name}; DROP TABLE origin_upgrade_${name}`);
    for (const sql of secondary[index]!) db.exec(sql);
  }
}
