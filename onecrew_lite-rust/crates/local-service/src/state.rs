//! Shared application graph and its lifecycle. Construction and local-data
//! upgrades alone hold `lifecycle`; business operations never hold this lock.
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use onecrew_creator_sync::{CreatorSyncAdmissionState, CreatorSyncRuntime};
use onecrew_local_runtime::{
    CapabilityCatalog, LocalDataState, LocalPlatform, LocalPlatformHostPaths,
    PresentationPreferences,
};
use onecrew_mcp_adapter::{LocalMcpService, OperationRegistryHandle};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::{
    sync::{Mutex, OnceCell, broadcast},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

use crate::creator_snapshot_supervisor::{
    CreatorSnapshotLifecycleObserver, CreatorSnapshotSupervisor, CreatorSnapshotTaskOutcome,
    HostedSnapshotRunner,
};
use crate::{
    blocking::run_blocking,
    login::{LoginOwner, LoginProjection},
    work::WorkOwner,
};

#[cfg(test)]
#[path = "state/identity_review_tests.rs"]
mod identity_review_tests;

#[derive(Clone, Serialize)]
pub(crate) struct ServiceEvent {
    pub(crate) event: &'static str,
    pub(crate) payload: Value,
}

struct Application {
    platform: Arc<LocalPlatform>,
    catalog: Arc<CapabilityCatalog>,
    mcp: LocalMcpService,
    cancel: CancellationToken,
    changes: JoinHandle<()>,
    gmail_bulk: JoinHandle<()>,
    snapshot: Arc<CreatorSnapshotSupervisor>,
    snapshot_observer: Arc<dyn CreatorSnapshotLifecycleObserver>,
}

/// One admitted login opens the platform and immutable catalog, then compiles
/// one MCP registry referencing that catalog. Web navigation reads the catalog
/// directly; MCP owns the compiled registry handle.
struct ApplicationGraph {
    platform: Arc<LocalPlatform>,
    catalog: Arc<CapabilityCatalog>,
    mcp: LocalMcpService,
}

/// Blocking: opens the vault, Creator store, and provider adapters, then
/// compiles the registry and its search index exactly once.
fn open_application_graph(
    path: PathBuf,
    paths: LocalPlatformHostPaths,
    sync: Arc<CreatorSyncRuntime>,
    preferences: Arc<PresentationPreferences>,
    recover_discovery: bool,
) -> Result<ApplicationGraph, String> {
    let platform = Arc::new(
        LocalPlatform::open_with_host_paths_and_creator_sync(path, paths, sync)?
            .with_presentation_preferences(preferences),
    );
    if recover_discovery {
        // The host owns the process lease and has not admitted business yet.
        // Later login-driven graph reconstruction must not interrupt live reads.
        platform
            .creators()?
            .interrupt_discovery_searches()
            .map_err(|_| "discovery_recovery_failed".to_owned())?;
    }
    let catalog = Arc::new(CapabilityCatalog::build().map_err(|error| error.to_string())?);
    assemble_application_graph(platform, catalog)
}

fn assemble_application_graph(
    platform: Arc<LocalPlatform>,
    catalog: Arc<CapabilityCatalog>,
) -> Result<ApplicationGraph, String> {
    let registry = OperationRegistryHandle::build(platform.clone(), catalog.clone())?;
    let mcp = LocalMcpService::new(platform.clone(), &registry);
    Ok(ApplicationGraph {
        platform,
        catalog,
        mcp,
    })
}

/// Publish a service event to the connected webpage. Having no subscriber is
/// an ordinary state (no page is open); it is recorded, never an error.
fn publish_service_event(events: &broadcast::Sender<ServiceEvent>, event: ServiceEvent) {
    if events.send(event).is_err() {
        log::debug!("service event published without a subscriber");
    }
}

/// Why business intake is closed for an admission projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionBlock {
    /// Only a new login can reopen business intake.
    LoginRequired,
    /// A saved login exists or an owner is still working; retry, do not log in.
    Retryable,
}

impl AdmissionBlock {
    fn message(self) -> &'static str {
        match self {
            Self::LoginRequired => "login_required: run onecrew login or use onecrew_login",
            Self::Retryable => {
                "login_temporarily_unavailable: OneCrew could not confirm the saved login right now; retry the same call shortly without starting a new login"
            }
        }
    }
}

/// Business gate over the whole admission projection, shared by the HTTP and
/// MCP peers. `TemporarilyUnavailable` is retryable only when saved-credential
/// validation could not reach Access (`ValidateAdmission`/`RemoteUnavailable`);
/// a failed login callback projects the same state with `Retry` and no saved
/// credential, so it needs a new login. No wildcard arm: a new state must choose.
pub(crate) fn business_admission(
    status: &onecrew_creator_sync::CreatorSyncStatus,
) -> Result<(), AdmissionBlock> {
    use CreatorSyncAdmissionState as S;
    use onecrew_creator_sync::{CreatorSyncAction, CreatorSyncIssue};
    match status.state {
        S::Active => Ok(()),
        S::Validating | S::PairingCompleting => Err(AdmissionBlock::Retryable),
        S::TemporarilyUnavailable
            if status.action == Some(CreatorSyncAction::ValidateAdmission)
                && status.issue == Some(CreatorSyncIssue::RemoteUnavailable) =>
        {
            Err(AdmissionBlock::Retryable)
        }
        S::TemporarilyUnavailable
        | S::Unconfigured
        | S::PairingRequired
        | S::PairingPending
        | S::ValidationRequired
        | S::Revoked
        | S::Unauthorized
        | S::RecoveryRequired => Err(AdmissionBlock::LoginRequired),
    }
}

pub(crate) struct ServiceState {
    pub(crate) data_dir: PathBuf,
    pub(crate) executable: PathBuf,
    binary_id: String,
    pub(crate) web_assets: &'static [(&'static str, &'static [u8])],
    pub(crate) work: Arc<WorkOwner>,
    pub(crate) controls: Arc<WorkOwner>,
    pub(crate) browser: crate::web::BrowserSessions,
    pub(crate) login: Arc<LoginOwner>,
    pub(crate) events: broadcast::Sender<ServiceEvent>,
    pub(crate) shutdown: CancellationToken,
    pub(crate) startup: Arc<crate::startup::StartupOwner>,
    pub(crate) presentation_preferences: Arc<PresentationPreferences>,
    pub(crate) workspace_activity: crate::discovery_workspace::WorkspaceActivityOwner,
    pub(crate) workspace_card_requests: crate::workspace_card_requests::CardRequests,
    pub(crate) discovery_execution: crate::discovery_execution::DiscoveryExecutionSupervisor,
    pub(crate) table_sync: crate::table_sync::TableSyncOwner,
    lifecycle: Mutex<()>,
    creator_sync: OnceCell<Arc<CreatorSyncRuntime>>,
    application: Mutex<Option<Application>>,
    discovery_recovered: AtomicBool,
    snapshot_sequence: AtomicU64,
    mcp_clients: AtomicUsize,
    cli_update: crate::cli_update_status::CliUpdateStatusOwner,
    update_command_path: Arc<crate::cli_installation::UpdateCommandPath>,
}

impl ServiceState {
    pub(crate) fn binary_id(&self) -> &str {
        &self.binary_id
    }
    pub(crate) fn new(
        data_dir: PathBuf,
        web_assets: &'static [(&'static str, &'static [u8])],
        executable: PathBuf,
        binary_id: String,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(128);
        Arc::new(Self {
            data_dir,
            executable,
            binary_id,
            web_assets,
            work: Arc::new(WorkOwner::default()),
            controls: Arc::new(WorkOwner::default()),
            browser: crate::web::BrowserSessions::default(),
            login: Arc::new(LoginOwner::default()),
            events,
            shutdown: CancellationToken::new(),
            startup: Arc::new(crate::startup::StartupOwner::default()),
            presentation_preferences: Arc::new(PresentationPreferences::default()),
            workspace_activity: crate::discovery_workspace::WorkspaceActivityOwner::default(),
            workspace_card_requests: crate::workspace_card_requests::CardRequests::default(),
            discovery_execution: crate::discovery_execution::DiscoveryExecutionSupervisor::default(
            ),
            table_sync: crate::table_sync::TableSyncOwner::default(),
            lifecycle: Mutex::new(()),
            creator_sync: OnceCell::new(),
            application: Mutex::new(None),
            discovery_recovered: AtomicBool::new(false),
            snapshot_sequence: AtomicU64::new(0),
            mcp_clients: AtomicUsize::new(0),
            cli_update: crate::cli_update_status::CliUpdateStatusOwner::default(),
            update_command_path: Arc::new(crate::cli_installation::UpdateCommandPath::default()),
        })
    }

    async fn sync_under_lifecycle(&self) -> Result<Arc<CreatorSyncRuntime>, String> {
        self.creator_sync.get_or_try_init(|| async {
            let path = self.data_dir.clone();
            run_blocking("creator_sync_open", move || {
                let preflight = LocalPlatform::preflight_data_dir(&path);
                if !matches!(preflight.state, LocalDataState::Fresh | LocalDataState::Ready) {
                    return Err("local_data_not_ready: inspect the local webpage or run onecrew data status".into());
                }
                CreatorSyncRuntime::open(path).map(Arc::new).map_err(|_| "creator_sync_unavailable".into())
            }).await?
        }).await.cloned()
    }

    pub(crate) async fn sync(&self) -> Result<Arc<CreatorSyncRuntime>, String> {
        self.startup.wait_ready().await?;
        let _lifecycle = self.lifecycle.lock().await;
        self.sync_under_lifecycle().await
    }

    pub(crate) async fn login_status(&self) -> Result<LoginProjection, String> {
        let sync = self.sync().await?;
        let status = run_blocking("login_status", move || sync.status()).await?;
        Ok(self.login.projection(status))
    }

    pub(crate) async fn validate_login(&self) -> Result<LoginProjection, String> {
        let sync = self.sync().await?;
        let status = Self::admission_status(sync).await?;
        Ok(self.login.projection(status))
    }

    pub(crate) async fn admission_status(
        sync: Arc<CreatorSyncRuntime>,
    ) -> Result<onecrew_creator_sync::CreatorSyncStatus, String> {
        let reader = sync.clone();
        let status = run_blocking("admission_status", move || reader.status()).await?;
        // ValidationRequired is the saved-credential startup state; Validating
        // means a validation owner holds the lock, so await its real result.
        // Do not overwrite a live pairing/cancellation/revocation projection
        // merely because that flow has not stored an active credential yet.
        if matches!(
            status.state,
            CreatorSyncAdmissionState::ValidationRequired | CreatorSyncAdmissionState::Validating
        ) {
            Ok(sync.ensure_admission_validated().await)
        } else {
            Ok(status)
        }
    }

    pub(crate) async fn emit_login_change(&self, previous: &mut Option<String>) {
        let Some(sync) = self.creator_sync.get().cloned() else {
            return;
        };
        let Ok(status) = run_blocking("login_event_status", move || sync.status()).await else {
            return;
        };
        let active = status.state == CreatorSyncAdmissionState::Active;
        let Ok(payload) = serde_json::to_value(self.login.projection(status)) else {
            return;
        };
        let encoded = payload.to_string();
        if previous.as_deref() != Some(encoded.as_str()) {
            *previous = Some(encoded);
            publish_service_event(
                &self.events,
                ServiceEvent {
                    event: "creator-sync://pairing-status-changed",
                    payload,
                },
            );
            let activate_mcp = active
                && self.mcp_clients.load(Ordering::Acquire) > 0
                && self.application.lock().await.is_none();
            if activate_mcp && let Err(error) = self.platform().await {
                log::warn!("application graph construction after pairing failed: {error}");
            }
        }
    }

    pub(crate) async fn platform(&self) -> Result<Arc<LocalPlatform>, String> {
        self.startup.wait_ready().await?;
        let _lifecycle = self.lifecycle.lock().await;
        let sync = self.sync_under_lifecycle().await?;
        let status = Self::admission_status(sync.clone()).await?;
        business_admission(&status).map_err(AdmissionBlock::message)?;
        let mut application = self.application.lock().await;
        if let Some(application) = application.as_ref() {
            return Ok(application.platform.clone());
        }
        let path = self.data_dir.clone();
        let preferences = self.presentation_preferences.clone();
        let recover_discovery = !self.discovery_recovered.load(Ordering::Acquire);
        let snapshot_observer: Arc<dyn CreatorSnapshotLifecycleObserver> =
            Arc::new(SnapshotObserver {
                events: self.events.clone(),
                login: self.login.clone(),
                sync: sync.clone(),
            });
        let ApplicationGraph {
            platform,
            catalog,
            mcp,
        } = run_blocking("application_open", move || {
            // This is a filesystem export capability, not a window dependency.
            let paths = dirs::desktop_dir()
                .map(LocalPlatformHostPaths::with_system_desktop)
                .unwrap_or_else(LocalPlatformHostPaths::without_system_desktop);
            open_application_graph(path, paths, sync, preferences, recover_discovery)
        })
        .await??;
        self.discovery_recovered.store(true, Ordering::Release);
        platform.creators()?.interrupt_gmail_bulk().await?;
        let creator_changes = platform.creators()?.subscribe_changes();
        let platform_changes = platform.subscribe_changes();
        let cancel = CancellationToken::new();
        let stop = cancel.clone();
        let events = self.events.clone();
        let task = tokio::spawn(crate::runtime_changes::run_change_merge(
            creator_changes,
            platform_changes,
            events,
            stop,
        ));
        let gmail_bulk = tokio::spawn(crate::gmail_bulk_worker::run(platform.creators()?.clone(), cancel.clone()));
        *application = Some(Application {
            platform: platform.clone(),
            catalog,
            mcp,
            cancel,
            changes: task,
            gmail_bulk,
            snapshot: CreatorSnapshotSupervisor::new(),
            snapshot_observer,
        });
        drop(application);
        if self.mcp_clients.load(Ordering::Acquire) > 0 {
            self.trigger_snapshot_edge().await;
        }
        Ok(platform)
    }

    pub(crate) async fn trigger_snapshot_edge(&self) {
        let owner = self.application.lock().await.as_ref().map(|application| {
            (
                application.snapshot.clone(),
                application.platform.clone(),
                application.snapshot_observer.clone(),
            )
        });
        let Some((snapshot, platform, observer)) = owner else {
            return;
        };
        let Ok(sequence) =
            self.snapshot_sequence
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
        else {
            return;
        };
        let runner = Arc::new(HostedSnapshotRunner {
            platform,
            work: self.work.clone(),
        });
        let _ = snapshot
            .trigger(sequence + 1, runner, Arc::downgrade(&observer))
            .await;
    }

    pub(crate) async fn mcp_connected(&self) {
        self.mcp_clients.fetch_add(1, Ordering::AcqRel);
        let constructed = self.application.lock().await.is_some();
        if constructed {
            self.trigger_snapshot_edge().await;
        } else {
            // First MCP-admitted graph construction triggers its start edge.
            // An unpaired service retains its limited login/status surface.
            if let Err(error) = self.platform().await {
                log::warn!("application graph construction on MCP admission failed: {error}");
            }
        }
    }

    pub(crate) async fn mcp_disconnected(&self) {
        self.mcp_clients.fetch_sub(1, Ordering::AcqRel);
        self.trigger_snapshot_edge().await;
    }

    pub(crate) async fn mcp(&self) -> Result<LocalMcpService, String> {
        self.platform().await?;
        self.application
            .lock()
            .await
            .as_ref()
            .map(|a| a.mcp.clone())
            .ok_or_else(|| "application_unavailable".into())
    }

    /// Read-only navigation shares the catalog used to compile this graph's
    /// MCP registry without borrowing any MCP protocol or execution model.
    pub(crate) async fn catalog(&self) -> Result<Arc<CapabilityCatalog>, String> {
        self.platform().await?;
        self.application
            .lock()
            .await
            .as_ref()
            .map(|a| a.catalog.clone())
            .ok_or_else(|| "application_unavailable".into())
    }

    pub(crate) async fn begin_login(&self) -> Result<Value, String> {
        self.discovery_execution.cancel_for("execution_interrupted");
        let _pause = self.work.pause(Duration::from_secs(30)).await?;
        let sync = self.sync().await?;
        let url = self.login.begin(sync).await?;
        Ok(json!({"authorize_url":url,"projection":self.login_status().await?}))
    }

    pub(crate) async fn cancel_login(&self) -> Result<Value, String> {
        self.login.cancel(self.sync().await?).await?;
        let projection = self.login_status().await?;
        let outcome = if projection.status.state == CreatorSyncAdmissionState::Active {
            "completed_before_cancellation"
        } else if matches!(projection.issue, Some(crate::login::LoginIssue::Canceled)) {
            "cancelled"
        } else {
            "already_terminal"
        };
        Ok(json!({"outcome":outcome,"projection":projection}))
    }

    pub(crate) async fn logout(&self) -> Result<Value, String> {
        self.discovery_execution.cancel_for("execution_interrupted");
        let _pause = self.work.pause(Duration::from_secs(30)).await?;
        let sync = self.sync().await?;
        self.login.cancel(sync.clone()).await?;
        sync.logout().await.map_err(|_| "logout_cleanup_failed")?;
        self.clear_application().await?;
        Ok(
            json!({"logged_out":true,"remote_revocation":false,"projection":self.login_status().await?}),
        )
    }

    async fn clear_application(&self) -> Result<(), String> {
        self.discovery_execution.shutdown().await?;
        let _lifecycle = self.lifecycle.lock().await;
        if let Some(application) = self.application.lock().await.take() {
            application.snapshot.close_intake();
            let report = application.snapshot.shutdown().await;
            application.cancel.cancel();
            let mut gmail_bulk = application.gmail_bulk;
            match tokio::time::timeout(Duration::from_secs(40), &mut gmail_bulk).await {
                Ok(Ok(())) => {},
                Ok(Err(error)) => log::error!("gmail bulk worker join failed: {error}"),
                Err(_) => { gmail_bulk.abort(); let _ = gmail_bulk.await; log::error!("gmail bulk worker shutdown timed out"); }
            }
            application
                .changes
                .await
                .map_err(|_| "runtime_events_join_failed")?;
            if report.timed_out != 0 || report.join_failed != 0 || report.panicked != 0 {
                return Err("creator_snapshot_shutdown_failed".into());
            }
        }
        // Zero is a forced-invalidation sentinel, outside the merge sequence.
        publish_service_event(
            &self.events,
            ServiceEvent {
                event: "runtime://changed",
                payload: json!({"sequence":0,"scopes":["all"]}),
            },
        );
        Ok(())
    }

    pub(crate) async fn data_status(&self) -> Result<Value, String> {
        serde_json::to_value(self.startup.report())
            .map_err(|_| "local_data_status_unavailable".into())
    }

    pub(crate) async fn service_status(
        &self,
        context: crate::status::StatusContext,
    ) -> Result<crate::status::LocalServiceStatus, String> {
        use onecrew_codex::{PluginState, PluginStatus, Verification};
        let startup = self.startup.report();
        let data = crate::status::current_data(&startup);
        let (login, login_error) = if startup.phase == crate::startup::StartupPhase::Ready {
            match self.validate_login().await {
                Ok(login) => (Some(login), None),
                Err(error) => (None, Some(error)),
            }
        } else {
            (None, None)
        };
        let plugin = run_blocking("cached_plugin_status", || {
            let layout = crate::cli_installation::UserInstallLayout::current()?;
            onecrew_codex::read_cached_plugin_status(&layout.codex_plugin())
        })
        .await?
        .unwrap_or(PluginStatus {
            state: PluginState::Unavailable,
            verification: Verification::NotChecked,
            installed_version: None,
            enabled: None,
            last_verified_at: None,
            diagnostic_code: Some("plugin_receipt_unavailable".into()),
        });
        let identity = crate::cli_update_status::UpdateIdentity::eligible(
            startup.phase == crate::startup::StartupPhase::Ready,
            login.as_ref(),
        );
        let data_dir = self.data_dir.clone();
        let identity_for_check = identity.clone();
        let cli_update = self
            .cli_update
            .read(identity, env!("CARGO_PKG_VERSION"), move || async move {
                let identity = identity_for_check
                    .ok_or(crate::cli_update_status::CheckFailure::CheckFailed)?;
                crate::cli_update_status::fetch_version(data_dir, identity).await
            })
            .await;
        let update_executable =
            if cli_update.state == crate::cli_update_status::CliUpdateState::Available {
                let resolver = self.update_command_path.clone();
                let source = self.executable.clone();
                let expected_hash = self.binary_id.clone();
                run_blocking("cli_update_command_path", move || {
                    resolver.resolve(&source, &expected_hash)
                })
                .await
                .ok()
                .flatten()
            } else {
                None
            };
        let (workspace_activity, workspace_activity_error) =
            if login_error.is_none() && login.is_some() {
                match crate::discovery_workspace::snapshot(self).await {
                    Ok(activity) => (activity, None),
                    Err(_) => (None, Some("workspace_activity_unavailable".into())),
                }
            } else {
                (None, None)
            };
        let mut status = crate::status::LocalServiceStatus {
            locale: self.presentation_preferences.locale(),
            workspace_activity,
            workspace_history: crate::discovery_workspace::history(self).await,
            workspace_activity_error,
            version: env!("CARGO_PKG_VERSION").into(),
            pid: std::process::id(),
            running: true,
            transport: "stdio".into(),
            binary_sha256: self.binary_id.clone(),
            data,
            startup: crate::status::StartupSummary::from_report(&startup),
            login,
            login_error,
            mcp_clients: self
                .mcp_clients
                .load(Ordering::Acquire)
                .try_into()
                .unwrap_or(u32::MAX),
            plugin,
            cli_update,
            next_actions: Vec::new(),
        };
        status.next_actions = crate::status::next_actions(
            &status,
            context,
            &self.executable,
            &self.data_dir,
            update_executable.as_deref(),
        );
        Ok(status)
    }

    pub(crate) async fn stop(&self) -> Result<(), String> {
        self.discovery_execution.cancel_for("execution_interrupted");
        let controls = self.controls.pause(Duration::from_secs(30)).await?;
        let pause = self.work.pause(Duration::from_secs(30)).await?;
        self.startup.join().await?;
        if let Some(sync) = self.creator_sync.get() {
            self.login.cancel(sync.clone()).await?;
        }
        self.clear_application().await?;
        // Intake remains closed until this process has relinquished its lease.
        controls.keep_closed();
        pause.keep_closed();
        self.shutdown.cancel();
        Ok(())
    }
}

struct SnapshotObserver {
    events: broadcast::Sender<ServiceEvent>,
    login: Arc<LoginOwner>,
    sync: Arc<CreatorSyncRuntime>,
}

impl CreatorSnapshotLifecycleObserver for SnapshotObserver {
    fn creator_sync_credential_invalidated(
        self: Arc<Self>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let sync = self.sync.clone();
            if let Ok(status) =
                run_blocking("snapshot_admission_status", move || sync.status()).await
                && let Ok(payload) = serde_json::to_value(self.login.projection(status))
            {
                publish_service_event(
                    &self.events,
                    ServiceEvent {
                        event: "creator-sync://pairing-status-changed",
                        payload,
                    },
                );
            }
        })
    }

    /// Creator snapshot sync is imperceptible to the user by product decision:
    /// the supervisor keeps its own outcome record and writes the terminal
    /// state to the service log, so this host observer publishes nothing. The
    /// service event it used to publish had no consumer in any adapter or Web
    /// surface, and the outcome no longer reaches a status projection.
    fn creator_snapshot_state_changed(
        self: Arc<Self>,
        _run_id: u64,
        _outcome: CreatorSnapshotTaskOutcome,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async {})
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use onecrew_creator_sync::{
        CreatorSyncAction as A, CreatorSyncAdmissionState as S, CreatorSyncIssue as I,
        CreatorSyncRuntime, CreatorSyncStatus,
    };
    use onecrew_local_runtime::{LocalPlatformHostPaths, PresentationPreferences};

    use super::{AdmissionBlock, business_admission, open_application_graph};

    #[test]
    fn first_host_graph_recovers_orphaned_discovery_but_reconstruction_does_not() {
        use onecrew_creator_runtime::{CreateDiscoveryCampaignInput, DiscoveryPlatformScope};
        let directory = tempfile::tempdir().unwrap();
        let platform = onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap();
        let creators = platform.creators().unwrap();
        creators
            .create_discovery_campaign(CreateDiscoveryCampaignInput {
                id: "orphaned-read".into(),
                brief: "AI tool creators".into(),
                opencli_version: "1.8.8".into(),
                criteria: serde_json::json!({"min_followers":10000}),
                scopes: vec![DiscoveryPlatformScope {
                    platform: "youtube".into(),
                    profile_ref: "dedicated".into(),
                    keywords: vec!["AI tools".into()],
                }],
                target_count: 30,
                max_calls: Some(10),
            })
            .unwrap();
        creators
            .approve_discovery_campaign("orphaned-read", 1)
            .unwrap();
        creators
            .claim_discovery_search("orphaned-read", 1, "youtube", "dedicated")
            .unwrap();
        drop(creators);
        drop(platform);
        let sync = Arc::new(CreatorSyncRuntime::open(directory.path()).unwrap());
        let graph = open_application_graph(
            directory.path().to_path_buf(),
            LocalPlatformHostPaths::without_system_desktop(),
            sync.clone(),
            Arc::new(PresentationPreferences::default()),
            true,
        )
        .unwrap();
        let owner = graph.platform.creators().unwrap();
        let recovered = owner.get_discovery_campaign("orphaned-read").unwrap();
        assert_eq!(
            recovered.platform_runs[0].stop_code.as_deref(),
            Some("interrupted_read")
        );
        assert_eq!(recovered.used_calls, 1);
        owner
            .claim_discovery_search("orphaned-read", 1, "youtube", "dedicated")
            .unwrap();
        // A graph recreated in this process must preserve that new in-flight read.
        let rebuilt = open_application_graph(
            directory.path().to_path_buf(),
            LocalPlatformHostPaths::without_system_desktop(),
            sync,
            Arc::new(PresentationPreferences::default()),
            false,
        )
        .unwrap();
        let current = rebuilt
            .platform
            .creators()
            .unwrap()
            .get_discovery_campaign("orphaned-read")
            .unwrap();
        assert_eq!(current.platform_runs[0].status, "searching");
        assert_eq!(current.used_calls, 2);
    }

    #[test]
    fn application_open_builds_one_catalog_shared_by_mcp_and_http_navigation() {
        let directory = tempfile::tempdir().unwrap();
        let sync = Arc::new(CreatorSyncRuntime::open(directory.path()).unwrap());
        let graph = open_application_graph(
            directory.path().to_path_buf(),
            LocalPlatformHostPaths::without_system_desktop(),
            sync,
            Arc::new(PresentationPreferences::default()),
            true,
        )
        .unwrap();

        assert!(Arc::ptr_eq(graph.mcp.registry().catalog(), &graph.catalog));
        let other_peer = onecrew_mcp_adapter::LocalMcpService::new(
            graph.platform.clone(),
            &graph.mcp.registry(),
        );
        assert!(
            other_peer
                .registry()
                .shares_build_with(&graph.mcp.registry())
        );
        let mut web_directory =
            serde_json::to_value(crate::toolkit_directory::project_directory(&graph.catalog))
                .unwrap();
        // The page's native model bridge is intentionally absent from host-model MCP.
        let operations = web_directory
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|toolkit| toolkit["id"] == "toolkit.onecrew")
            .unwrap()["operations"]
            .as_array_mut()
            .unwrap();
        assert_eq!(
            operations
                .iter()
                .filter(|operation| operation["id"] == "onecrew.discovery.draft.prepare")
                .count(),
            1
        );
        operations.retain(|operation| operation["id"] != "onecrew.discovery.draft.prepare");
        assert_eq!(
            web_directory,
            serde_json::to_value(graph.mcp.registry().toolkit_directory()).unwrap(),
        );
        drop(graph.platform);
    }

    #[test]
    fn missing_catalog_bindings_cannot_construct_a_ready_application_graph() {
        let directory = tempfile::tempdir().unwrap();
        let platform =
            Arc::new(onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap());
        let catalog =
            Arc::new(onecrew_local_runtime::CapabilityCatalog::from_static(&[], &[]).unwrap());
        assert!(super::assemble_application_graph(platform, catalog).is_err());
    }

    fn status(state: S, action: Option<A>, issue: Option<I>) -> CreatorSyncStatus {
        CreatorSyncStatus {
            state,
            action,
            issue,
            installation_id: None,
            credential_id: None,
        }
    }

    #[test]
    fn business_admission_reserves_login_required_for_login_states() {
        let retryable = [
            // Saved-credential validation could not reach Access.
            status(
                S::TemporarilyUnavailable,
                Some(A::ValidateAdmission),
                Some(I::RemoteUnavailable),
            ),
            status(S::Validating, None, None),
            status(S::PairingCompleting, None, None),
        ];
        let login = [
            // A failed login callback: no saved credential, only a new login helps.
            status(
                S::TemporarilyUnavailable,
                Some(A::Retry),
                Some(I::RemoteUnavailable),
            ),
            status(
                S::TemporarilyUnavailable,
                Some(A::Retry),
                Some(I::RemoteRateLimited),
            ),
            status(S::Unconfigured, Some(A::Retry), None),
            status(S::PairingRequired, Some(A::BeginPairing), None),
            status(S::PairingPending, Some(A::AwaitPairingCallback), None),
            status(S::ValidationRequired, Some(A::ValidateAdmission), None),
            status(S::Revoked, Some(A::BeginPairing), None),
            status(S::Unauthorized, Some(A::BeginPairing), None),
            status(S::RecoveryRequired, Some(A::BeginPairing), None),
        ];
        assert_eq!(business_admission(&status(S::Active, None, None)), Ok(()));
        for case in &retryable {
            assert_eq!(
                business_admission(case),
                Err(AdmissionBlock::Retryable),
                "{case:?}"
            );
        }
        for case in &login {
            assert_eq!(
                business_admission(case),
                Err(AdmissionBlock::LoginRequired),
                "{case:?}"
            );
        }
        assert!(
            AdmissionBlock::Retryable
                .message()
                .starts_with("login_temporarily_unavailable: ")
        );
        assert!(
            AdmissionBlock::LoginRequired
                .message()
                .starts_with("login_required: ")
        );
    }
}
