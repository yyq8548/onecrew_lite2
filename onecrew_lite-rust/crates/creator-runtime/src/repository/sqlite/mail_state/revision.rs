use rusqlite::{Connection, OptionalExtension};

pub(super) fn ensure_revision_change(
    connection: &Connection,
    table: &str,
    id: &str,
    expected: u64,
    changed: usize,
    entity: &str,
) -> Result<(), String> {
    if changed == 1 {
        return Ok(());
    }
    let actual =
        current_revision(connection, table, id)?.ok_or_else(|| format!("{entity} not found"))?;
    Err(revision_conflict(entity, expected, actual))
}

pub(super) fn current_revision(
    connection: &Connection,
    table: &str,
    id: &str,
) -> Result<Option<u64>, String> {
    let sql = match table {
        "email_templates" => "SELECT revision FROM email_templates WHERE id = ?1",
        "email_drafts" => "SELECT revision FROM email_drafts WHERE id = ?1",
        _ => return Err("unsupported revision entity".into()),
    };
    connection
        .query_row(sql, [id], |row| row.get(0))
        .optional()
        .map_err(|error| error.to_string())
}

pub(super) fn revision_conflict(entity: &str, expected: u64, actual: u64) -> String {
    format!("{entity} revision conflict: expected {expected}, current {actual}")
}

pub(super) fn conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}
