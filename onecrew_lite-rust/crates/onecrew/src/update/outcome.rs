//! Typed completion; only the main CLI boundary serializes and prints it.
use super::plugin_guidance::{self, PluginGuidance};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum UpdateAction {
    Installed,
    Updated,
    AlreadyCurrent,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CommandInstruction {
    program: PathBuf,
    args: Vec<String>,
}

impl CommandInstruction {
    fn scoped(program: &Path, args: &[&str], data_dir: Option<&Path>) -> Self {
        let mut args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        if let Some(path) = data_dir {
            args.push("--data-dir".into());
            args.push(path.to_string_lossy().into_owned());
        }
        Self {
            program: program.to_owned(),
            args,
        }
    }

    pub(super) fn plugin_install(program: &Path, data_dir: Option<&Path>) -> Self {
        Self::scoped(program, &["plugin", "install"], data_dir)
    }

    pub(super) fn plugin_refresh(program: &Path, data_dir: Option<&Path>) -> Self {
        Self::scoped(program, &["plugin", "status", "--refresh"], data_dir)
    }

    fn login(program: &Path, data_dir: Option<&Path>) -> Self {
        Self::scoped(program, &["login", "--json"], data_dir)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct UpdateOutcome {
    pub action: UpdateAction,
    pub version: String,
    pub installed_path: PathBuf,
    pub plugin: PluginGuidance,
    pub next_action: Option<CommandInstruction>,
}

pub(super) async fn collect_outcome(
    action: UpdateAction,
    version: &str,
    installed: &Path,
    target: &'static str,
    data_dir: Option<PathBuf>,
) -> UpdateOutcome {
    let next_action = (action == UpdateAction::Installed)
        .then(|| CommandInstruction::login(installed, data_dir.as_deref()));
    let plugin = plugin_guidance::collect(installed.to_owned(), target, data_dir).await;
    UpdateOutcome {
        action,
        version: version.to_owned(),
        installed_path: installed.to_owned(),
        plugin,
        next_action,
    }
}
