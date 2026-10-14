//! Explicit desktop launch of the two packaged applications. Each service
//! retains its own lock, authentication, data profile and startup lifecycle.
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryWeb {
    url: String,
}

fn library_url(bytes: &[u8]) -> Result<String, String> {
    let receipt: LibraryWeb =
        serde_json::from_slice(bytes).map_err(|_| "workspace_library_invalid_receipt")?;
    let url = url::Url::parse(&receipt.url).map_err(|_| "workspace_library_invalid_url")?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none_or(|port| port == 0)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/library"
        || url.query().is_some()
        || url.fragment().is_some()
        || url.as_str() != receipt.url
    {
        return Err("workspace_library_invalid_url".into());
    }
    Ok(receipt.url)
}

fn companion_path(executable: &Path, installed: &Path) -> Result<PathBuf, String> {
    let executable = if crate::is_managed_cli_copy(executable) {
        installed
    } else {
        executable
    };
    Ok(executable
        .parent()
        .ok_or("workspace_package_directory_unavailable")?
        .join("onecrew_lite_library")
        .join(if cfg!(target_os = "windows") {
            "onecrew_lite.exe"
        } else {
            "onecrew_lite"
        }))
}

pub async fn library_workspace_url(executable: &Path) -> Result<String, String> {
    let installed = crate::UserInstallLayout::current()?;
    let companion = companion_path(executable, installed.executable())?;
    if !crate::blocking::run_blocking("workspace_companion", {
        let companion = companion.clone();
        move || companion.is_file()
    })
    .await?
    {
        return Err(
            "workspace_library_not_packaged: use the complete onecrew_lite download".into(),
        );
    }
    let mut child = tokio::process::Command::new(&companion)
        // The persistent Library must not hold the plugin cache as its cwd.
        .current_dir(
            companion
                .parent()
                .ok_or("workspace_package_directory_unavailable")?,
        )
        .arg("web")
        .env_remove("ONECREW_LITE_DATA_DIR")
        .env_remove("ONECREW_LITE_RUNTIME_FILE")
        .env_remove("PORT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| "workspace_library_start_failed")?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or("workspace_library_output_unavailable")?
        .take(8193);
    let mut bytes = Vec::new();
    // A timed-out startup is reported, never retried or forcibly interpreted
    // as a stopped service: the Library owner may already have started it.
    let (read, status) = tokio::time::timeout(Duration::from_secs(45), async {
        tokio::join!(stdout.read_to_end(&mut bytes), child.wait())
    })
    .await
    .map_err(|_| "workspace_library_start_unknown: inspect Library status before retrying")?;
    read.map_err(|_| "workspace_library_output_unavailable")?;
    if bytes.len() > 8192 {
        return Err("workspace_library_invalid_receipt".into());
    }
    if !status
        .map_err(|_| "workspace_library_wait_failed")?
        .success()
    {
        return Err("workspace_library_start_failed: inspect the Library service log".into());
    }
    let url = library_url(&bytes)?;
    let health_url = url.trim_end_matches("/library").to_owned() + "/healthz";
    let health: serde_json::Value = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "workspace_library_health_unavailable")?
        .get(health_url)
        .send()
        .await
        .map_err(|_| "workspace_library_health_unavailable")?
        .error_for_status()
        .map_err(|_| "workspace_library_health_unavailable")?
        .json()
        .await
        .map_err(|_| "workspace_library_health_unavailable")?;
    verify_library_health(&health)?;
    Ok(url)
}

fn verify_library_health(health: &serde_json::Value) -> Result<(), String> {
    if health["ok"] != true
        || health["service"] != "onecrew_lite"
        || health["protocol_version"] != 1
        || health["engine"] != "teable-v2-pglite"
    {
        return Err("workspace_library_health_unavailable".into());
    }
    let capabilities = health["capabilities"].as_array();
    if !onecrew_local_runtime::REQUIRED_LIBRARY_CAPABILITIES
        .iter()
        .all(|required| {
            capabilities
                .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(required)))
        })
    {
        return Err("workspace_library_upgrade_required".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_requires_a_compatible_library_not_just_a_live_port() {
        let mut health = serde_json::json!({"ok":true,"service":"onecrew_lite","protocol_version":1,"engine":"teable-v2-pglite"});
        assert_eq!(
            verify_library_health(&health).unwrap_err(),
            "workspace_library_upgrade_required"
        );
        health["capabilities"] =
            serde_json::json!(onecrew_local_runtime::REQUIRED_LIBRARY_CAPABILITIES);
        assert!(verify_library_health(&health).is_ok());
        health["ok"] = false.into();
        assert_eq!(
            verify_library_health(&health).unwrap_err(),
            "workspace_library_health_unavailable"
        );
    }

    #[test]
    fn cached_cli_uses_installed_companion_and_portable_cli_keeps_its_own() {
        let installed = Path::new("/Users/customer/.local/bin/onecrew_lite");
        let expected = installed
            .parent()
            .unwrap()
            .join("onecrew_lite_library")
            .join(if cfg!(windows) {
                "onecrew_lite.exe"
            } else {
                "onecrew_lite"
            });
        for cached in [
            "/Users/customer/.codex/plugins/cache/onecrew_lite/version/bin/onecrew_lite",
            "/Users/customer/.local/share/onecrew_lite/service-binaries/hash/onecrew_lite",
        ] {
            assert_eq!(
                companion_path(Path::new(cached), installed).unwrap(),
                expected
            );
        }
        let portable = Path::new("/Downloads/lite/onecrew_lite");
        assert_eq!(
            companion_path(portable, installed)
                .unwrap()
                .parent()
                .unwrap(),
            Path::new("/Downloads/lite/onecrew_lite_library")
        );
    }

    #[test]
    fn only_the_verified_library_page_can_be_opened() {
        assert_eq!(
            library_url(br#"{"url":"http://127.0.0.1:8097/library"}"#).unwrap(),
            "http://127.0.0.1:8097/library"
        );
        for bad in [
            "https://example.com/library",
            "http://localhost:8091/library",
            "http://user:secret@127.0.0.1:8091/library",
            "http://127.0.0.1:8091/library?redirect=https://example.com",
            "http://127.0.0.1:8091/library#secret",
            "http://127.0.0.1:8091/login",
            "http://127.0.0.1/library",
        ] {
            assert!(library_url(serde_json::json!({ "url": bad }).to_string().as_bytes()).is_err());
        }
        assert!(
            library_url(br#"{"url":"http://127.0.0.1:8091/library","control_token":"secret"}"#)
                .is_err()
        );
    }
}
