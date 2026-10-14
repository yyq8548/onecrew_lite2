use super::*;

#[test]
fn upload_probe_format_negotiation_preserves_every_credential_binding() {
    let (metadata, _) = sample_credential();
    let fake = SnapshotFake::new(SnapshotMode::Normal, metadata.clone());
    let valid = fake.probe();
    let accepts = |value: Value| {
        let probe: crate::wire::CredentialProbe = serde_json::from_value(value).unwrap();
        crate::runtime::credential_store::probe_accepts_current_bundle(&metadata, &probe)
    };
    assert!(accepts(valid.clone()));
    for field in [
        "installation_id",
        "credential_id",
        "access_user_id",
        "access_device_id",
        "source_instance_id",
        "access_pairing_id",
        "protocol",
    ] {
        let mut changed = valid.clone();
        changed[field] = json!("different-binding");
        assert!(!accepts(changed), "accepted a changed {field}");
    }
    for version in [1, 2, 4] {
        let mut changed = valid.clone();
        changed["accepted_bundle"]["format_version"] = json!(version);
        assert!(!accepts(changed));
    }
    let mut changed = valid.clone();
    changed["accepted_bundle"]["max_chunk_bytes"] = json!(42);
    assert!(!accepts(changed));
    let mut changed = valid;
    changed["protocol_version"] = json!(2);
    assert!(!accepts(changed));
}

/// Records which thread ran each vault call, so a test can prove admission
/// validation keeps vault I/O off the async worker that awaits it.
struct ThreadRecordingStore {
    inner: FakeStore,
    calls: Mutex<Vec<(&'static str, std::thread::ThreadId)>>,
}

impl ThreadRecordingStore {
    fn new(inner: FakeStore) -> Self {
        Self {
            inner,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: &'static str) {
        self.calls
            .lock()
            .unwrap()
            .push((call, std::thread::current().id()));
    }
}

impl CreatorSyncCredentialStore for ThreadRecordingStore {
    fn load(&self) -> Result<Option<StoredCredentialState>, ()> {
        self.record("load");
        self.inner.load()
    }

    fn save(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: BearerSecret,
    ) -> Result<(), ()> {
        self.record("save");
        self.inner.save(metadata, bearer)
    }

    fn invalidate(&self) -> Result<(), ()> {
        self.record("invalidate");
        self.inner.invalidate()
    }

    fn delete(&self) -> Result<(), ()> {
        self.record("delete");
        self.inner.delete()
    }
}

#[tokio::test]
async fn admission_validation_keeps_vault_calls_off_the_async_worker() {
    // The test runtime is current-thread: this thread is its only async worker.
    let worker = std::thread::current().id();

    let (stored_metadata, bearer) = sample_credential();
    let mut remote_metadata = stored_metadata.clone();
    remote_metadata.last_seen_at = Some(stored_metadata.updated_at + 1);
    let saving = Arc::new(ThreadRecordingStore::new(FakeStore::with_credential(
        stored_metadata,
        bearer,
    )));
    let validated = runtime(
        Arc::new(SnapshotFake::new(SnapshotMode::Normal, remote_metadata)),
        saving.clone(),
    )
    .ensure_admission_validated()
    .await;
    assert_eq!(validated.state, CreatorSyncAdmissionState::Active);

    let (metadata, bearer) = sample_credential();
    let cleaning = Arc::new(ThreadRecordingStore::new(FakeStore::with_credential(
        metadata.clone(),
        bearer,
    )));
    let rejected = runtime(
        Arc::new(SnapshotFake::new(
            SnapshotMode::UnauthorizedUnrecognized,
            metadata,
        )),
        cleaning.clone(),
    )
    .ensure_admission_validated()
    .await;
    assert_eq!(rejected.state, CreatorSyncAdmissionState::Unauthorized);

    for (store, expected) in [
        (saving, &["load", "save"][..]),
        (cleaning, &["load", "invalidate", "delete", "load"][..]),
    ] {
        let calls = store.calls.lock().unwrap();
        let names: Vec<_> = calls.iter().map(|(call, _)| *call).collect();
        assert_eq!(names, expected);
        assert!(
            calls.iter().all(|(_, thread)| *thread != worker),
            "vault call ran on the async worker: {calls:?}"
        );
    }
}

#[tokio::test]
async fn persisted_active_requires_authoritative_startup_validation() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(transport.clone(), store);

    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::ValidationRequired
    );
    assert!(matches!(
        runtime.begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
        ),
        Err(CreatorSyncPairingError::AdmissionValidationRequired)
    ));
    let (first, second) = tokio::join!(
        runtime.ensure_admission_validated(),
        runtime.ensure_admission_validated()
    );
    assert_eq!(first.state, CreatorSyncAdmissionState::Active);
    assert_eq!(second.state, CreatorSyncAdmissionState::Active);
    assert_eq!(runtime.status().state, CreatorSyncAdmissionState::Active);
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            format!(
                "/installations/{}",
                runtime.status().installation_id.unwrap()
            ),
            "/creator-sync/credential".to_string(),
        ]
    );
}

#[tokio::test]
async fn startup_validation_reports_network_unavailable_without_discarding_credential() {
    let (metadata, bearer) = sample_credential();
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(Arc::new(AlwaysUnavailable), store.clone());

    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::ValidationRequired
    );

    let outcome = runtime.ensure_admission_validated().await;

    assert_eq!(
        outcome.state,
        CreatorSyncAdmissionState::TemporarilyUnavailable
    );
    assert_eq!(
        outcome.action,
        Some(crate::CreatorSyncAction::ValidateAdmission)
    );
    assert_eq!(outcome.issue, Some(CreatorSyncIssue::RemoteUnavailable));
    assert!(matches!(
        store.load().unwrap(),
        Some(StoredCredentialState::Active(_))
    ));
    assert!(matches!(
        runtime.begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
        ),
        Err(CreatorSyncPairingError::AdmissionValidationRequired)
    ));
}

#[tokio::test]
async fn startup_validation_accepts_and_persists_advanced_last_seen_at() {
    let (stored_metadata, bearer) = sample_credential();
    let mut remote_metadata = stored_metadata.clone();
    remote_metadata.last_seen_at = Some(stored_metadata.updated_at + 1);
    let transport = Arc::new(SnapshotFake::new(
        SnapshotMode::Normal,
        remote_metadata.clone(),
    ));
    let store = Arc::new(FakeStore::with_credential(stored_metadata, bearer));
    let runtime = runtime(transport, store.clone());

    let outcome = runtime.ensure_admission_validated().await;

    assert_eq!(outcome.state, CreatorSyncAdmissionState::Active);
    assert_eq!(outcome.issue, None);
    let saved = match store.load().unwrap().unwrap() {
        StoredCredentialState::Active(saved) => saved,
        StoredCredentialState::NonActive(_) => panic!("validated credential must stay active"),
    };
    assert_eq!(saved.metadata, remote_metadata);
}

#[tokio::test]
async fn startup_validation_rejects_last_seen_at_rollback() {
    let (mut stored_metadata, bearer) = sample_credential();
    stored_metadata.last_seen_at = Some(stored_metadata.updated_at + 2);
    let mut remote_metadata = stored_metadata.clone();
    remote_metadata.last_seen_at = Some(stored_metadata.updated_at + 1);
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, remote_metadata));
    let store = Arc::new(FakeStore::with_credential(stored_metadata, bearer));
    let runtime = runtime(transport, store);

    let outcome = runtime.ensure_admission_validated().await;

    assert_eq!(outcome.state, CreatorSyncAdmissionState::ValidationRequired);
    assert_eq!(outcome.issue, Some(CreatorSyncIssue::ProtocolMismatch));
}

#[tokio::test]
async fn startup_validation_fails_closed_when_last_seen_refresh_cannot_be_saved() {
    let (stored_metadata, bearer) = sample_credential();
    let mut remote_metadata = stored_metadata.clone();
    remote_metadata.last_seen_at = Some(stored_metadata.updated_at + 1);
    let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, remote_metadata));
    let store = Arc::new(FakeStore::with_credential(stored_metadata, bearer));
    store.fail_save.store(true, Ordering::Release);
    let runtime = runtime(transport, store);

    let outcome = runtime.ensure_admission_validated().await;

    assert_eq!(outcome.state, CreatorSyncAdmissionState::ValidationRequired);
    assert_eq!(outcome.issue, Some(CreatorSyncIssue::LocalVaultUnavailable));
}

#[tokio::test]
async fn startup_revoked_invalidates_then_deletes_before_repairing() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(
        SnapshotMode::UnauthorizedRevoked,
        metadata.clone(),
    ));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let active_runtime = runtime(transport, store.clone());

    let outcome = active_runtime.ensure_admission_validated().await;
    assert_eq!(outcome.state, CreatorSyncAdmissionState::Revoked);
    assert_eq!(
        active_runtime.status().state,
        CreatorSyncAdmissionState::Revoked
    );
    assert_eq!(
        *store.cleanup_calls.lock().unwrap(),
        vec!["invalidate", "delete"]
    );
    assert!(store.load().unwrap().is_none());

    let rebuilt = runtime(Arc::new(AlwaysUnavailable), store);
    assert_eq!(
        rebuilt.status().state,
        CreatorSyncAdmissionState::PairingRequired
    );
}

#[tokio::test]
async fn authoritative_unauthorized_invalidates_then_deletes() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(
        SnapshotMode::UnauthorizedUnrecognized,
        metadata.clone(),
    ));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let active_runtime = runtime(transport, store.clone());

    let outcome = active_runtime.ensure_admission_validated().await;
    assert_eq!(outcome.state, CreatorSyncAdmissionState::Unauthorized);
    assert_eq!(
        active_runtime.status().state,
        CreatorSyncAdmissionState::Unauthorized
    );
    assert_eq!(
        *store.cleanup_calls.lock().unwrap(),
        vec!["invalidate", "delete"]
    );
    assert!(store.load().unwrap().is_none());
}

#[tokio::test]
async fn missing_remote_installation_deletes_stale_credential_and_allows_new_pairing() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(
        SnapshotMode::InstallationNotFound,
        metadata.clone(),
    ));
    let installation_id = metadata.installation_id;
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(transport.clone(), store.clone());

    let outcome = runtime.ensure_admission_validated().await;
    assert_eq!(outcome.state, CreatorSyncAdmissionState::Unauthorized);
    assert_eq!(outcome.issue, Some(CreatorSyncIssue::RemoteUnauthorized));
    assert_eq!(
        *store.cleanup_calls.lock().unwrap(),
        vec!["invalidate", "delete"]
    );
    assert!(store.load().unwrap().is_none());
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            format!("/installations/{installation_id}"),
            format!("/installations/{installation_id}"),
        ]
    );

    let begin = runtime
        .begin_pairing(
            begin_input(2),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        begin.status.state,
        CreatorSyncAdmissionState::PairingPending
    );
    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::PairingPending
    );
}

#[tokio::test]
async fn cleanup_failures_never_restore_process_admission() {
    for failure in ["invalidate", "delete", "readback"] {
        let (metadata, bearer) = sample_credential();
        let transport = Arc::new(SnapshotFake::new(
            SnapshotMode::UnauthorizedUnrecognized,
            metadata.clone(),
        ));
        let store = Arc::new(FakeStore::with_credential(metadata, bearer));
        match failure {
            "invalidate" => store.fail_invalidate.store(true, Ordering::Release),
            "delete" => store.fail_delete.store(true, Ordering::Release),
            "readback" => store
                .fail_readback_after_delete
                .store(true, Ordering::Release),
            _ => unreachable!(),
        }
        let runtime = runtime(transport, store.clone());

        let outcome = runtime.ensure_admission_validated().await;
        assert_eq!(outcome.state, CreatorSyncAdmissionState::Unauthorized);
        assert_eq!(outcome.issue, Some(CreatorSyncIssue::SafeCleanupIncomplete));
        assert_eq!(
            runtime.status().state,
            CreatorSyncAdmissionState::Unauthorized
        );
        assert_eq!(
            runtime.status().issue,
            Some(CreatorSyncIssue::SafeCleanupIncomplete)
        );
        assert_eq!(
            *store.cleanup_calls.lock().unwrap(),
            vec!["invalidate", "delete"]
        );
    }
}

#[tokio::test]
async fn restart_never_trusts_active_row_left_by_failed_cleanup() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(SnapshotFake::new(
        SnapshotMode::UnauthorizedUnrecognized,
        metadata.clone(),
    ));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    store.fail_invalidate.store(true, Ordering::Release);
    store.fail_delete.store(true, Ordering::Release);
    let active_runtime = runtime(transport, store.clone());

    let outcome = active_runtime.ensure_admission_validated().await;
    assert_eq!(outcome.state, CreatorSyncAdmissionState::Unauthorized);
    assert_eq!(outcome.issue, Some(CreatorSyncIssue::SafeCleanupIncomplete));

    let rebuilt = runtime(Arc::new(AlwaysUnavailable), store);
    assert_eq!(
        rebuilt.status().state,
        CreatorSyncAdmissionState::ValidationRequired
    );
    assert_eq!(
        rebuilt.ensure_admission_validated().await.state,
        CreatorSyncAdmissionState::TemporarilyUnavailable
    );
    assert_ne!(rebuilt.status().state, CreatorSyncAdmissionState::Active);
}

#[tokio::test]
async fn non_active_vault_state_never_materializes_bearer_or_http() {
    let (metadata, bearer) = sample_credential();
    drop(bearer);
    let transport = Arc::new(AlwaysUnavailable);
    let store = Arc::new(FakeStore::with_non_active(metadata));
    let runtime = runtime(transport, store);

    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::PairingRequired
    );
    assert_eq!(
        runtime.ensure_admission_validated().await.state,
        CreatorSyncAdmissionState::PairingRequired
    );
}

#[tokio::test]
async fn new_pairing_after_revocation_takes_live_pending_priority_and_completes() {
    let (metadata, bearer) = sample_credential();
    let transport = Arc::new(PairingFake::with_startup_revoked(&metadata));
    let store = Arc::new(FakeStore::with_credential(metadata, bearer));
    let runtime = runtime(transport, store);

    assert_eq!(
        runtime.ensure_admission_validated().await.state,
        CreatorSyncAdmissionState::Revoked
    );
    let begin = runtime
        .begin_pairing(
            begin_input(2),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::PairingPending
    );
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 2))
            .await
            .unwrap()
            .state,
        CreatorSyncAdmissionState::Active
    );
}

#[tokio::test]
async fn incompatible_or_widened_contract_fails_closed_before_http() {
    let (mut incompatible, bearer) = sample_credential();
    incompatible.accepted_bundle.max_chunk_bytes -= 1;
    let mut invalid = vec![(incompatible, bearer)];

    let (metadata, bearer) = sample_credential();
    let mut user = metadata.clone();
    user.access_user_id = "u".repeat(129);
    invalid.push((user, bearer));

    let (metadata, bearer) = sample_credential();
    let mut device = metadata.clone();
    device.access_device_id = "dev_too_short".into();
    invalid.push((device, bearer));

    let (mut pairing, bearer) = sample_credential();
    pairing.access_pairing_id = "pair_too_short".into();
    invalid.push((pairing, bearer));

    for (metadata, bearer) in invalid {
        let transport = Arc::new(SnapshotFake::new(SnapshotMode::Normal, metadata.clone()));
        let store = Arc::new(FakeStore::with_credential(metadata, bearer));
        let runtime = runtime(transport.clone(), store);
        assert_eq!(
            runtime.status().state,
            CreatorSyncAdmissionState::ValidationRequired
        );
        assert_eq!(
            runtime.sync_creator_bundle(fixture_bundle()).await.state,
            CreatorSnapshotSyncState::ValidationRequired
        );
        assert!(transport.requests.lock().unwrap().is_empty());
    }
}

#[test]
fn credential_store_moves_zeroizing_ownership_without_plaintext_clone() {
    let source = include_str!("../credential_store.rs");
    assert!(source.contains("bearer.into_zeroizing()"));
    assert!(source.contains("ServiceCredentialSecret::from_zeroizing"));
    assert!(source.contains("bearer.into_zeroizing(),"));
    assert!(!source.contains("expose_secret().to_owned()"));
    assert!(!source.contains("bearer.expose().to_owned()"));
    assert!(!source.contains("bearer.clone()"));
}
