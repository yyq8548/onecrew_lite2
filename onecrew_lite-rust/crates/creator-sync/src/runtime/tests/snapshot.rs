use super::*;

#[tokio::test]
async fn production_snapshots_are_retired_before_credentials_or_network() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let store = Arc::new(FakeStore::with_credential(metadata.clone(), bearer));
    let runtime = CreatorSyncRuntime::from_parts(true, Some(transport.clone()), store.clone());
    mark_admission_validated(&runtime);
    store.fail_load.store(true, Ordering::Release);
    assert_eq!(
        runtime.can_sync_now().await.unwrap_err().state,
        CreatorSnapshotSyncState::Retired
    );
    assert_eq!(
        runtime.sync_creator_bundle(fixture_bundle()).await.state,
        CreatorSnapshotSyncState::Retired
    );
    assert!(transport.requests.lock().unwrap().is_empty());
    store.fail_load.store(false, Ordering::Release);
    match store.load().unwrap().unwrap() {
        StoredCredentialState::Active(saved) => assert_eq!(saved.metadata, metadata),
        StoredCredentialState::NonActive(_) => panic!("retirement must preserve pairing"),
    }
}

#[tokio::test]
async fn v3_upload_reprobes_released_enrollment_without_rewriting_credential_metadata() {
    for stored_version in [1, 2] {
        let (mut metadata, bearer) = sample_credential();
        metadata.accepted_bundle.format_version = stored_version;
        let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
        let store = Arc::new(FakeStore::with_credential(metadata.clone(), bearer));
        let runtime = runtime(transport.clone(), store.clone());
        assert_eq!(
            runtime.ensure_admission_validated().await.state,
            CreatorSyncAdmissionState::Active
        );
        transport.requests.lock().unwrap().clear();
        for _ in 0..2 {
            let result = runtime.sync_creator_bundle(fixture_bundle()).await;
            assert!(matches!(
                result.state,
                CreatorSnapshotSyncState::Quarantined | CreatorSnapshotSyncState::NoChanges
            ));
        }
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests[0].path, "/creator-sync/credential");
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.path == "/creator-sync/credential")
                .count(),
            2
        );
        drop(requests);
        match store.load().unwrap().unwrap() {
            StoredCredentialState::Active(stored) => assert_eq!(stored.metadata, metadata),
            StoredCredentialState::NonActive(_) => {
                panic!("capability change must preserve enrollment")
            }
        }
    }
}

#[tokio::test]
async fn unsupported_or_unverified_upload_contract_never_plans_or_writes() {
    for (mode, state, issue) in [
        (
            SnapshotMode::ProbeV2,
            CreatorSnapshotSyncState::RecoveryRequired,
            CreatorSyncIssue::ProtocolMismatch,
        ),
        (
            SnapshotMode::ProbeV1,
            CreatorSnapshotSyncState::RecoveryRequired,
            CreatorSyncIssue::ProtocolMismatch,
        ),
        (
            SnapshotMode::ProbeWrongBinding,
            CreatorSnapshotSyncState::RecoveryRequired,
            CreatorSyncIssue::ProtocolMismatch,
        ),
        (
            SnapshotMode::ProbeUnavailable,
            CreatorSnapshotSyncState::TemporarilyUnavailable,
            CreatorSyncIssue::RemoteUnavailable,
        ),
    ] {
        let (metadata, bearer) = sample_credential();
        let transport = Arc::new(SnapshotFake::new(mode, metadata.clone()));
        let store = Arc::new(FakeStore::with_credential(metadata.clone(), bearer));
        let runtime = runtime(transport.clone(), store.clone());
        mark_admission_validated(&runtime);
        let result = runtime.sync_creator_bundle(fixture_bundle()).await;
        assert_eq!(result.state, state);
        assert_eq!(result.issue, Some(issue));
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/creator-sync/credential");
        assert_eq!(requests[0].method, Method::GET);
        assert!(store.cleanup_calls.lock().unwrap().is_empty());
        assert!(matches!(
            store.load().unwrap(),
            Some(StoredCredentialState::Active(_))
        ));
        assert!(!runtime.inner.sync_running.load(Ordering::Acquire));
    }
}

fn validated_runtime(
    transport: Arc<SnapshotFake>,
    metadata: CreatorSyncCredentialMetadata,
    bearer: BearerSecret,
) -> CreatorSyncRuntime {
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(transport, store);
    mark_admission_validated(&runtime);
    runtime
}

#[tokio::test]
async fn delta_sync_uploads_missing_creators_then_unchanged_is_a_noop() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let runtime = validated_runtime(transport.clone(), metadata, bearer);
    let bundle = fixture_bundle();

    let first = runtime.sync_creator_bundle(bundle.clone()).await;
    assert_eq!(first.state, CreatorSnapshotSyncState::Quarantined);
    assert_eq!(first.creator_count, Some(bundle.creator_count));
    assert_eq!(first.content_sha256, Some(bundle.content_sha256.clone()));
    assert!(!first.replayed);

    let second = runtime.sync_creator_bundle(bundle.clone()).await;
    assert_eq!(second.state, CreatorSnapshotSyncState::NoChanges);
    assert_eq!(second.creator_count, Some(bundle.creator_count));
    assert_eq!(second.content_sha256, Some(bundle.content_sha256.clone()));
    assert!(second.snapshot_id.is_none());

    let requests = transport.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/creator-backups/plan")
            .count(),
        2
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| {
                request.method == Method::POST && request.path == "/creator-snapshots"
            })
            .count(),
        1
    );
    assert!(requests.iter().any(|request| request.method == Method::PUT));
    assert!(
        requests
            .iter()
            .any(|request| request.path.ends_with("/complete"))
    );
    let init = requests
        .iter()
        .find(|request| request.method == Method::POST && request.path == "/creator-snapshots")
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&init.body).unwrap()["backup_kind"],
        "full"
    );
}

#[tokio::test]
async fn empty_local_bundle_never_deletes_remote_creators() {
    let remote = Arc::new(Mutex::new(BTreeMap::from([(
        "remote-only".to_owned(),
        format!("sha256:{}", "a".repeat(64)),
    )])));
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::with_remote_manifest(
        SnapshotMode::Normal,
        metadata.clone(),
        remote.clone(),
    ));
    let runtime = validated_runtime(transport.clone(), metadata, bearer);

    let result = runtime.sync_creator_bundle(empty_bundle()).await;

    assert_eq!(result.state, CreatorSnapshotSyncState::NoChanges);
    assert_eq!(result.creator_count, Some(0));
    assert!(remote.lock().unwrap().contains_key("remote-only"));
    assert_eq!(transport.requests.lock().unwrap().len(), 2);
    assert_eq!(
        transport.requests.lock().unwrap()[1].path,
        "/creator-backups/plan"
    );
}

#[tokio::test]
async fn semantic_change_selects_only_the_changed_creator_and_keeps_remote_extras() {
    let bundle = fixture_bundle();
    let record_key = bundle.creators[0].record_key.clone();
    let remote = Arc::new(Mutex::new(BTreeMap::from([
        (record_key.clone(), format!("sha256:{}", "b".repeat(64))),
        (
            "remote-only".to_owned(),
            format!("sha256:{}", "c".repeat(64)),
        ),
    ])));
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::with_remote_manifest(
        SnapshotMode::Normal,
        metadata.clone(),
        remote.clone(),
    ));
    let runtime = validated_runtime(transport.clone(), metadata, bearer);

    let result = runtime.sync_creator_bundle(bundle.clone()).await;

    assert_eq!(result.state, CreatorSnapshotSyncState::Quarantined);
    let remote = remote.lock().unwrap();
    assert_eq!(remote[&record_key], bundle.creators[0].semantic_sha256);
    assert!(remote.contains_key("remote-only"));
    let requests = transport.requests.lock().unwrap();
    let init = requests
        .iter()
        .find(|request| request.method == Method::POST && request.path == "/creator-snapshots")
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&init.body).unwrap()["backup_kind"],
        "delta"
    );
}

#[tokio::test]
async fn a_new_device_source_for_the_same_user_reuses_the_user_manifest() {
    let remote = Arc::new(Mutex::new(BTreeMap::new()));
    let bundle = fixture_bundle();

    let (first_metadata, first_bearer) = sample_credential();
    let first_transport = Arc::new(SnapshotFake::with_remote_manifest(
        SnapshotMode::Normal,
        first_metadata.clone(),
        remote.clone(),
    ));
    let first = validated_runtime(first_transport, first_metadata, first_bearer);
    assert_eq!(
        first.sync_creator_bundle(bundle.clone()).await.state,
        CreatorSnapshotSyncState::Quarantined
    );

    let (second_metadata, second_bearer) = sample_credential();
    let second_transport = Arc::new(SnapshotFake::with_remote_manifest(
        SnapshotMode::Normal,
        second_metadata.clone(),
        remote,
    ));
    let second = validated_runtime(second_transport.clone(), second_metadata, second_bearer);
    let result = second.sync_creator_bundle(bundle.clone()).await;

    assert_eq!(result.state, CreatorSnapshotSyncState::NoChanges);
    assert_eq!(second_transport.requests.lock().unwrap().len(), 2);
    assert_eq!(
        second_transport.requests.lock().unwrap()[1].path,
        "/creator-backups/plan"
    );
}

#[tokio::test]
async fn plan_contract_rejects_unknown_fields_ids_duplicates_and_wrong_binding() {
    for mode in [
        SnapshotMode::PlanUnknownField,
        SnapshotMode::PlanUnknownCreator,
        SnapshotMode::PlanDuplicateCreator,
        SnapshotMode::PlanWrongDigestEcho,
    ] {
        let (metadata, bearer) = sample_credential();
        let transport = Arc::new(SnapshotFake::new(mode, metadata.clone()));
        let runtime = validated_runtime(transport, metadata, bearer);

        let result = runtime.sync_creator_bundle(fixture_bundle()).await;

        assert_eq!(result.state, CreatorSnapshotSyncState::RecoveryRequired);
        assert_eq!(result.issue, Some(CreatorSyncIssue::ProtocolMismatch));
    }
}

#[tokio::test]
async fn plan_read_retries_once_and_manifest_conflict_replans_once() {
    for mode in [
        SnapshotMode::PlanTransportOnce,
        SnapshotMode::ManifestChangedOnce,
    ] {
        let (metadata, bearer) = sample_credential();
        let transport = Arc::new(SnapshotFake::new(mode, metadata.clone()));
        let runtime = validated_runtime(transport.clone(), metadata, bearer);

        let result = runtime.sync_creator_bundle(fixture_bundle()).await;

        assert_eq!(result.state, CreatorSnapshotSyncState::Quarantined);
        assert_eq!(transport.plan_count.load(Ordering::Acquire), 2);
    }
}

#[tokio::test]
async fn can_sync_now_reports_each_early_exit_without_taking_the_flight_slot() {
    let store = Arc::new(FakeStore::default());
    let unconfigured = CreatorSyncRuntime::from_parts(false, None, store.clone());
    let result = unconfigured.can_sync_now().await.unwrap_err();
    assert_eq!(result.state, CreatorSnapshotSyncState::NotPaired);
    assert_eq!(
        result.issue,
        Some(CreatorSyncIssue::ConfigurationUnavailable)
    );

    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(transport.clone(), store.clone());
    let result = runtime.can_sync_now().await.unwrap_err();
    assert_eq!(result.state, CreatorSnapshotSyncState::ValidationRequired);
    assert_eq!(
        result.issue,
        Some(CreatorSyncIssue::AdmissionValidationRequired)
    );

    mark_admission_validated(&runtime);
    store.fail_load.store(true, Ordering::Release);
    let result = runtime.can_sync_now().await.unwrap_err();
    assert_eq!(
        result.state,
        CreatorSnapshotSyncState::TemporarilyUnavailable
    );
    assert_eq!(result.issue, Some(CreatorSyncIssue::CredentialUnavailable));
    store.fail_load.store(false, Ordering::Release);

    assert!(runtime.can_sync_now().await.is_ok());
    assert!(!runtime.inner.sync_running.load(Ordering::Acquire));
    assert!(transport.requests.lock().unwrap().is_empty());

    // A sync parked on a held validation lock owns the flight slot; the
    // pre-check reports it without queueing behind that same lock, and the
    // slot is reported free again once the flight ends.
    let held = runtime.inner.validation.lock().await;
    let syncing = runtime.clone();
    let mut sync = tokio::spawn(async move { syncing.sync_creator_bundle(fixture_bundle()).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut sync)
            .await
            .is_err(),
        "sync must block on the held validation lock"
    );
    assert!(runtime.inner.sync_running.load(Ordering::Acquire));
    let result = runtime.can_sync_now().await.unwrap_err();
    assert_eq!(result.state, CreatorSnapshotSyncState::AlreadyRunning);
    assert_eq!(result.issue, None);
    assert!(transport.requests.lock().unwrap().is_empty());
    drop(held);
    assert_eq!(
        sync.await.unwrap().state,
        CreatorSnapshotSyncState::Quarantined
    );
    assert!(!runtime.inner.sync_running.load(Ordering::Acquire));
    assert!(runtime.can_sync_now().await.is_ok());

    store.delete().unwrap();
    let result = runtime.can_sync_now().await.unwrap_err();
    assert_eq!(result.state, CreatorSnapshotSyncState::NotPaired);
    assert_eq!(result.issue, None);

    let (metadata, _) = sample_credential();
    let non_active = runtime_for_store(Arc::new(FakeStore::with_non_active(metadata)));
    mark_admission_validated(&non_active);
    let result = non_active.can_sync_now().await.unwrap_err();
    assert_eq!(result.state, CreatorSnapshotSyncState::NotPaired);
}

fn runtime_for_store(store: Arc<dyn CreatorSyncCredentialStore>) -> CreatorSyncRuntime {
    runtime(Arc::new(AlwaysUnavailable), store)
}

#[test]
fn snapshot_blocking_tasks_cite_the_host_09_exemption() {
    let source = include_str!("../snapshot.rs");
    assert_eq!(
        source.matches("tokio::task::spawn_blocking(").count(),
        2,
        "the HOST-09 exemption covers exactly the plan and delta encoders"
    );
    assert!(source.contains("HOST-09 exemption (accepted, tracked in"));
    assert!(source.contains("phases/phase-1.md"));
    assert!(!source.contains("block_on("));
    assert!(!source.contains("block_in_place("));
}

#[tokio::test]
async fn sync_waits_for_a_held_validation_lock_and_uses_the_real_state() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let runtime = runtime(
        transport.clone(),
        Arc::new(FakeStore::with_credential(metadata, bearer)),
    );

    // Another task (an admission validation in flight) holds the lock while
    // the state is still `Required`; the sync must wait, not fail closed.
    let mut held = runtime.inner.validation.lock().await;
    assert!(*held == AdmissionValidation::Required);
    let syncing = runtime.clone();
    let mut sync = tokio::spawn(async move { syncing.sync_creator_bundle(fixture_bundle()).await });
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut sync)
            .await
            .is_err(),
        "sync must block on the held validation lock"
    );
    assert!(runtime.inner.sync_running.load(Ordering::Acquire));
    assert!(transport.requests.lock().unwrap().is_empty());
    assert_eq!(
        runtime.sync_creator_bundle(fixture_bundle()).await.state,
        CreatorSnapshotSyncState::AlreadyRunning
    );

    *held = AdmissionValidation::Validated;
    drop(held);

    let result = sync.await.unwrap();
    assert_eq!(result.state, CreatorSnapshotSyncState::Quarantined);
    assert!(!runtime.inner.sync_running.load(Ordering::Acquire));

    // The same wait with a real non-validated outcome still fails closed.
    let (metadata, bearer) = sample_credential();
    let runtime = runtime_for_store(Arc::new(FakeStore::with_credential(metadata, bearer)));
    let held = runtime.inner.validation.lock().await;
    let syncing = runtime.clone();
    let sync = tokio::spawn(async move { syncing.sync_creator_bundle(fixture_bundle()).await });
    tokio::task::yield_now().await;
    drop(held);
    let result = sync.await.unwrap();
    assert_eq!(result.state, CreatorSnapshotSyncState::ValidationRequired);
    assert_eq!(
        result.issue,
        Some(CreatorSyncIssue::AdmissionValidationRequired)
    );
}
