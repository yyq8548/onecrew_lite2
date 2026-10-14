//! Explicit, read-only compatibility inspection. Never installs or starts a companion.
use crate::ServiceState;
use serde::Serialize;
use std::{
    path::Path,
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::io::AsyncReadExt;

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct ComponentsReport {
    version: String,
    built_at: u64,
    binary_sha256: String,
    checked_at: u64,
    plugin_version: Option<String>,
    plugin_state: String,
    library_version: Option<String>,
    library_state: String,
    opencli_version: Option<String>,
    opencli_state: String,
}

async fn library_version_at(executable: &Path) -> Option<String> {
    let path = executable
        .parent()?
        .join("onecrew_lite_library")
        .join(if cfg!(windows) {
            "onecrew_lite.exe"
        } else {
            "onecrew_lite"
        });
    let mut child = tokio::process::Command::new(path)
        .arg("--version")
        .env_remove("ONECREW_LITE_DATA_DIR")
        .env_remove("ONECREW_LITE_RUNTIME_FILE")
        .env_remove("PORT")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut output = child.stdout.take()?.take(8193);
    let mut bytes = Vec::new();
    let (read, exit) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(output.read_to_end(&mut bytes), child.wait())
    })
    .await
    .ok()?;
    if read.is_err() || !exit.ok()?.success() || bytes.len() > 8192 {
        return None;
    }
    let text = std::str::from_utf8(&bytes).ok()?.trim();
    text.split_whitespace()
        .last()
        .filter(|s| s.len() <= 80)
        .map(str::to_owned)
}

async fn library_version() -> Option<String> {
    // The service runs from a content-addressed cache, while the companion is
    // adjacent to the installed CLI, just as in the MCP workspace launcher.
    let layout = crate::UserInstallLayout::current().ok()?;
    library_version_at(layout.executable()).await
}

pub(crate) async fn check(state: &ServiceState) -> Result<ComponentsReport, String> {
    let plugin = crate::codex_plugin_status::refresh(state).await?;
    let setup = crate::opencli_onboarding::status(state).await?;
    let library_version = library_version().await;
    let library_state = if library_version.as_deref() != Some("0.1.4") {
        "version_unverified".into()
    } else {
        match state.platform().await?.discovery_library_readiness().await {
            Ok(()) => "ready".into(),
            Err(code) => code,
        }
    };
    let opencli_state = if setup.version.as_deref() != Some(setup.required_version) {
        "version_incompatible"
    } else if setup.addons.iter().any(|a| {
        serde_json::to_value(a)
            .ok()
            .is_none_or(|v| v["state"] != "ready")
    }) {
        "addons_unavailable"
    } else if !setup.extension_connected {
        "extension_unavailable"
    } else if !setup.daemon_running {
        "daemon_unavailable"
    } else if setup.issue.is_some() {
        "check_failed"
    } else {
        "ready"
    };
    Ok(ComponentsReport {
        version: env!("CARGO_PKG_VERSION").into(),
        built_at: env!("ONECREW_BUILT_AT").parse().unwrap_or(0),
        binary_sha256: state.binary_id().into(),
        checked_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        plugin_version: plugin.installed_version,
        plugin_state: if plugin.state == onecrew_codex::PluginState::Installed
            && plugin.enabled == Some(true)
            && plugin.diagnostic_code.is_none()
        {
            "ready"
        } else {
            "needs_attention"
        }
        .into(),
        library_version,
        library_state,
        opencli_version: setup.version,
        opencli_state: opencli_state.into(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn companion_version_probe_is_bounded_and_never_starts_the_service() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("onecrew_lite");
        let companion = directory.path().join("onecrew_lite_library/onecrew_lite");
        std::fs::create_dir(companion.parent().unwrap()).unwrap();
        std::fs::write(&companion, "#!/bin/sh\n[ \"$#\" = 1 ] && [ \"$1\" = --version ] || exit 1\nprintf 'onecrew_lite 0.1.3\\n'\n").unwrap();
        std::fs::set_permissions(&companion, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            library_version_at(&executable).await.as_deref(),
            Some("0.1.3")
        );
        assert_eq!(
            library_version_at(&directory.path().join("service-binaries/hash/onecrew_lite")).await,
            None
        );
        std::fs::write(&companion, "#!/bin/sh\nprintf '0.1.3\\n'\nexit 1\n").unwrap();
        assert_eq!(library_version_at(&executable).await, None);
        std::fs::write(&companion, "#!/bin/sh\nhead -c 8193 /dev/zero\n").unwrap();
        assert_eq!(library_version_at(&executable).await, None);
    }
}
