//! CRM v85 -> v86 (app 0.6.89): admit Gmail mailbox projections.
use rusqlite::Connection;

const OLD: &str = "'resend','smtp','imap'";
const NEW: &str = "'resend','smtp','imap','gmail'";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, OLD, NEW)
}

fn rebuild(db: &Connection, from: &str, to: &str) -> Result<(), String> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='mailbox_refs'",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !sql.contains(from) {
        return Err("mailbox provider constraint differs from predecessor".into());
    }
    db.execute_batch("CREATE TEMP TABLE mailbox_refs_copy AS SELECT * FROM mailbox_refs; DROP TABLE mailbox_refs;")
        .map_err(|e| e.to_string())?;
    db.execute_batch(&sql.replace(from, to))
        .map_err(|e| e.to_string())?;
    db.execute_batch(
        "INSERT INTO mailbox_refs SELECT * FROM mailbox_refs_copy; DROP TABLE mailbox_refs_copy;",
    )
    .map_err(|e| e.to_string())
}

/// Historical reference construction only; production upgrades never downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    super::v86_to_v87::strip_reference(db)?;
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='mailbox_refs'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !sql.contains(NEW) {
        return Ok(());
    }
    db.execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(|e| e.to_string())?;
    rebuild(db, NEW, OLD)
}
