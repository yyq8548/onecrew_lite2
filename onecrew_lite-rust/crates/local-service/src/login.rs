//! One pairing owner shared by CLI, web and MCP. Only authorization URLs leave
//! this module; callback secrets pass directly to Creator Sync's typed API.
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicU64, Ordering},
};

use onecrew_creator_sync::{
    CreatorSyncAdmissionState, CreatorSyncClientPlatform, CreatorSyncPairingBeginInput,
    CreatorSyncPairingCancelInput, CreatorSyncPairingCompleteInput, CreatorSyncRuntime,
    CreatorSyncStatus, PairingAuthorizeUrl,
};
use serde::Serialize;
use tokio::{sync::Mutex as AsyncMutex, task::JoinHandle};

use crate::pairing_loopback::{PairingLoopbackDisposition, PairingLoopbackSupervisor};

#[derive(Clone, Copy, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum LoginIssue {
    DeviceNameUnavailable,
    DeviceNameInvalid,
    CallbackExpired,
    Canceled,
    Replay,
    OwnerChanged,
    PairingRejected,
    RecoveryRequired,
}

#[derive(Clone, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct LoginProjection {
    pub(crate) status: CreatorSyncStatus,
    pub(crate) flow_id: Option<String>,
    pub(crate) browser_opened: bool,
    pub(crate) callback_received: bool,
    pub(crate) issue: Option<LoginIssue>,
}

struct Pending {
    flow_id: String,
    generation: u64,
    url: PairingAuthorizeUrl,
    listener: Arc<PairingLoopbackSupervisor>,
}

#[derive(Default)]
struct State {
    pending: Option<Arc<Pending>>,
    task: Option<JoinHandle<()>>,
    callback_received: bool,
    browser_opened: bool,
    issue: Option<LoginIssue>,
}

#[derive(Default)]
pub(crate) struct LoginOwner {
    // Serializes complete begin/cancel owner transitions, including joins.
    operation: AsyncMutex<()>,
    generation: AtomicU64,
    state: Mutex<State>,
}

impl LoginOwner {
    pub(crate) fn projection(&self, status: CreatorSyncStatus) -> LoginProjection {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        LoginProjection {
            status,
            flow_id: state.pending.as_ref().map(|p| p.flow_id.clone()),
            browser_opened: state.browser_opened,
            callback_received: state.callback_received,
            issue: state.issue,
        }
    }

    pub(crate) fn browser_opened(&self) {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .browser_opened = true;
    }

    pub(crate) fn reopen(&self) -> Result<String, String> {
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pending
            .as_ref()
            .map(|pending| pending.url.as_str().to_owned())
            .ok_or_else(|| "no_pending_login".into())
    }

    pub(crate) async fn begin(
        self: &Arc<Self>,
        sync: Arc<CreatorSyncRuntime>,
    ) -> Result<String, String> {
        let _operation = self.operation.lock().await;
        let previous = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.pending.is_some() {
                return Err("login_already_pending".into());
            }
            state.task.take()
        };
        if let Some(previous) = previous {
            previous.await.map_err(|_| "login_owner_failed")?;
        }
        let status = sync.ensure_admission_validated().await;
        if status.state == CreatorSyncAdmissionState::Active {
            return Err("already_logged_in".into());
        }
        let client_platform = client_platform()?;
        let label = match crate::blocking::run_blocking(
            "login_device_name",
            crate::device_name::current_device_label,
        )
        .await?
        {
            Ok(label) => label,
            Err(error) => {
                let issue = match error {
                    crate::device_name::DeviceNameError::Unavailable => {
                        LoginIssue::DeviceNameUnavailable
                    }
                    _ => LoginIssue::DeviceNameInvalid,
                };
                self.state.lock().unwrap_or_else(|p| p.into_inner()).issue = Some(issue);
                return Err("device_name_unavailable_or_invalid".into());
            }
        };
        let listener = Arc::new(
            PairingLoopbackSupervisor::begin()
                .await
                .map_err(|_| "callback_listener_unavailable")?,
        );
        let generation = self
            .generation
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| "login_owner_exhausted")?
            + 1;
        let input = CreatorSyncPairingBeginInput {
            client_platform,
            app_version: env!("CARGO_PKG_VERSION").into(),
            callback_port: listener.bound_port(),
            owner_generation: generation,
        };
        let runtime = sync.clone();
        let begun = crate::blocking::run_blocking("login_begin", move || {
            runtime.begin_pairing(input, &label)
        })
        .await?;
        let begun = match begun {
            Ok(begun) => begun,
            Err(error) => {
                let _ = listener.cancel_and_join().await;
                return Err(format!("login_begin_{error:?}"));
            }
        };
        let url = begun.authorize_url.as_str().to_owned();
        let pending = Arc::new(Pending {
            flow_id: begun.flow_id,
            generation,
            url: begun.authorize_url,
            listener,
        });
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.pending = Some(pending.clone());
            state.callback_received = false;
            state.browser_opened = false;
            state.issue = None;
        }
        let owner = Arc::downgrade(self);
        let task = tokio::spawn(async move {
            finish_pairing(owner, pending, sync).await;
        });
        self.state.lock().unwrap_or_else(|p| p.into_inner()).task = Some(task);
        Ok(url)
    }

    /// Never abort an enrollment after its request may have reached Access.
    /// Join it before allowing another identity or deleting its credentials.
    pub(crate) async fn cancel(&self, sync: Arc<CreatorSyncRuntime>) -> Result<(), String> {
        let _operation = self.operation.lock().await;
        let (pending, task) = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            (state.pending.clone(), state.task.take())
        };
        if let Some(pending) = &pending {
            let _ = pending.listener.cancel_and_join().await;
        }
        if let Some(task) = task {
            task.await.map_err(|_| "login_owner_failed")?;
        }
        if let Some(pending) = pending {
            let input = CreatorSyncPairingCancelInput {
                flow_id: pending.flow_id.clone(),
                owner_generation: pending.generation,
            };
            // A completed or rejected one-use callback is already terminal.
            crate::blocking::run_blocking("login_cancel", move || {
                if let Err(error) = sync.cancel_pairing(input) {
                    log::debug!("login cancel found the pairing already terminal: {error:?}");
                }
            })
            .await?;
        }
        Ok(())
    }
}

fn client_platform() -> Result<CreatorSyncClientPlatform, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(CreatorSyncClientPlatform::Macos)
    }
    #[cfg(target_os = "windows")]
    {
        Ok(CreatorSyncClientPlatform::Windows)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("unsupported_login_platform".into())
    }
}

async fn finish_pairing(
    owner: Weak<LoginOwner>,
    pending: Arc<Pending>,
    sync: Arc<CreatorSyncRuntime>,
) {
    let result = match pending.listener.receive_callback().await {
        Ok(accepted) => {
            if let Some(owner) = owner.upgrade() {
                let mut state = owner.state.lock().unwrap_or_else(|p| p.into_inner());
                if state
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.generation == pending.generation)
                {
                    state.callback_received = true;
                }
            }
            let (code, callback_state, response) = accepted.into_parts();
            let input = CreatorSyncPairingCompleteInput::new(
                pending.flow_id.clone(),
                pending.generation,
                code,
                callback_state,
            );
            let result = match input {
                Ok(input) => sync.complete_pairing(input).await,
                Err(error) => Err(error),
            };
            let disposition = match &result {
                Ok(status) if status.state == CreatorSyncAdmissionState::Active => {
                    PairingLoopbackDisposition::Succeeded
                }
                Err(onecrew_creator_sync::CreatorSyncPairingError::RateLimited) => {
                    PairingLoopbackDisposition::RateLimited
                }
                Err(onecrew_creator_sync::CreatorSyncPairingError::RemoteUnavailable) => {
                    PairingLoopbackDisposition::RemoteUnavailable
                }
                _ => PairingLoopbackDisposition::Rejected,
            };
            response.respond(disposition);
            let _ = pending.listener.join_after_completion().await;
            result.map(|_| ()).map_err(|error| match error {
                onecrew_creator_sync::CreatorSyncPairingError::Expired => {
                    LoginIssue::CallbackExpired
                }
                onecrew_creator_sync::CreatorSyncPairingError::Cancelled => LoginIssue::Canceled,
                onecrew_creator_sync::CreatorSyncPairingError::Replayed => LoginIssue::Replay,
                onecrew_creator_sync::CreatorSyncPairingError::OwnerChanged => {
                    LoginIssue::OwnerChanged
                }
                onecrew_creator_sync::CreatorSyncPairingError::RecoveryRequired => {
                    LoginIssue::RecoveryRequired
                }
                _ => LoginIssue::PairingRejected,
            })
        }
        Err(error) => {
            let runtime = sync.clone();
            let cancel = CreatorSyncPairingCancelInput {
                flow_id: pending.flow_id.clone(),
                owner_generation: pending.generation,
            };
            match crate::blocking::run_blocking("login_callback_cancel", move || {
                runtime.cancel_pairing(cancel)
            })
            .await
            {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => {
                    log::debug!(
                        "login callback cancel found the pairing already terminal: {error:?}"
                    );
                }
                Err(error) => log::warn!("login callback cancel could not run: {error}"),
            }
            let _ = pending.listener.cancel_and_join().await;
            Err(match error {
                crate::pairing_loopback::PairingLoopbackReceiveError::TimedOut => {
                    LoginIssue::CallbackExpired
                }
                crate::pairing_loopback::PairingLoopbackReceiveError::Canceled => {
                    LoginIssue::Canceled
                }
                _ => LoginIssue::PairingRejected,
            })
        }
    };
    if let Some(owner) = owner.upgrade() {
        let mut state = owner.state.lock().unwrap_or_else(|p| p.into_inner());
        if state
            .pending
            .as_ref()
            .is_some_and(|p| p.generation == pending.generation)
        {
            state.pending = None;
            state.issue = result.err();
        }
    }
}

#[cfg(test)]
mod contract_manifest_tests {
    use super::*;
    use onecrew_schema_manifest::{SchemaDirection, SchemaManifest, contract_schema_root};

    #[test]
    fn web_creator_sync_contract_manifest_is_exact_and_exportable() {
        let manifest = SchemaManifest::new(
            "web-creator-sync",
            vec![
                contract_schema_root::<CreatorSyncStatus>(
                    "CreatorSyncStatus",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<LoginProjection>(
                    "LoginProjection",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<crate::status::LocalServiceStatus>(
                    "LocalServiceStatus",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<crate::startup::DataStartupStatus>(
                    "DataStartupStatus",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(manifest.roots.len(), 4);
        let encoded = serde_json::to_vec_pretty(&manifest).unwrap();
        let text = std::str::from_utf8(&encoded).unwrap();
        for forbidden in [
            "authorize_url",
            "pairing_code",
            "pairing_state",
            "code_verifier",
            "bearer",
            "UpdateStatus",
        ] {
            assert!(
                !text.contains(forbidden),
                "public status schema contains {forbidden}"
            );
        }
        if let Some(path) = std::env::var_os("ONECREW_WEB_CONTRACT_MANIFEST_OUT") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, encoded).unwrap();
        }
    }
}
