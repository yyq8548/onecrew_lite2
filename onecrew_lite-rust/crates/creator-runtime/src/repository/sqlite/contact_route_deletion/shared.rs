use rusqlite::{Connection, ToSql};

pub(super) fn string_rows(
    connection: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<String>, String> {
    let mut statement = connection.prepare(sql).map_err(|error| error.to_string())?;
    statement
        .query_map(params, |row| row.get(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn to_sql_error(error: impl ToString) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::other(error.to_string())),
    )
}
