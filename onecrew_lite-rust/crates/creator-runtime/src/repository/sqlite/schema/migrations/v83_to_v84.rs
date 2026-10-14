//! CRM v83 -> v84 (app 0.6.80): public messaging contact routes.
//! Extend contact routes without altering saved tasks, approvals or observations.
use rusqlite::Connection;
const OLD: &str = "kind IN ('email','telegram','hidden_email','social_profile')";
const NEW: &str = "kind IN ('email','telegram','hidden_email','social_profile','whatsapp','wechat','wechat_official')";
fn rebuild(db: &Connection, old: &str, new: &str) -> Result<(), String> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_contact_observations'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if sql.matches(old).count() != 1 {
        return Err("contact_kind_constraint_mismatch".into());
    }
    let mut query = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name='discovery_contact_observations' AND type IN ('index','trigger') AND sql IS NOT NULL").map_err(|e|e.to_string())?;
    let secondary = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(query);
    db.execute_batch("CREATE TEMP TABLE contact_route_upgrade AS SELECT * FROM discovery_contact_observations; DROP TABLE discovery_contact_observations;").map_err(|e|e.to_string())?;
    db.execute_batch(&sql.replacen(old, new, 1))
        .map_err(|e| e.to_string())?;
    db.execute_batch("INSERT INTO discovery_contact_observations SELECT * FROM contact_route_upgrade; DROP TABLE contact_route_upgrade;").map_err(|e|e.to_string())?;
    for sql in secondary {
        db.execute_batch(&sql).map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, OLD, NEW)
}
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_contact_observations'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if sql.contains(NEW) {
        rebuild(db, NEW, OLD)?;
    }
    Ok(())
}
