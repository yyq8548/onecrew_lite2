use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, backup::Backup};
use uuid::Uuid;

use crate::{CreatorDatabaseUpgradeError, CreatorDatabaseUpgradeFailureKind};

use super::upgrade_error;

#[cfg(windows)]
mod windows;

#[cfg(all(test, windows))]
mod windows_tests;

pub(super) fn probe_backup_environment(
    path: &Path,
    available_space: &impl Fn(&Path) -> Result<u64, String>,
) -> Result<(), CreatorDatabaseUpgradeError> {
    let parent = path.parent().ok_or_else(|| {
        upgrade_error(
            CreatorDatabaseUpgradeFailureKind::BackupFailed,
            "LOCAL_DATA_BACKUP_PATH_FAILED",
            "source has no parent directory",
            false,
        )
    })?;
    let probe = parent.join(format!(".local-data-write-probe-{}", Uuid::new_v4()));
    let probe_result = OpenOptions::new().write(true).create_new(true).open(&probe);
    match probe_result {
        Ok(_) => {
            if let Err(error) = std::fs::remove_file(&probe) {
                return Err(upgrade_error(
                    CreatorDatabaseUpgradeFailureKind::BackupFailed,
                    "LOCAL_DATA_WRITE_PROBE_CLEANUP_FAILED",
                    error,
                    false,
                ));
            }
        }
        Err(error) => {
            return Err(upgrade_error(
                CreatorDatabaseUpgradeFailureKind::BackupFailed,
                "LOCAL_DATA_TARGET_NOT_WRITABLE",
                error,
                false,
            ));
        }
    }
    let source_size = std::fs::metadata(path)
        .map(|value| value.len())
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_SOURCE_SIZE_FAILED",
                error,
                false,
            )
        })?;
    let wal_size = std::fs::metadata(path.with_extension("db-wal"))
        .map(|value| value.len())
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Ok(0)
            } else {
                Err(error)
            }
        })
        .map_err(|error| {
            upgrade_error(
                CreatorDatabaseUpgradeFailureKind::UnsafeData,
                "LOCAL_DATA_SOURCE_SIZE_FAILED",
                error,
                false,
            )
        })?;
    const SAFETY_MARGIN: u64 = 16 * 1024 * 1024;
    let required = source_size
        .saturating_add(wal_size)
        .saturating_mul(2)
        .saturating_add(SAFETY_MARGIN);
    let available = available_space(parent).map_err(|error| {
        upgrade_error(
            CreatorDatabaseUpgradeFailureKind::UnsafeData,
            "LOCAL_DATA_SPACE_CHECK_FAILED",
            error,
            false,
        )
    })?;
    if available < required {
        return Err(upgrade_error(
            CreatorDatabaseUpgradeFailureKind::InsufficientSpace,
            "LOCAL_DATA_INSUFFICIENT_SPACE",
            format!("required {required} bytes, available {available} bytes"),
            false,
        ));
    }
    Ok(())
}

pub(in crate::repository::sqlite) fn create_online_backup(
    source: &Connection,
    path: &Path,
) -> Result<(), String> {
    // Keep the identity handle alive until SQLite closes the destination. On
    // Windows this also denies deletion/renaming while sensitive bytes are written.
    let private_file = create_private_backup_file(path)
        .map_err(|error| format!("Creator database backup could not be created: {error}"))?;

    let result = (|| {
        let mut destination = Connection::open(path).map_err(|error| error.to_string())?;
        let backup = Backup::new(source, &mut destination).map_err(|error| error.to_string())?;
        let completed: Result<(), String> =
            match backup.step(-1).map_err(|error| error.to_string())? {
                rusqlite::backup::StepResult::Done => Ok(()),
                rusqlite::backup::StepResult::Busy | rusqlite::backup::StepResult::Locked => Err(
                    "Creator database is busy and a consistent backup could not be created".into(),
                ),
                rusqlite::backup::StepResult::More => {
                    Err("Creator database backup did not complete".into())
                }
                _ => Err("Creator database backup returned an unknown state".into()),
            };
        drop(backup);
        completed?;
        destination
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    drop(private_file);
    if result.is_err() {
        cleanup_backup_artifacts(path);
    }
    result
}

fn create_private_backup_file(path: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(windows)]
    {
        windows::create_private_backup_file(path)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "private database backups are unsupported on this platform",
        ))
    }
}

pub(super) fn cleanup_backup_artifacts(path: &Path) {
    let base = path.to_string_lossy();
    for artifact in [
        base.to_string(),
        format!("{base}-wal"),
        format!("{base}-shm"),
    ] {
        // A missing artifact is the desired end state; anything else is
        // recorded so a leftover backup is not silently kept.
        if let Err(error) = std::fs::remove_file(&artifact)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("backup artifact cleanup failed: {error}");
        }
    }
}

pub(super) fn backup_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Creator database path has no parent directory".to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Creator database path has no valid file name".to_string())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_secs();
    Ok(parent.join(format!("{name}.backup-{timestamp}-{}", Uuid::new_v4())))
}
