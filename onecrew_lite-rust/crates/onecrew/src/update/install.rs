//! The validated candidate owns drain/lease/replacement; old CLI versions need no updater verb.
use super::{
    binary,
    outcome::{UpdateAction, UpdateOutcome, collect_outcome},
    remote, replacement,
};
use crate::Arguments;
use onecrew_local_service::stop_for_update;
use std::{path::Path, process::Stdio};

pub(super) async fn bootstrap(
    arguments: &Arguments,
    source: &Path,
    target: &'static str,
) -> Result<UpdateOutcome, String> {
    let version = arguments
        .expected_version
        .as_deref()
        .ok_or("bootstrap_expected_version_required")?;
    remote::version(version)?;
    if version != env!("CARGO_PKG_VERSION") {
        return Err("bootstrap_version_mismatch".into());
    }
    let expected_hash = arguments
        .expected_sha256
        .as_deref()
        .filter(|value| remote::valid_digest(value))
        .ok_or("bootstrap_hash_invalid")?;
    let expected_size = arguments
        .expected_size
        .filter(|size| *size > 0 && *size <= 512 * 1024 * 1024)
        .ok_or("bootstrap_size_invalid")?;
    let destination = binary::target_path(
        arguments
            .install_path
            .as_deref()
            .ok_or("bootstrap_install_path_required")?,
    )?;
    let source = source
        .canonicalize()
        .map_err(|_| "update_binary_unavailable")?;
    let expected = (expected_size, expected_hash.to_owned());
    let existed = destination
        .try_exists()
        .map_err(|_| "update_target_unavailable")?;
    let identical = {
        let source = source.clone();
        let destination = destination.clone();
        let expected = expected.clone();
        tokio::task::spawn_blocking(move || {
            binary::validate_header(&source, target)?;
            if binary::digest(&source)? != expected {
                return Err("bootstrap_binary_validation_failed".into());
            }
            Ok::<bool, String>(existed && binary::digest(&destination)? == expected)
        })
        .await
        .map_err(|_| "update_validation_worker_failed")??
    };
    let data_dir = arguments
        .explicit_data_dir
        .then(|| arguments.data_dir.clone());
    if identical {
        return Ok(collect_outcome(
            UpdateAction::AlreadyCurrent,
            version,
            &destination,
            target,
            data_dir,
        )
        .await);
    }
    let staged = binary::stage_verified_copy(source, destination.clone(), expected, target).await?;
    let lease = stop_for_update(arguments.data_dir.clone()).await?;
    let destination_copy = destination.clone();
    let mut task = tokio::task::spawn_blocking(move || {
        let _lease = lease;
        let backup = staged.directory.path().join("onecrew_lite.exe.old");
        let result = replacement::replace(&staged.path, &destination_copy, &backup);
        if backup.try_exists().unwrap_or(true) {
            let retained = staged.directory.keep();
            if result.is_err() {
                return Err(format!(
                    "update_replace_and_restore_failed: retained backup at {}",
                    retained.display()
                ));
            }
        }
        result
    });
    tokio::select! {
        result = &mut task => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Waiting for the update to finish safely…");
            task.await
        }
    }
    .map_err(|_| "update_replace_worker_failed")??;
    let action = if existed {
        UpdateAction::Updated
    } else {
        UpdateAction::Installed
    };
    Ok(collect_outcome(action, version, &destination, target, data_dir).await)
}

pub(super) async fn delegate(
    candidate: &Path,
    installed: &Path,
    release: &remote::Release,
    arguments: &Arguments,
) -> Result<UpdateOutcome, String> {
    let mut command = tokio::process::Command::new(candidate);
    command
        .arg("__bootstrap-update")
        .arg("--install-path")
        .arg(installed)
        .arg("--expected-version")
        .arg(&release.version)
        .arg("--expected-sha256")
        .arg(&release.asset.sha256)
        .arg("--expected-size")
        .arg(release.asset.size.to_string())
        .arg("--json");
    if arguments.explicit_data_dir {
        command.arg("--data-dir").arg(&arguments.data_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut task = tokio::spawn(async move { command.output().await });
    let output = tokio::select! {
        result = &mut task => result,
        _ = tokio::signal::ctrl_c() => {
            eprintln!("Waiting for the update to finish safely…");
            task.await
        }
    }
    .map_err(|_| "update_candidate_wait_failed")?
    .map_err(|_| "update_candidate_start_failed")?;
    if !output.status.success() {
        return Err("update_candidate_failed; inspect the preceding update result".into());
    }
    if output.stdout.len() > 64 * 1024 {
        return Err("update_installed_but_result_unavailable".into());
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "update_installed_but_result_unavailable".into())
}
