//! Identity-scoped update status. Lite requires complete workspace packages;
//! this owner never reads credentials or contacts the retired CLI release endpoint.
use crate::login::LoginProjection;
use onecrew_access_client::parse_cli_version;
use onecrew_creator_sync::CreatorSyncAdmissionState;
use serde::Serialize;
use std::{
    future::Future,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::Mutex,
    time::{Instant, timeout_at},
};

const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const CHECK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum CliUpdateState {
    NotChecked,
    Available,
    Current,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
enum StatusSource {
    FreshCheck,
    MemoryCache,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckFailure {
    CheckFailed,
    TimedOut,
    WorkspaceUpdateRequired,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(crate) struct CliUpdateStatus {
    pub(crate) state: CliUpdateState,
    current_version: String,
    latest_version: Option<String>,
    checked_at: Option<u64>,
    source: Option<StatusSource>,
    diagnostic_code: Option<CheckFailure>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct UpdateIdentity {
    installation_id: String,
    credential_id: String,
}

impl UpdateIdentity {
    pub(crate) fn eligible(ready: bool, login: Option<&LoginProjection>) -> Option<Self> {
        let status = &login?.status;
        if !ready || status.state != CreatorSyncAdmissionState::Active {
            return None;
        }
        Some(Self {
            installation_id: status.installation_id.clone()?,
            credential_id: status.credential_id.clone()?,
        })
    }
}

struct CacheEntry {
    identity: UpdateIdentity,
    stored_at: Instant,
    status: CliUpdateStatus,
}

#[derive(Default)]
pub(crate) struct CliUpdateStatusOwner {
    cache: Mutex<Option<CacheEntry>>,
}

impl CliUpdateStatusOwner {
    pub(crate) async fn read<F, Fut>(
        &self,
        identity: Option<UpdateIdentity>,
        current: &str,
        fetch: F,
    ) -> CliUpdateStatus
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<String, CheckFailure>>,
    {
        let Some(identity) = identity else {
            return CliUpdateStatus::not_checked(current);
        };
        let deadline = Instant::now() + CHECK_TIMEOUT;
        let Ok(mut cache) = timeout_at(deadline, self.cache.lock()).await else {
            return CliUpdateStatus::failed(current, CheckFailure::TimedOut);
        };
        if let Some(entry) = cache.as_ref()
            && entry.identity == identity
            && entry.status.current_version == current
            && entry.stored_at.elapsed() < CACHE_TTL
        {
            let mut status = entry.status.clone();
            if status.source.is_some() {
                status.source = Some(StatusSource::MemoryCache);
            }
            return status;
        }
        // The bounded async mutex coalesces requests; no repository/vault lock crosses HTTP.
        let status = match timeout_at(deadline, fetch()).await {
            Ok(Ok(latest)) => match (parse_cli_version(current), parse_cli_version(&latest)) {
                (Ok(current_version), Ok(latest_version)) => CliUpdateStatus {
                    state: if latest_version > current_version {
                        CliUpdateState::Available
                    } else {
                        CliUpdateState::Current
                    },
                    current_version: current.to_owned(),
                    latest_version: Some(latest),
                    checked_at: checked_at(),
                    source: Some(StatusSource::FreshCheck),
                    diagnostic_code: None,
                },
                _ => CliUpdateStatus::failed(current, CheckFailure::CheckFailed),
            },
            Ok(Err(failure)) => CliUpdateStatus::failed(current, failure),
            Err(_) => CliUpdateStatus::failed(current, CheckFailure::TimedOut),
        };
        *cache = Some(CacheEntry {
            identity,
            stored_at: Instant::now(),
            status: status.clone(),
        });
        status
    }
}

impl CliUpdateStatus {
    pub(crate) fn available_versions(&self) -> Option<(&str, &str)> {
        (self.state == CliUpdateState::Available).then_some((
            self.current_version.as_str(),
            self.latest_version.as_deref()?,
        ))
    }

    pub(crate) fn is_cached(&self) -> bool {
        self.source == Some(StatusSource::MemoryCache)
    }

    pub(crate) fn not_checked(current: &str) -> Self {
        Self {
            state: CliUpdateState::NotChecked,
            current_version: current.to_owned(),
            latest_version: None,
            checked_at: None,
            source: None,
            diagnostic_code: None,
        }
    }

    fn failed(current: &str, diagnostic: CheckFailure) -> Self {
        let checked = diagnostic != CheckFailure::WorkspaceUpdateRequired;
        Self {
            state: CliUpdateState::Unavailable,
            current_version: current.to_owned(),
            latest_version: None,
            checked_at: checked.then(checked_at).flatten(),
            source: checked.then_some(StatusSource::FreshCheck),
            diagnostic_code: Some(diagnostic),
        }
    }
}

fn checked_at() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|time| time.as_secs())
}

#[cfg(test)]
mod tests;
