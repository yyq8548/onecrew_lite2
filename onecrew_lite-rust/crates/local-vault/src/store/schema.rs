mod current;
pub(super) mod upgrade;
mod v2;
mod v3;

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::VaultError;

pub(in crate::store) const CURRENT_SCHEMA_VERSION: i64 = 4;
pub(super) const PREVIOUS_SCHEMA_VERSION: i64 = 3;
pub(super) const EARLIEST_MIGRATABLE_SCHEMA_VERSION: i64 = 2;
const REQUIRED_TABLES: &[&str] = &["profiles", "service_credentials"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExistingSchemaStatus {
    Ready,
    UpgradeRequired { from_version: i64, to_version: i64 },
}

pub(super) fn initialize_schema(connection: &Connection) -> Result<(), VaultError> {
    let user_version = schema_version(connection)?;
    let table_count = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(database_error)?;

    if user_version == 0 && table_count == 0 {
        current::create_schema_unchecked(connection)?;
        return validate_current_schema(connection);
    }

    match inspect_existing_schema(connection)? {
        ExistingSchemaStatus::Ready => Ok(()),
        ExistingSchemaStatus::UpgradeRequired {
            from_version,
            to_version,
        } => Err(VaultError::UpgradeRequired {
            from_version,
            to_version,
        }),
    }
}

pub(super) fn inspect_existing_schema(
    connection: &Connection,
) -> Result<ExistingSchemaStatus, VaultError> {
    let version = schema_version(connection)?;
    match version {
        CURRENT_SCHEMA_VERSION => {
            validate_current_schema(connection)?;
            Ok(ExistingSchemaStatus::Ready)
        }
        2 => {
            validate_v2_schema(connection)?;
            Ok(ExistingSchemaStatus::UpgradeRequired {
                from_version: 2,
                to_version: CURRENT_SCHEMA_VERSION,
            })
        }
        PREVIOUS_SCHEMA_VERSION => {
            validate_v3_schema(connection)?;
            Ok(ExistingSchemaStatus::UpgradeRequired {
                from_version: PREVIOUS_SCHEMA_VERSION,
                to_version: CURRENT_SCHEMA_VERSION,
            })
        }
        _ => Err(unsupported_schema(version)),
    }
}

pub(super) fn validate_current_schema(connection: &Connection) -> Result<(), VaultError> {
    validate_schema_against(
        connection,
        CURRENT_SCHEMA_VERSION,
        current::create_schema_unchecked,
    )
}

pub(super) fn validate_v2_schema(connection: &Connection) -> Result<(), VaultError> {
    validate_schema_against(connection, 2, v2::create_schema_unchecked)
}

pub(super) fn validate_v3_schema(connection: &Connection) -> Result<(), VaultError> {
    validate_schema_against(connection, 3, v3::create_schema_unchecked)
}

pub(super) fn validate_schema_for_version(
    connection: &Connection,
    version: i64,
) -> Result<(), VaultError> {
    match version {
        2 => validate_v2_schema(connection),
        3 => validate_v3_schema(connection),
        CURRENT_SCHEMA_VERSION => validate_current_schema(connection),
        _ => Err(unsupported_schema(version)),
    }
}

fn validate_schema_against(
    connection: &Connection,
    version: i64,
    create_reference: fn(&Connection) -> Result<(), VaultError>,
) -> Result<(), VaultError> {
    if schema_version(connection)? != version {
        return Err(schema_mismatch(version, "version marker differs"));
    }
    let reference = Connection::open_in_memory().map_err(database_error)?;
    create_reference(&reference)?;

    let expected_tables = load_table_names(&reference)?;
    let actual_tables = load_table_names(connection)?;
    if actual_tables != expected_tables {
        let missing = expected_tables
            .iter()
            .filter(|table| !actual_tables.contains(table))
            .cloned()
            .collect::<Vec<_>>();
        let unexpected = actual_tables
            .iter()
            .filter(|table| !expected_tables.contains(table))
            .cloned()
            .collect::<Vec<_>>();
        return Err(schema_mismatch(
            version,
            format!(
                "table set differs (missing: {}; unexpected: {})",
                display_names(&missing),
                display_names(&unexpected)
            ),
        ));
    }

    debug_assert_eq!(expected_tables.len(), REQUIRED_TABLES.len());
    let expected_objects = load_schema_objects(&reference)?;
    let actual_objects = load_schema_objects(connection)?;
    if actual_objects != expected_objects {
        let differing_object = expected_objects
            .keys()
            .chain(actual_objects.keys())
            .find(|name| expected_objects.get(*name) != actual_objects.get(*name))
            .map(String::as_str)
            .unwrap_or("unknown");
        return Err(schema_mismatch(
            version,
            format!(
                "column, index, or constraint definition differs at schema object {differing_object}"
            ),
        ));
    }

    Ok(())
}

pub(super) fn schema_version(connection: &Connection) -> Result<i64, VaultError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(database_error)
}

fn load_table_names(connection: &Connection) -> Result<Vec<String>, VaultError> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .map_err(database_error)?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .map(|row| row.map_err(database_error))
        .collect()
}

fn load_schema_objects(connection: &Connection) -> Result<BTreeMap<String, String>, VaultError> {
    let mut statement = connection
        .prepare(
            "SELECT type || ':' || name, sql FROM sqlite_schema
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(database_error)?;
    statement
        .query_map([], |row| {
            let name = row.get::<_, String>(0)?;
            let sql = row.get::<_, String>(1)?;
            Ok((name, normalize_sql(&sql)))
        })
        .map_err(database_error)?
        .map(|row| row.map_err(database_error))
        .collect()
}

fn normalize_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn display_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn unsupported_schema(version: i64) -> VaultError {
    VaultError::Database(format!(
        "unsupported local-vault schema version {version}; this build accepts version {CURRENT_SCHEMA_VERSION} and can explicitly upgrade version {PREVIOUS_SCHEMA_VERSION}"
    ))
}

fn schema_mismatch(version: i64, detail: impl std::fmt::Display) -> VaultError {
    VaultError::Database(format!(
        "local-vault schema version {version} failed its frozen identity check: {detail}"
    ))
}

pub(super) fn database_error(error: rusqlite::Error) -> VaultError {
    VaultError::Database(error.to_string())
}

#[cfg(test)]
pub(super) fn create_v2_schema_for_test(connection: &Connection) -> Result<(), VaultError> {
    v2::create_schema_unchecked(connection)
}

#[cfg(test)]
pub(super) fn create_v3_schema_for_test(connection: &Connection) -> Result<(), VaultError> {
    v3::create_schema_unchecked(connection)
}
