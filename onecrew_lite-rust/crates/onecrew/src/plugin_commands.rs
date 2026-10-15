//! Explicit installation/setup adapters. This module is never reached by a
//! service start, MCP startup, or ordinary service status request.
use onecrew_codex::{PluginState, PluginStatus};
use onecrew_local_service::UserInstallLayout;
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use tokio::io::AsyncWriteExt;

use crate::Arguments;

pub(super) async fn execute(arguments: &Arguments, executable: &Path) -> Result<(), String> {
    let executable = executable.to_owned();
    let data_dir = arguments
        .explicit_data_dir
        .then(|| arguments.data_dir.clone());
    match arguments.command.as_str() {
        "plugin install" => {
            let codex_cli = explicit_codex_cli(arguments.codex_cli.as_deref())?;
            let task = tokio::task::spawn_blocking(move || {
                onecrew_codex::install_plugin(
                    &UserInstallLayout::current()?.codex_plugin(),
                    &executable,
                    data_dir.as_deref(),
                    codex_cli.as_deref(),
                )
            });
            let status = join_plugin_operation(task).await?;
            print_install_status(status)
        }
        "plugin status" => {
            let refresh = arguments.refresh;
            let codex_cli = explicit_codex_cli(arguments.codex_cli.as_deref())?;
            let task = tokio::task::spawn_blocking(move || {
                let plugin_root = UserInstallLayout::current()?.codex_plugin();
                if refresh {
                    onecrew_codex::refresh_plugin_status(&plugin_root, codex_cli.as_deref())
                } else {
                    onecrew_codex::read_cached_plugin_status(&plugin_root)
                }
            });
            print_status(join_plugin_operation(task).await?)
        }
        "plugin package" => {
            let binary = arguments.package_binary.clone().unwrap_or(executable);
            let target = arguments
                .package_target
                .clone()
                .ok_or("plugin package requires --target")?;
            let output = arguments
                .package_output
                .clone()
                .ok_or("plugin package requires --output")?;
            let package = tokio::task::spawn_blocking(move || {
                onecrew_codex::package_plugin(&binary, &target, &output)
            })
            .await
            .map_err(|_| "plugin_package_worker_failed")??;
            crate::print_json(
                &serde_json::to_value(package).map_err(|_| "plugin_package_result_unavailable")?,
            );
            Ok(())
        }
        "setup" => {
            let text = onecrew_codex::setup_text(&executable, data_dir.as_deref())?;
            if arguments.copy {
                copy_setup(&text).await?;
                println!("Setup copied to clipboard.");
            } else {
                print!("{text}");
            }
            Ok(())
        }
        _ => Err("unknown_plugin_command".into()),
    }
}

/// Validate `--codex` before anything can reach Codex or a receipt.
///
/// A mistyped path is an argument error, not a plugin outcome: letting it
/// through would make the operation fail and overwrite an Installed receipt
/// with an Unavailable one.
fn explicit_codex_cli(path: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.is_absolute() {
        return Err("--codex requires an absolute path".into());
    }
    if !path.is_file() {
        return Err("--codex names no existing file".into());
    }
    Ok(Some(path.to_owned()))
}

/// Name what discovery looked at, and how to replace it, at the moment it
/// fails. Discovery checks PATH and supported desktop application layouts;
/// an explicit path remains available for other installations.
fn print_codex_cli_hint() {
    eprintln!("onecrew_lite could not find the Codex CLI. Paths tried, in order:");
    let candidates = onecrew_codex::codex_cli_candidates();
    if candidates.is_empty() {
        eprintln!("  (none: this process has no PATH)");
    }
    for candidate in &candidates {
        eprintln!("  {}", candidate.display());
    }
    if cfg!(windows) {
        eprintln!(
            "Rerun naming the CLI explicitly, for example:\n  onecrew_lite plugin install --codex (Get-Command codex.exe).Source"
        );
    } else {
        eprintln!(
            "Rerun naming the CLI explicitly, for example:\n  onecrew_lite plugin install --codex \"$(command -v codex)\""
        );
    }
}

/// Say which directory the plugin state is keyed by, and how to name it.
///
/// There is no flag for this: `CODEX_HOME` is the same variable Codex itself
/// reads, so a second way to say it could disagree with the CLI being driven.
fn print_codex_home_hint() {
    eprintln!(
        "onecrew_lite could not resolve the Codex home directory. It reads CODEX_HOME, and otherwise uses the .codex directory in your home directory."
    );
    if let Some(home) = dirs::home_dir() {
        eprintln!(
            "Default for this account: {}",
            home.join(".codex").display()
        );
    } else {
        eprintln!("This process has no home directory, so CODEX_HOME must be set.");
    }
}

async fn join_plugin_operation(
    mut task: tokio::task::JoinHandle<Result<PluginStatus, String>>,
) -> Result<PluginStatus, String> {
    tokio::select! {
        result = &mut task => result.map_err(|_| "plugin_worker_failed")?,
        _ = tokio::signal::ctrl_c() => {
            // Keep the generation lease with the real worker until Codex has
            // returned and readback/receipt has reached its terminal outcome.
            eprintln!("Waiting for the plugin operation to finish safely…");
            task.await.map_err(|_| "plugin_worker_failed")?
        }
    }
}

/// Codex's registration names a marketplace it cannot load, or one this CLI no
/// longer maintains; only an explicit install regenerates and re-registers it.
fn print_marketplace_repair_hint() {
    eprintln!(
        "Codex's onecrew_lite-cli marketplace registration points to a missing or relocated source, so new Codex tasks may not load the onecrew_lite plugin. Run onecrew_lite plugin install, then fully quit and restart Codex."
    );
}

fn print_status(status: PluginStatus) -> Result<(), String> {
    let failed = status.state == PluginState::Unavailable;
    let repair = status.requires_marketplace_repair();
    let diagnostic = status.diagnostic_code.clone();
    crate::print_json(&serde_json::to_value(status).map_err(|_| "plugin_status_unavailable")?);
    match diagnostic.as_deref() {
        Some("codex_cli_unavailable") => print_codex_cli_hint(),
        Some("codex_home_unavailable") => print_codex_home_hint(),
        _ if repair => print_marketplace_repair_hint(),
        _ => (),
    }
    if failed {
        Err(diagnostic.unwrap_or_else(|| "plugin_operation_unavailable".into()))
    } else {
        Ok(())
    }
}

fn print_install_status(status: PluginStatus) -> Result<(), String> {
    if status.diagnostic_code.as_deref() == Some("plugin_identity_change_required") {
        eprintln!(
            "Automatic replacement of a retired OneCrew plugin could not be verified. In Codex, remove the old onecrew-local plugin from onecrew_lite-cli or onecrew_lite-local, then run onecrew_lite plugin install and fully quit and restart Codex. Installation has not been verified complete."
        );
    }
    if status.state != PluginState::Installed || status.enabled != Some(true) {
        return print_status(status);
    }
    let mut value = serde_json::to_value(status).map_err(|_| "plugin_status_unavailable")?;
    value["next_step"] = serde_json::json!({
        "action": "restart_codex",
        "requires_user_action": true,
        "instruction": "Ask the user to fully quit and restart Codex, then verify onecrew_status and onecrew_web in the current task. A new window or task is not required. Stop setup here until the user restarts. If the transport remains closed, inspect host startup and plugin status before suggesting a new task as an optional diagnostic. Codex automatically launches this plugin's stdio MCP; do not add a standalone MCP entry, manually start mcp --stdio, or attempt Codex UI automation. Installation success does not prove the current task has loaded its tools."
    });
    crate::print_json(&value);
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn copy_setup(_text: &str) -> Result<(), String> {
    Err("clipboard_unsupported_on_this_platform".into())
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn copy_setup(text: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = tokio::process::Command::new("/usr/bin/pbcopy");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = tokio::process::Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "[Console]::InputEncoding = [Text.UTF8Encoding]::new($false); $text = [Console]::In.ReadToEnd(); Set-Clipboard -Value $text"]);
        command
    };
    let mut child = command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "clipboard_unavailable")?;
    let mut input = child.stdin.take().ok_or("clipboard_input_unavailable")?;
    input
        .write_all(text.as_bytes())
        .await
        .map_err(|_| "clipboard_write_failed")?;
    input
        .shutdown()
        .await
        .map_err(|_| "clipboard_write_failed")?;
    drop(input);
    if child
        .wait()
        .await
        .map_err(|_| "clipboard_write_failed")?
        .success()
    {
        Ok(())
    } else {
        Err("clipboard_write_failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_codex_cli_is_checked_before_anything_can_reach_a_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let cli = directory.path().join("codex");
        std::fs::write(&cli, b"not executed").unwrap();

        assert_eq!(explicit_codex_cli(None).unwrap(), None);
        assert_eq!(
            explicit_codex_cli(Some(&cli)).unwrap().as_deref(),
            Some(cli.as_path())
        );
        // A relative path would resolve against whatever directory the agent
        // shell happened to be in.
        assert!(explicit_codex_cli(Some(Path::new("codex"))).is_err());
        assert!(explicit_codex_cli(Some(&directory.path().join("absent"))).is_err());
        // A directory is not a CLI, and spawning one fails much later.
        assert!(explicit_codex_cli(Some(directory.path())).is_err());
    }
}
