//! Single process owner for web, control and MCP listeners. The filesystem
//! lease is released only after intake closes and accepted work is joined.
use std::{net::Ipv4Addr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};

use crate::{
    ServiceState,
    blocking::run_blocking,
    ipc::{self, Endpoint},
    paths,
};

/// Explicit host paths supplied by the executable, never by an MCP tool.
pub struct ServiceOptions {
    /// Stable user data directory, independent of executable/plugin location.
    pub data_dir: PathBuf,
    /// Executable identity used to refuse unsafe automatic process replacement.
    pub executable: PathBuf,
    /// Browser assets built and embedded by the distributing executable. The
    /// service library and schema generator do not depend on a frontend build.
    pub web_assets: &'static [(&'static str, &'static [u8])],
}

struct DiscoveryLease {
    path: PathBuf,
    _owner: fslock::LockFile,
}
impl Drop for DiscoveryLease {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            eprintln!("OneCrew: service discovery cleanup failed");
        }
    }
}

/// Run the service in the current process. No Tauri, Codex subprocess, GUI
/// window, implicit data migration or platform startup item is involved.
pub async fn run_service(options: ServiceOptions) -> Result<(), String> {
    if !options
        .web_assets
        .iter()
        .any(|(name, _)| *name == "index.html")
    {
        return Err("web_assets_missing".into());
    }
    let directory = options.data_dir.clone();
    let executable_path = options.executable.clone();
    let executable = options.executable;
    let (owner, binary_id, web_port) = run_blocking("service_owner", move || {
        let owner = paths::acquire_owner(&directory)?;
        let package = if crate::is_managed_cli_copy(&executable) {
            crate::default_cli_installation()?
        } else {
            executable.clone()
        };
        #[cfg(windows)]
        let package = if let Some(installed) =
            std::env::var_os("ONECREW_LITE_HOST_INSTALLED_EXECUTABLE")
        {
            crate::service_host::verified_package(&executable, std::path::Path::new(&installed))?
        } else {
            package
        };
        crate::cli_installation::require_complete_workspace(&package)?;
        Ok::<_, String>((
            owner,
            paths::executable_identity(&executable)?,
            paths::web_port(&directory)?,
        ))
    })
    .await??;
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .map_err(|_| "control_listener_unavailable")?;
    let web_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, web_port))
        .await
        .map_err(|_| "web_listener_unavailable: the stable local web port is occupied")?;
    let endpoint = Arc::new(Endpoint {
        protocol: ipc::PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        binary_id,
        pid: std::process::id(),
        port: listener
            .local_addr()
            .map_err(|_| "control_listener_unavailable")?
            .port(),
        web_port: web_listener
            .local_addr()
            .map_err(|_| "web_listener_unavailable")?
            .port(),
        token: ipc::random_secret(),
    });
    let state = ServiceState::new(
        options.data_dir.clone(),
        options.web_assets,
        executable_path,
        endpoint.binary_id.clone(),
    );
    state.browser.set_port(endpoint.web_port)?;
    let discovery_path = options.data_dir.join(paths::ENDPOINT_FILE);
    let path = discovery_path.clone();
    let descriptor = endpoint.clone();
    run_blocking("service_discovery_write", move || {
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                std::fs::remove_file(&path).map_err(|_| "stale_discovery_cleanup_failed")?
            }
            Ok(_) => return Err("unsafe_service_discovery".to_owned()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err("service_discovery_unavailable".to_owned()),
        }
        let bytes = zeroize::Zeroizing::new(
            serde_json::to_vec(descriptor.as_ref())
                .map_err(|_| "service_discovery_encoding_failed")?,
        );
        crate::private_file::write_private_file(&path, &bytes)
    })
    .await??;
    let _lease = DiscoveryLease {
        path: discovery_path,
        _owner: owner,
    };
    state.startup.start(options.data_dir)?;
    crate::table_sync::ensure_started(&state);
    let web_state = state.clone();
    let web_stop = state.shutdown.clone();
    let mut web_task = tokio::spawn(async move {
        axum::serve(web_listener, crate::web::router(web_state))
            .with_graceful_shutdown(web_stop.cancelled_owned())
            .await
    });
    let mut connections = JoinSet::new();
    let slots = Arc::new(Semaphore::new(128));
    let mut login_poll = tokio::time::interval(Duration::from_millis(500));
    let mut previous_login = None;
    let signal = shutdown_signal();
    tokio::pin!(signal);
    let mut web_finished = false;
    loop {
        tokio::select! {
            _ = state.shutdown.cancelled() => break,
            _ = &mut signal => {
                signal.set(shutdown_signal());
                if let Err(error) = state.stop().await {
                    eprintln!("OneCrew: shutdown delayed: {error}");
                    // A busy operation retains the data lease. Explicit stop
                    // can be retried through the control socket.
                }
            }
            result = &mut web_task, if !web_finished => {
                web_finished = true;
                if !matches!(result, Ok(Ok(()))) { eprintln!("OneCrew: web listener failed"); }
                if let Err(error) = state.stop().await { eprintln!("OneCrew: shutdown delayed: {error}"); }
            }
            _ = login_poll.tick() => {
                state.emit_login_change(&mut previous_login).await;
            }
            accepted = listener.accept() => {
                let (stream, _) = match accepted {
                    Ok(accepted) => accepted,
                    Err(_) => { eprintln!("OneCrew: control accept failed"); tokio::time::sleep(Duration::from_millis(100)).await; continue; }
                };
                let Ok(permit) = slots.clone().try_acquire_owned() else { drop(stream); continue; };
                let endpoint = endpoint.clone();
                let state = state.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    // Protocol/credential errors deliberately have no raw
                    // request logging: every code is a fixed identifier. EOF
                    // is a normal client lifecycle edge and stays at debug.
                    if let Err(code) = ipc::serve_connection(stream, endpoint, state).await {
                        match code.as_str() {
                            "mcp_initialize_timeout" | "mcp_initialize_failed" | "mcp_session_failed" => {
                                log::warn!("MCP session ended abnormally: {code}");
                            }
                            _ => log::debug!("control connection closed: {code}"),
                        }
                    }
                });
            }
            Some(joined) = connections.join_next(), if !connections.is_empty() => {
                if joined.is_err() { eprintln!("OneCrew: client session join failed"); }
            }
        }
    }
    drop(listener);
    while let Some(joined) = connections.join_next().await {
        if joined.is_err() {
            eprintln!("OneCrew: client session join failed during shutdown");
        }
    }
    if !web_finished {
        web_task
            .await
            .map_err(|_| "web_listener_join_failed")?
            .map_err(|_| "web_listener_failed")?;
    }
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => (), _ = terminate.recv() => () }
            return;
        }
    }
    if let Err(error) = tokio::signal::ctrl_c().await {
        log::warn!("ctrl-c signal listener failed: {error}");
    }
}
