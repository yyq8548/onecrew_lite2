//! Explicit command orchestration. Each native/network/credential boundary has one owner.
mod authorization;
mod binary;
mod install;
mod outcome;
mod plugin_guidance;
mod remote;
mod replacement;

use crate::Arguments;
use outcome::{UpdateAction, UpdateOutcome, collect_outcome};
use std::path::Path;

pub(super) async fn execute(
    arguments: &Arguments,
    executable: &Path,
) -> Result<UpdateOutcome, String> {
    if arguments.data_dir.to_str().is_none() || executable.to_str().is_none() {
        return Err("update_path_not_utf8".into());
    }
    let target = binary::host_target()?;
    if arguments.command == "__bootstrap-update" {
        return install::bootstrap(arguments, executable, target).await;
    }
    let installed = binary::inspect_target(executable.to_owned()).await?;
    let authorization = authorization::load(arguments.data_dir.clone()).await?;
    let remote = remote::Remote::production()?;
    let stage = binary::download_directory(installed.clone()).await?;
    let candidate = stage.path().join(if cfg!(windows) {
        "onecrew_lite.exe"
    } else {
        "onecrew_lite"
    });
    for attempt in 0..2 {
        let release = remote.latest(authorization.bearer(), target).await?;
        let current_version = remote::version(env!("CARGO_PKG_VERSION"))?;
        let release_version = remote::version(&release.version)?;
        let installed_hash = binary::fingerprint(installed.clone()).await?;
        let identical = installed_hash == (release.asset.size, release.asset.sha256.clone());
        if release_version < current_version || (release_version == current_version && identical) {
            let data_dir = arguments
                .explicit_data_dir
                .then(|| arguments.data_dir.clone());
            return Ok(collect_outcome(
                UpdateAction::AlreadyCurrent,
                env!("CARGO_PKG_VERSION"),
                &installed,
                target,
                data_dir,
            )
            .await);
        }
        match remote
            .download(&release, authorization.bearer(), &candidate)
            .await?
        {
            remote::Download::ReleaseChanged if attempt == 0 => continue,
            remote::Download::ReleaseChanged => return Err("update_release_changed_again".into()),
            remote::Download::Complete => {}
        }
        binary::validate_candidate(&candidate, target, &release.version).await?;
        authorization
            .verify_current(arguments.data_dir.clone())
            .await?;
        return install::delegate(&candidate, &installed, &release, arguments).await;
    }
    Err("update_release_changed_again".into())
}
