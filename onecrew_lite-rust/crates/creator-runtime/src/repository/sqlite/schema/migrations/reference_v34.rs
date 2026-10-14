use rusqlite::Connection;

/// Reconstruct the released v34 schema for validation and predecessor fixtures only.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v34(
    connection: &Connection,
) -> Result<(), String> {
    super::downgrade_reference_to_v35(connection)?;
    if super::super::user_version(connection)? != 35 {
        return Ok(());
    }
    connection
        .execute_batch("ALTER TABLE creators DROP COLUMN tags_json; PRAGMA user_version = 34;")
        .map_err(|error| error.to_string())
}
