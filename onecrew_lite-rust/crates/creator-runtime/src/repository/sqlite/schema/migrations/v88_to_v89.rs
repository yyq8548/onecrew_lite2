//! CRM v88 -> v89 (app 0.6.95): allow audited target decreases.
use rusqlite::Connection;

const OLD: &str = "new_target_count > previous_target_count";
const NEW: &str = "new_target_count != previous_target_count";

fn rebuild(db: &Connection, from: &str, to: &str) -> Result<(), String> {
    let sql: Option<String> = db.query_row("SELECT sql FROM sqlite_master WHERE name='discovery_target_increases'", [], |r| r.get(0))
        .optional().map_err(|e| e.to_string())?;
    let Some(sql) = sql else { return Err("target decision predecessor table missing".into()); };
    if !sql.contains(from) { return Err("target decision predecessor constraint mismatch".into()); }
    db.execute_batch("CREATE TEMP TABLE target_decision_copy AS SELECT * FROM discovery_target_increases; DROP TABLE discovery_target_increases;").map_err(|e|e.to_string())?;
    db.execute_batch(&sql.replace(from, to)).map_err(|e|e.to_string())?;
    db.execute_batch("INSERT INTO discovery_target_increases SELECT * FROM target_decision_copy; DROP TABLE target_decision_copy;").map_err(|e|e.to_string())
}

use rusqlite::OptionalExtension;
pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> { rebuild(db, OLD, NEW) }
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    super::v89_to_v90::strip_reference(db)?;
    let sql: Option<String> = db.query_row("SELECT sql FROM sqlite_master WHERE name='discovery_target_increases'", [], |r| r.get(0))
        .optional().map_err(|e| e.to_string())?;
    if sql.is_none_or(|sql| !sql.contains(NEW)) { return Ok(()); }
    rebuild(db, NEW, OLD)
}
