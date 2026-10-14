use super::*;

#[test]
fn begin_pairing_generates_canonical_unique_pkce_and_state() {
    let runtime = runtime(Arc::new(AlwaysUnavailable), Arc::new(FakeStore::default()));
    let first = runtime
        .begin_pairing(
            begin_input(7),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        first.status.state,
        CreatorSyncAdmissionState::PairingPending
    );
    assert!(Uuid::parse_str(&first.flow_id).is_ok());
    let first_url = url::Url::parse(first.authorize_url.as_str()).unwrap();
    let first_state = query_value(&first_url, "state");
    let first_challenge = query_value(&first_url, "code_challenge");
    assert!(is_urlsafe_43(&first_state));
    assert!(is_urlsafe_43(&first_challenge));
    assert_eq!(query_value(&first_url, "label"), "Fixture Mac");
    assert_eq!(query_value(&first_url, "platform"), "macos");
    assert_eq!(query_value(&first_url, "app_version"), "0.5.11");
    assert_eq!(query_value(&first_url, "callback_port"), "49152");

    runtime
        .cancel_pairing(CreatorSyncPairingCancelInput {
            flow_id: first.flow_id.clone(),
            owner_generation: 7,
        })
        .unwrap();
    let second = runtime
        .begin_pairing(
            begin_input(7),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let second_url = url::Url::parse(second.authorize_url.as_str()).unwrap();
    assert_ne!(first.flow_id, second.flow_id);
    assert_ne!(first_state, query_value(&second_url, "state"));
    assert_ne!(first_challenge, query_value(&second_url, "code_challenge"));
}

#[tokio::test]
async fn pairing_posts_exact_body_then_commits_only_after_both_readbacks() {
    let transport = Arc::new(PairingFake::new(PairingMode::Success));
    let store = Arc::new(FakeStore::default());
    let runtime = runtime(transport.clone(), store.clone());
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let authorize_url = begin.authorize_url.as_str().to_owned();
    let state = query_value(&url::Url::parse(&authorize_url).unwrap(), "state");

    let status = runtime
        .complete_pairing(complete_input(&begin, 1))
        .await
        .unwrap();
    assert_eq!(status.state, CreatorSyncAdmissionState::Active);
    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].method, Method::POST);
    assert_eq!(requests[0].path, "/installations");
    assert!(requests[0].authorization.is_none());
    assert!(requests[0].json_body);
    assert!(requests[0].body.len() <= 4 * 1024);
    assert!(
        requests[0]
            .idempotency_key
            .as_deref()
            .is_some_and(is_urlsafe_43)
    );
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body.as_object().unwrap().len(), 7);
    assert!(PairingCode::parse(body["pairing_code"].as_str().unwrap().to_owned()).is_ok());
    assert!(body["code_verifier"].as_str().is_some_and(is_pkce_verifier));
    assert!(body.get("state").is_none());
    assert!(body.get("redirect_uri").is_none());
    assert!(body["bearer"].as_str().is_some_and(|value| {
        value.starts_with(body["credential_id"].as_str().unwrap())
            && (80..=165).contains(&value.len())
    }));
    let raw = String::from_utf8(requests[0].body.clone()).unwrap();
    let positions = [
        "\"pairing_code\"",
        "\"code_verifier\"",
        "\"installation_id\"",
        "\"credential_id\"",
        "\"bearer\"",
        "\"client_platform\"",
        "\"app_version\"",
    ]
    .map(|key| raw.find(key).unwrap());
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(requests[1].method, Method::GET);
    assert_eq!(requests[2].path, "/creator-sync/credential");

    let code = body["pairing_code"].as_str().unwrap();
    let verifier = body["code_verifier"].as_str().unwrap();
    let bearer = body["bearer"].as_str().unwrap();
    let public = serde_json::to_string(&status).unwrap();
    assert!(!public.contains(code));
    assert!(!public.contains(verifier));
    assert!(!public.contains(bearer));
    assert!(!public.contains(&state));
    assert!(!public.contains(&authorize_url));
    drop(requests);

    let saved = match store.load().unwrap().unwrap() {
        StoredCredentialState::Active(saved) => saved,
        StoredCredentialState::NonActive(_) => panic!("saved pairing must be active"),
    };
    assert_eq!(
        &saved.metadata.installation_id.to_string(),
        status.installation_id.as_ref().unwrap()
    );
}

#[test]
fn invalid_code_and_callback_state_are_rejected_before_http() {
    let transport = Arc::new(PairingFake::new(PairingMode::Success));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();

    assert!(matches!(
        PairingCode::parse("reacher_pairing_v1.short".into()),
        Err(CreatorSyncPairingError::InvalidPairingCode)
    ));
    assert!(matches!(
        PairingState::parse("not-canonical".into()),
        Err(CreatorSyncPairingError::InvalidPairingState)
    ));
    assert!(matches!(
        PairingState::parse("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB".into()),
        Err(CreatorSyncPairingError::InvalidPairingState)
    ));
    assert!(transport.requests.lock().unwrap().is_empty());

    runtime
        .cancel_pairing(CreatorSyncPairingCancelInput {
            flow_id: begin.flow_id,
            owner_generation: 1,
        })
        .unwrap();
}

#[tokio::test]
async fn state_mismatch_is_safe_then_correct_callback_is_one_use() {
    let transport = Arc::new(PairingFake::new(PairingMode::Success));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(3),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let correct = complete_input(&begin, 3);
    let replay = complete_input(&begin, 3);
    let wrong = CreatorSyncPairingCompleteInput::new(
        begin.flow_id.clone(),
        3,
        pairing_code(),
        PairingState::parse("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into()).unwrap(),
    )
    .unwrap();

    let mismatch = runtime.complete_pairing(wrong).await.unwrap_err();
    assert_eq!(mismatch, CreatorSyncPairingError::StateMismatch);
    assert!(transport.requests.lock().unwrap().is_empty());
    let public_error = serde_json::to_string(&mismatch).unwrap();
    assert!(!public_error.contains(begin.authorize_url.as_str()));

    assert_eq!(
        runtime.complete_pairing(correct).await.unwrap().state,
        CreatorSyncAdmissionState::Active
    );
    assert_eq!(
        runtime.complete_pairing(replay).await.unwrap_err(),
        CreatorSyncPairingError::Replayed
    );
}

#[tokio::test]
async fn cancel_expiry_and_owner_generation_clear_pending_secrets() {
    let transport = Arc::new(PairingFake::new(PairingMode::Success));
    let clock = Arc::new(FakeClock::new(Instant::now()));
    let runtime = runtime_with_clock(
        transport.clone(),
        Arc::new(FakeStore::default()),
        clock.clone(),
    );

    let cancelled = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let cancelled_completion = complete_input(&cancelled, 1);
    assert_eq!(
        runtime
            .cancel_pairing(CreatorSyncPairingCancelInput {
                flow_id: cancelled.flow_id,
                owner_generation: 1,
            })
            .unwrap()
            .state,
        CreatorSyncAdmissionState::PairingRequired
    );
    assert_eq!(
        runtime
            .complete_pairing(cancelled_completion)
            .await
            .unwrap_err(),
        CreatorSyncPairingError::Cancelled
    );

    let expired = runtime
        .begin_pairing(
            begin_input(2),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let expired_completion = complete_input(&expired, 2);
    clock.advance(Duration::from_secs(301));
    assert_eq!(
        runtime
            .complete_pairing(expired_completion)
            .await
            .unwrap_err(),
        CreatorSyncPairingError::Expired
    );
    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::PairingRequired
    );

    let owner_changed = runtime
        .begin_pairing(
            begin_input(3),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    let old_replay = complete_input(&owner_changed, 3);
    let wrong_owner = complete_input(&owner_changed, 4);
    assert_eq!(
        runtime.complete_pairing(wrong_owner).await.unwrap_err(),
        CreatorSyncPairingError::OwnerChanged
    );
    assert_eq!(
        runtime.complete_pairing(old_replay).await.unwrap_err(),
        CreatorSyncPairingError::Replayed
    );
    assert!(
        runtime
            .begin_pairing(
                begin_input(4),
                &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
            )
            .is_ok()
    );
    assert!(transport.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_effect_only_reads_back_and_new_pairing_uses_new_tuple() {
    let transport = Arc::new(PairingFake::new(PairingMode::TransportUnknownNotFound));
    let store = Arc::new(FakeStore::default());
    let runtime = runtime(transport.clone(), store.clone());
    let first = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&first, 1))
            .await
            .unwrap_err(),
        CreatorSyncPairingError::RecoveryRequired
    );
    let first_request = {
        let requests = transport.requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|request| request.method == Method::POST)
                .count(),
            1
        );
        (
            requests[0].body.clone(),
            requests[0].idempotency_key.clone(),
        )
    };

    let second = runtime
        .begin_pairing(
            begin_input(2),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&second, 2))
            .await
            .unwrap()
            .state,
        CreatorSyncAdmissionState::Active
    );
    let requests = transport.requests.lock().unwrap();
    let posts = requests
        .iter()
        .filter(|request| request.method == Method::POST)
        .collect::<Vec<_>>();
    assert_eq!(posts.len(), 2);
    assert_ne!(posts[1].body, first_request.0);
    assert_ne!(posts[1].idempotency_key, first_request.1);
    assert!(store.load().unwrap().is_some());
}

#[tokio::test]
async fn transport_unknown_with_authoritative_readback_succeeds_without_repost() {
    let transport = Arc::new(PairingFake::new(
        PairingMode::TransportUnknownReadbackSuccess,
    ));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap()
            .state,
        CreatorSyncAdmissionState::Active
    );
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == Method::POST)
            .count(),
        1
    );
}

#[tokio::test]
async fn native_transport_failure_is_reported_as_remote_unavailable_without_repost() {
    let transport = Arc::new(PairingFake::new(PairingMode::TransportUnavailable));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();

    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap_err(),
        CreatorSyncPairingError::RemoteUnavailable
    );
    let status = runtime.status();
    assert_eq!(
        status.state,
        CreatorSyncAdmissionState::TemporarilyUnavailable
    );
    assert_eq!(status.issue, Some(CreatorSyncIssue::RemoteUnavailable));
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == Method::POST)
            .count(),
        1
    );

    assert!(
        runtime
            .begin_pairing(
                begin_input(2),
                &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
            )
            .is_ok()
    );
}

#[tokio::test]
async fn exact_remote_limit_is_retryable_without_readback_or_repost() {
    let transport = Arc::new(PairingFake::new(PairingMode::EnrollmentRateLimited));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();

    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap_err(),
        CreatorSyncPairingError::RateLimited
    );
    let status = runtime.status();
    assert_eq!(
        status.state,
        CreatorSyncAdmissionState::TemporarilyUnavailable
    );
    assert_eq!(status.issue, Some(CreatorSyncIssue::RemoteRateLimited));
    let requests = transport.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == Method::POST)
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.method == Method::GET)
            .count(),
        0
    );
}

#[tokio::test]
async fn generic_unavailable_never_reposts_pairing_code() {
    let transport = Arc::new(PairingFake::new(PairingMode::GenericUnavailableNotFound));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap_err(),
        CreatorSyncPairingError::RecoveryRequired
    );
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == Method::POST)
            .count(),
        1
    );
}

#[tokio::test]
async fn explicit_in_progress_is_the_only_same_pairing_repost() {
    let transport = Arc::new(PairingFake::new(PairingMode::InProgressThenSuccess));
    let runtime = runtime(transport.clone(), Arc::new(FakeStore::default()));
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap()
            .state,
        CreatorSyncAdmissionState::Active
    );
    let requests = transport.requests.lock().unwrap();
    let posts = requests
        .iter()
        .filter(|request| request.method == Method::POST)
        .collect::<Vec<_>>();
    assert_eq!(posts.len(), 2);
    assert_eq!(posts[0].body, posts[1].body);
    assert_eq!(posts[0].idempotency_key, posts[1].idempotency_key);
}

#[tokio::test]
async fn pairing_reject_and_secret_contamination_are_terminal_and_redacted() {
    for mode in [
        PairingMode::PairingRejected,
        PairingMode::SecretLeakCreate,
        PairingMode::SecretLeakProbe,
        PairingMode::SecretLeakReadback,
        PairingMode::MismatchedReadback,
    ] {
        let transport = Arc::new(PairingFake::new(mode));
        let store = Arc::new(FakeStore::default());
        let runtime = runtime(transport.clone(), store.clone());
        let begin = runtime
            .begin_pairing(
                begin_input(1),
                &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
            )
            .unwrap();
        let result = runtime.complete_pairing(complete_input(&begin, 1)).await;
        assert!(matches!(
            result,
            Err(CreatorSyncPairingError::PairingRejected
                | CreatorSyncPairingError::RecoveryRequired)
        ));
        assert!(store.load().unwrap().is_none());
        let requests = transport.requests.lock().unwrap();
        let request = requests
            .iter()
            .find(|request| request.method == Method::POST)
            .unwrap();
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let error = serde_json::to_string(&result.unwrap_err()).unwrap();
        for secret in ["pairing_code", "code_verifier", "bearer"] {
            assert!(!error.contains(body[secret].as_str().unwrap()));
        }
        assert!(!error.contains(begin.authorize_url.as_str()));
    }
}

#[tokio::test]
async fn enrollment_unauthorized_immediately_blocks_process_and_cleans_vault() {
    let transport = Arc::new(PairingFake::new(PairingMode::EnrollmentUnauthorized));
    let store = Arc::new(FakeStore::default());
    let runtime = runtime(transport, store.clone());
    let begin = runtime
        .begin_pairing(
            begin_input(1),
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();

    assert_eq!(
        runtime
            .complete_pairing(complete_input(&begin, 1))
            .await
            .unwrap_err(),
        CreatorSyncPairingError::RecoveryRequired
    );
    assert_eq!(
        runtime.status().state,
        CreatorSyncAdmissionState::Unauthorized
    );
    assert_eq!(
        *store.cleanup_calls.lock().unwrap(),
        vec!["invalidate", "delete"]
    );
}

#[test]
fn app_version_and_cross_repo_pairing_fixture_are_frozen() {
    let runtime = runtime(Arc::new(AlwaysUnavailable), Arc::new(FakeStore::default()));
    for invalid in ["", "a..b", "a_", "a.", "123456789012345678901234567890123"] {
        let mut input = begin_input(1);
        input.app_version = invalid.into();
        assert!(matches!(
            runtime.begin_pairing(
                input,
                &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
            ),
            Err(CreatorSyncPairingError::InvalidAppVersion)
        ));
    }
    for invalid_port in [1, 1023] {
        let mut invalid = begin_input(1);
        invalid.callback_port = invalid_port;
        assert!(matches!(
            runtime.begin_pairing(
                invalid,
                &crate::DeviceLabel::new("Fixture Mac".into()).unwrap()
            ),
            Err(CreatorSyncPairingError::InvalidCallbackPort)
        ));
    }
    let mut minimum_port = begin_input(1);
    minimum_port.callback_port = 1024;
    let minimum = runtime
        .begin_pairing(
            minimum_port,
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        query_value(
            &url::Url::parse(minimum.authorize_url.as_str()).unwrap(),
            "callback_port"
        ),
        "1024"
    );
    runtime
        .cancel_pairing(CreatorSyncPairingCancelInput {
            flow_id: minimum.flow_id,
            owner_generation: 1,
        })
        .unwrap();
    let mut max_port = begin_input(1);
    max_port.callback_port = u16::MAX;
    let max = runtime
        .begin_pairing(
            max_port,
            &crate::DeviceLabel::new("Fixture Mac".into()).unwrap(),
        )
        .unwrap();
    assert_eq!(
        query_value(
            &url::Url::parse(max.authorize_url.as_str()).unwrap(),
            "callback_port"
        ),
        u16::MAX.to_string()
    );
    runtime
        .cancel_pairing(CreatorSyncPairingCancelInput {
            flow_id: max.flow_id,
            owner_generation: 1,
        })
        .unwrap();

    let bytes = include_bytes!("../../../tests/fixtures/desktop_pairing_admission_v1.json");
    assert_eq!(bytes.len(), 413);
    assert_eq!(
        snapshot_encode::sha256_digest(bytes),
        "sha256:7d51c3615d1e696b7195ea55af7410d1f1878a2edb83d74e1a0ab853e1e091ac"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(
        fixture["request"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["code", "codeVerifier"]
    );
    assert!(PairingCode::parse(fixture["request"]["code"].as_str().unwrap().to_owned()).is_ok());
    assert!(is_pkce_verifier(
        fixture["request"]["codeVerifier"].as_str().unwrap()
    ));
    assert!(
        fixture["admitted"]["deviceId"]
            .as_str()
            .unwrap()
            .strip_prefix("dev_")
            .is_some_and(is_urlsafe_24)
    );
    assert!(
        fixture["admitted"]["pairingId"]
            .as_str()
            .unwrap()
            .strip_prefix("pair_")
            .is_some_and(is_urlsafe_24)
    );
}

fn query_value(url: &url::Url, key: &str) -> String {
    url.query_pairs()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.into_owned())
        .unwrap()
}

fn is_urlsafe_43(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_urlsafe_24(value: &str) -> bool {
    value.len() == 24
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn is_pkce_verifier(value: &str) -> bool {
    (43..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'-'))
}
