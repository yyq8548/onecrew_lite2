//! Explicit, per-user OpenCLI installation. No shell, global npm, PATH writes,
//! browser data changes, or provider reads. Only this owned directory is replaced.
use crate::{OpenCli, OpenCliError, process::run};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

pub const VERIFIED_VERSION: &str = "1.8.8";
const NODE_VERSION: &str = "24.21.0";
const MAX_DOWNLOAD: usize = 96 * 1024 * 1024;

struct Distribution {
    platform: &'static str,
    suffix: &'static str,
    sha256: &'static str,
}

fn distribution(os: &str, arch: &str) -> Result<Distribution, &'static str> {
    let (platform, suffix, sha256) = match (os, arch) {
        ("macos", "aarch64") => (
            "darwin-arm64",
            "tar.gz",
            "bed7eea5325e1108f32ce5228ddd6a5f0f08a499ee42aa7442aea583702f6057",
        ),
        ("macos", "x86_64") => (
            "darwin-x64",
            "tar.gz",
            "1462cb3b3046b815cf8ea436d3da450ec1a9f11dac7e5a46b0ada5305d7e8097",
        ),
        ("windows", "x86_64") => (
            "win-x64",
            "zip",
            "158f7685b44de51f6c0df1d153526cbcd3e1bc739a8dfc607721cef75de9e541",
        ),
        _ => return Err("opencli_repair_unsupported_platform"),
    };
    Ok(Distribution {
        platform,
        suffix,
        sha256,
    })
}

fn installation_root() -> Result<PathBuf, &'static str> {
    let root = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("onecrew_lite"))
    } else {
        dirs::home_dir().map(|path| path.join(".local/share/onecrew_lite"))
    }
    .ok_or("opencli_repair_home_unavailable")?;
    if !root.is_absolute() {
        return Err("opencli_repair_home_unavailable");
    }
    Ok(root.join("opencli"))
}

fn entrypoint(root: &Path, platform: &str) -> (PathBuf, PathBuf) {
    let node_root = root.join(format!("node-v{NODE_VERSION}-{platform}"));
    let node = if platform.starts_with("win-") {
        node_root.join("node.exe")
    } else {
        node_root.join("bin/node")
    };
    (
        node,
        root.join("cli/node_modules/@jackwener/opencli/dist/src/main.js"),
    )
}

async fn entrypoint_at(
    root: &Path,
    platform: &str,
) -> Result<Option<(PathBuf, PathBuf)>, OpenCliError> {
    match tokio::fs::symlink_metadata(root).await {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => (),
        _ => return Err(OpenCliError::Unavailable),
    }
    let (node, script) = entrypoint(root, platform);
    for path in [&node, &script] {
        let metadata = tokio::fs::symlink_metadata(path)
            .await
            .map_err(|_| OpenCliError::Unavailable)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(OpenCliError::Unavailable);
        }
    }
    Ok(Some((node, script)))
}

pub(super) async fn installed_entrypoint() -> Result<Option<(PathBuf, PathBuf)>, OpenCliError> {
    let Ok(spec) = distribution(std::env::consts::OS, std::env::consts::ARCH) else {
        return Ok(None);
    };
    let root = installation_root().map_err(|_| OpenCliError::Unavailable)?;
    // A broken managed installation must never silently fall back to an old CLI.
    entrypoint_at(&root.join("managed-v1"), spec.platform).await
}

async fn validate(root: &Path, spec: &Distribution) -> Result<(), &'static str> {
    let (node, script) = entrypoint_at(root, spec.platform)
        .await
        .map_err(|_| "opencli_repair_readback_failed")?
        .ok_or("opencli_repair_readback_failed")?;
    let cli = OpenCli::inspect_with_script(node, Some(script))
        .await
        .map_err(|_| "opencli_repair_readback_failed")?;
    if cli.version != VERIFIED_VERSION {
        return Err("opencli_repair_version_mismatch");
    }
    Ok(())
}

async fn download(spec: &Distribution, destination: &Path) -> Result<(), &'static str> {
    let url = format!(
        "https://nodejs.org/dist/v{NODE_VERSION}/node-v{NODE_VERSION}-{}.{}",
        spec.platform, spec.suffix
    );
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| "opencli_repair_download_failed")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "opencli_repair_download_failed")?
        .error_for_status()
        .map_err(|_| "opencli_repair_download_failed")?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "opencli_repair_download_failed")?
    {
        if bytes.len() + chunk.len() > MAX_DOWNLOAD {
            return Err("opencli_repair_download_too_large");
        }
        bytes.extend_from_slice(&chunk);
    }
    if format!("{:x}", Sha256::digest(&bytes)) != spec.sha256 {
        return Err("opencli_repair_checksum_failed");
    }
    tokio::fs::write(destination, bytes)
        .await
        .map_err(|_| "opencli_repair_write_failed")
}

fn acquire(root: &Path) -> Result<fs::File, &'static str> {
    fs::create_dir_all(root).map_err(|_| "opencli_repair_write_failed")?;
    // This is an owned install directory, not a legacy package or user data.
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("install.lock"))
        .map_err(|_| "opencli_repair_write_failed")?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| "opencli_repair_busy")?;
    Ok(file)
}

#[cfg(windows)]
fn extract_zip(archive: &Path, destination: &Path) -> Result<(), &'static str> {
    let file = fs::File::open(archive).map_err(|_| "opencli_repair_extract_failed")?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "opencli_repair_extract_failed")?;
    if zip.len() > 30_000 {
        return Err("opencli_repair_extract_failed");
    }
    let mut total = 0;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|_| "opencli_repair_extract_failed")?;
        total += entry.size();
        if total > 512 * 1024 * 1024 || entry.is_symlink() {
            return Err("opencli_repair_extract_failed");
        }
        let path = entry
            .enclosed_name()
            .ok_or("opencli_repair_extract_failed")?;
        if path.to_string_lossy().contains(':') {
            return Err("opencli_repair_extract_failed");
        }
        let path = destination.join(path);
        if entry.is_dir() {
            fs::create_dir_all(path).map_err(|_| "opencli_repair_extract_failed")?;
        } else {
            fs::create_dir_all(path.parent().ok_or("opencli_repair_extract_failed")?)
                .map_err(|_| "opencli_repair_extract_failed")?;
            let mut output = fs::File::create(path).map_err(|_| "opencli_repair_extract_failed")?;
            std::io::copy(&mut entry, &mut output).map_err(|_| "opencli_repair_extract_failed")?;
        }
    }
    Ok(())
}

/// Called only after a person's explicit setup click. Staging and validation
/// complete before publication; unknown legacy installations are never removed.
pub async fn repair_managed_installation(approved: bool) -> Result<(), String> {
    if !approved {
        return Err("opencli_repair_approval_required".into());
    }
    repair().await.map_err(str::to_owned)
}

async fn repair() -> Result<(), &'static str> {
    let spec = distribution(std::env::consts::OS, std::env::consts::ARCH)?;
    let root = installation_root()?;
    repair_at(root, spec).await
}

async fn repair_at(root: PathBuf, spec: Distribution) -> Result<(), &'static str> {
    let worker_root = root.clone();
    let (lease, staging) = tokio::task::spawn_blocking(move || {
        let lease = acquire(&worker_root)?;
        let staging = tempfile::Builder::new()
            .prefix("staging-")
            .tempdir_in(worker_root)
            .map_err(|_| "opencli_repair_write_failed")?;
        Ok::<_, &'static str>((lease, staging))
    })
    .await
    .map_err(|_| "opencli_repair_worker_failed")??;
    let _lease = std::sync::Arc::new(lease);
    let staging = std::sync::Arc::new(staging);
    let installed = root.join("managed-v1");
    if validate(&installed, &spec).await.is_ok() {
        return Ok(());
    }
    let archive = staging.path().join(format!("node.{}", spec.suffix));
    download(&spec, &archive).await?;
    #[cfg(target_os = "macos")]
    run(
        Path::new("/usr/bin/tar"),
        None,
        &[
            "-xzf",
            archive.to_str().ok_or("opencli_repair_write_failed")?,
            "-C",
            staging
                .path()
                .to_str()
                .ok_or("opencli_repair_write_failed")?,
        ],
        Duration::from_secs(60),
    )
    .await
    .map_err(|_| "opencli_repair_extract_failed")?;
    #[cfg(windows)]
    {
        let archive = archive.clone();
        let worker_staging = staging.clone();
        let worker_lease = _lease.clone();
        tokio::task::spawn_blocking(move || {
            let _lease = worker_lease;
            extract_zip(&archive, worker_staging.path())
        })
        .await
        .map_err(|_| "opencli_repair_worker_failed")??;
    }
    let (node, _) = entrypoint(staging.path(), spec.platform);
    let node_root = node.parent().ok_or("opencli_repair_extract_failed")?;
    let npm = if cfg!(windows) {
        node_root.join("node_modules/npm/bin/npm-cli.js")
    } else {
        node_root.join("../lib/node_modules/npm/bin/npm-cli.js")
    };
    let empty_config = staging.path().join("empty.npmrc");
    tokio::fs::write(&empty_config, b"")
        .await
        .map_err(|_| "opencli_repair_write_failed")?;
    let global_config = staging.path().join("global.npmrc");
    tokio::fs::write(&global_config, b"")
        .await
        .map_err(|_| "opencli_repair_write_failed")?;
    let prefix = staging.path().join("cli");
    let cache = staging.path().join("npm-cache");
    let package = format!("@jackwener/opencli@{VERIFIED_VERSION}");
    run(
        &node,
        Some(&npm),
        &[
            "install",
            "--prefix",
            prefix.to_str().ok_or("opencli_repair_write_failed")?,
            "--cache",
            cache.to_str().ok_or("opencli_repair_write_failed")?,
            "--userconfig",
            empty_config.to_str().ok_or("opencli_repair_write_failed")?,
            "--globalconfig",
            global_config
                .to_str()
                .ok_or("opencli_repair_write_failed")?,
            "--registry=https://registry.npmjs.org",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--save-exact",
            &package,
        ],
        Duration::from_secs(240),
    )
    .await
    .map_err(|_| "opencli_repair_package_install_failed")?;
    validate(staging.path(), &spec).await?;
    tokio::fs::remove_file(&archive)
        .await
        .map_err(|_| "opencli_repair_write_failed")?;
    // Cache is installation scratch, not an account/plan store.
    tokio::fs::remove_dir_all(cache)
        .await
        .map_err(|_| "opencli_repair_write_failed")?;
    let backup = root.join("previous-v1");
    if tokio::fs::try_exists(&backup)
        .await
        .map_err(|_| "opencli_repair_write_failed")?
    {
        return Err("opencli_repair_backup_retained");
    }
    let existed = tokio::fs::try_exists(&installed)
        .await
        .map_err(|_| "opencli_repair_write_failed")?;
    if existed {
        tokio::fs::rename(&installed, &backup)
            .await
            .map_err(|_| "opencli_repair_publish_failed")?;
    }
    if tokio::fs::rename(staging.path(), &installed).await.is_err() {
        if existed && tokio::fs::rename(&backup, &installed).await.is_err() {
            return Err("opencli_repair_restore_failed");
        }
        return Err("opencli_repair_publish_failed");
    }
    if let Err(error) = validate(&installed, &spec).await {
        if tokio::fs::rename(&installed, staging.path()).await.is_err()
            || (existed && tokio::fs::rename(&backup, &installed).await.is_err())
        {
            return Err("opencli_repair_restore_failed");
        }
        return Err(error);
    }
    if existed {
        tokio::fs::remove_dir_all(&backup)
            .await
            .map_err(|_| "opencli_repair_backup_retained")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn requires_explicit_approval_before_any_installation_work() {
        assert_eq!(
            repair_managed_installation(false).await,
            Err("opencli_repair_approval_required".into())
        );
    }
    #[tokio::test]
    async fn absent_and_broken_managed_installs_are_distinct() {
        let root = tempfile::tempdir().unwrap();
        let managed = root.path().join("managed-v1");
        assert!(
            entrypoint_at(&managed, "darwin-arm64")
                .await
                .unwrap()
                .is_none()
        );
        fs::create_dir(&managed).unwrap();
        assert_eq!(
            entrypoint_at(&managed, "darwin-arm64").await,
            Err(OpenCliError::Unavailable)
        );
    }
    #[test]
    fn fixed_downloads_have_target_specific_pinned_checksums() {
        for (os, arch) in [
            ("macos", "aarch64"),
            ("macos", "x86_64"),
            ("windows", "x86_64"),
        ] {
            let spec = distribution(os, arch).unwrap();
            assert_eq!(spec.sha256.len(), 64);
            assert!(spec.sha256.bytes().all(|value| value.is_ascii_hexdigit()));
        }
        assert!(distribution("windows", "aarch64").is_err());
    }
    #[test]
    fn competing_repairs_cannot_publish_concurrently() {
        let root = tempfile::tempdir().unwrap();
        let lease = acquire(root.path()).unwrap();
        assert_eq!(acquire(root.path()).unwrap_err(), "opencli_repair_busy");
        drop(lease);
        assert!(acquire(root.path()).is_ok());
    }

    #[tokio::test]
    #[ignore = "downloads the pinned official Node and npm OpenCLI packages into an isolated temporary install"]
    async fn live_install_and_repair_verify_the_real_package_without_global_changes() {
        let root = tempfile::tempdir().unwrap();
        let spec = distribution(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        repair_at(root.path().to_owned(), spec).await.unwrap();
        let spec = distribution(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        let installed = root.path().join("managed-v1");
        validate(&installed, &spec).await.unwrap();
        let before = fs::read(entrypoint(&installed, spec.platform).1).unwrap();
        repair_at(root.path().to_owned(), spec).await.unwrap();
        let spec = distribution(std::env::consts::OS, std::env::consts::ARCH).unwrap();
        assert_eq!(
            fs::read(entrypoint(&installed, spec.platform).1).unwrap(),
            before
        );
        assert!(!root.path().join("previous-v1").exists());
    }
}
