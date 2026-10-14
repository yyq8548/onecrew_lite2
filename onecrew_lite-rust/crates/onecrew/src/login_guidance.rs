//! Post-login directions keep Codex plugin setup separate from other clients.
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn response(
    status: &Value,
    executable: &Path,
    data_dir: Option<&Path>,
) -> Result<Value, String> {
    if status
        .pointer("/login/status/state")
        .and_then(Value::as_str)
        != Some("active")
    {
        return Err("login is not active; run onecrew status".into());
    }
    let executable = executable.to_str().ok_or("executable_path_not_utf8")?;
    let command = |args: &[&str]| -> Result<Value, String> {
        let mut args: Vec<_> = args.iter().map(|arg| (*arg).to_owned()).collect();
        if let Some(data_dir) = data_dir {
            args.extend([
                "--data-dir".into(),
                data_dir
                    .to_str()
                    .ok_or("data_directory_not_utf8")?
                    .to_owned(),
            ]);
        }
        Ok(json!({"executable": executable, "args": args}))
    };
    let installed = status.pointer("/plugin/state").and_then(Value::as_str) == Some("installed");
    let disabled = status.pointer("/plugin/enabled").and_then(Value::as_bool) == Some(false);
    let codex = if installed {
        json!({"client": "codex", "action": if disabled { "enable_plugin" } else { "restart_codex" },
            "instruction": "The last-verified record contains the Codex plugin. Enable it if disabled, then ask the user to fully quit and restart Codex and continue in a new task. Codex automatically starts the bundled MCP. Do not add a separate MCP entry or manually launch mcp --stdio.", "command": null})
    } else {
        json!({"client": "codex", "action": "install_plugin",
            "instruction": "Install the OneCrew Codex plugin, then ask the user to fully quit and restart Codex and continue in a new task. Stop setup until the user restarts; do not try to reload Codex through UI automation. Codex automatically starts the bundled MCP. Do not add a separate MCP entry or manually launch mcp --stdio.",
            "command": command(&["plugin", "install"])?})
    };
    Ok(json!({
        "logged_in": true,
        "service_running": status["running"],
        "version": status["version"],
        "instruction": "Choose the connection route for your client. The webpage is independent of either route.",
        "connection_options": [codex, {
            "client": "other_mcp_clients", "action": "configure_mcp",
            "instruction": "Get the standalone MCP configuration and add it to your client. That client starts onecrew mcp --stdio. Do not install the Codex plugin for this route.",
            "command": command(&["setup"])?
        }],
        "web": {"instruction": "Open the local webpage.", "command": command(&["web"])?}
    }))
}

fn shell_command(command: &Value) -> Option<String> {
    let executable = command.get("executable")?.as_str()?;
    let args = command.get("args")?.as_array()?;
    let quote = |value: &str| {
        if cfg!(windows) {
            format!("'{}'", value.replace('\'', "''"))
        } else {
            format!("'{}'", value.replace('\'', "'\"'\"'"))
        }
    };
    let mut parts = vec![quote(executable)];
    for arg in args {
        parts.push(quote(arg.as_str()?));
    }
    Some(format!(
        "{}{}",
        if cfg!(windows) { "& " } else { "" },
        parts.join(" ")
    ))
}

pub(super) fn print(value: &Value) {
    println!("Signed in. The local service is running.\nChoose one client route:");
    for option in value["connection_options"].as_array().into_iter().flatten() {
        println!(
            "\n{}: {}",
            if option["client"] == "codex" {
                "Codex"
            } else {
                "Other MCP clients"
            },
            option["instruction"].as_str().unwrap_or_default()
        );
        if let Some(command) = shell_command(&option["command"]) {
            println!("  {command}");
        }
    }
    if let Some(command) = shell_command(&value["web"]["command"]) {
        println!("\nWebpage (optional):\n  {command}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn login_routes_are_alternatives_and_preserve_the_selected_data_directory() {
        let status = json!({"running":true,"version":"0.5.19","login":{"status":{"state":"active"}},"plugin":{"state":"unknown"}});
        let value = response(
            &status,
            Path::new("/tmp/onecrew"),
            Some(Path::new("/tmp/用户 data")),
        )
        .unwrap();
        assert_eq!(
            value["connection_options"][0]["command"]["args"],
            json!(["plugin", "install", "--data-dir", "/tmp/用户 data"])
        );
        assert_eq!(
            value["connection_options"][1]["command"]["args"],
            json!(["setup", "--data-dir", "/tmp/用户 data"])
        );
        assert_eq!(
            value["web"]["command"]["args"],
            json!(["web", "--data-dir", "/tmp/用户 data"])
        );
    }
    #[test]
    fn cached_installation_does_not_request_reinstall_or_claim_a_codex_connection() {
        let status = json!({"running":true,"login":{"status":{"state":"active"}},"plugin":{"state":"installed","enabled":false},"mcp_clients":2});
        let value = response(&status, Path::new("/tmp/onecrew"), None).unwrap();
        assert_eq!(value["connection_options"][0]["action"], "enable_plugin");
        assert!(value["connection_options"][0]["command"].is_null());
        assert!(
            response(
                &json!({"login":{"status":{"state":"pairing_pending"}}}),
                Path::new("onecrew"),
                None
            )
            .is_err()
        );
    }
}
