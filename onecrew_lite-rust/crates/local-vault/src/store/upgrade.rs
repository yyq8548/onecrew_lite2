use std::{
    fmt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, ErrorCode, TransactionBehavior, backup::Backup};
use uuid::Uuid;

use super::{
    LocalVault, VaultDataStatus, inspect_existing, open_sqlite_connection,
    schema::{self, CURRENT_SCHEMA_VERSION, EARLIEST_MIGRATABLE_SCHEMA_VERSION},
    sqlite_read_only_flags, sqlite_read_write_flags,
};
use crate::VaultError;
use crate::path_security::{AnchoredParentPath, same_file_identity, set_private_file_permissions};

const UPGRADE_BUSY_TIMEOUT: Duration = Duration::ZERO;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultDataUpgradeStatus {
    Completed,
    AlreadyCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultDataUpgradeResult {
    pub status: VaultDataUpgradeStatus,
    pub from_version: Option<i64>,
    pub to_version: i64,
    pub backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultDataUpgradeFailureKind {
    Busy,
    BackupFailed,
    NewerVersion,
    UnsafeData,
    InitializationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultDataUpgradeError {
    pub kind: VaultDataUpgradeFailureKind,
    pub support_code: String,
    pub storage_upgraded: bool,
}

impl fmt::Display for VaultDataUpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "local-vault upgrade failed ({})",
            self.support_code
        )
    }
}

impl std::error::Error for VaultDataUpgradeError {}

impl LocalVault {
    pub fn upgrade(
        path: impl AsRef<Path>,
    ) -> Result<VaultDataUpgradeResult, VaultDataUpgradeError> {
        upgrade_with_hooks(path.as_ref(), || {}, |_| {}, || Ok(()), || Ok(()))
    }
}

pub(super) fn upgrade_with_hooks(
    path: &Path,
    before_write_lock: impl FnOnce(),
    after_backup: impl FnOnce(&Path),
    before_commit: impl FnOnce() -> Result<(), VaultDataUpgradeError>,
    after_commit: impl FnOnce() -> Result<(), VaultDataUpgradeError>,
) -> Result<VaultDataUpgradeResult, VaultDataUpgradeError> {
    let inspection = inspect_existing(path).map_err(|error| {
        upgrade_error(
            if detected_version(path).is_some_and(|version| version > CURRENT_SCHEMA_VERSION) {
                VaultDataUpgradeFailureKind::NewerVersion
            } else {
                VaultDataUpgradeFailureKind::UnsafeData
            },
            "LOCAL_VAULT_SOURCE_UNSAFE",
            false,
            error,
        )
    })?;
    let from_version = match inspection.status {
        VaultDataStatus::Ready => {
            return Ok(VaultDataUpgradeResult {
                status: VaultDataUpgradeStatus::AlreadyCurrent,
                from_version: None,
                to_version: CURRENT_SCHEMA_VERSION,
                backup_path: None,
            });
        }
        VaultDataStatus::UpgradeRequired {
            from_version,
            to_version: CURRENT_SCHEMA_VERSION,
        } if (EARLIEST_MIGRATABLE_SCHEMA_VERSION..CURRENT_SCHEMA_VERSION)
            .contains(&from_version) =>
        {
            from_version
        }
        VaultDataStatus::Fresh => {
            return Err(upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_SOURCE_MISSING",
                false,
                "local-vault has no records to upgrade",
            ));
        }
        VaultDataStatus::UpgradeRequired { .. } | VaultDataStatus::Unsupported { .. } => {
            return Err(upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_UNSUPPORTED_UPGRADE",
                false,
                "local-vault source is not a supported predecessor",
            ));
        }
    };

    let preflight_identity = source_identity_from_path(path).map_err(|error| {
        upgrade_error(
            VaultDataUpgradeFailureKind::UnsafeData,
            "LOCAL_VAULT_PREFLIGHT_FAILED",
            false,
            error,
        )
    })?;
    before_write_lock();

    let mut connection =
        open_sqlite_connection(path, sqlite_read_write_flags()).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_OPEN_FAILED",
                false,
                error,
            )
        })?;
    connection
        .busy_timeout(UPGRADE_BUSY_TIMEOUT)
        .map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_TIMEOUT_SETUP_FAILED",
                false,
                error,
            )
        })?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            upgrade_error(
                if is_busy(&error) {
                    VaultDataUpgradeFailureKind::Busy
                } else {
                    VaultDataUpgradeFailureKind::UnsafeData
                },
                if is_busy(&error) {
                    "LOCAL_VAULT_BUSY"
                } else {
                    "LOCAL_VAULT_WRITE_LOCK_FAILED"
                },
                false,
                error,
            )
        })?;

    let backup_path = backup_path(path).map_err(|error| {
        upgrade_error(
            VaultDataUpgradeFailureKind::BackupFailed,
            "LOCAL_VAULT_BACKUP_PATH_FAILED",
            false,
            error,
        )
    })?;
    let migration_result = (|| -> Result<(), VaultDataUpgradeError> {
        schema::validate_schema_for_version(&transaction, from_version).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_SOURCE_CHANGED",
                false,
                error,
            )
        })?;
        validate_integrity(&transaction).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_INTEGRITY_FAILED",
                false,
                error,
            )
        })?;
        let locked_inspection = inspect_existing(path).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_SOURCE_CHANGED",
                false,
                error,
            )
        })?;
        if locked_inspection.status
            != (VaultDataStatus::UpgradeRequired {
                from_version,
                to_version: CURRENT_SCHEMA_VERSION,
            })
            || source_identity(&transaction).map_err(|error| {
                upgrade_error(
                    VaultDataUpgradeFailureKind::UnsafeData,
                    "LOCAL_VAULT_SOURCE_CHANGED",
                    false,
                    error,
                )
            })? != preflight_identity
        {
            return Err(upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_SOURCE_CHANGED",
                false,
                "local-vault contents changed before the write lock was acquired",
            ));
        }

        {
            let backup_source =
                open_sqlite_connection(path, sqlite_read_only_flags()).map_err(|error| {
                    upgrade_error(
                        VaultDataUpgradeFailureKind::BackupFailed,
                        "LOCAL_VAULT_BACKUP_SOURCE_FAILED",
                        false,
                        error,
                    )
                })?;
            schema::validate_schema_for_version(&backup_source, from_version).map_err(|error| {
                upgrade_error(
                    VaultDataUpgradeFailureKind::UnsafeData,
                    "LOCAL_VAULT_BACKUP_SOURCE_CHANGED",
                    false,
                    error,
                )
            })?;
            create_online_backup(&backup_source, &backup_path).map_err(|error| {
                upgrade_error(
                    VaultDataUpgradeFailureKind::BackupFailed,
                    "LOCAL_VAULT_BACKUP_FAILED",
                    false,
                    error,
                )
            })?;
        }
        after_backup(&backup_path);

        let backup_inspection = inspect_existing(&backup_path);
        let backup_identity = source_identity_from_path(&backup_path);
        if !matches!(
            backup_inspection,
            Ok(super::VaultDataInspection {
                status: VaultDataStatus::UpgradeRequired {
                    from_version: backup_from_version,
                    to_version: CURRENT_SCHEMA_VERSION,
                },
                ..
            }) if backup_from_version == from_version
        ) || !matches!(backup_identity, Ok(ref identity) if identity == &preflight_identity)
        {
            cleanup_backup_artifacts(&backup_path);
            return Err(upgrade_error(
                VaultDataUpgradeFailureKind::BackupFailed,
                "LOCAL_VAULT_BACKUP_INVALID",
                false,
                "local-vault recovery point did not match the locked source",
            ));
        }

        schema::upgrade::apply_forward_migrations(
            &transaction,
            from_version,
            CURRENT_SCHEMA_VERSION,
        )
        .map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_MIGRATION_FAILED",
                false,
                error,
            )
        })?;
        transaction
            .pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)
            .map_err(|error| {
                upgrade_error(
                    VaultDataUpgradeFailureKind::UnsafeData,
                    "LOCAL_VAULT_VERSION_WRITE_FAILED",
                    false,
                    error,
                )
            })?;
        schema::validate_current_schema(&transaction).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_VALIDATION_FAILED",
                false,
                error,
            )
        })?;
        validate_integrity(&transaction).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::UnsafeData,
                "LOCAL_VAULT_INTEGRITY_FAILED",
                false,
                error,
            )
        })?;
        before_commit()?;
        Ok(())
    })();
    if let Err(error) = migration_result {
        if let Err(rollback) = transaction.rollback() {
            log::warn!("vault upgrade rollback failed: {rollback}");
        }
        return Err(error);
    }

    transaction.commit().map_err(|error| {
        upgrade_error(
            VaultDataUpgradeFailureKind::UnsafeData,
            "LOCAL_VAULT_COMMIT_FAILED",
            false,
            error,
        )
    })?;
    if let Err(mut error) = after_commit() {
        error.storage_upgraded = true;
        return Err(error);
    }

    let postflight = inspect_existing(path).map_err(|error| {
        upgrade_error(
            VaultDataUpgradeFailureKind::InitializationFailed,
            "LOCAL_VAULT_POSTFLIGHT_FAILED",
            true,
            error,
        )
    })?;
    let expected_identity = preflight_identity.migrated_to_current(from_version);
    if postflight.status != VaultDataStatus::Ready
        || source_identity_from_path(path).map_err(|error| {
            upgrade_error(
                VaultDataUpgradeFailureKind::InitializationFailed,
                "LOCAL_VAULT_POSTFLIGHT_FAILED",
                true,
                error,
            )
        })? != expected_identity
    {
        return Err(upgrade_error(
            VaultDataUpgradeFailureKind::InitializationFailed,
            "LOCAL_VAULT_POSTFLIGHT_FAILED",
            true,
            "upgraded local-vault did not pass postflight validation",
        ));
    }

    Ok(VaultDataUpgradeResult {
        status: VaultDataUpgradeStatus::Completed,
        from_version: Some(from_version),
        to_version: CURRENT_SCHEMA_VERSION,
        backup_path: Some(backup_path),
    })
}

pub(super) fn validate_integrity(connection: &Connection) -> Result<(), VaultError> {
    let result = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| VaultError::Database(error.to_string()))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(VaultError::Database(
            "local account data integrity check failed".into(),
        ))
    }
}

fn create_online_backup(source: &Connection, path: &Path) -> Result<(), String> {
    let anchored = AnchoredParentPath::new(path).map_err(|error| error.to_string())?;
    let created = anchored
        .create_final_private()
        .map_err(|error| error.to_string())?;
    if set_private_file_permissions(&created).is_err() {
        cleanup_backup_artifacts(path);
        return Err("local-vault backup permissions could not be restricted".into());
    }

    let result = (|| {
        let mut destination = open_sqlite_connection(path, sqlite_read_write_flags())
            .map_err(|error| error.to_string())?;
        let reopened = anchored
            .open_final_nofollow(false)
            .map_err(|error| error.to_string())?;
        if !same_file_identity(&created, &reopened).map_err(|error| error.to_string())? {
            return Err("local-vault backup identity changed".into());
        }
        let backup = Backup::new(source, &mut destination).map_err(|error| error.to_string())?;
        match backup.step(-1).map_err(|error| error.to_string())? {
            rusqlite::backup::StepResult::Done => {}
            rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => {
                return Err("local-vault backup source is busy".into());
            }
            rusqlite::backup::StepResult::More => {
                return Err("local-vault backup did not complete".into());
            }
            _ => return Err("local-vault backup returned an unknown state".into()),
        }
        drop(backup);
        destination
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        cleanup_backup_artifacts(path);
    }
    result
}

fn cleanup_backup_artifacts(path: &Path) {
    for artifact in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        let Ok(anchored) = AnchoredParentPath::new(&artifact) else {
            continue;
        };
        let Ok(file) = anchored.open_final_for_delete() else {
            continue;
        };
        if let Err(error) = anchored.remove_verified_final(&file) {
            log::warn!("vault upgrade artifact cleanup failed: {error}");
        }
    }
}

fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "local-vault source has no parent directory".to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "local-vault source has no valid file name".to_string())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    Ok(parent.join(format!("{name}.backup-{timestamp}-{}", Uuid::new_v4())))
}

fn detected_version(path: &Path) -> Option<i64> {
    let connection = open_sqlite_connection(path, sqlite_read_only_flags()).ok()?;
    schema::schema_version(&connection).ok()
}

fn source_identity_from_path(path: &Path) -> Result<VaultSourceIdentity, VaultError> {
    let connection = open_sqlite_connection(path, sqlite_read_only_flags())?;
    source_identity(&connection)
}

fn source_identity(connection: &Connection) -> Result<VaultSourceIdentity, VaultError> {
    let mut profiles_statement = connection
        .prepare(
            "SELECT id, platform, provider_account_id, username, encrypted_secret,
                    created_at, updated_at FROM profiles ORDER BY id",
        )
        .map_err(|error| VaultError::Database(error.to_string()))?;
    let profiles = profiles_statement
        .query_map([], |row| {
            Ok(VaultProfileSourceRow {
                id: row.get(0)?,
                platform: row.get(1)?,
                provider_account_id: row.get(2)?,
                username: row.get(3)?,
                encrypted_secret: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .map_err(|error| VaultError::Database(error.to_string()))?
        .map(|row| row.map_err(|error| VaultError::Database(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;
    drop(profiles_statement);

    let mut credentials_statement = connection
        .prepare(
            "SELECT id, provider, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at
             FROM service_credentials ORDER BY id",
        )
        .map_err(|error| VaultError::Database(error.to_string()))?;
    let credentials = credentials_statement
        .query_map([], |row| {
            Ok(VaultCredentialSourceRow {
                id: row.get(0)?,
                provider: row.get(1)?,
                label: row.get(2)?,
                metadata_json: row.get(3)?,
                encrypted_secret: row.get(4)?,
                disabled: row.get(5)?,
                cooldown_until: row.get(6)?,
                last_used_at: row.get(7)?,
                last_error: row.get(8)?,
                created_at: row.get(9)?,
                updated_at: row.get(10)?,
            })
        })
        .map_err(|error| VaultError::Database(error.to_string()))?
        .map(|row| row.map_err(|error| VaultError::Database(error.to_string())))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(VaultSourceIdentity {
        profiles,
        credentials,
    })
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(inner, _)
            if matches!(inner.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked)
    )
}

fn upgrade_error(
    kind: VaultDataUpgradeFailureKind,
    support_code: &str,
    storage_upgraded: bool,
    _detail: impl fmt::Display,
) -> VaultDataUpgradeError {
    VaultDataUpgradeError {
        kind,
        support_code: support_code.into(),
        storage_upgraded,
    }
}

#[derive(PartialEq, Eq)]
struct VaultSourceIdentity {
    profiles: Vec<VaultProfileSourceRow>,
    credentials: Vec<VaultCredentialSourceRow>,
}

impl VaultSourceIdentity {
    fn migrated_to_current(mut self, from_version: i64) -> Self {
        if from_version < 4 {
            for credential in &mut self.credentials {
                if credential.provider == "resend" {
                    credential.provider = "email_resend_api_key".into();
                }
            }
        }
        self
    }
}

#[derive(PartialEq, Eq)]
struct VaultProfileSourceRow {
    id: String,
    platform: String,
    provider_account_id: String,
    username: String,
    encrypted_secret: Vec<u8>,
    created_at: String,
    updated_at: String,
}

#[derive(PartialEq, Eq)]
struct VaultCredentialSourceRow {
    id: String,
    provider: String,
    label: String,
    metadata_json: String,
    encrypted_secret: Vec<u8>,
    disabled: i64,
    cooldown_until: Option<i64>,
    last_used_at: Option<i64>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}
