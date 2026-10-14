//! The sole data initialization owner. Supported upgrades run automatically
//! under the process lease, independently of web or MCP clients.
use onecrew_local_runtime::{
    LocalDataPreflight, LocalDataState, LocalDataUpgradeError, LocalDataUpgradeResult,
    LocalDataUpgradeStatus, LocalPlatform,
};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{sync::watch, task::JoinHandle};

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartupPhase {
    Inspecting,
    Upgrading,
    Ready,
    Failed,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct DataStartupStatus {
    pub(crate) phase: StartupPhase,
    pub(crate) preflight: Option<LocalDataPreflight>,
    pub(crate) postflight: Option<LocalDataPreflight>,
    pub(crate) upgrade: Option<LocalDataUpgradeResult>,
    pub(crate) failure: Option<LocalDataUpgradeError>,
    pub(crate) diagnostic_code: Option<String>,
}

impl DataStartupStatus {
    fn inspecting() -> Self {
        Self {
            phase: StartupPhase::Inspecting,
            preflight: None,
            postflight: None,
            upgrade: None,
            failure: None,
            diagnostic_code: None,
        }
    }
}

pub(crate) struct StartupOwner {
    status: watch::Sender<DataStartupStatus>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Default for StartupOwner {
    fn default() -> Self {
        let (status, _) = watch::channel(DataStartupStatus::inspecting());
        Self {
            status,
            worker: Mutex::new(None),
        }
    }
}

impl StartupOwner {
    /// Called once after the host owns its process lease and the discovery
    /// guard/listeners needed to expose limited startup diagnostics.
    pub(crate) fn start(self: &Arc<Self>, data_dir: PathBuf) -> Result<(), String> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|_| "startup_owner_unavailable")?;
        if worker.is_some() {
            return Err("startup_already_started".into());
        }
        let owner = self.clone();
        *worker = Some(tokio::spawn(async move {
            let publisher = owner.clone();
            let result = tokio::task::spawn_blocking(move || {
                prepare_with(
                    &data_dir,
                    |status| {
                        publisher.status.send_replace(status);
                    },
                    |path| LocalPlatform::preflight_data_dir(path),
                    |path| LocalPlatform::upgrade_data_dir(path),
                )
            })
            .await;
            match result {
                Ok(status) => {
                    owner.status.send_replace(status);
                }
                Err(_) => {
                    let mut status = owner.report();
                    status.phase = StartupPhase::Failed;
                    status.diagnostic_code = Some("LOCAL_DATA_STARTUP_WORKER_FAILED".into());
                    owner.status.send_replace(status);
                }
            }
        }));
        Ok(())
    }

    pub(crate) fn report(&self) -> DataStartupStatus {
        self.status.borrow().clone()
    }

    pub(crate) async fn wait_ready(&self) -> Result<(), String> {
        let mut status = self.status.subscribe();
        loop {
            match status.borrow().phase {
                StartupPhase::Ready => return Ok(()),
                StartupPhase::Failed => {
                    return Err(
                        "data_startup_failed: inspect onecrew status for recovery information"
                            .into(),
                    );
                }
                StartupPhase::Inspecting | StartupPhase::Upgrading => (),
            }
            status
                .changed()
                .await
                .map_err(|_| "startup_owner_unavailable")?;
        }
    }

    /// Never abort a blocking backup/transaction or release the process lease
    /// while it still runs, even when the original client has disconnected.
    pub(crate) async fn join(&self) -> Result<(), String> {
        let worker = self
            .worker
            .lock()
            .map_err(|_| "startup_owner_unavailable")?
            .take();
        if let Some(worker) = worker {
            worker.await.map_err(|_| "startup_worker_join_failed")?;
        }
        Ok(())
    }
}

fn prepare_with(
    data_dir: &Path,
    publish: impl Fn(DataStartupStatus),
    inspect: impl Fn(&Path) -> LocalDataPreflight,
    upgrade: impl Fn(&Path) -> Result<LocalDataUpgradeResult, LocalDataUpgradeError>,
) -> DataStartupStatus {
    let preflight = inspect(data_dir);
    let mut status = DataStartupStatus::inspecting();
    status.preflight = Some(preflight.clone());
    if matches!(
        preflight.state,
        LocalDataState::Fresh | LocalDataState::Ready
    ) {
        status.phase = StartupPhase::Ready;
        status.postflight = Some(preflight);
        return status;
    }
    if preflight.state != LocalDataState::UpgradeRequired
        || !preflight.recoverable_backup
        || preflight.remote_data_deleted
    {
        status.phase = StartupPhase::Failed;
        status.diagnostic_code = preflight
            .support_code
            .or(Some("LOCAL_DATA_STARTUP_BLOCKED".into()));
        return status;
    }
    status.phase = StartupPhase::Upgrading;
    publish(status.clone());
    match upgrade(data_dir) {
        Ok(result) => {
            let succeeded = matches!(
                result.status,
                LocalDataUpgradeStatus::Completed | LocalDataUpgradeStatus::AlreadyCurrent
            ) && !result.remote_data_deleted;
            let postflight = inspect(data_dir);
            status.phase = if succeeded
                && matches!(
                    postflight.state,
                    LocalDataState::Ready | LocalDataState::Fresh
                ) {
                StartupPhase::Ready
            } else {
                StartupPhase::Failed
            };
            if status.phase == StartupPhase::Failed {
                status.diagnostic_code = result
                    .support_code
                    .clone()
                    .or(Some("LOCAL_DATA_STARTUP_POSTFLIGHT_FAILED".into()));
            }
            status.postflight = Some(postflight);
            status.upgrade = Some(result);
        }
        Err(error) => {
            // A committed subset or a currently readable marker cannot turn
            // a typed upgrade failure into permission to open business.
            status.phase = StartupPhase::Failed;
            status.diagnostic_code = Some(error.support_code.clone());
            status.postflight = Some(inspect(data_dir));
            status.failure = Some(error);
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn compatible_data_never_calls_upgrade() {
        let data = tempfile::tempdir().unwrap();
        let upgrades = AtomicUsize::new(0);
        let status = prepare_with(
            data.path(),
            |_| (),
            |path| LocalPlatform::preflight_data_dir(path),
            |_| {
                upgrades.fetch_add(1, Ordering::SeqCst);
                unreachable!()
            },
        );
        assert!(status.phase == StartupPhase::Ready);
        assert!(status.upgrade.is_none());
        assert_eq!(upgrades.load(Ordering::SeqCst), 0);
        drop(LocalPlatform::open(data.path()).unwrap());
        let current = prepare_with(
            data.path(),
            |_| (),
            |path| LocalPlatform::preflight_data_dir(path),
            |_| {
                upgrades.fetch_add(1, Ordering::SeqCst);
                unreachable!()
            },
        );
        assert!(current.phase == StartupPhase::Ready);
        assert_eq!(current.preflight.unwrap().state, LocalDataState::Ready);
        assert!(current.upgrade.is_none());
        assert_eq!(upgrades.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn real_v35_predecessor_is_upgraded_without_web_or_mcp_requests() {
        let data = tempfile::tempdir().unwrap();
        drop(LocalPlatform::open(data.path()).unwrap());
        onecrew_creator_runtime::CreatorRuntime::downgrade_database_to_v35_for_test(
            data.path().join("crm.db"),
        )
        .unwrap();
        let preflight = LocalPlatform::preflight_data_dir(data.path());
        assert_eq!(preflight.state, LocalDataState::UpgradeRequired);
        assert_eq!(preflight.targets.len(), 1);
        assert_eq!(preflight.targets[0].from_version, 35);
        assert_eq!(
            preflight.targets[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        // Optional fixture export lets the native CLI/plugin smoke test start
        // from the same real predecessor without a production mutation hook.
        if let Some(destination) = std::env::var_os("ONECREW_PREDECESSOR_FIXTURE_OUT") {
            let destination = PathBuf::from(destination);
            assert!(destination.is_absolute());
            std::fs::create_dir(&destination).expect("fixture destination must not already exist");
            for entry in std::fs::read_dir(data.path()).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file() {
                    std::fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
                }
            }
            assert_eq!(
                LocalPlatform::preflight_data_dir(&destination).state,
                LocalDataState::UpgradeRequired
            );
        }
        let owner = Arc::new(StartupOwner::default());
        owner.start(data.path().into()).unwrap();
        owner.wait_ready().await.unwrap();
        owner.join().await.unwrap();
        let status = owner.report();
        assert!(status.phase == StartupPhase::Ready);
        let upgrade = status.upgrade.as_ref().unwrap();
        assert_eq!(upgrade.results.len(), 1);
        assert!(upgrade.results[0].recovery_point_created);
        assert_eq!(upgrade.results[0].from_version, 35);
        assert_eq!(
            upgrade.results[0].to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        // The service status carries the owner's typed retirement
        // counters under their own names; an empty predecessor reports zeros.
        let impact = upgrade.results[0].impact.as_ref().unwrap();
        assert_eq!(
            *impact,
            onecrew_creator_runtime::CreatorDatabaseUpgradeImpact::default()
        );
        let wire = serde_json::to_value(&status).unwrap();
        let impact_wire = &wire["upgrade"]["results"][0]["impact"];
        for counter in [
            "removed_campaigns",
            "removed_campaign_templates",
            "removed_campaign_account_bindings",
            "removed_campaign_delete_operations",
            "removed_campaign_template_delete_operations",
            "converted_campaign_search_snapshots",
            "converted_campaign_search_origins",
            "removed_unattributable_campaign_origins",
            "converted_campaign_community_sources",
            "cleared_audit_campaign_refs",
            "removed_campaign_idempotency_records",
            "removed_candidate_snapshots",
            "rewritten_contact_route_delete_impacts",
            "removed_tiktok_one_account_bindings",
            "removed_creator_save_confirmations",
            "removed_contact_confirmations",
            "unresolved_retired_outreach_actions",
        ] {
            assert_eq!(impact_wire[counter], serde_json::json!(0), "{counter}");
        }
        assert_eq!(
            LocalPlatform::preflight_data_dir(data.path()).state,
            LocalDataState::Ready
        );
    }

    #[tokio::test]
    async fn real_v36_startup_retires_storage_without_claiming_an_unknown_write_failed() {
        use onecrew_creator_runtime::{
            CreatorOutreachStatus, CreatorRetiredOutreachPhase, CreatorRuntime,
        };
        let data = tempfile::tempdir().unwrap();
        drop(LocalPlatform::open(data.path()).unwrap());
        let database = data.path().join("crm.db");
        CreatorRuntime::downgrade_database_to_v36_for_test(&database).unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute(
                "INSERT INTO idempotency_records
                (key, action, request_fingerprint, state, result_json, created_at, updated_at)
             VALUES ('unresolved-v36', 'direct_message_send', 'old-fingerprint',
                     'remote_accepted', '{\"receipt\":\"fixture-receipt\"}', 1, 1)",
                [],
            )
            .unwrap();
        drop(connection);
        let owner = Arc::new(StartupOwner::default());
        owner.start(data.path().into()).unwrap();
        owner.wait_ready().await.unwrap();
        owner.join().await.unwrap();
        let status = owner.report();
        assert!(status.phase == StartupPhase::Ready);
        let result = &status.upgrade.as_ref().unwrap().results[0];
        assert_eq!(result.from_version, 36);
        assert_eq!(
            result.to_version,
            onecrew_creator_runtime::CREATOR_SCHEMA_VERSION
        );
        assert!(result.recovery_point_created);
        assert_eq!(
            result
                .impact
                .as_ref()
                .unwrap()
                .unresolved_retired_outreach_actions,
            1
        );
        let connection = rusqlite::Connection::open(&database).unwrap();
        let retired_tables: u64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'
             AND name IN ('creator_candidate_snapshots','tiktok_one_account_bindings')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retired_tables, 0);
        drop(connection);
        let platform = LocalPlatform::open(data.path()).unwrap();
        let action = platform
            .creators()
            .unwrap()
            .creator_outreach_status("unresolved-v36")
            .await
            .unwrap();
        assert!(matches!(
            action,
            Some(CreatorOutreachStatus::Retired {
                phase: CreatorRetiredOutreachPhase::RetiredActionUnresolved,
                marked: true,
                ..
            })
        ));
    }

    #[test]
    fn partial_failure_stays_failed_even_when_postflight_is_current() {
        let data = tempfile::tempdir().unwrap();
        let ready = LocalPlatform::preflight_data_dir(data.path());
        let mut required = ready.clone();
        required.state = LocalDataState::UpgradeRequired;
        required.recoverable_backup = true;
        let inspections = AtomicUsize::new(0);
        let status = prepare_with(
            data.path(),
            |_| (),
            |_| {
                if inspections.fetch_add(1, Ordering::SeqCst) == 0 {
                    required.clone()
                } else {
                    ready.clone()
                }
            },
            |_| {
                Err(LocalDataUpgradeError {
                    kind: onecrew_local_runtime::LocalDataFailureKind::InitializationFailed,
                    message: "synthetic post-commit failure".into(),
                    support_code: "TEST_POST_COMMIT".into(),
                    storage_upgraded: true,
                    results: Vec::new(),
                })
            },
        );
        assert!(status.phase == StartupPhase::Failed);
        assert!(status.failure.unwrap().storage_upgraded);
        assert_eq!(status.diagnostic_code.as_deref(), Some("TEST_POST_COMMIT"));
    }

    #[tokio::test]
    async fn concurrent_start_and_repeated_status_reads_never_duplicate_the_attempt() {
        let data = tempfile::tempdir().unwrap();
        drop(LocalPlatform::open(data.path()).unwrap());
        onecrew_creator_runtime::CreatorRuntime::downgrade_database_to_v35_for_test(
            data.path().join("crm.db"),
        )
        .unwrap();
        let recovery_points = || {
            std::fs::read_dir(data.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
                .count()
        };
        let owner = Arc::new(StartupOwner::default());
        owner.start(data.path().into()).unwrap();
        assert_eq!(
            owner.start(data.path().into()),
            Err(String::from("startup_already_started")),
            "a concurrent caller is rejected while the attempt runs"
        );
        for _ in 0..3 {
            assert!(owner.report().phase != StartupPhase::Failed);
        }
        owner.wait_ready().await.unwrap();
        assert_eq!(
            owner.start(data.path().into()),
            Err(String::from("startup_already_started")),
            "a completed attempt is never restarted"
        );
        owner.join().await.unwrap();
        let status = owner.report();
        assert!(status.phase == StartupPhase::Ready);
        assert_eq!(status.upgrade.as_ref().unwrap().results.len(), 1);
        assert_eq!(recovery_points(), 1, "exactly one upgrade attempt ran");
        for _ in 0..3 {
            let repeated = owner.report();
            assert!(repeated.phase == StartupPhase::Ready);
            assert_eq!(repeated.upgrade.as_ref().unwrap().results.len(), 1);
        }
        assert_eq!(recovery_points(), 1, "status delivery is read-only");
        assert_eq!(
            LocalPlatform::preflight_data_dir(data.path()).state,
            LocalDataState::Ready
        );
    }
}
