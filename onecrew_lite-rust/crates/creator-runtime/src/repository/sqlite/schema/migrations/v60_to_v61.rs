//! CRM v60 -> v61 (app 0.6.41): version language-only comments without commenter IDs.
//! All historical sample JSON and review hashes remain unchanged as format 1.
use rusqlite::Connection;

pub(in crate::repository::sqlite) const SQL: &str = "
ALTER TABLE discovery_language_samples ADD COLUMN sample_format_version INTEGER NOT NULL DEFAULT 1
  CHECK(sample_format_version IN (1,2));
";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(SQL).map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn strip_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("ALTER TABLE discovery_language_samples DROP COLUMN sample_format_version;")
        .map_err(|e| e.to_string())
}
