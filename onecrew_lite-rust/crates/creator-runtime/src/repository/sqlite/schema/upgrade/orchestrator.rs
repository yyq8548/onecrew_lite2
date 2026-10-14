use std::path::Path;

use rusqlite::{Connection, OpenFlags, TransactionBehavior};

use crate::{
    CreatorDatabaseStatus, CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeFailureKind,
    CreatorDatabaseUpgradeResult,
};

use super::super::{
    CURRENT_SCHEMA_VERSION, EARLIEST_MIGRATABLE_SCHEMA_VERSION, apply_forward_migrations,
    collect_chain_migration_impact, inspect, validate_current_database,
    validate_database_for_version, validate_integrity,
};
use super::{
    backup::{
        backup_path, cleanup_backup_artifacts, create_online_backup, probe_backup_environment,
    },
    upgrade_error,
};

pub(in crate::repository::sqlite::schema) fn upgrade(
    path: &Path,
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(
        path,
        |parent| fs2::available_space(parent).map_err(|error| error.to_string()),
        || {},
        |_| {},
        |_| {},
        |_| {},
    )
}

fn upgrade_with_space_probe(
    path: &Path,
    available_space: impl Fn(&Path) -> Result<u64, String>,
    before_write_lock: impl FnOnce(),
    after_backup: impl FnOnce(&Path),
    after_migrations: impl FnOnce(&rusqlite::Transaction<'_>),
    after_commit: impl FnOnce(&Path),
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    let (from_version, preflight_impact) = match inspect(path) {
        CreatorDatabaseStatus::UpgradeRequired {
            from_version,
            to_version: CURRENT_SCHEMA_VERSION,
            impact,
        } if (EARLIEST_MIGRATABLE_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION)
            .contains(&from_version) =>
        {
            (from_version, impact)
        }
        CreatorDatabaseStatus::Ready => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_ALREADY_CURRENT",
                "Creator data is already current",
                false,
            ));
        }
        CreatorDatabaseStatus::Fresh => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_NOTHING_TO_UPGRADE",
                "Creator data has no records to upgrade",
                false,
            ));
        }
        CreatorDatabaseStatus::UpgradeRequired {
            from_version,
            to_version,
            ..
        } => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_UNSUPPORTED_UPGRADE",
                format!("unsupported Creator data upgrade: {from_version} -> {to_version}"),
                false,
            ));
        }
        CreatorDatabaseStatus::Blocked { blocker, .. } => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                blocker.support_code(),
                blocker.safe_message(),
                false,
            ));
        }
        CreatorDatabaseStatus::Unsupported { version, reason } => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_UNSAFE_SOURCE",
                format!("Creator data version {version} cannot be safely upgraded: {reason}"),
                false,
            ));
        }
    };

    let mut connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_OPEN_FAILED",
                error,
                false,
            )
        })?;
    before_write_lock();
    connection
        .busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_TIMEOUT_SETUP_FAILED",
                error,
                false,
            )
        })?;

    connection
        .execute_batch("PRAGMA foreign_keys=OFF;")
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_PREPARE_FAILED",
                error,
                false,
            )
        })?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            let kind = if is_sqlite_busy(&error) {
                CreatorDatabaseUpgradeFailureKind::Busy
            } else {
                CreatorDatabaseUpgradeFailureKind::UnsafeData
            };
            upgrade_error(kind, "LOCAL_DATA_BUSY", error, false)
        })?;

    // The source is revalidated after the write lock is held. The backup is
    // then taken from this exact transaction snapshot, so no writer can commit
    // between the recovery point and the migration.
    let backup_path = backup_path(path).map_err(|error| {
        upgrade_error(
            CreatorDatabaseUpgradeFailureKind::BackupFailed,
            "LOCAL_DATA_BACKUP_PATH_FAILED",
            error,
            false,
        )
    })?;
    let migration_result = (|| -> Result<(), CreatorDatabaseUpgradeError> {
        validate_database_for_version(&transaction, from_version).map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_SOURCE_CHANGED",
                error,
                false,
            )
        })?;
        validate_integrity(&transaction).map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_INTEGRITY_FAILED",
                error,
                false,
            )
        })?;
        let locked_impact =
            collect_chain_migration_impact(&transaction, from_version).map_err(|error| {
                upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::UnsafeData,
                    match &error {
                        super::super::migrations::MigrationPreflightError::Blocked(blocker) => {
                            blocker.support_code()
                        }
                        _ => "LOCAL_DATA_SOURCE_CHANGED",
                    },
                    error,
                    false,
                )
            })?;
        if locked_impact != preflight_impact {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_SOURCE_CHANGED",
                "Creator data upgrade impact changed before the write lock was acquired",
                false,
            ));
        }
        probe_backup_environment(path, &available_space)?;
        // SQLite's backup API cannot use the connection that owns a write
        // transaction. A second read-only connection is safe here: the
        // transaction above holds the RESERVED write lock, so no other writer
        // can commit while this source snapshot is copied.
        {
            let backup_source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|error| {
                    upgrade_error(
                        CreatorDatabaseUpgradeFailureKind::BackupFailed,
                        "LOCAL_DATA_BACKUP_SOURCE_FAILED",
                        error,
                        false,
                    )
                })?;
            validate_database_for_version(&backup_source, from_version).map_err(|error| {
                upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::UnsafeData,
                    "LOCAL_DATA_BACKUP_SOURCE_CHANGED",
                    error,
                    false,
                )
            })?;
            create_online_backup(&backup_source, &backup_path).map_err(|error| {
                upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::BackupFailed,
                    "LOCAL_DATA_BACKUP_FAILED",
                    error,
                    false,
                )
            })?;
        }
        after_backup(&backup_path);
        let backup_validation = inspect(&backup_path);
        if backup_validation
            != (CreatorDatabaseStatus::UpgradeRequired {
                from_version,
                to_version: CURRENT_SCHEMA_VERSION,
                impact: preflight_impact.clone(),
            })
        {
            cleanup_backup_artifacts(&backup_path);
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::BackupFailed,
                "LOCAL_DATA_BACKUP_INVALID",
                format!("recovery point validation failed: {backup_validation:?}"),
                false,
            ));
        }
        apply_forward_migrations(&transaction, from_version, CURRENT_SCHEMA_VERSION).map_err(
            |error| {
                upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::UnsafeData,
                    "LOCAL_DATA_MIGRATION_FAILED",
                    error,
                    false,
                )
            },
        )?;
        after_migrations(&transaction);
        // The complete current validator runs while the migration transaction
        // is still rollback-capable. Structural validity alone is not enough:
        // archive membership and locator ownership are business invariants and
        // must fail before the version stamp or commit.
        validate_current_database(&transaction).map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_VALIDATION_FAILED",
                error,
                false,
            )
        })?;
        transaction
            .execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
            .map_err(|error| {
                upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::UnsafeData,
                    "LOCAL_DATA_VERSION_WRITE_FAILED",
                    error,
                    false,
                )
            })?;
        Ok(())
    })();
    if let Err(error) = migration_result {
        if let Err(rollback) = transaction.rollback() {
            log::warn!("schema upgrade rollback failed: {rollback}");
        }
        if let Err(pragma) = connection.execute_batch("PRAGMA foreign_keys=ON;") {
            log::warn!("re-enabling foreign keys after a failed upgrade failed: {pragma}");
        }
        return Err(error);
    }
    transaction.commit().map_err(|error| {
        upgrade_error(
            CreatorDatabaseUpgradeFailureKind::UnsafeData,
            "LOCAL_DATA_COMMIT_FAILED",
            error,
            false,
        )
    })?;
    connection
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_FOREIGN_KEYS_FAILED",
                error,
                true,
            )
        })?;
    drop(connection);
    after_commit(path);
    // Reopen after commit so postflight is an independent storage readback,
    // not validation through the connection that performed the migration.
    let readback =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_POSTFLIGHT_OPEN_FAILED",
                error,
                true,
            )
        })?;
    readback
        .execute_batch("PRAGMA foreign_keys=ON;")
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_POSTFLIGHT_PREPARE_FAILED",
                error,
                true,
            )
        })?;
    validate_current_database(&readback).map_err(|error| {
        upgrade_error(
            CreatorDatabaseUpgradeFailureKind::UnsafeData,
            "LOCAL_DATA_POSTFLIGHT_FAILED",
            error,
            true,
        )
    })?;
    Ok(CreatorDatabaseUpgradeResult {
        from_version,
        to_version: CURRENT_SCHEMA_VERSION,
        backup_path: backup_path.to_string_lossy().into_owned(),
        impact: preflight_impact,
    })
}

fn is_sqlite_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
    )
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn upgrade_with_available_space_for_test(
    path: &Path,
    available: u64,
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(path, |_| Ok(available), || {}, |_| {}, |_| {}, |_| {})
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn upgrade_with_before_lock_for_test(
    path: &Path,
    before_write_lock: impl FnOnce(),
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(
        path,
        |parent| fs2::available_space(parent).map_err(|error| error.to_string()),
        before_write_lock,
        |_| {},
        |_| {},
        |_| {},
    )
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn upgrade_with_invalid_backup_for_test(
    path: &Path,
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(
        path,
        |parent| fs2::available_space(parent).map_err(|error| error.to_string()),
        || {},
        |backup| std::fs::write(backup, b"invalid recovery point").unwrap(),
        |_| {},
        |_| {},
    )
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn upgrade_with_post_migration_mutation_for_test(
    path: &Path,
    after_migrations: impl FnOnce(&rusqlite::Transaction<'_>),
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(
        path,
        |parent| fs2::available_space(parent).map_err(|error| error.to_string()),
        || {},
        |_| {},
        after_migrations,
        |_| {},
    )
}

#[cfg(test)]
pub(in crate::repository::sqlite) fn upgrade_with_post_commit_mutation_for_test(
    path: &Path,
    after_commit: impl FnOnce(&Path),
) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
    upgrade_with_space_probe(
        path,
        |parent| fs2::available_space(parent).map_err(|error| error.to_string()),
        || {},
        |_| {},
        |_| {},
        after_commit,
    )
}
