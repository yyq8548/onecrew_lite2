//! Replace a verified staged executable while retaining a recoverable old image.
#[cfg(any(windows, test))]
use std::io;
use std::{fs, path::Path};

/// Unix replaces the running image atomically; the old inode stays mapped
/// for the running process, so no backup copy is needed.
#[cfg(unix)]
pub(super) fn replace(staged: &Path, target: &Path, _backup: &Path) -> Result<(), String> {
    fs::rename(staged, target).map_err(|_| "update_replace_failed".into())
}

#[cfg(windows)]
pub(super) fn replace(staged: &Path, target: &Path, backup: &Path) -> Result<(), String> {
    if !target
        .try_exists()
        .map_err(|_| "update_target_unavailable")?
    {
        return fs::rename(staged, target).map_err(|_| "update_replace_failed".into());
    }
    windows_swap(staged, target, backup, &mut |from, to| fs::rename(from, to))
}

#[cfg(not(any(unix, windows)))]
pub(super) fn replace(_staged: &Path, _target: &Path, _backup: &Path) -> Result<(), String> {
    Err("update_unsupported_platform".into())
}

#[cfg(any(windows, test))]
fn windows_swap(
    staged: &Path,
    target: &Path,
    backup: &Path,
    rename: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    match fs::symlink_metadata(backup) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        _ => return Err("update_backup_already_exists".into()),
    }
    rename(target, backup).map_err(|_| "update_old_binary_rename_failed")?;
    if rename(staged, target).is_err() {
        if rename(backup, target).is_err() {
            return Err("update_replace_and_restore_failed".into());
        }
        return Err("update_replace_failed_original_restored".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_failure_restores_old_file_and_never_deletes_existing_backup() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("onecrew_lite.exe");
        let new = dir.path().join("new.exe");
        let backup = dir.path().join("onecrew_lite.exe.old");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        let result = windows_swap(&new, &old, &backup, &mut |from, to| {
            if from == new {
                Err(io::Error::other("injected"))
            } else {
                fs::rename(from, to)
            }
        });
        assert_eq!(
            result.unwrap_err(),
            "update_replace_failed_original_restored"
        );
        assert_eq!(fs::read(&old).unwrap(), b"old");
        fs::write(&backup, b"unrelated").unwrap();
        assert_eq!(
            windows_swap(&new, &old, &backup, &mut |from, to| fs::rename(from, to)).unwrap_err(),
            "update_backup_already_exists"
        );
        assert_eq!(fs::read(&backup).unwrap(), b"unrelated");
    }
    #[test]
    fn rollback_failure_preserves_the_only_old_copy_and_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("onecrew_lite.exe");
        let new = dir.path().join("new.exe");
        let backup = dir.path().join("onecrew_lite.exe.old");
        fs::write(&old, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        let error = windows_swap(&new, &old, &backup, &mut |from, to| {
            if from == old {
                fs::rename(from, to)
            } else {
                Err(io::Error::other("injected"))
            }
        })
        .unwrap_err();
        assert_eq!(error, "update_replace_and_restore_failed");
        assert_eq!(fs::read(backup).unwrap(), b"old");
    }
}
