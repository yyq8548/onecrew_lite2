use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::CreatorDatabaseStatus;

use super::{
    CURRENT_SCHEMA_VERSION,
    migrations::collect_chain_migration_impact,
    validation::{validate_current_database, validate_database_for_version},
};

pub(super) const EARLIEST_MIGRATABLE_SCHEMA_VERSION: i64 = 27;

pub(super) fn inspect(path: &Path) -> CreatorDatabaseStatus {
    if !path.exists() {
        return CreatorDatabaseStatus::Fresh;
    }
    let connection = match Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(_) => {
            return CreatorDatabaseStatus::Unsupported {
                version: -1,
                reason: "local business data cannot be safely read".into(),
            };
        }
    };
    if let Err(error) = connection.busy_timeout(std::time::Duration::from_secs(5)) {
        return CreatorDatabaseStatus::Unsupported {
            version: -1,
            reason: format!("local business data cannot be inspected: {error}"),
        };
    }
    let version = match user_version(&connection) {
        Ok(version) => version,
        Err(_) => {
            return CreatorDatabaseStatus::Unsupported {
                version: -1,
                reason: "local business data version cannot be read".into(),
            };
        }
    };
    let table_count = match table_count(&connection) {
        Ok(table_count) => table_count,
        Err(_) => {
            return CreatorDatabaseStatus::Unsupported {
                version,
                reason: "local business data catalog cannot be read".into(),
            };
        }
    };
    if table_count == 0 && version == 0 {
        return CreatorDatabaseStatus::Fresh;
    }
    let validation = match version {
        CURRENT_SCHEMA_VERSION => validate_current_database(&connection),
        version
            if (EARLIEST_MIGRATABLE_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION).contains(&version) =>
        {
            validate_database_for_version(&connection, version)
        }
        _ => {
            return CreatorDatabaseStatus::Unsupported {
                version,
                reason: if version > CURRENT_SCHEMA_VERSION {
                    "local business data was created by a newer application version".into()
                } else {
                    "local business data version has no supported upgrade path".into()
                },
            };
        }
    };
    if let Err(error) = validation {
        return CreatorDatabaseStatus::Unsupported {
            version,
            reason: error,
        };
    }
    if version == CURRENT_SCHEMA_VERSION {
        CreatorDatabaseStatus::Ready
    } else {
        let impact = match collect_chain_migration_impact(&connection, version) {
            Ok(impact) => impact,
            Err(super::migrations::MigrationPreflightError::Blocked(blocker)) => {
                return CreatorDatabaseStatus::Blocked { version, blocker };
            }
            Err(super::migrations::MigrationPreflightError::UnsafeData(reason)) => {
                return CreatorDatabaseStatus::Unsupported { version, reason };
            }
        };
        CreatorDatabaseStatus::UpgradeRequired {
            from_version: version,
            to_version: CURRENT_SCHEMA_VERSION,
            impact,
        }
    }
}

pub(super) fn user_version(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

pub(super) fn table_count(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}
