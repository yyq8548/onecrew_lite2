//! CRM v84 -> v85 (app 0.6.88): preserve newly observed search order.
//! Historical hits have no recoverable provider order and remain unranked.
use rusqlite::Connection;

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    db.execute_batch(
        "CREATE TABLE discovery_search_positions (
            position INTEGER PRIMARY KEY,
            search_hit_id TEXT NOT NULL UNIQUE REFERENCES discovery_search_hits(id) ON DELETE CASCADE
        );",
    ).map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    super::v85_to_v86::strip_reference(db)?;
    db.execute_batch("DROP TABLE IF EXISTS discovery_search_positions;")
        .map_err(|e| e.to_string())
}
