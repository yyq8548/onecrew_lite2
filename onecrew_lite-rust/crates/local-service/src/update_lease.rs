//! Explicit updater ownership. A stop reply is not proof the old host released its lease.
use crate::{ServiceClient, blocking::run_blocking, paths};
use std::{path::PathBuf, time::Duration};

pub struct ServiceUpdateLease {
    _owner: fslock::LockFile,
}

/// Never starts a service or opens business data. Retain this guard through replacement.
pub async fn stop_for_update(data_dir: PathBuf) -> Result<ServiceUpdateLease, String> {
    match ServiceClient::existing(&data_dir).await {
        Ok(client) => {
            client.request("stop").await?;
        }
        Err(error) if error == "service_not_running" => {}
        Err(error) => return Err(error),
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    loop {
        let directory = data_dir.clone();
        match run_blocking("update_owner_lease", move || {
            paths::acquire_owner(&directory)
        })
        .await?
        {
            Ok(owner) => return Ok(ServiceUpdateLease { _owner: owner }),
            Err(error) if error == "service_already_running" => {
                if tokio::time::Instant::now() >= deadline {
                    return Err("update_service_drain_timeout".into());
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => return Err(error),
        }
    }
}
