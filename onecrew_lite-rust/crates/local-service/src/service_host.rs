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
        let status = tokio::process::Command::new(&executable)
            .args(["__service", "--data-dir"])
            .arg(data)
            .env("LOCALAPPDATA", local)
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
