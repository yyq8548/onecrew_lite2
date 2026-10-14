use std::{fs, path::Path};

use fslock::LockFile;
#[cfg(test)]
use tempfile::{Builder, TempDir};

const DIRECTORY_PREFIX: &str = "onecrew-local-telegram-qr-";
const FILE_PREFIX: &str = "telegram-login-qr-";
const OWNER_MARKER_FILE: &str = ".onecrew-owner-v1";
const OWNER_MARKER_CONTENT: &[u8] = b"onecrew-local telegram qr v1\n";
const ACTIVE_LOCK_FILE: &str = ".active.lock";

/// Remove only adapter-owned QR directories left behind by an unclean prior
/// process. The exact owner marker prevents prefix collisions, while the
/// per-instance lock keeps another live adapter's directory untouched.
pub(crate) fn cleanup_stale_files() {
    let Ok(directories) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for directory in directories.flatten() {
        let name = directory.file_name();
        if !name.to_string_lossy().starts_with(DIRECTORY_PREFIX) {
            continue;
        }
        let Ok(file_type) = directory.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let path = directory.path();
        let marker_path = path.join(OWNER_MARKER_FILE);
        let lock_path = path.join(ACTIVE_LOCK_FILE);
        if !is_regular_file(&marker_path)
            || !is_regular_file(&lock_path)
            || fs::read(&marker_path).ok().as_deref() != Some(OWNER_MARKER_CONTENT)
        {
            continue;
        }
        let Ok(mut active_lock) = LockFile::open(&lock_path) else {
            continue;
        };
        if !matches!(active_lock.try_lock(), Ok(true)) {
            continue;
        }

        let Ok(files) = fs::read_dir(&path) else {
            log_cleanup_step("unlock", active_lock.unlock());
            continue;
        };
        let mut qr_files = Vec::new();
        let mut contains_unknown_entry = false;
        for file in files.flatten() {
            let file_name = file.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name == OWNER_MARKER_FILE || file_name == ACTIVE_LOCK_FILE {
                continue;
            }
            if file_name.starts_with(FILE_PREFIX)
                && file_name.ends_with(".png")
                && is_regular_file(&file.path())
            {
                qr_files.push(file.path());
            } else {
                contains_unknown_entry = true;
                break;
            }
        }
        if contains_unknown_entry {
            log_cleanup_step("unlock", active_lock.unlock());
            continue;
        }

        for qr_file in qr_files {
            log_cleanup_step("remove QR image", fs::remove_file(qr_file));
        }
        // Removing the marker while holding the lock makes the directory
        // ineligible for any concurrently starting cleaner before unlock.
        log_cleanup_step("remove owner marker", fs::remove_file(&marker_path));
        log_cleanup_step("unlock", active_lock.unlock());
        drop(active_lock);
        log_cleanup_step("remove lock file", fs::remove_file(&lock_path));
        log_cleanup_step("remove directory", fs::remove_dir(path));
    }
}

/// Stale QR directories are cleaned best-effort: a step that fails leaves
/// the directory for the next cleaner run and is recorded rather than
/// aborting the sweep.
fn log_cleanup_step(step: &str, result: std::io::Result<()>) {
    if let Err(error) = result {
        log::debug!("stale QR image cleanup step '{step}' failed: {error}");
    }
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

#[cfg(all(test, unix))]
fn restrict_to_current_user(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure Telegram QR image: {error}"))
}

#[cfg(all(test, not(unix)))]
fn restrict_to_current_user(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_cleanup_fixture(hold_lock: bool) -> (TempDir, Option<LockFile>, PathBuf, PathBuf) {
        let directory = Builder::new()
            .prefix(DIRECTORY_PREFIX)
            .tempdir()
            .expect("cleanup fixture directory");
        let path = directory.path().to_path_buf();
        let lock_path = path.join(ACTIVE_LOCK_FILE);
        let mut active_lock = LockFile::open(&lock_path).expect("cleanup fixture lock");
        restrict_to_current_user(&lock_path).unwrap();
        if hold_lock {
            active_lock.lock().expect("active cleanup fixture lock");
        }
        let marker_path = path.join(OWNER_MARKER_FILE);
        fs::write(&marker_path, OWNER_MARKER_CONTENT).expect("cleanup fixture marker");
        restrict_to_current_user(&marker_path).unwrap();
        let qr_path = path.join(format!("{FILE_PREFIX}fixture.png"));
        fs::write(&qr_path, b"png").expect("cleanup fixture QR");
        restrict_to_current_user(&qr_path).unwrap();
        (directory, hold_lock.then_some(active_lock), path, qr_path)
    }

    #[test]
    fn startup_cleanup_skips_a_live_adapter_and_removes_it_after_unlock() {
        let (_directory, mut active_lock, path, qr_path) = create_cleanup_fixture(true);

        cleanup_stale_files();

        assert!(path.exists());
        assert!(qr_path.exists());

        active_lock
            .as_mut()
            .unwrap()
            .unlock()
            .expect("release cleanup fixture lock");
        drop(active_lock);
        cleanup_stale_files();

        assert!(!qr_path.exists());
        assert!(!path.exists());
    }

    #[test]
    fn startup_cleanup_never_touches_a_prefix_collision_without_owner_marker() {
        let directory = Builder::new()
            .prefix(DIRECTORY_PREFIX)
            .tempdir()
            .expect("unowned prefix collision");
        let unrelated = directory.path().join(format!("{FILE_PREFIX}unrelated.png"));
        fs::write(&unrelated, b"not ours").unwrap();

        cleanup_stale_files();

        assert_eq!(fs::read(unrelated).unwrap(), b"not ours");
    }
}
