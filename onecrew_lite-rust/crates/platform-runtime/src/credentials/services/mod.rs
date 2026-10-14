//! Runtime-owned credential workflows and provider admission shared by Creator gateways.
mod gmail;
mod mailbox;
mod registry;
mod resend;
#[cfg(test)]
mod resend_full_access_tests;
mod youtube;
use super::EmailMailboxAuthResolver;
use onecrew_local_vault::LocalVault;
pub(super) use registry::ResolvedResendMailbox;
pub use resend::ResendCredentialTest;
pub(crate) use resend::{ResendCredentialTestError, ResendCredentialValidation};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, atomic::AtomicU64},
};
use uuid::Uuid;
pub use youtube::YoutubeCredentialRejectionIssue;
pub(crate) use youtube::{
    YoutubeCredentialRead, YoutubeCredentialReadError, YoutubeCredentialTest,
    YoutubeCredentialTestError, YoutubeCredentialValidation,
};

pub(crate) struct CredentialServices {
    vault: LocalVault,
    email_transport: Arc<onecrew_email::ResendAdmission>,
    email_registry: registry::EmailProviderRegistry,
    youtube_clients: Mutex<HashMap<Uuid, youtube::CachedYoutubeClient>>,
    youtube_limiters: Mutex<HashMap<Uuid, onecrew_youtube::YoutubeRequestLimiter>>,
    youtube_health: Mutex<HashMap<Uuid, youtube::YoutubeHealthState>>,
    youtube_observations: AtomicU64,
    youtube_flights: Arc<youtube::YoutubeReadFlights>,
    #[cfg(test)]
    youtube_read_snapshot_hook: Mutex<Option<Arc<youtube::YoutubeReadSnapshotHook>>>,
}
impl CredentialServices {
    pub(crate) fn new(
        vault: LocalVault,
        email_transport: Arc<onecrew_email::ResendAdmission>,
    ) -> Self {
        Self {
            email_registry: registry::EmailProviderRegistry::new(
                vault.clone(),
                Arc::clone(&email_transport),
            ),
            vault,
            email_transport,
            youtube_clients: Mutex::new(HashMap::new()),
            youtube_limiters: Mutex::new(HashMap::new()),
            youtube_health: Mutex::new(HashMap::new()),
            youtube_observations: AtomicU64::new(0),
            youtube_flights: Arc::new(youtube::YoutubeReadFlights::default()),
            #[cfg(test)]
            youtube_read_snapshot_hook: Mutex::new(None),
        }
    }
    pub(crate) fn bind_email_mailbox_resolver(&self, resolver: Arc<dyn EmailMailboxAuthResolver>) {
        self.email_registry.bind_resolver(resolver);
    }

    #[cfg(test)]
    fn set_youtube_read_snapshot_hook(&self, hook: Arc<youtube::YoutubeReadSnapshotHook>) {
        *self.youtube_read_snapshot_hook.lock().unwrap() = Some(hook);
    }
}
/// Runs one synchronous vault or Creator repository segment of a credential
/// test on the blocking pool, so the provider round trip is the only work the
/// test awaits on an async worker. A lost task maps to the caller's `lost`.
pub(crate) async fn blocking_segment<T, E, F>(lost: E, work: F) -> Result<T, E>
where
    T: Send + 'static,
    E: Send + 'static,
    F: FnOnce() -> Result<T, E> + Send + 'static,
{
    tokio::task::spawn_blocking(work).await.map_err(|_| lost)?
}

fn normalize_email_address(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let Some((local, domain)) = value.split_once('@') else {
        return Err("Resend sender email_address is invalid".into());
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || value.chars().any(char::is_whitespace)
    {
        return Err("Resend sender email_address is invalid".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::blocking_segment;

    #[tokio::test]
    async fn blocking_segments_leave_the_async_worker_and_map_a_lost_task() {
        let worker = std::thread::current().id();
        let segment = blocking_segment("lost", || Ok::<_, &str>(std::thread::current().id()))
            .await
            .unwrap();
        assert_ne!(segment, worker, "the segment ran on the async worker");

        let lost = blocking_segment::<(), _, _>("lost", || panic!("segment panicked")).await;
        assert_eq!(lost, Err("lost"));
    }
}
