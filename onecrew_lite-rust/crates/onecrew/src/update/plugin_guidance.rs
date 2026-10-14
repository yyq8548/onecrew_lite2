//! Cached/local evidence only: no Codex process and no plugin receipt writes.
use super::outcome::CommandInstruction;
use onecrew_codex::{PluginState, PluginStatus, Verification};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Installed,
    NotInstalled,
    Unverified,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Compatibility {
    Compatible,
    PluginUpdateRequired,
    Optional,
    Unknown,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PluginGuidance {
    pub state: State,
    pub compatibility: Compatibility,
    pub verification: Verification,
    pub installed_version: Option<String>,
    pub expected_version: Option<String>,
    pub next_action: Option<CommandInstruction>,
    pub optional_action: Option<CommandInstruction>,
    pub after_install: Option<String>,
    pub diagnostic_code: Option<String>,
}

const AFTER_INSTALL: &str =
    "Fully quit and restart Codex after a successful explicit plugin install.";

pub(super) async fn collect(
    executable: PathBuf,
    target: &'static str,
    data_dir: Option<PathBuf>,
) -> PluginGuidance {
    let worker_path = executable.clone();
    let worker_data = data_dir.clone();
    tokio::task::spawn_blocking(move || {
        let cached = onecrew_local_service::UserInstallLayout::current()
            .and_then(|layout| onecrew_codex::read_cached_plugin_status(&layout.codex_plugin()));
        let expected = match &cached {
            Ok(status) if status.state == PluginState::Installed => {
                onecrew_codex::expected_plugin_version(&worker_path, target, worker_data.as_deref())
                    .ok()
            }
            _ => None,
        };
        project(cached.ok(), expected, &worker_path, worker_data.as_deref())
    })
    .await
    .unwrap_or_else(|_| unverified(&executable, data_dir.as_deref()))
}

fn unverified(executable: &Path, data_dir: Option<&Path>) -> PluginGuidance {
    PluginGuidance {
        state: State::Unverified,
        compatibility: Compatibility::Unknown,
        verification: Verification::NotChecked,
        installed_version: None,
        expected_version: None,
        next_action: Some(CommandInstruction::plugin_refresh(executable, data_dir)),
        optional_action: None,
        after_install: None,
        diagnostic_code: None,
    }
}

fn project(
    cached: Option<PluginStatus>,
    expected: Option<String>,
    executable: &Path,
    data_dir: Option<&Path>,
) -> PluginGuidance {
    let Some(status) = cached else {
        return unverified(executable, data_dir);
    };
    // Refreshing cannot help: Codex itself fails to load such a registration.
    if status.requires_marketplace_repair() {
        return PluginGuidance {
            state: State::Unverified,
            compatibility: Compatibility::PluginUpdateRequired,
            verification: status.verification,
            installed_version: None,
            expected_version: None,
            next_action: Some(CommandInstruction::plugin_install(executable, data_dir)),
            optional_action: None,
            after_install: Some(AFTER_INSTALL.to_owned()),
            diagnostic_code: status.diagnostic_code,
        };
    }
    if status.state == PluginState::NotInstalled {
        return PluginGuidance {
            state: State::NotInstalled,
            compatibility: Compatibility::Optional,
            verification: Verification::Cached,
            installed_version: None,
            expected_version: None,
            next_action: None,
            optional_action: Some(CommandInstruction::plugin_install(executable, data_dir)),
            after_install: None,
            diagnostic_code: None,
        };
    }
    let Some(expected) = expected.filter(|_| status.state == PluginState::Installed) else {
        return unverified(executable, data_dir);
    };
    let compatible = status.installed_version.as_deref() == Some(expected.as_str());
    PluginGuidance {
        state: State::Installed,
        compatibility: if compatible {
            Compatibility::Compatible
        } else {
            Compatibility::PluginUpdateRequired
        },
        verification: Verification::Cached,
        installed_version: status.installed_version,
        expected_version: Some(expected),
        next_action: (!compatible)
            .then(|| CommandInstruction::plugin_install(executable, data_dir)),
        optional_action: None,
        after_install: (!compatible).then(|| AFTER_INSTALL.to_owned()),
        diagnostic_code: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_states_produce_guidance_without_running_commands() {
        for (state, version, compatibility) in [
            (
                PluginState::Installed,
                Some("old"),
                Compatibility::PluginUpdateRequired,
            ),
            (
                PluginState::Installed,
                Some("new"),
                Compatibility::Compatible,
            ),
            (PluginState::NotInstalled, None, Compatibility::Optional),
            (PluginState::Unknown, None, Compatibility::Unknown),
            (PluginState::Unavailable, None, Compatibility::Unknown),
        ] {
            let status = PluginStatus {
                state,
                verification: Verification::Cached,
                installed_version: version.map(str::to_owned),
                enabled: Some(true),
                last_verified_at: Some(1),
                diagnostic_code: None,
            };
            let result = project(
                Some(status),
                Some("new".into()),
                Path::new("/tmp/onecrew"),
                None,
            );
            assert_eq!(result.compatibility, compatibility);
        }
    }

    #[test]
    fn a_broken_marketplace_registration_requires_an_explicit_install() {
        for code in [
            "codex_marketplace_source_missing",
            "codex_marketplace_source_relocated",
        ] {
            let status = PluginStatus {
                state: PluginState::Unavailable,
                verification: Verification::Cached,
                installed_version: None,
                enabled: None,
                last_verified_at: Some(1),
                diagnostic_code: Some(code.into()),
            };
            let result = project(Some(status), None, Path::new("/tmp/onecrew"), None);
            assert_eq!(result.compatibility, Compatibility::PluginUpdateRequired);
            assert_eq!(result.diagnostic_code.as_deref(), Some(code));
            let action = serde_json::to_value(result.next_action).unwrap();
            assert_eq!(action["args"], serde_json::json!(["plugin", "install"]));
            assert!(result.after_install.is_some());
        }
    }
}
