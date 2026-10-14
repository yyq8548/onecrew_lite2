//! Local CLI control client and single-owner discovery. Startup races contend
//! on the host's lifetime lease; clients never repair or open application data.
use serde_json::Value;
#[cfg(not(windows))]
use std::process::Stdio;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::BufReader, net::TcpStream};

use crate::{
    blocking::run_blocking,
    ipc::{self, Endpoint},
    paths,
};

/// Authenticated connection details for the user's local service. Credentials
/// remain private to this client and cannot be formatted or serialized.
#[derive(Clone)]
pub struct ServiceClient {
    endpoint: Arc<Endpoint>,
}

/// Execute the candidate's version command in the OS host without opening data.
pub fn service_start_preflight(executable: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let directory = tempfile::tempdir().map_err(|_| "service_host_probe_directory_failed")?;
        crate::service_host::configure_service_host(executable, directory.path(), "probe")
    }
    #[cfg(not(windows))]
    {
        let mut command = std::process::Command::new(executable);
        command
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let status = command.status().map_err(|error| {
            format!(
                "service_independent_start_required: independent process creation failed: {error}"
            )
        })?;
        if !status.success() {
            return Err("service_preflight_child_failed".into());
        }
        Ok(())
    }
}

impl ServiceClient {
    /// Connect to an existing service, including an older binary so a user can
    /// explicitly inspect or stop it. This never starts or upgrades a process.
    pub async fn existing(data_dir: &Path) -> Result<Self, String> {
        let path = data_dir.join(paths::ENDPOINT_FILE);
        let endpoint = run_blocking("service_discovery", move || read_endpoint(&path)).await??;
        let client = Self {
            endpoint: Arc::new(endpoint),
        };
        client.request("ping").await?;
        Ok(client)
    }

    /// Execute one allowlisted local control command. Results never include
    /// the private transport credential.
    pub async fn request(&self, command: &str) -> Result<Value, String> {
        let mut stream = ipc::connect(&self.endpoint, "control", Some(command)).await?;
        let response: Value = ipc::read_reply(&mut stream).await?;
        if response.get("ok").and_then(Value::as_bool) == Some(true) {
            response
                .get("result")
                .cloned()
                .ok_or_else(|| "invalid_service_response".into())
        } else {
            let error = response.get("error").ok_or("invalid_service_response")?;
            Err(error
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| error.to_string()))
        }
    }

    /// Open an authenticated raw MCP stream. Its caller bridges all bytes in
    /// both directions, including elicitation and MCP Apps notifications.
    pub async fn connect_mcp(&self) -> Result<BufReader<TcpStream>, String> {
        let mut stream = ipc::connect(&self.endpoint, "mcp", None).await?;
        let response: Value = ipc::read_reply(&mut stream).await?;
        if response.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err("mcp_connection_rejected".into());
        }
        Ok(stream)
    }
}

fn read_endpoint(path: &Path) -> Result<Endpoint, String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "service_not_running"
        } else {
            "service_discovery_unavailable"
        }
    })?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 8192 {
        return Err("unsafe_service_discovery".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("service_discovery_not_private".into());
        }
    }
    let bytes =
        zeroize::Zeroizing::new(std::fs::read(path).map_err(|_| "service_discovery_unavailable")?);
    serde_json::from_slice(&bytes).map_err(|_| "invalid_service_discovery".into())
}

/// Start this executable's shared service when none is reachable. A different
/// running binary is never silently replaced: stop it explicitly, then retry.
pub async fn ensure_service(
    data_dir: PathBuf,
    executable: PathBuf,
) -> Result<ServiceClient, String> {
    let executable_for_hash = executable.clone();
    let binary_id = run_blocking("cli_binary_identity", move || {
        paths::executable_identity(&executable_for_hash)
    })
    .await??;
    if let Ok(client) = ServiceClient::existing(&data_dir).await {
        return matching_binary(client, &binary_id);
    }
    #[cfg(windows)]
    {
        let directory = data_dir.clone();
        run_blocking("service_host_start", move || {
            crate::service_host::configure_service_host(&executable, &directory, "start")
        })
        .await??;
    }
    #[cfg(not(windows))]
    {
        let expected_binary_id = binary_id.clone();
        let executable = run_blocking("service_binary_prepare", move || {
            crate::runtime_binary::prepare(&executable, &expected_binary_id)
        })
        .await??;
        let directory = data_dir.clone();
        run_blocking("service_spawn", move || {
            let mut command = std::process::Command::new(&executable);
            command
                .arg("__service")
                .arg("--data-dir")
                .arg(directory)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(unix)]
            {
                use std::os::unix::process::CommandExt;
                command.process_group(0);
            }
            match command.spawn() {
                Ok(_) => (),
                Err(error) => return Err(format!("service_spawn_failed: {error}")),
            }
            Ok::<(), String>(())
        })
        .await??;
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(client) = ServiceClient::existing(&data_dir).await {
            return matching_binary(client, &binary_id);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("service_start_failed: run onecrew serve --foreground with the same --data-dir for diagnostics".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn matching_binary(client: ServiceClient, binary_id: &str) -> Result<ServiceClient, String> {
    if client.endpoint.binary_id != binary_id {
        return Err("service_version_conflict: a different OneCrew binary owns this data directory; finish active work, run onecrew stop, then retry".into());
    }
    Ok(client)
}
