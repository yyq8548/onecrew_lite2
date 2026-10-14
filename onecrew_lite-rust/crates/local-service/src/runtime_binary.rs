//! Immutable service executable storage in the per-user install root's
//! `service-binaries` directory, independent of Codex's removable plugin cache
//! and of OS cache cleanup. A plugin can be disabled/reinstalled without
//! unlinking the running service image or invalidating its setup/login command
//! paths.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) fn prepare(source: &Path, expected_sha256: &str) -> Result<PathBuf, String> {
    let root = crate::cli_installation::UserInstallLayout::current()?.service_binaries();
    prepare_in(&root, source, expected_sha256)
}

fn prepare_in(root: &Path, source: &Path, expected_sha256: &str) -> Result<PathBuf, String> {
    let directory = root.join(expected_sha256);
    fs::create_dir_all(&directory).map_err(|_| "service_binary_directory_unavailable")?;
    let metadata =
        fs::symlink_metadata(&directory).map_err(|_| "service_binary_directory_unavailable")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("unsafe_service_binary_directory".into());
    }
    let destination = directory.join(if cfg!(windows) {
        "onecrew_lite.exe"
    } else {
        "onecrew_lite"
    });
    if destination
        .try_exists()
        .map_err(|_| "service_binary_unavailable")?
    {
        verify(&destination, expected_sha256)?;
        return Ok(destination);
    }
    let temporary = tempfile::NamedTempFile::new_in(&directory)
        .map_err(|_| "service_binary_staging_failed")?
        .into_temp_path();
    copy_with_origin_metadata(source, &temporary)?;
    verify(&temporary, expected_sha256)?;
    sync_copied_file(&temporary).map_err(|_| "service_binary_flush_failed")?;
    match temporary.persist_noclobber(&destination) {
        Ok(()) => (),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify(&destination, expected_sha256)?
        }
        Err(_) => return Err("service_binary_publish_failed".into()),
    }
    Ok(destination)
}

fn verify(path: &Path, expected_sha256: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "service_binary_unavailable")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("unsafe_service_binary".into());
    }
    if crate::paths::executable_identity(path)? != expected_sha256 {
        return Err("service_binary_identity_mismatch".into());
    }
    Ok(())
}

/// Flush one freshly copied file's bytes to durable storage.
///
/// Windows needs a writable handle: `sync_all` is `FlushFileBuffers`, which
/// requires `GENERIC_WRITE`, and `File::open` asks only for `GENERIC_READ`, so
/// the flush fails with access denied. `CopyFileExW` also carries
/// `FILE_ATTRIBUTE_READONLY` over from the source, which would refuse a
/// writable open, so a read-only copy has the attribute cleared first. The copy
/// is private and content-addressed, and every reader verifies its digest
/// before use, so the attribute is not what protects it.
///
/// POSIX allows `fsync` on a read-only descriptor, and the macOS copy keeps the
/// source's mode, which can be 0555 and therefore cannot be reopened for write.
/// So every other platform keeps the read-only handle.
pub fn sync_copied_file(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let metadata = fs::metadata(path)?;
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "Windows-only: clears FILE_ATTRIBUTE_READONLY, not Unix permission bits; the DACL is unchanged"
            )]
            permissions.set_readonly(false);
            fs::set_permissions(path, permissions)?;
        }
        fs::OpenOptions::new().write(true).open(path)?.sync_all()
    }
    #[cfg(not(windows))]
    {
        fs::File::open(path)?.sync_all()
    }
}

/// Copy executable bytes while retaining quarantine/Mark-of-the-Web metadata.
pub fn copy_with_origin_metadata(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // macOS cp preserves extended attributes by default; -p also preserves
        // mode/ACL metadata. Do not strip quarantine when making this copy.
        let status = std::process::Command::new("/bin/cp")
            .arg("-p")
            .arg(source)
            .arg(destination)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|_| "service_binary_copy_failed")?;
        if !status.success() {
            return Err("service_binary_copy_failed".into());
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        fs::copy(source, destination).map_err(|_| "service_binary_copy_failed")?;
    }
    #[cfg(target_os = "windows")]
    {
        // Preserve Mark-of-the-Web explicitly as well as CopyFile metadata.
        let mut source_zone = source.as_os_str().to_os_string();
        source_zone.push(":Zone.Identifier");
        let mut destination_zone = destination.as_os_str().to_os_string();
        destination_zone.push(":Zone.Identifier");
        match fs::read(&source_zone) {
            Ok(zone) => fs::write(&destination_zone, zone)
                .map_err(|_| "service_binary_origin_copy_failed")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("service_binary_origin_read_failed".into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Windows arm of this is what `service_binary_flush_failed` and
    /// `update_staging_failed` were: a copy that inherited the read-only
    /// attribute, flushed through a handle that never asked to write.
    #[test]
    fn a_read_only_copy_can_still_be_flushed() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("read only copy");
        fs::write(&destination, b"synthetic executable bytes").unwrap();
        let mut permissions = fs::metadata(&destination).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&destination, permissions).unwrap();
        sync_copied_file(&destination).unwrap();
    }

    #[test]
    fn service_image_is_published_once_under_its_digest() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("running cli");
        fs::write(&source, b"synthetic executable bytes").unwrap();
        let digest = crate::paths::executable_identity(&source).unwrap();
        let root = directory.path().join("service-binaries");
        let staged = prepare_in(&root, &source, &digest).unwrap();
        assert_eq!(
            staged,
            root.join(&digest).join(if cfg!(windows) {
                "onecrew_lite.exe"
            } else {
                "onecrew_lite"
            })
        );
        assert_eq!(fs::read(&staged).unwrap(), b"synthetic executable bytes");
        fs::write(&source, b"a different build").unwrap();
        assert_eq!(prepare_in(&root, &source, &digest).unwrap(), staged);
        assert_eq!(fs::read(&staged).unwrap(), b"synthetic executable bytes");
    }

    #[test]
    fn copied_bytes_are_verified_before_execution_is_allowed() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source file");
        let destination = directory.path().join("destination file");
        fs::write(&source, b"synthetic executable bytes").unwrap();
        let digest = crate::paths::executable_identity(&source).unwrap();
        copy_with_origin_metadata(&source, &destination).unwrap();
        verify(&destination, &digest).unwrap();
        fs::write(&destination, b"tampered").unwrap();
        assert!(verify(&destination, &digest).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_service_copy_preserves_quarantine_attribute() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("quarantined source");
        let destination = directory.path().join("copied image");
        fs::write(&source, b"not executed").unwrap();
        let value = "0081;00000000;OneCrew-test;";
        assert!(
            std::process::Command::new("/usr/bin/xattr")
                .args(["-w", "com.apple.quarantine", value])
                .arg(&source)
                .status()
                .unwrap()
                .success()
        );
        copy_with_origin_metadata(&source, &destination).unwrap();
        let read = std::process::Command::new("/usr/bin/xattr")
            .args(["-p", "com.apple.quarantine"])
            .arg(&destination)
            .output()
            .unwrap();
        assert!(read.status.success());
        // macOS can add copy flags and rewrite provenance/time fields. The
        // security invariant is that the original quarantine flags survive.
        let copied = String::from_utf8(read.stdout).unwrap();
        let flags = u16::from_str_radix(copied.trim().split(';').next().unwrap(), 16).unwrap();
        assert_eq!(flags & 0x0081, 0x0081);
    }
}
