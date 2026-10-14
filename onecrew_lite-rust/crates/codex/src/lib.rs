//! Explicit native plugin packaging and installation. No business-data ownership.
mod package;
mod status;

pub use package::{PluginPackage, expected_plugin_version, package_plugin};
pub use status::{
    PluginState, PluginStatus, Verification, codex_cli_candidates, install_plugin,
    read_cached_plugin_status, refresh_plugin_status, refresh_plugin_status_for_generations,
    resolve_codex_cli,
};

use std::path::Path;

/// Standalone MCP configuration for clients that do not use the Codex plugin.
/// This function never starts a process or installs anything.
pub fn setup_text(executable: &Path, data_dir: Option<&Path>) -> Result<String, String> {
    let executable = executable.to_str().ok_or("executable_path_not_utf8")?;
    let args = package::mcp_args(data_dir)?;
    let config = serde_json::to_string_pretty(
        &serde_json::json!({"mcpServers": {"onecrew_lite": {"command": executable, "args": args}}}),
    )
    .map_err(|_| "setup_encoding_failed")?;
    Ok(format!("{config}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_scopes_explicit_data_directory_and_omits_default_override() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("测试 one's data");
        let plain = setup_text(Path::new("onecrew_lite"), None).unwrap();
        assert!(!plain.contains("--data-dir"));
        let scoped = setup_text(Path::new("onecrew_lite"), Some(&path)).unwrap();
        assert!(scoped.contains("--data-dir"));
        assert!(scoped.contains(&serde_json::to_string(path.to_str().unwrap()).unwrap()));
        let config: serde_json::Value = serde_json::from_str(&scoped).unwrap();
        assert_eq!(
            config["mcpServers"]["onecrew_lite"]["args"],
            serde_json::json!(["mcp", "--stdio", "--data-dir", path.to_str().unwrap()])
        );
    }

    #[test]
    fn setup_preserves_command_path_without_shell_interpretation() {
        let text = setup_text(Path::new("/tmp/入口 one's $(file)/onecrew_lite"), None).unwrap();
        assert!(text.contains("\"command\": \"/tmp/入口 one's $(file)/onecrew_lite\""));
        let config: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            config["mcpServers"]["onecrew_lite"]["command"],
            "/tmp/入口 one's $(file)/onecrew_lite"
        );
        assert_eq!(
            config["mcpServers"]["onecrew_lite"]["args"],
            serde_json::json!(["mcp", "--stdio"])
        );
    }
}
