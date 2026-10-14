use std::sync::mpsc;

use super::*;

/// Parks the first vault read until released, so a second status reader
/// overlaps a first one that still holds the pairing guard.
struct BlockingLoadStore {
    inner: FakeStore,
    first: AtomicBool,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl CreatorSyncCredentialStore for BlockingLoadStore {
    fn load(&self) -> Result<Option<StoredCredentialState>, ()> {
        if self
            .first
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.entered.send(()).unwrap();
            self.release.lock().unwrap().recv().unwrap();
        }
        self.inner.load()
    }

    fn save(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: BearerSecret,
    ) -> Result<(), ()> {
        self.inner.save(metadata, bearer)
    }

    fn invalidate(&self) -> Result<(), ()> {
        self.inner.invalidate()
    }

    fn delete(&self) -> Result<(), ()> {
        self.inner.delete()
    }
}

#[test]
fn concurrent_status_reads_keep_a_validated_admission_active() {
    let (metadata, bearer) = sample_credential();
    let (entered, entered_rx) = mpsc::channel();
    let (release_tx, release) = mpsc::channel();
    let store = Arc::new(BlockingLoadStore {
        inner: FakeStore::with_credential(metadata, bearer),
        first: AtomicBool::new(false),
        entered,
        release: Mutex::new(release),
    });
    let runtime = runtime(Arc::new(AlwaysUnavailable), store);
    mark_admission_validated(&runtime);

    let first_reader = runtime.clone();
    let first = std::thread::spawn(move || first_reader.status().state);
    entered_rx.recv().unwrap();
    let second_reader = runtime.clone();
    let second = std::thread::spawn(move || second_reader.status().state);
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !second.is_finished(),
        "a second status reader must wait, not report pairing_completing"
    );
    release_tx.send(()).unwrap();
    assert_eq!(first.join().unwrap(), CreatorSyncAdmissionState::Active);
    assert_eq!(second.join().unwrap(), CreatorSyncAdmissionState::Active);
}

#[tokio::test]
async fn admission_wait_uses_the_held_validation_result() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let runtime = runtime(
        transport.clone(),
        Arc::new(FakeStore::with_credential(metadata, bearer)),
    );
    let mut held = runtime.inner.validation.lock().await;
    *held = AdmissionValidation::Validated;
    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::Validating
    );

    let waiter = runtime.clone();
    let mut ensure = tokio::spawn(async move { waiter.ensure_admission_validated().await });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut ensure)
            .await
            .is_err(),
        "admission must wait for the held validation lock"
    );
    drop(held);
    assert_eq!(
        ensure.await.unwrap().state,
        CreatorSyncAdmissionState::Active
    );
    assert!(transport.requests.lock().unwrap().is_empty());
}

struct BlockingPairing {
    entered: Notify,
    release: Notify,
    first: AtomicBool,
}

#[async_trait]
impl HttpTransport for BlockingPairing {
    async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        if self
            .first
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Err(HttpFailure::Transport)
    }
}

#[tokio::test]
async fn cloned_coordinator_shares_pairing_completion_lock() {
    let transport = Arc::new(BlockingPairing {
        entered: Notify::new(),
        release: Notify::new(),
        first: AtomicBool::new(false),
    });
    let coordinator = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = coordinator
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let completion = complete_input(&begin, 1);
    let first_entry = coordinator.clone();
    let first = tokio::spawn(async move { first_entry.complete_pairing(completion).await });
    transport.entered.notified().await;

    assert_eq!(
        coordinator.status().state,
        CreatorSyncAdmissionState::PairingCompleting
    );
    transport.release.notify_one();
    assert_eq!(
        first.await.unwrap().unwrap_err(),
        CreatorSyncPairingError::RemoteUnavailable
    );
    assert_eq!(
        coordinator.status().issue,
        Some(CreatorSyncIssue::RemoteUnavailable)
    );
}

#[test]
fn unconfigured_and_missing_credential_fail_closed() {
    let store = Arc::new(FakeStore::default());
    let unconfigured = CreatorSyncRuntime::from_parts(false, None, store.clone());
    assert_eq!(
        unconfigured.status().state,
        CreatorSyncAdmissionState::Unconfigured
    );
    let configured = CreatorSyncRuntime::from_parts(true, Some(Arc::new(AlwaysUnavailable)), store);
    assert_eq!(
        configured.status().state,
        CreatorSyncAdmissionState::PairingRequired
    );
}

#[tokio::test]
async fn network_unknown_is_retryable_and_concurrent_sync_is_single_flight() {
    let (metadata, bearer) = sample_credential();
    let blocking_bearer = BearerSecret::generate(metadata.credential_id);
    let unavailable = runtime(
        Arc::new(AlwaysUnavailable),
        Arc::new(FakeStore::with_credential(metadata.clone(), bearer)),
    );
    mark_admission_validated(&unavailable);
    assert_eq!(
        unavailable
            .sync_creator_bundle(fixture_bundle())
            .await
            .state,
        CreatorSnapshotSyncState::TemporarilyUnavailable
    );

    let blocking = Arc::new(BlockingUnavailable {
        entered: Notify::new(),
        release: Notify::new(),
        first: AtomicBool::new(false),
    });
    let runtime = runtime(
        blocking.clone(),
        Arc::new(FakeStore::with_credential(metadata, blocking_bearer)),
    );
    mark_admission_validated(&runtime);
    let first_runtime = runtime.clone();
    let bundle = fixture_bundle();
    let first =
        tokio::spawn(async move { first_runtime.sync_creator_bundle(bundle.clone()).await });
    blocking.entered.notified().await;
    assert_eq!(
        runtime.sync_creator_bundle(fixture_bundle()).await.state,
        CreatorSnapshotSyncState::AlreadyRunning
    );
    blocking.release.notify_one();
    assert_eq!(
        first.await.unwrap().state,
        CreatorSnapshotSyncState::TemporarilyUnavailable
    );
}
