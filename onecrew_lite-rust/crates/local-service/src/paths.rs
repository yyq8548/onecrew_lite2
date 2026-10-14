//! Stable user data and ephemeral service discovery. Plugin cache lifetime is
//! never a storage lifetime, and opening a service never migrates a directory.
use fslock::LockFile;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub(crate) const OWNER_FILE: &str = ".onecrew_lite-service-owner.lock";
pub(crate) const ENDPOINT_FILE: &str = ".onecrew_lite-service.json";

/// Resolve the existing OneCrew data identity on the current OS.
pub fn default_data_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    let base = crate::cli_installation::windows_local_data_dir();
    #[cfg(not(target_os = "windows"))]
    let base = dirs::data_dir();
    base.map(|path| path.join("io.onecrewai.onecrew_lite"))
        .ok_or_else(|| "user_data_directory_unavailable".into())
}

pub(crate) fn acquire_owner(data_dir: &Path) -> Result<LockFile, String> {
    std::fs::create_dir_all(data_dir).map_err(|_| "data_directory_unavailable")?;
    let metadata = std::fs::symlink_metadata(data_dir).map_err(|_| "data_directory_unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("unsafe_data_directory".into());
    }
    reject_legacy_desktop_owner(data_dir)?;
    let path = data_dir.join(OWNER_FILE);
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            return Err("unsafe_owner_lock".into());
        }
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Competing first starts may create the same empty lock. Both then
            // contend on the same OS lock; no marker alone grants ownership.
            if crate::private_file::write_private_file(&path, b"").is_err() {
                let metadata =
                    std::fs::symlink_metadata(&path).map_err(|_| "owner_lock_unavailable")?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err("unsafe_owner_lock".into());
                }
            }
        }
        Err(_) => return Err("owner_lock_unavailable".into()),
    }
    let mut lock = LockFile::open(&path).map_err(|_| "owner_lock_unavailable")?;
    if !lock.try_lock().map_err(|_| "owner_lock_unavailable")? {
        return Err("service_already_running".into());
    }
    Ok(lock)
}

fn reject_legacy_desktop_owner(data_dir: &Path) -> Result<(), String> {
    // A separate process lock must never authorize opening the advanced edition's DB.
    // The explicit offline copy omits these transient markers and preserves the source.
    for name in [".onecrew-service-owner.lock", ".onecrew-service.json"] {
        match std::fs::symlink_metadata(data_dir.join(name)) {
            Ok(_) => {
                return Err(
                    "lite_requires_separate_data_directory: copy legacy data offline first".into(),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("data_directory_unavailable".into()),
        }
    }
    Ok(())
}

pub(crate) fn executable_identity(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "executable_unavailable")?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "executable_unavailable")?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A stable, loopback-only origin preserves browser preferences across service
/// restarts. It is derived from the canonical data directory, not a plugin
/// version. A conflicting listener fails explicitly rather than changing the
/// origin or attaching to an unrelated process.
pub(crate) fn web_port(data_dir: &Path) -> Result<u16, String> {
    let canonical = data_dir
        .canonicalize()
        .map_err(|_| "data_directory_unavailable")?;
    let digest = Sha256::digest(canonical.as_os_str().as_encoded_bytes());
    Ok(22000 + u16::from_be_bytes([digest[0], digest[1]]) % 10000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lite_has_separate_defaults_and_refuses_advanced_service_data() {
        assert!(
            default_data_dir()
                .unwrap()
                .ends_with("io.onecrewai.onecrew_lite")
        );
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join(".onecrew-service-owner.lock"), b"").unwrap();
        assert!(
            acquire_owner(data.path())
                .err()
                .unwrap()
                .contains("separate_data_directory")
        );
        assert!(!data.path().join(OWNER_FILE).exists());
    }
    #[test]
    fn lease_is_held_until_owner_drop_and_never_cleans_user_data() {
        let data = tempfile::tempdir().unwrap();
        std::fs::write(data.path().join("keep.txt"), b"user data").unwrap();
        let lease = acquire_owner(data.path()).unwrap();
        assert!(acquire_owner(data.path()).is_err());
        drop(lease);
        let _second = acquire_owner(data.path()).unwrap();
        assert_eq!(
            std::fs::read(data.path().join("keep.txt")).unwrap(),
            b"user data"
        );
    }
}
