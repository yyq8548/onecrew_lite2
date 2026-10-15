//! Explicit current-user OS ownership; MCP clients never own the service Job.
use std::path::Path;

/// The scheduled owner forwards the physical installation environment to its
/// child. AppData virtualization does not follow a process out of Codex's job.
pub async fn run_service_host(executable: &Path, data: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let executable = physical_path(executable)?;
        let generation = executable.parent().ok_or("service_host_invalid_layout")?;
        let images = generation.parent().ok_or("service_host_invalid_layout")?;
        let root = images.parent().ok_or("service_host_invalid_layout")?;
        if images.file_name().and_then(|p| p.to_str()) != Some("service-binaries")
            || root.file_name().and_then(|p| p.to_str()) != Some("onecrew_lite")
        {
            return Err("service_host_invalid_layout".into());
        }
        let local = root.parent().ok_or("service_host_invalid_layout")?;
        // Task Scheduler's WorkingDirectory records the physical package path.
        // Never infer it from the content-addressed service copy or AppData.
        let package = std::env::current_dir()
            .map_err(|_| "service_host_package_invalid")?
            .join("onecrew_lite.exe");
        let service = executable.clone();
        let package = crate::blocking::run_blocking("service_host_package", move || {
            verified_package(&service, &package)
        })
        .await??;
        let status = tokio::process::Command::new(&executable)
            .args(["__service", "--data-dir"])
            .arg(data)
            .env("LOCALAPPDATA", local)
            .env("ONECREW_LITE_HOST_INSTALLED_EXECUTABLE", package)
            .creation_flags(0x08000000)
            .status()
            .await
            .map_err(|error| format!("service_host_child_failed: {error}"))?;
        if !status.success() {
            return Err(format!("service_host_child_failed: {status}"));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (executable, data);
        Err("service_host_unsupported".into())
    }
}

pub fn configure_service_host(executable: &Path, data: &Path, action: &str) -> Result<(), String> {
    if !["install", "start", "remove", "probe"].contains(&action) {
        return Err("service_host_invalid_action".into());
    }
    #[cfg(windows)]
    {
        if action == "install" || action == "probe" {
            std::fs::create_dir_all(data).map_err(|_| "service_host_data_directory_unavailable")?;
        }
        // Packaged desktop hosts can redirect AppData transparently. The OS
        // scheduler must receive the physical identity, not that virtual alias.
        let data = physical_path(data)
            .map_err(|error| format!("service_host_{action}_failed: {error}"))?;
        let package = physical_path(executable)?;
        let stored;
        let executable = if action == "install" || action == "probe" {
            let digest = crate::paths::executable_identity(executable)?;
            stored = crate::runtime_binary::prepare(executable, &digest)?;
            stored.as_path()
        } else {
            executable
        };
        let executable = physical_path(executable)?;
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let system = std::env::var_os("SystemRoot").ok_or("service_host_system_root_missing")?;
        let output =
            Command::new(Path::new(&system).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                .args([
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    include_str!("windows_service_host.ps1"),
                ])
                .env("ONECREW_HOST_BINARY", &executable)
                .env("ONECREW_HOST_PACKAGE", &package)
                .env("ONECREW_HOST_DATA", &data)
                .env("ONECREW_HOST_ACTION", action)
                .stdin(Stdio::null())
                .creation_flags(0x08000000)
                .output()
                .map_err(|error| format!("service_host_unavailable: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "service_host_{action}_failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (executable, data);
        Err("service_host_unsupported".into())
    }
}

/// An installation path is usable only when it is the same build as the host.
/// This check does not move or select a different business-data directory.
#[cfg(any(windows, test))]
pub(crate) fn verified_package(
    service: &Path,
    package: &Path,
) -> Result<std::path::PathBuf, String> {
    if !package.is_absolute()
        || package.file_name().and_then(|name| name.to_str()) != Some("onecrew_lite.exe")
        || crate::is_managed_cli_copy(package)
    {
        return Err("service_host_package_invalid".into());
    }
    let package = package
        .canonicalize()
        .map_err(|_| "service_host_package_unavailable")?;
    if crate::is_managed_cli_copy(&package)
        || crate::paths::executable_identity(&package)?
            != crate::paths::executable_identity(service)?
    {
        return Err("service_host_package_mismatch: register the installed service again".into());
    }
    Ok(package)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_package_must_match_service_even_outside_its_storage_root() {
        let temp = tempfile::tempdir().unwrap();
        let service = temp
            .path()
            .join("onecrew_lite/service-binaries/hash/onecrew_lite.exe");
        let package = temp.path().join("separate package/onecrew_lite.exe");
        std::fs::create_dir_all(service.parent().unwrap()).unwrap();
        std::fs::create_dir_all(package.parent().unwrap()).unwrap();
        std::fs::write(&service, b"build-r5").unwrap();
        std::fs::write(&package, b"build-r5").unwrap();
        assert_eq!(
            verified_package(&service, &package).unwrap(),
            package.canonicalize().unwrap()
        );
        assert!(verified_package(&service, &service).is_err());
        assert!(verified_package(&service, Path::new("onecrew_lite.exe")).is_err());
        std::fs::write(&package, b"other-build").unwrap();
        assert!(
            verified_package(&service, &package)
                .unwrap_err()
                .contains("package_mismatch")
        );
        std::fs::remove_file(&package).unwrap();
        assert!(verified_package(&service, &package).is_err());
    }
}

#[cfg(windows)]
fn physical_path(path: &Path) -> Result<std::path::PathBuf, String> {
    let canonical = path
        .canonicalize()
        .map_err(|_| "service_host_path_unavailable")?;
    let text = canonical.to_str().ok_or("service_host_path_encoding")?;
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        Ok(format!(r"\\{unc}").into())
    } else {
        Ok(text.strip_prefix(r"\\?\").unwrap_or(text).into())
    }
}
