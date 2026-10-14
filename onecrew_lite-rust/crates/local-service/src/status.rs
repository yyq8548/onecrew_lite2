//! Secret-free startup/status projection and explicit next actions. Computing
//! this view never installs a plugin or starts a Codex subprocess.
use onecrew_codex::{PluginState, PluginStatus};
use onecrew_creator_sync::CreatorSyncAdmissionState;
use onecrew_local_runtime::{LocalDataPreflight, LocalDataState, LocalDataUpgradeError};
use serde::Serialize;
use std::path::Path;

use crate::login::LoginProjection;

#[derive(Clone, Copy)]
pub(crate) enum StatusContext {
    Human,
    Mcp,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum NextActionKind {
    AwaitStartup,
    Login,
    AwaitLogin,
    InstallPlugin,
    EnablePlugin,
    OpenWeb,
    InspectData,
    UpdateCli,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct CliInvocation {
    executable: String,
    args: Vec<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct NextAction {
    kind: NextActionKind,
    command: Option<CliInvocation>,
    tool: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instruction: Option<String>,
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct StartupSummary {
    pub(crate) phase: crate::startup::StartupPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<LocalDataUpgradeError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diagnostic_code: Option<String>,
}

impl StartupSummary {
    pub(crate) fn from_report(report: &crate::startup::DataStartupStatus) -> Self {
        let failed = report.phase == crate::startup::StartupPhase::Failed;
        Self {
            phase: report.phase,
            failure: failed.then(|| report.failure.clone()).flatten(),
            diagnostic_code: failed.then(|| report.diagnostic_code.clone()).flatten(),
        }
    }
}

pub(crate) fn current_data(
    report: &crate::startup::DataStartupStatus,
) -> Option<LocalDataPreflight> {
    if report.phase != crate::startup::StartupPhase::Ready {
        return None;
    }
    report
        .postflight
        .as_ref()
        .or(report.preflight.as_ref())
        .filter(|data| matches!(data.state, LocalDataState::Fresh | LocalDataState::Ready))
        .cloned()
}

#[derive(Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalServiceStatus {
    pub(crate) locale: onecrew_local_runtime::PresentationLocale,
    pub(crate) workspace_activity: Option<crate::discovery_workspace::WorkspaceActivity>,
    pub(crate) workspace_history: Vec<crate::discovery_workspace::WorkspaceActivity>,
    pub(crate) workspace_activity_error: Option<String>,
    pub(crate) version: String,
    pub(crate) pid: u32,
    pub(crate) running: bool,
    pub(crate) transport: String,
    pub(crate) mcp_clients: u32,
    pub(crate) binary_sha256: String,
    pub(crate) data: Option<LocalDataPreflight>,
    pub(crate) startup: StartupSummary,
    pub(crate) login: Option<LoginProjection>,
    pub(crate) login_error: Option<String>,
    pub(crate) plugin: PluginStatus,
    pub(crate) cli_update: crate::cli_update_status::CliUpdateStatus,
    pub(crate) next_actions: Vec<NextAction>,
}

pub(crate) fn next_actions(
    status: &LocalServiceStatus,
    context: StatusContext,
    executable: &Path,
    data_dir: &Path,
    update_executable: Option<&Path>,
) -> Vec<NextAction> {
    let command = |args: &[&str]| {
        Some(CliInvocation {
            executable: executable.to_string_lossy().into_owned(),
            args: args
                .iter()
                .map(|value| (*value).to_owned())
                .chain([
                    "--data-dir".to_owned(),
                    data_dir.to_string_lossy().into_owned(),
                ])
                .collect(),
        })
    };
    let mut actions = Vec::new();
    match status.startup.phase {
        crate::startup::StartupPhase::Inspecting | crate::startup::StartupPhase::Upgrading => {
            actions.push(NextAction {
                kind: NextActionKind::AwaitStartup,
                command: command(&["status"]),
                tool: Some("onecrew_status".into()),
                instruction: None,
            });
            return actions;
        }
        crate::startup::StartupPhase::Failed => {
            actions.push(NextAction {
                kind: NextActionKind::InspectData,
                command: command(&["data", "status"]),
                tool: None,
                instruction: None,
            });
            return actions;
        }
        crate::startup::StartupPhase::Ready => (),
    }
    if status.login.as_ref().is_some_and(|projection| {
        matches!(
            projection.status.state,
            CreatorSyncAdmissionState::PairingPending
                | CreatorSyncAdmissionState::PairingCompleting
        )
    }) {
        actions.push(NextAction {
            kind: NextActionKind::AwaitLogin,
            command: command(&["status"]),
            tool: Some("onecrew_status".into()),
            instruction: None,
        });
    } else if !status
        .login
        .as_ref()
        .is_some_and(|projection| projection.status.state == CreatorSyncAdmissionState::Active)
    {
        actions.push(NextAction {
            kind: NextActionKind::Login,
            command: command(&["login", "--no-browser"]),
            tool: Some("onecrew_login".into()),
            instruction: None,
        });
    }
    // An already connected MCP client gets no installation prompt: the Codex
    // router never installs or refreshes the plugin, and a broken marketplace
    // registration reaches it through plugin.diagnostic_code. Human
    // startup/setup may offer an explicit action from cached evidence.
    if matches!(context, StatusContext::Human) {
        if status.plugin.requires_marketplace_repair() {
            actions.push(NextAction {
                kind: NextActionKind::InstallPlugin,
                command: command(&["plugin", "install"]),
                tool: None,
                instruction: Some("Codex's registered OneCrew plugin marketplace is missing or in a location this OneCrew no longer maintains, so new Codex tasks may not load the current plugin. Tell the user, and run this plugin install command only when the user chooses to regenerate and re-register it; afterwards the user must fully quit and restart Codex and continue in a new task. A status hint is not authorization.".into()),
            });
        } else if status.plugin.state != PluginState::Installed {
            actions.push(NextAction {
                kind: NextActionKind::InstallPlugin,
                command: command(&["plugin", "install"]),
                tool: None,
                instruction: None,
            });
        } else if status.plugin.enabled == Some(false) {
            actions.push(NextAction {
                kind: NextActionKind::EnablePlugin,
                command: None,
                tool: None,
                instruction: None,
            });
        }
    }
    actions.push(NextAction {
        kind: NextActionKind::OpenWeb,
        command: command(&["web", "--no-browser"]),
        tool: Some("onecrew_web".into()),
        instruction: None,
    });
    if let Some((current, latest)) = status.cli_update.available_versions() {
        let update = update_executable.map(|path| {
            let mut args = vec!["update".to_owned()];
            if crate::paths::default_data_dir().ok().as_deref() != Some(data_dir) {
                args.extend([
                    "--data-dir".to_owned(),
                    data_dir.to_string_lossy().into_owned(),
                ]);
            }
            CliInvocation {
                executable: path.to_string_lossy().into_owned(),
                args,
            }
        });
        let method = if update.is_some() {
            "Show the user the optional onecrew update command returned here. Run it only when the user explicitly requests an update; this command does not automatically install plugins."
        } else {
            "Tell the user they can choose to install or update the standalone OneCrew CLI from the Access account center. Do not run update from a bundled plugin or service-cache copy."
        };
        let evidence = if status.cli_update.is_cached() {
            " This is cached release information from the reported last check, not a fresh check."
        } else {
            ""
        };
        actions.push(NextAction {
            kind: NextActionKind::UpdateCli,
            instruction: Some(format!("A new OneCrew CLI version is available: {current} → {latest}. Briefly inform the user and wait for their update preference. Continue the requested work; do not pause it solely for this notice. {method} A status hint is not authorization.{evidence}")),
            command: update,
            tool: None,
        });
    }
    actions
}

#[cfg(test)]
mod tests;
