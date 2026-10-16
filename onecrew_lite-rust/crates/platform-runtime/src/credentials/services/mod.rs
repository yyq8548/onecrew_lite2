//! Runtime-owned credential workflows and provider admission shared by Creator gateways.
mod gmail;
mod youtube;
use super::EmailMailboxAuthResolver;
use onecrew_local_vault::LocalVault;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock, atomic::AtomicU64},
};
use uuid::Uuid;
pub use youtube::YoutubeCredentialRejectionIssue;
pub(crate) use youtube::{
    YoutubeCredentialRead, YoutubeCredentialReadError, YoutubeCredentialTest,
    YoutubeCredentialTestError, YoutubeCredentialValidation,
};

pub(crate) struct CredentialServices {
    vault: LocalVault,
    mailbox_resolver: RwLock<Option<Arc<dyn EmailMailboxAuthResolver>>>,
    youtube_clients: Mutex<HashMap<Uuid, youtube::CachedYoutubeClient>>,
    youtube_limiters: Mutex<HashMap<Uuid, onecrew_youtube::YoutubeRequestLimiter>>,
    youtube_health: Mutex<HashMap<Uuid, youtube::YoutubeHealthState>>,
    youtube_observations: AtomicU64,
    youtube_flights: Arc<youtube::YoutubeReadFlights>,
    #[cfg(test)]
    youtube_read_snapshot_hook: Mutex<Option<Arc<youtube::YoutubeReadSnapshotHook>>>,
}
impl CredentialServices {
    pub(crate) fn new(vault: LocalVault) -> Self {
        Self {
            mailbox_resolver: RwLock::new(None),
            vault,
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
        *self.mailbox_resolver.write().expect("email resolver lock poisoned") = Some(resolver);
    }

    fn mailbox_resolver(&self) -> Result<Arc<dyn EmailMailboxAuthResolver>, String> {
        self.mailbox_resolver.read().map_err(|_| "email mailbox resolver is unavailable".to_owned())?
            .clone().ok_or_else(|| "email mailbox resolver is unavailable".to_owned())
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
