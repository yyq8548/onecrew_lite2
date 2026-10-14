//! Shared Creator Sync coordinator and its private lifecycle branches.

mod credential_store;
mod enrollment;
mod snapshot;
mod table_sync;
pub use table_sync::{TableSyncRequest, TableSyncResponse};

#[cfg(feature = "test-support")]
pub mod test_support;

use credential_store::{
    AdmissionValidation, CreatorSyncCredentialStore, LocalVaultCreatorSyncStore,
    RemoteAdmissionOverride,
};
#[cfg(test)]
use credential_store::{StoredCredential, StoredCredentialState};
#[cfg(test)]
use enrollment::{BearerSecret, client_platform_str};
pub use enrollment::{
    CodeVerifier, CreatorSyncPairingBeginInput, CreatorSyncPairingBeginResult,
    CreatorSyncPairingCancelInput, CreatorSyncPairingCompleteInput, PairingAuthorizeUrl,
    PairingCode, PairingState,
};
use enrollment::{
    PairingClock, PairingLifecycle, PairingStatus, SystemPairingClock, pairing_status,
};

use crate::{
    config::CreatorSyncEndpoints,
    http,
    model::{
        CreatorSyncAdmissionState, CreatorSyncSetupError, CreatorSyncStatus,
        status_for_remote_override, unconfigured_status,
    },
};

use std::{
    path::Path,
    sync::{Arc, Mutex, PoisonError, atomic::AtomicBool},
};

use http::{HttpFailure, HttpRequest, HttpResponse, HttpTransport, ReqwestTransport};
use onecrew_local_vault::LocalVault;
use serde::de::DeserializeOwned;

#[derive(Clone)]
pub struct CreatorSyncRuntime {
    inner: Arc<CreatorSyncInner>,
}

struct CreatorSyncInner {
    configured: bool,
    transport: Option<Arc<dyn HttpTransport>>,
    endpoints: CreatorSyncEndpoints,
    credentials: Arc<dyn CreatorSyncCredentialStore>,
    pairing: tokio::sync::Mutex<PairingLifecycle>,
    /// Serializes `status()` readers among themselves so their `try_lock`
    /// probes observe only real pairing or validation owners.
    status_reads: Mutex<()>,
    clock: Arc<dyn PairingClock>,
    validation: tokio::sync::Mutex<AdmissionValidation>,
    sync_running: AtomicBool,
    remote_override: Mutex<Option<RemoteAdmissionOverride>>,
}

impl CreatorSyncRuntime {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, CreatorSyncSetupError> {
        let vault = LocalVault::open(data_dir.as_ref().join("agent-platform.db"))
            .map_err(|_| CreatorSyncSetupError::LocalVaultUnavailable)?;
        Self::with_vault(vault)
    }

    pub fn with_vault(vault: LocalVault) -> Result<Self, CreatorSyncSetupError> {
        let transport = ReqwestTransport::new()
            .map(|transport| Arc::new(transport) as Arc<dyn HttpTransport>)
            .map_err(|_| CreatorSyncSetupError::HttpClientUnavailable)?;
        Ok(Self::from_parts(
            true,
            Some(transport),
            Arc::new(LocalVaultCreatorSyncStore::new(vault)),
        ))
    }

    fn from_parts(
        configured: bool,
        transport: Option<Arc<dyn HttpTransport>>,
        credentials: Arc<dyn CreatorSyncCredentialStore>,
    ) -> Self {
        Self::from_parts_with_clock(
            configured,
            transport,
            credentials,
            Arc::new(SystemPairingClock),
        )
    }

    fn from_parts_with_clock(
        configured: bool,
        transport: Option<Arc<dyn HttpTransport>>,
        credentials: Arc<dyn CreatorSyncCredentialStore>,
        clock: Arc<dyn PairingClock>,
    ) -> Self {
        Self {
            inner: Arc::new(CreatorSyncInner {
                configured,
                transport,
                endpoints: CreatorSyncEndpoints::production(),
                credentials,
                pairing: tokio::sync::Mutex::new(PairingLifecycle::new()),
                status_reads: Mutex::new(()),
                clock,
                validation: tokio::sync::Mutex::new(AdmissionValidation::Required),
                sync_running: AtomicBool::new(false),
                remote_override: Mutex::new(None),
            }),
        }
    }

    /// Blocking admission projection; call it off async workers. Concurrent
    /// readers wait for each other (at most one vault read), so
    /// `PairingCompleting` means a pairing owner holds the lifecycle, never
    /// another status reader.
    pub fn status(&self) -> CreatorSyncStatus {
        if !self.inner.configured {
            return unconfigured_status();
        }
        let _status_read = self
            .inner
            .status_reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Ok(mut lifecycle) = self.inner.pairing.try_lock() else {
            return crate::model::status(
                CreatorSyncAdmissionState::PairingCompleting,
                None,
                None,
                None,
            );
        };
        match pairing_status(&mut lifecycle, self.inner.clock.now()) {
            PairingStatus::DeferToVault(issue) => {
                if let Some(override_state) = *self
                    .inner
                    .remote_override
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                {
                    status_for_remote_override(
                        override_state.state,
                        override_state.cleanup_incomplete,
                    )
                } else {
                    self.status_from_vault(issue)
                }
            }
            PairingStatus::Status(status) => {
                if status.state == CreatorSyncAdmissionState::PairingPending {
                    status
                } else if let Some(override_state) = *self
                    .inner
                    .remote_override
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                {
                    status_for_remote_override(
                        override_state.state,
                        override_state.cleanup_incomplete,
                    )
                } else {
                    status
                }
            }
        }
    }

    pub(super) async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        self.inner
            .transport
            .as_ref()
            .ok_or(HttpFailure::Transport)?
            .send(request)
            .await
    }
}

pub(super) fn parse_json<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ()> {
    serde_json::from_slice(bytes).map_err(|_| ())
}

#[cfg(test)]
mod tests;
