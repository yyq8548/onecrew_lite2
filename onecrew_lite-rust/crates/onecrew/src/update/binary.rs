//! Native target identity, binary validation and private same-directory staging.
use onecrew_local_service::{copy_with_origin_metadata, sync_copied_file};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

pub(super) use onecrew_local_service::current_cli_target as host_target;
use onecrew_local_service::{default_cli_installation, is_managed_cli_copy};

pub(super) fn target_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path.file_name()
            != Some(std::ffi::OsStr::new(if cfg!(windows) {
                "onecrew_lite.exe"
            } else {
                "onecrew_lite"
            }))
    {
        return Err("update_target_invalid".into());
    }
    let parent = path
        .parent()
        .ok_or("update_target_invalid")?
        .canonicalize()
        .map_err(|_| "update_install_directory_unavailable")?;
    let target = parent.join(path.file_name().ok_or("update_target_invalid")?);
    if is_managed_cli_copy(&target) {
        return Err(format!(
            "update_managed_copy: use the installed CLI at {}",
            default_cli_installation()?.display()
        ));
    }
    match fs::symlink_metadata(&target) {
        Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
            Err("update_target_not_regular".into())
        }
        Ok(_) => Ok(target),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(_) => Err("update_target_unavailable".into()),
    }
}
pub(super) fn digest(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|_| "update_binary_unavailable")?;
    let mut buffer = [0u8; 64 * 1024];
    let mut hash = Sha256::new();
    let mut size = 0;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "update_binary_unavailable")?;
        if count == 0 {
            break;
        }
        size += count as u64;
        hash.update(&buffer[..count]);
    }
    Ok((size, format!("{:x}", hash.finalize())))
}
pub(super) fn validate_header(path: &Path, target: &str) -> Result<(), String> {
    let mut file = fs::File::open(path).map_err(|_| "update_binary_unavailable")?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header)
        .map_err(|_| "update_binary_invalid")?;
    let valid = match target {
        "aarch64-apple-darwin" | "x86_64-apple-darwin" => {
            header[..4] == [0xcf, 0xfa, 0xed, 0xfe]
                && header[4..8]
                    == (if target.starts_with("aarch64") {
                        0x0100000cu32
                    } else {
                        0x01000007
                    })
                    .to_le_bytes()
                && header[12..16] == 2u32.to_le_bytes()
        }
        "x86_64-pc-windows-msvc" => {
            let offset = u32::from_le_bytes(
                header[60..64]
                    .try_into()
                    .map_err(|_| "update_binary_invalid")?,
            ) as u64;
            if header[..2] != *b"MZ"
                || offset
                    > file
                        .metadata()
                        .map_err(|_| "update_binary_invalid")?
                        .len()
                        .saturating_sub(26)
            {
                false
            } else {
                let mut pe = [0u8; 26];
                file.seek(SeekFrom::Start(offset))
                    .and_then(|_| file.read_exact(&mut pe))
                    .map_err(|_| "update_binary_invalid")?;
                pe[..6] == *b"PE\0\0\x64\x86" && pe[24..26] == [0x0b, 0x02]
            }
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("update_binary_architecture_mismatch".into())
    }
}
#[cfg(unix)]
pub(super) fn executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .map_err(|_| "update_permissions_failed")?;
    Ok(())
}

/// Non-Unix targets take executability from the file type, not a mode bit.
#[cfg(not(unix))]
pub(super) fn executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

pub(super) async fn inspect_target(path: PathBuf) -> Result<PathBuf, String> {
    tokio::task::spawn_blocking(move || {
        let path = path
            .canonicalize()
            .map_err(|_| "update_target_unavailable")?;
        target_path(&path)
    })
    .await
    .map_err(|_| "update_target_worker_failed")?
}

pub(super) async fn fingerprint(path: PathBuf) -> Result<(u64, String), String> {
    tokio::task::spawn_blocking(move || digest(&path))
        .await
        .map_err(|_| "update_hash_worker_failed")?
}

pub(super) async fn download_directory(target: PathBuf) -> Result<tempfile::TempDir, String> {
    tokio::task::spawn_blocking(move || {
        tempfile::Builder::new()
            .prefix(".onecrew-download-")
            .tempdir_in(target.parent().ok_or("update_target_invalid")?)
            .map_err(|_| "update_staging_failed".to_owned())
    })
    .await
    .map_err(|_| "update_staging_worker_failed")?
}

pub(super) async fn validate_candidate(
    path: &Path,
    target: &'static str,
    version: &str,
) -> Result<(), String> {
    let candidate = path.to_owned();
    tokio::task::spawn_blocking(move || {
        validate_header(&candidate, target)?;
        executable(&candidate)
    })
    .await
    .map_err(|_| "update_validation_worker_failed")??;
    let output = tokio::time::timeout(
        Duration::from_secs(15),
        tokio::process::Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "update_version_probe_timeout")?
    .map_err(|_| "update_version_probe_failed")?;
    if !output.status.success() || output.stdout != format!("onecrew {version}\n").as_bytes() {
        return Err("update_version_mismatch".into());
    }
    Ok(())
}

pub(super) struct StagedBinary {
    pub directory: tempfile::TempDir,
    pub path: PathBuf,
}

pub(super) async fn stage_verified_copy(
    source: PathBuf,
    destination: PathBuf,
    expected: (u64, String),
    target: &'static str,
) -> Result<StagedBinary, String> {
    tokio::task::spawn_blocking(move || {
        let directory = tempfile::Builder::new()
            .prefix(".onecrew-update-")
            .tempdir_in(destination.parent().ok_or("update_target_invalid")?)
            .map_err(|_| "update_staging_failed")?;
        let path = directory
            .path()
            .join(if cfg!(windows) { "new.exe" } else { "new" });
        copy_with_origin_metadata(&source, &path)?;
        executable(&path)?;
        if digest(&path)? != expected {
            return Err("update_staging_hash_mismatch".into());
        }
        validate_header(&path, target)?;
        sync_copied_file(&path).map_err(|_| "update_staging_failed")?;
        Ok(StagedBinary { directory, path })
    })
    .await
    .map_err(|_| "update_staging_worker_failed")?
}
