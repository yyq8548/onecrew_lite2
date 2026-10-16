use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use onecrew_local_domain::{ServiceCredential, ServiceCredentialSecret, ServiceProvider};
use onecrew_local_vault::ServiceCredentialHealthCommit;
use onecrew_youtube::{
    MAX_IN_FLIGHT_REQUESTS, YoutubeApiFailureKind, YoutubeClientError, YoutubePlatform,
    YoutubePlatformError, YoutubeRequestFailureKind, YoutubeRequestLimiter, YoutubeTransportError,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::sync::Semaphore;
use uuid::Uuid;

use super::{CredentialServices, blocking_segment};

pub(super) struct CachedYoutubeClient {
    pub(super) revision: String,
    pub(super) platform: YoutubePlatform,
    pub(super) limiter: YoutubeRequestLimiter,
}

pub(crate) struct YoutubeCredentialRead {
    pub(crate) id: Uuid,
    pub(crate) revision: String,
    pub(crate) observation: u64,
    pub(crate) platform: YoutubePlatform,
    #[cfg(test)]
    pub(crate) limiter: YoutubeRequestLimiter,
}

pub(super) struct YoutubeHealthState {
    material_revision: String,
    latest_revision: Option<String>,
    latest_observation: u64,
}

#[cfg(test)]
pub(super) struct YoutubeReadSnapshotHook {
    pub(super) snapshot_taken: std::sync::Barrier,
    pub(super) release: std::sync::Barrier,
}

#[derive(Default)]
pub(super) struct YoutubeReadFlights {
    active: AtomicUsize,
    changed: Notify,
}

pub(crate) struct YoutubeReadGuard {
    flights: Arc<YoutubeReadFlights>,
}

impl Drop for YoutubeReadGuard {
    fn drop(&mut self) {
        self.flights.active.fetch_sub(1, Ordering::SeqCst);
        self.flights.changed.notify_waiters();
    }
}

/// Local credential admission failures, independent of Creator read policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum YoutubeCredentialReadError {
    InvalidCredentialId,
    CredentialUnavailable,
    Infrastructure,
}

impl CredentialServices {
    pub(crate) fn begin_youtube_read(&self) -> YoutubeReadGuard {
        self.youtube_flights.active.fetch_add(1, Ordering::SeqCst);
        YoutubeReadGuard {
            flights: Arc::clone(&self.youtube_flights),
        }
    }

    #[cfg(test)]
    pub(super) fn active_youtube_reads(&self) -> usize {
        self.youtube_flights.active.load(Ordering::SeqCst)
    }

    /// Validate a candidate key with a throwaway client. This method never
    /// reads or writes the local vault, so callers can safely validate before
    /// replacing an existing credential with the same label.
    pub(crate) async fn validate_youtube_api_key(
        &self,
        api_key: &str,
    ) -> Result<YoutubeCredentialValidation, String> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return Err("请输入 YouTube API Key".into());
        }
        let platform = YoutubePlatform::from_api_key(api_key.to_owned())
            .map_err(|_| "YouTube validation infrastructure is unavailable".to_string())?;
        match platform.resolve_handle("YouTube").await {
            Ok(channel) => Ok(YoutubeCredentialValidation {
                checked_at: now(),
                channel_id: Some(channel.channel_id),
                message: "YouTube Data API Key 可用".into(),
            }),
            Err(error) => Err(classify_youtube_health(&error).message().into()),
        }
    }

    pub(crate) fn list_youtube_credentials(&self) -> Result<Vec<ServiceCredential>, String> {
        self.vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn put_youtube_credential(
        &self,
        label: impl Into<String>,
        project: Option<String>,
        api_key: impl Into<String>,
    ) -> Result<ServiceCredential, String> {
        let label = label.into().trim().to_owned();
        let api_key = api_key.into().trim().to_owned();
        if label.is_empty() {
            return Err("请输入 Key 名称".into());
        }
        if api_key.is_empty() {
            return Err("请输入 YouTube API Key".into());
        }
        let project = project
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let credential = self
            .vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                label,
                serde_json::json!({ "api": "youtube_data_api_v3", "project": project }),
                &ServiceCredentialSecret::new(api_key),
            )
            .map_err(|error| error.to_string())?;
        self.invalidate_youtube_client(credential.id)?;
        Ok(credential)
    }

    pub(crate) fn delete_youtube_credential(&self, credential_id: &str) -> Result<bool, String> {
        let id = parse_credential_id(credential_id)?;
        let deleted = self
            .vault
            .delete_service_credential(id)
            .map_err(|error| error.to_string())?;
        if deleted {
            self.discard_youtube_client(id)?;
        }
        Ok(deleted)
    }

    /// Probes a stored key and records its health. The vault read and the
    /// health write run on the blocking pool around the provider round trip.
    pub(crate) async fn test_youtube_credential(
        self: &Arc<Self>,
        credential_id: &str,
    ) -> Result<YoutubeCredentialTest, YoutubeCredentialTestError> {
        let id = Uuid::parse_str(credential_id)
            .map_err(|_| YoutubeCredentialTestError::InvalidCredentialId)?;
        let services = Arc::clone(self);
        let opened = blocking_segment(YoutubeCredentialTestError::Infrastructure, move || {
            services.youtube_platform_for_test(id)
        })
        .await?;
        let flight = self.begin_youtube_read();
        let provider_result = opened.platform.resolve_handle("YouTube").await;
        let services = Arc::clone(self);
        blocking_segment(YoutubeCredentialTestError::Infrastructure, move || {
            let _flight = flight;
            services.finish_youtube_credential_test(
                id,
                &opened.revision,
                opened.observation,
                provider_result,
            )
        })
        .await
    }

    fn finish_youtube_credential_test(
        &self,
        id: Uuid,
        expected_revision: &str,
        observation: u64,
        provider_result: Result<onecrew_youtube::YoutubeChannelSummary, YoutubePlatformError>,
    ) -> Result<YoutubeCredentialTest, YoutubeCredentialTestError> {
        match provider_result {
            Ok(channel) => {
                let commit = self
                    .mark_youtube_read_success_if_revision(id, expected_revision, observation)
                    .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
                if matches!(commit, ServiceCredentialHealthCommit::Superseded) {
                    return Err(YoutubeCredentialTestError::Infrastructure);
                }
                Ok(YoutubeCredentialTest {
                    credential_id: id.to_string(),
                    ok: true,
                    checked_at: now(),
                    channel_id: Some(channel.channel_id),
                    issue: None,
                    message: "YouTube Data API Key 可用".into(),
                })
            }
            Err(error) => match classify_provider_test_error(&error) {
                YoutubeTestFailure::Rejected { issue, health } => {
                    let commit = self
                        .mark_youtube_read_failure_if_revision(
                            id,
                            expected_revision,
                            observation,
                            &error,
                        )
                        .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
                    if matches!(commit, ServiceCredentialHealthCommit::Superseded) {
                        return Err(YoutubeCredentialTestError::Infrastructure);
                    }
                    Ok(YoutubeCredentialTest {
                        credential_id: id.to_string(),
                        ok: false,
                        checked_at: now(),
                        channel_id: None,
                        issue: Some(issue),
                        message: health.message().into(),
                    })
                }
                YoutubeTestFailure::Failed {
                    error: mapped_error,
                    health: _,
                } => {
                    let commit = self
                        .mark_youtube_read_failure_if_revision(
                            id,
                            expected_revision,
                            observation,
                            &error,
                        )
                        .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
                    if matches!(commit, ServiceCredentialHealthCommit::Superseded) {
                        return Err(YoutubeCredentialTestError::Infrastructure);
                    }
                    Err(mapped_error)
                }
            },
        }
    }

    pub(crate) fn youtube_platform_for_read(
        &self,
        credential_id: &str,
    ) -> Result<YoutubeCredentialRead, YoutubeCredentialReadError> {
        let id = Uuid::parse_str(credential_id)
            .map_err(|_| YoutubeCredentialReadError::InvalidCredentialId)?;
        // Credential mutation writes the vault before taking this same health
        // lock to invalidate the cache. Taking the lock first makes the vault
        // snapshot and cache admission one ordered boundary: a reader cannot
        // reopen an old secret after invalidation has completed.
        let mut health = self
            .youtube_health
            .lock()
            .map_err(|_| YoutubeCredentialReadError::Infrastructure)?;
        let (credential, secret) = self
            .vault
            .get_service_credential(id)
            .map_err(|_| YoutubeCredentialReadError::CredentialUnavailable)?;
        #[cfg(test)]
        self.wait_after_youtube_snapshot();
        if credential.provider != ServiceProvider::Youtube
            || credential
                .metadata
                .get("api")
                .and_then(serde_json::Value::as_str)
                != Some("youtube_data_api_v3")
        {
            return Err(YoutubeCredentialReadError::CredentialUnavailable);
        }
        let unavailable = credential.disabled
            || credential.last_error.is_some()
            || credential.cooldown_until.is_some_and(|until| until > now());
        if unavailable {
            return Err(YoutubeCredentialReadError::CredentialUnavailable);
        }
        let credential_revision = credential.updated_at.clone();
        let state = health.entry(id).or_insert_with(|| YoutubeHealthState {
            material_revision: credential_revision.clone(),
            latest_revision: None,
            latest_observation: 0,
        });
        let mut clients = self
            .youtube_clients
            .lock()
            .map_err(|_| YoutubeCredentialReadError::Infrastructure)?;
        if clients
            .get(&id)
            .is_none_or(|cached| cached.revision != credential_revision)
            && state.latest_revision.as_deref() != Some(credential_revision.as_str())
        {
            state.material_revision = credential_revision.clone();
            state.latest_revision = None;
            state.latest_observation = 0;
        }
        let previous_observation = self
            .youtube_observations
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |current| current.checked_add(1),
            )
            .map_err(|_| YoutubeCredentialReadError::Infrastructure)?;
        let observation = previous_observation + 1;
        if let Some(cached) = clients.get(&id)
            && cached.revision == credential.updated_at
        {
            return Ok(YoutubeCredentialRead {
                id,
                revision: cached.revision.clone(),
                observation,
                platform: cached.platform.clone(),
                #[cfg(test)]
                limiter: Arc::clone(&cached.limiter),
            });
        }
        let limiter = if let Some(cached) = clients.get(&id) {
            Arc::clone(&cached.limiter)
        } else {
            let mut limiters = self
                .youtube_limiters
                .lock()
                .map_err(|_| YoutubeCredentialReadError::Infrastructure)?;
            Arc::clone(
                limiters
                    .entry(id)
                    .or_insert_with(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS))),
            )
        };
        let platform = YoutubePlatform::from_api_key_with_request_limiter(
            secret.expose_secret(),
            Arc::clone(&limiter),
        )
        .map_err(|_| YoutubeCredentialReadError::Infrastructure)?;
        let revision = credential.updated_at;
        clients.insert(
            id,
            CachedYoutubeClient {
                revision: revision.clone(),
                platform: platform.clone(),
                limiter: Arc::clone(&limiter),
            },
        );
        Ok(YoutubeCredentialRead {
            id,
            revision,
            observation,
            platform,
            #[cfg(test)]
            limiter,
        })
    }

    fn invalidate_youtube_client(&self, id: Uuid) -> Result<(), String> {
        self.youtube_health
            .lock()
            .map_err(|_| "YouTube provider registry lock poisoned".to_owned())?
            .remove(&id);
        self.youtube_clients
            .lock()
            .map_err(|_| "YouTube provider registry lock poisoned".to_owned())?
            .remove(&id);
        Ok(())
    }

    fn discard_youtube_client(&self, id: Uuid) -> Result<(), String> {
        self.invalidate_youtube_client(id)?;
        self.youtube_limiters
            .lock()
            .map_err(|_| "YouTube provider registry lock poisoned".to_owned())?
            .remove(&id);
        Ok(())
    }

    #[cfg(test)]
    fn wait_after_youtube_snapshot(&self) {
        let hook = self.youtube_read_snapshot_hook.lock().unwrap().take();
        if let Some(hook) = hook {
            hook.snapshot_taken.wait();
            hook.release.wait();
        }
    }

    fn youtube_platform_for_test(
        &self,
        credential_id: Uuid,
    ) -> Result<YoutubeCredentialRead, YoutubeCredentialTestError> {
        // Keep the probe's vault snapshot and health state under the same
        // admission lock used by ordinary reads. A rotation can therefore
        // invalidate this state before a later probe is allowed to open it.
        let mut health = self
            .youtube_health
            .lock()
            .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
        let (credential, secret) = self
            .vault
            .get_service_credential(credential_id)
            .map_err(|_| YoutubeCredentialTestError::CredentialUnavailable)?;
        if credential.provider != ServiceProvider::Youtube {
            return Err(YoutubeCredentialTestError::CredentialUnavailable);
        }
        let revision = credential.updated_at.clone();
        let state = health
            .entry(credential_id)
            .or_insert_with(|| YoutubeHealthState {
                material_revision: revision.clone(),
                latest_revision: None,
                latest_observation: 0,
            });
        if state.material_revision != revision
            && state.latest_revision.as_deref() != Some(revision.as_str())
        {
            state.material_revision = revision.clone();
            state.latest_revision = None;
            state.latest_observation = 0;
        }
        let previous_observation = self
            .youtube_observations
            .fetch_update(
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
                |current| current.checked_add(1),
            )
            .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
        let observation = previous_observation + 1;
        let limiter = {
            let mut limiters = self
                .youtube_limiters
                .lock()
                .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
            Arc::clone(
                limiters
                    .entry(credential_id)
                    .or_insert_with(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS))),
            )
        };
        let platform = YoutubePlatform::from_api_key_with_request_limiter(
            secret.expose_secret(),
            Arc::clone(&limiter),
        )
        .map_err(|_| YoutubeCredentialTestError::Infrastructure)?;
        Ok(YoutubeCredentialRead {
            id: credential_id,
            revision,
            observation,
            platform,
            #[cfg(test)]
            limiter,
        })
    }

    pub(crate) fn mark_youtube_read_success_if_revision(
        &self,
        id: Uuid,
        revision: &str,
        observation: u64,
    ) -> Result<ServiceCredentialHealthCommit, String> {
        let mut health = self
            .youtube_health
            .lock()
            .map_err(|_| "YouTube health registry lock poisoned".to_owned())?;
        let Some(current) = self.current_youtube_revision(id)? else {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        };
        let state = health.entry(id).or_insert_with(|| YoutubeHealthState {
            material_revision: revision.to_owned(),
            latest_revision: None,
            latest_observation: 0,
        });
        if observation <= state.latest_observation {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        }
        let expected_revision = if current == revision
            || (state.material_revision == revision
                && state.latest_revision.as_deref() == Some(current.as_str()))
        {
            current
        } else {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        };
        let commit = self
            .vault
            .mark_service_credential_success_if_revision(id, &expected_revision)
            .map_err(|error| error.to_string())?;
        self.refresh_cached_youtube_revision(id, &expected_revision, &commit)?;
        if let ServiceCredentialHealthCommit::Applied { revision } = &commit {
            state.latest_revision = Some(revision.clone());
            state.latest_observation = observation;
        }
        Ok(commit)
    }

    pub(crate) fn mark_youtube_read_failure_if_revision(
        &self,
        id: Uuid,
        revision: &str,
        observation: u64,
        error: &YoutubePlatformError,
    ) -> Result<ServiceCredentialHealthCommit, String> {
        let failure = classify_youtube_health(error);
        let mut health = self
            .youtube_health
            .lock()
            .map_err(|_| "YouTube health registry lock poisoned".to_owned())?;
        let Some(current) = self.current_youtube_revision(id)? else {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        };
        let state = health.entry(id).or_insert_with(|| YoutubeHealthState {
            material_revision: revision.to_owned(),
            latest_revision: None,
            latest_observation: 0,
        });
        if observation <= state.latest_observation {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        }
        let expected_revision = if current == revision
            || (state.material_revision == revision
                && state.latest_revision.as_deref() == Some(current.as_str()))
        {
            current
        } else {
            return Ok(ServiceCredentialHealthCommit::Superseded);
        };
        let commit = self
            .vault
            .mark_service_credential_failure_if_revision(
                id,
                &expected_revision,
                failure.message(),
                failure.disables_credential(),
                failure.cooldown_until(),
            )
            .map_err(|error| error.to_string())?;
        self.refresh_cached_youtube_revision(id, &expected_revision, &commit)?;
        if let ServiceCredentialHealthCommit::Applied { revision } = &commit {
            state.latest_revision = Some(revision.clone());
            state.latest_observation = observation;
        }
        Ok(commit)
    }

    fn current_youtube_revision(&self, id: Uuid) -> Result<Option<String>, String> {
        self.vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|credential| credential.id == id)
            .map(|credential| credential.updated_at)
            .map_or(Ok(None), |revision| Ok(Some(revision)))
    }

    fn refresh_cached_youtube_revision(
        &self,
        id: Uuid,
        expected_revision: &str,
        commit: &ServiceCredentialHealthCommit,
    ) -> Result<(), String> {
        let ServiceCredentialHealthCommit::Applied { revision } = commit else {
            return Ok(());
        };
        let mut clients = self
            .youtube_clients
            .lock()
            .map_err(|_| "YouTube provider registry lock poisoned".to_owned())?;
        if let Some(cached) = clients.get_mut(&id)
            && cached.revision == expected_revision
        {
            cached.revision = revision.clone();
        }
        Ok(())
    }
}

pub(crate) fn parse_credential_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| "invalid YouTube credential id".to_string())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct YoutubeCredentialTest {
    pub credential_id: String,
    pub ok: bool,
    pub checked_at: u64,
    pub channel_id: Option<String>,
    pub issue: Option<YoutubeCredentialRejectionIssue>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeCredentialRejectionIssue {
    ApiKeyInvalid,
    AccessNotConfigured,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum YoutubeCredentialTestError {
    InvalidCredentialId,
    CredentialUnavailable,
    RateLimited,
    /// YouTube answered with a server error or an unexpected missing channel.
    Transient,
    /// No HTTP response: connect, timeout, body, or network failure.
    NetworkUnreachable,
    AmbiguousForbidden,
    Infrastructure,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum YoutubeHealthFailure {
    ApiKeyInvalid,
    AccessNotConfigured,
    QuotaExceeded,
    RateLimited,
    Transient,
    AmbiguousForbidden,
    Infrastructure,
}

impl YoutubeHealthFailure {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::ApiKeyInvalid => "YouTube API key was rejected",
            Self::AccessNotConfigured => "YouTube Data API access is not configured",
            Self::QuotaExceeded => "YouTube API quota is exhausted",
            Self::RateLimited => "YouTube API rate limit is active",
            Self::Transient => "YouTube provider is temporarily unavailable",
            Self::AmbiguousForbidden => "YouTube provider returned an ambiguous forbidden response",
            Self::Infrastructure => "YouTube validation infrastructure is unavailable",
        }
    }

    const fn disables_credential(self) -> bool {
        matches!(self, Self::ApiKeyInvalid | Self::AccessNotConfigured)
    }

    fn cooldown_until(self) -> Option<u64> {
        matches!(self, Self::QuotaExceeded | Self::RateLimited)
            .then(|| now().saturating_add(15 * 60))
    }
}

#[derive(Debug, Clone, Copy)]
enum YoutubeTestFailure {
    Rejected {
        issue: YoutubeCredentialRejectionIssue,
        health: YoutubeHealthFailure,
    },
    Failed {
        error: YoutubeCredentialTestError,
        health: YoutubeHealthFailure,
    },
}

pub(crate) fn classify_youtube_health(error: &YoutubePlatformError) -> YoutubeHealthFailure {
    match classify_provider_test_error(error) {
        YoutubeTestFailure::Rejected { health, .. } | YoutubeTestFailure::Failed { health, .. } => {
            health
        }
    }
}

/// The credential test result and health outcome of one failed YouTube call,
/// decided from the transport's API failure kind and the client's own errors.
fn classify_provider_test_error(error: &YoutubePlatformError) -> YoutubeTestFailure {
    const TRANSIENT: YoutubeTestFailure = YoutubeTestFailure::Failed {
        error: YoutubeCredentialTestError::Transient,
        health: YoutubeHealthFailure::Transient,
    };
    const INFRASTRUCTURE: YoutubeTestFailure = YoutubeTestFailure::Failed {
        error: YoutubeCredentialTestError::Infrastructure,
        health: YoutubeHealthFailure::Infrastructure,
    };
    let transport = match error {
        YoutubePlatformError::Transport(error)
        | YoutubePlatformError::Client(YoutubeClientError::Transport(error)) => error,
        YoutubePlatformError::Client(
            YoutubeClientError::EmptyQuery
            | YoutubeClientError::InvalidSearchCount
            | YoutubeClientError::InvalidPublishedAfter
            | YoutubeClientError::InvalidChannelId
            | YoutubeClientError::ChannelNotFound,
        ) => return TRANSIENT,
        YoutubePlatformError::Client(YoutubeClientError::Parse(_)) => return INFRASTRUCTURE,
    };
    match transport {
        YoutubeTransportError::Api { failure_kind, .. } => match failure_kind {
            YoutubeApiFailureKind::CredentialRejected => YoutubeTestFailure::Rejected {
                issue: YoutubeCredentialRejectionIssue::ApiKeyInvalid,
                health: YoutubeHealthFailure::ApiKeyInvalid,
            },
            YoutubeApiFailureKind::AccessNotConfigured => YoutubeTestFailure::Rejected {
                issue: YoutubeCredentialRejectionIssue::AccessNotConfigured,
                health: YoutubeHealthFailure::AccessNotConfigured,
            },
            YoutubeApiFailureKind::QuotaExceeded => YoutubeTestFailure::Failed {
                error: YoutubeCredentialTestError::RateLimited,
                health: YoutubeHealthFailure::QuotaExceeded,
            },
            YoutubeApiFailureKind::RateLimited => YoutubeTestFailure::Failed {
                error: YoutubeCredentialTestError::RateLimited,
                health: YoutubeHealthFailure::RateLimited,
            },
            YoutubeApiFailureKind::AmbiguousForbidden => YoutubeTestFailure::Failed {
                error: YoutubeCredentialTestError::AmbiguousForbidden,
                health: YoutubeHealthFailure::AmbiguousForbidden,
            },
            YoutubeApiFailureKind::Server => TRANSIENT,
            YoutubeApiFailureKind::Client => INFRASTRUCTURE,
        },
        YoutubeTransportError::Request { failure_kind, .. } => match failure_kind {
            YoutubeRequestFailureKind::Timeout
            | YoutubeRequestFailureKind::Connect
            | YoutubeRequestFailureKind::Body
            | YoutubeRequestFailureKind::Network => YoutubeTestFailure::Failed {
                error: YoutubeCredentialTestError::NetworkUnreachable,
                health: YoutubeHealthFailure::Transient,
            },
            YoutubeRequestFailureKind::Decode | YoutubeRequestFailureKind::Status => INFRASTRUCTURE,
        },
        YoutubeTransportError::InvalidUrl
        | YoutubeTransportError::HostNotAllowed
        | YoutubeTransportError::BodyTooLarge { .. }
        | YoutubeTransportError::Decode => INFRASTRUCTURE,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct YoutubeCredentialValidation {
    pub checked_at: u64,
    pub channel_id: Option<String>,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use onecrew_local_domain::{ServiceCredentialSecret, ServiceProvider};
    use onecrew_local_vault::{LocalVault, ServiceCredentialHealthCommit};
    use onecrew_youtube::{
        MAX_IN_FLIGHT_REQUESTS, YoutubeApiFailureKind, YoutubeChannelSummary, YoutubeClientError,
        YoutubePlatformError, YoutubeRequestFailureKind, YoutubeTransportError,
    };
    use uuid::Uuid;

    use super::{CredentialServices, blocking_segment};

    use super::{
        YoutubeCredentialReadError, YoutubeCredentialRejectionIssue, YoutubeCredentialTestError,
        YoutubeHealthFailure, YoutubeTestFailure, classify_provider_test_error,
    };

    fn current_revision(vault: &LocalVault, id: Uuid) -> String {
        vault.get_service_credential(id).unwrap().0.updated_at
    }

    #[tokio::test]
    async fn read_guard_stays_owned_by_started_blocking_health_work_after_caller_abort() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let services = std::sync::Arc::new(CredentialServices::new(
            vault,
        ));
        let guard = services.begin_youtube_read();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let release = std::sync::Arc::new(std::sync::Barrier::new(2));
        let worker_release = std::sync::Arc::clone(&release);
        let worker = tokio::spawn(async move {
            blocking_segment((), move || {
                started_tx.send(()).expect("health worker start receiver");
                worker_release.wait();
                drop(guard);
                Ok::<_, ()>(())
            })
            .await
        });
        started_rx.await.unwrap();
        assert_eq!(services.active_youtube_reads(), 1);

        worker.abort();
        tokio::task::yield_now().await;
        assert_eq!(
            services.active_youtube_reads(),
            1,
            "aborting the caller must not release a started health task's guard"
        );

        release.wait();
        for _ in 0..20 {
            if services.active_youtube_reads() == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(services.active_youtube_reads(), 0);
    }

    #[test]
    fn health_revision_advances_without_rebuilding_the_cached_client_or_budget() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "cached-read",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-cached"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );
        let first = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        let first_limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&credential.id)
            .unwrap()
            .limiter
            .clone();
        let commit = services
            .mark_youtube_read_success_if_revision(
                credential.id,
                &first.revision,
                first.observation,
            )
            .unwrap();
        assert!(matches!(
            commit,
            ServiceCredentialHealthCommit::Applied { .. }
        ));
        let second = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        let second_limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&credential.id)
            .unwrap()
            .limiter
            .clone();
        assert_ne!(first.revision, second.revision);
        assert!(std::sync::Arc::ptr_eq(&first_limiter, &second_limiter));
    }

    #[test]
    fn stored_probe_and_read_share_one_credential_limiter_domain() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "probe-shared-budget",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-probe-shared"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );
        let read = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        let probe = services.youtube_platform_for_test(credential.id).unwrap();
        assert!(std::sync::Arc::ptr_eq(&read.limiter, &probe.limiter));
        let held = futures::executor::block_on(
            read.limiter
                .clone()
                .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32),
        )
        .unwrap();
        assert!(probe.limiter.try_acquire().is_err());
        drop(held);
        assert!(probe.limiter.try_acquire().is_ok());
    }

    #[test]
    fn a_reader_snapshot_cannot_repopulate_cache_after_rotation_invalidation() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "read-rotation-fence",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-read-old"),
            )
            .unwrap();
        let services = std::sync::Arc::new(CredentialServices::new(
            vault,
        ));
        let hook = std::sync::Arc::new(super::YoutubeReadSnapshotHook {
            snapshot_taken: std::sync::Barrier::new(2),
            release: std::sync::Barrier::new(2),
        });
        services.set_youtube_read_snapshot_hook(std::sync::Arc::clone(&hook));
        let reader_services = std::sync::Arc::clone(&services);
        let credential_id = credential.id.to_string();
        let reader =
            std::thread::spawn(move || reader_services.youtube_platform_for_read(&credential_id));
        hook.snapshot_taken.wait();

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        let rotation_services = std::sync::Arc::clone(&services);
        let rotation = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let result = rotation_services.put_youtube_credential(
                "read-rotation-fence",
                None,
                "AIza-read-new",
            );
            finished_tx.send(result).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            finished_rx
                .recv_timeout(std::time::Duration::from_millis(100))
                .is_err(),
            "rotation must wait for the snapshot/cache admission boundary"
        );
        hook.release.wait();
        reader.join().unwrap().unwrap();
        let rotated = finished_rx.recv().unwrap().unwrap();
        rotation.join().unwrap();
        assert_eq!(rotated.id, credential.id);
        assert!(
            services
                .youtube_clients
                .lock()
                .unwrap()
                .get(&credential.id)
                .is_none()
        );
    }

    #[test]
    fn a_late_health_commit_after_delete_does_not_retain_state() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "delete-health-state",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-delete-health"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );
        let read = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        assert_eq!(services.youtube_health.lock().unwrap().len(), 1);
        assert!(
            services
                .delete_youtube_credential(&credential.id.to_string())
                .unwrap()
        );
        assert!(services.youtube_health.lock().unwrap().is_empty());
        assert_eq!(
            services
                .mark_youtube_read_success_if_revision(
                    credential.id,
                    &read.revision,
                    read.observation,
                )
                .unwrap(),
            ServiceCredentialHealthCommit::Superseded
        );
        assert!(services.youtube_health.lock().unwrap().is_empty());
    }

    #[test]
    fn stale_stored_probe_cannot_mark_a_rotated_credential() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let old = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "probe-rotation-fence",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-old-probe"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault.clone(),
        );
        let opened = services.youtube_platform_for_test(old.id).unwrap();
        let rotated = services
            .put_youtube_credential("probe-rotation-fence", None, "AIza-new-probe")
            .unwrap();
        let result = services.finish_youtube_credential_test(
            old.id,
            &opened.revision,
            opened.observation,
            Ok(YoutubeChannelSummary {
                channel_id: "UC0000000000000000000001".into(),
                title: "stale probe".into(),
                thumbnail_url: None,
                description: String::new(),
                custom_url: None,
                country: None,
            }),
        );
        assert!(matches!(
            result,
            Err(YoutubeCredentialTestError::Infrastructure)
        ));
        let (stored, secret) = vault.get_service_credential(old.id).unwrap();
        assert_eq!(stored.id, rotated.id);
        assert_eq!(stored.updated_at, rotated.updated_at);
        assert!(stored.last_error.is_none());
        assert_eq!(secret.expose_secret(), "AIza-new-probe");
    }

    #[test]
    fn later_auth_failure_wins_when_two_reads_started_at_the_same_revision() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "ordered-health",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-ordered"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault.clone(),
        );
        let first = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        let second = services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        assert_eq!(first.revision, second.revision);

        assert!(matches!(
            services
                .mark_youtube_read_success_if_revision(
                    first.id,
                    &first.revision,
                    first.observation,
                )
                .unwrap(),
            ServiceCredentialHealthCommit::Applied { .. }
        ));
        let auth_failure = YoutubePlatformError::Transport(YoutubeTransportError::Api {
            status: 401,
            failure_kind: YoutubeApiFailureKind::CredentialRejected,
            reason: Some("keyInvalid".into()),
        });
        assert!(matches!(
            services
                .mark_youtube_read_failure_if_revision(
                    second.id,
                    &second.revision,
                    second.observation,
                    &auth_failure,
                )
                .unwrap(),
            ServiceCredentialHealthCommit::Applied { .. }
        ));
        assert_eq!(
            vault
                .get_service_credential(credential.id)
                .unwrap()
                .0
                .last_error
                .as_deref(),
            Some("YouTube API key was rejected")
        );
        assert_eq!(
            services
                .mark_youtube_read_success_if_revision(
                    first.id,
                    &first.revision,
                    first.observation,
                )
                .unwrap(),
            ServiceCredentialHealthCommit::Superseded
        );
    }

    #[test]
    fn direct_and_prepare_connection_reads_share_one_credential_budget_domain() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let first = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "shared-connection",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-shared"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );

        // Two distinct Creator connection references (direct read and save
        // preparation) resolve the same stable credential id. Their opened
        // handles must draw from one domain, not one six-request allowance
        // each.
        let direct = services
            .youtube_platform_for_read(&first.id.to_string())
            .unwrap();
        let preparation = services
            .youtube_platform_for_read(&first.id.to_string())
            .unwrap();
        assert_eq!(direct.id, preparation.id);
        let limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&first.id)
            .unwrap()
            .limiter
            .clone();
        let held = futures::executor::block_on(
            limiter
                .clone()
                .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32),
        )
        .unwrap();
        assert!(limiter.try_acquire().is_err());
        drop(held);
        assert!(limiter.try_acquire().is_ok());
    }

    #[test]
    fn different_credentials_are_isolated_while_rotation_keeps_old_in_flight_budget() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let first = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "first-isolated",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-first"),
            )
            .unwrap();
        let second = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "second-isolated",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-second"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );
        services
            .youtube_platform_for_read(&first.id.to_string())
            .unwrap();
        services
            .youtube_platform_for_read(&second.id.to_string())
            .unwrap();
        let first_limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&first.id)
            .unwrap()
            .limiter
            .clone();
        let second_limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&second.id)
            .unwrap()
            .limiter
            .clone();
        assert!(!std::sync::Arc::ptr_eq(&first_limiter, &second_limiter));
        let first_held = futures::executor::block_on(
            first_limiter
                .clone()
                .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32),
        )
        .unwrap();
        assert!(second_limiter.try_acquire().is_ok());

        // Rotation invalidates the old client but retains its limiter domain,
        // so a new client cannot add a seventh request while old work is live.
        let rotated = services
            .put_youtube_credential("first-isolated", None, "AIza-first-rotated")
            .unwrap();
        assert_eq!(rotated.id, first.id);
        services
            .youtube_platform_for_read(&first.id.to_string())
            .unwrap();
        let rotated_limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&first.id)
            .unwrap()
            .limiter
            .clone();
        assert!(std::sync::Arc::ptr_eq(&first_limiter, &rotated_limiter));
        assert!(rotated_limiter.try_acquire().is_err());
        drop(first_held);
        assert!(rotated_limiter.try_acquire().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn waiting_for_a_credential_permit_can_be_cancelled_or_timed_out() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "cancel-budget",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-cancel"),
            )
            .unwrap();
        let services = CredentialServices::new(
            vault,
        );
        services
            .youtube_platform_for_read(&credential.id.to_string())
            .unwrap();
        let limiter = services
            .youtube_clients
            .lock()
            .unwrap()
            .get(&credential.id)
            .unwrap()
            .limiter
            .clone();
        let held = limiter
            .clone()
            .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32)
            .await
            .unwrap();

        let waiting = tokio::spawn({
            let limiter = limiter.clone();
            async move { limiter.acquire_owned().await }
        });
        tokio::task::yield_now().await;
        waiting.abort();
        assert!(waiting.await.is_err());
        assert!(limiter.try_acquire().is_err());

        let timed_out = tokio::spawn({
            let limiter = limiter.clone();
            async move {
                tokio::time::timeout(std::time::Duration::from_secs(30), limiter.acquire_owned())
                    .await
            }
        });
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_secs(30)).await;
        assert!(timed_out.await.unwrap().is_err());
        assert!(limiter.try_acquire().is_err());
        drop(held);
        assert!(limiter.try_acquire().is_ok());
    }

    #[test]
    fn facade_read_categories_preserve_credential_test_and_health_policy() {
        let api = |failure_kind| {
            YoutubePlatformError::Client(YoutubeClientError::Transport(
                YoutubeTransportError::Api {
                    status: 403,
                    failure_kind,
                    reason: None,
                },
            ))
        };
        let request = |failure_kind| {
            YoutubePlatformError::Client(YoutubeClientError::Transport(
                YoutubeTransportError::Request {
                    operation: "probe",
                    failure_kind,
                },
            ))
        };
        for (error, expected_error, expected_health) in [
            (
                YoutubePlatformError::Client(YoutubeClientError::ChannelNotFound),
                YoutubeCredentialTestError::Transient,
                YoutubeHealthFailure::Transient,
            ),
            (
                YoutubePlatformError::Client(YoutubeClientError::EmptyQuery),
                YoutubeCredentialTestError::Transient,
                YoutubeHealthFailure::Transient,
            ),
            (
                YoutubePlatformError::Client(YoutubeClientError::InvalidChannelId),
                YoutubeCredentialTestError::Transient,
                YoutubeHealthFailure::Transient,
            ),
            (
                YoutubePlatformError::Client(YoutubeClientError::Parse("BODY_SENTINEL".into())),
                YoutubeCredentialTestError::Infrastructure,
                YoutubeHealthFailure::Infrastructure,
            ),
            (
                request(YoutubeRequestFailureKind::Body),
                YoutubeCredentialTestError::NetworkUnreachable,
                YoutubeHealthFailure::Transient,
            ),
            (
                request(YoutubeRequestFailureKind::Decode),
                YoutubeCredentialTestError::Infrastructure,
                YoutubeHealthFailure::Infrastructure,
            ),
            (
                request(YoutubeRequestFailureKind::Status),
                YoutubeCredentialTestError::Infrastructure,
                YoutubeHealthFailure::Infrastructure,
            ),
            (
                api(YoutubeApiFailureKind::Client),
                YoutubeCredentialTestError::Infrastructure,
                YoutubeHealthFailure::Infrastructure,
            ),
            (
                api(YoutubeApiFailureKind::Server),
                YoutubeCredentialTestError::Transient,
                YoutubeHealthFailure::Transient,
            ),
            (
                api(YoutubeApiFailureKind::AmbiguousForbidden),
                YoutubeCredentialTestError::AmbiguousForbidden,
                YoutubeHealthFailure::AmbiguousForbidden,
            ),
            (
                api(YoutubeApiFailureKind::QuotaExceeded),
                YoutubeCredentialTestError::RateLimited,
                YoutubeHealthFailure::QuotaExceeded,
            ),
            (
                api(YoutubeApiFailureKind::RateLimited),
                YoutubeCredentialTestError::RateLimited,
                YoutubeHealthFailure::RateLimited,
            ),
        ] {
            let YoutubeTestFailure::Failed { error, health } = classify_provider_test_error(&error)
            else {
                panic!("non-credential failure must not disable the credential");
            };
            assert_eq!(error, expected_error);
            assert_eq!(health.message(), expected_health.message());
            assert!(!health.disables_credential());
            assert_eq!(
                health.cooldown_until().is_some(),
                expected_health.cooldown_until().is_some()
            );
        }
        for (kind, expected_issue, expected_health) in [
            (
                YoutubeApiFailureKind::CredentialRejected,
                YoutubeCredentialRejectionIssue::ApiKeyInvalid,
                YoutubeHealthFailure::ApiKeyInvalid,
            ),
            (
                YoutubeApiFailureKind::AccessNotConfigured,
                YoutubeCredentialRejectionIssue::AccessNotConfigured,
                YoutubeHealthFailure::AccessNotConfigured,
            ),
        ] {
            let YoutubeTestFailure::Rejected { issue, health } =
                classify_provider_test_error(&api(kind))
            else {
                panic!("rejected credentials must have a durable disabling outcome");
            };
            assert_eq!(issue, expected_issue);
            assert_eq!(health.message(), expected_health.message());
            assert!(health.disables_credential());
        }
    }

    #[test]
    fn read_admission_uses_local_credential_errors_for_invalid_missing_and_mismatched_keys() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let services = CredentialServices::new(
            vault.clone(),
        );
        assert!(matches!(
            services.youtube_platform_for_read("invalid"),
            Err(YoutubeCredentialReadError::InvalidCredentialId)
        ));
        assert!(matches!(
            services.youtube_platform_for_read(&uuid::Uuid::new_v4().to_string()),
            Err(YoutubeCredentialReadError::CredentialUnavailable)
        ));
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "wrong-api",
                serde_json::json!({"api": "wrong_api"}),
                &ServiceCredentialSecret::new("SECRET"),
            )
            .unwrap();
        assert!(matches!(
            services.youtube_platform_for_read(&credential.id.to_string()),
            Err(YoutubeCredentialReadError::CredentialUnavailable)
        ));
        let stored = vault.get_service_credential(credential.id).unwrap().0;
        assert!(stored.last_used_at.is_none());
        assert!(stored.last_error.is_none());
    }

    #[test]
    fn specified_youtube_discovery_rejects_unavailable_credentials_but_test_can_revalidate() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "discovery",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-test"),
            )
            .unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );
        let credential_id = credential.id.to_string();
        let read = adapter.youtube_platform_for_read(&credential_id).unwrap();
        assert_eq!(read.id, credential.id);

        vault
            .mark_service_credential_failure(credential.id, "quota exceeded", false, Some(u64::MAX))
            .unwrap();
        assert!(matches!(
            adapter.youtube_platform_for_read(&credential_id),
            Err(YoutubeCredentialReadError::CredentialUnavailable)
        ));
        assert!(adapter.youtube_platform_for_test(credential.id).is_ok());
    }

    #[test]
    fn persisted_youtube_failure_is_closed_static_health_without_provider_text() {
        const KEY: &str = "FAKE_YOUTUBE_SECRET_DO_NOT_LEAK";
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "leak-test",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new(KEY),
            )
            .unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );
        assert!(matches!(
            adapter
                .mark_youtube_read_failure_if_revision(
                    credential.id,
                    &credential.updated_at,
                    1,
                    &YoutubePlatformError::Transport(YoutubeTransportError::Request {
                        operation: "probe",
                        failure_kind: YoutubeRequestFailureKind::Network,
                    }),
                )
                .unwrap(),
            ServiceCredentialHealthCommit::Applied { .. }
        ));
        let stored = vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .pop()
            .unwrap()
            .last_error
            .unwrap();
        assert_eq!(stored, YoutubeHealthFailure::Transient.message());
        assert!(!stored.contains(KEY));
        assert!(!stored.contains("?key="));
    }

    #[test]
    fn production_test_path_returns_structured_rejection_after_persisting_safe_health() {
        const KEY: &str = "FAKE_YOUTUBE_TEST_SECRET";
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "rejected",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new(KEY),
            )
            .unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );

        let result = adapter
            .finish_youtube_credential_test(
                credential.id,
                &credential.updated_at,
                1,
                Err(YoutubePlatformError::Transport(
                    YoutubeTransportError::Api {
                        status: 403,
                        failure_kind: YoutubeApiFailureKind::CredentialRejected,
                        reason: None,
                    },
                )),
            )
            .unwrap();
        assert!(!result.ok);
        assert_eq!(result.credential_id, credential.id.to_string());
        assert!(result.channel_id.is_none());
        assert_eq!(
            result.issue,
            Some(YoutubeCredentialRejectionIssue::ApiKeyInvalid)
        );
        assert!(!result.message.contains(KEY));
        let stored = vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .pop()
            .unwrap();
        assert!(stored.last_error.is_some());
        assert!(!stored.last_error.unwrap().contains(KEY));
    }

    #[test]
    fn non_rejection_provider_failures_remain_typed_and_public_wires_are_closed() {
        const SENTINEL: &str = "PROVIDER_BODY_QUERY_KEY_SENTINEL";
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "typed-failures",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new(SENTINEL),
            )
            .unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );

        for (observation, (kind, expected)) in [
            (
                YoutubeApiFailureKind::QuotaExceeded,
                YoutubeCredentialTestError::RateLimited,
            ),
            (
                YoutubeApiFailureKind::AmbiguousForbidden,
                YoutubeCredentialTestError::AmbiguousForbidden,
            ),
            (
                YoutubeApiFailureKind::Server,
                YoutubeCredentialTestError::Transient,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let error = adapter
                .finish_youtube_credential_test(
                    credential.id,
                    &current_revision(&vault, credential.id),
                    observation as u64 + 1,
                    Err(YoutubePlatformError::Transport(
                        YoutubeTransportError::Api {
                            status: 403,
                            failure_kind: kind,
                            reason: None,
                        },
                    )),
                )
                .unwrap_err();
            assert_eq!(error, expected);
            let public = format!("{error:?}");
            assert!(!public.contains(SENTINEL));
            assert!(!public.contains("http"));
        }
        let stored = vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .pop()
            .unwrap()
            .last_error
            .unwrap();
        assert!(!stored.contains(SENTINEL));
        assert!(!stored.contains("http"));
    }

    #[test]
    fn a_missing_http_response_is_network_unreachable_with_transient_health() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let credential = vault
            .upsert_service_credential(
                ServiceProvider::Youtube,
                "offline",
                serde_json::json!({"api": "youtube_data_api_v3"}),
                &ServiceCredentialSecret::new("AIza-offline"),
            )
            .unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );

        for (observation, kind) in [
            YoutubeRequestFailureKind::Connect,
            YoutubeRequestFailureKind::Timeout,
            YoutubeRequestFailureKind::Network,
        ]
        .into_iter()
        .enumerate()
        {
            let error = adapter
                .finish_youtube_credential_test(
                    credential.id,
                    &current_revision(&vault, credential.id),
                    observation as u64 + 1,
                    Err(YoutubePlatformError::Transport(
                        YoutubeTransportError::Request {
                            operation: "sending API request",
                            failure_kind: kind,
                        },
                    )),
                )
                .unwrap_err();
            assert_eq!(error, YoutubeCredentialTestError::NetworkUnreachable);
        }
        let stored = vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(
            stored.last_error.as_deref(),
            Some(YoutubeHealthFailure::Transient.message())
        );
        assert!(!stored.disabled);
    }

    #[tokio::test]
    async fn candidate_validation_rejects_empty_keys_without_touching_the_vault() {
        let directory = tempfile::tempdir().unwrap();
        let vault = LocalVault::open(directory.path().join("agent-platform.db")).unwrap();
        let adapter = CredentialServices::new(
            vault.clone(),
        );

        assert!(adapter.validate_youtube_api_key("   ").await.is_err());
        assert!(
            vault
                .list_service_credentials(ServiceProvider::Youtube, false)
                .unwrap()
                .is_empty()
        );
    }
}
