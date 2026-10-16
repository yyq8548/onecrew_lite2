use super::*;

#[tokio::test]
async fn browser_x_chat_contract_is_required_and_rejected_before_any_connection_side_effect() {
    let missing_preference =
        serde_json::from_value::<BrowserAccountConnectionStartInput>(serde_json::json!({
            "platform": "x",
            "profile_name": "operator",
            "proxy": {
                "kind": "http",
                "host": "proxy.example",
                "port": 8080
            },
            "idempotency_key": "missing-preference"
        }));
    assert!(missing_preference.is_err());

    let (platform, directory) = platform();
    let cases = [
        (
            OperatorAccountPlatform::X,
            XChatConnectionPreference::Enabled,
            None,
            crate::RETIRED_OPERATION_CODE,
        ),
        (
            OperatorAccountPlatform::X,
            XChatConnectionPreference::Enabled,
            Some("   "),
            crate::RETIRED_OPERATION_CODE,
        ),
        (
            OperatorAccountPlatform::X,
            XChatConnectionPreference::Disabled,
            Some("must-not-be-accepted"),
            crate::RETIRED_OPERATION_CODE,
        ),
        (
            OperatorAccountPlatform::Instagram,
            XChatConnectionPreference::Enabled,
            None,
            crate::RETIRED_OPERATION_CODE,
        ),
        (
            OperatorAccountPlatform::Instagram,
            XChatConnectionPreference::Disabled,
            Some("must-not-be-accepted"),
            crate::RETIRED_OPERATION_CODE,
        ),
    ];
    for (index, (account_platform, preference, passcode, expected_error)) in
        cases.into_iter().enumerate()
    {
        let error = platform
            .browser_account_connection_start(BrowserAccountConnectionStartInput {
                platform: account_platform,
                profile_name: "operator".into(),
                proxy: ProxyConfig {
                    kind: ProxyKind::Http,
                    host: String::new(),
                    port: 0,
                    username: Some("proxy-user-secret".into()),
                    password: Some("proxy-password-secret".into()),
                },
                x_chat_preference: preference,
                x_chat_passcode: passcode.map(str::to_owned),
                idempotency_key: format!("invalid-x-chat-contract-{index}"),
            })
            .await
            .unwrap_err();
        assert_eq!(error, expected_error);
        assert!(!error.contains("proxy-user-secret") && !error.contains("proxy-password-secret"));
        assert!(
            platform
                .pending_account_connections
                .lock()
                .unwrap()
                .is_empty()
        );
        assert!(platform.pending_browser.lock().unwrap().is_empty());
        assert!(platform.vault.list(None).unwrap().is_empty());
    }

    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: None,
            label: Some("operator".into()),
        })
        .await
        .unwrap();
    assert!(prepared.browser_capability.is_some());
    let configure_error = platform
        .operator_account_connection_configure_browser_in_app(
            &prepared.flow_id,
            "operator".into(),
            dummy_secret().proxy.unwrap().into(),
            XChatConnectionPreference::Disabled,
            Some("must-not-enter-pending-state".into()),
        )
        .await
        .unwrap_err();
    assert_eq!(
        configure_error,
        crate::RETIRED_OPERATION_CODE
    );
    {
        let flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get(&prepared.flow_id).unwrap();
        assert_eq!(
            flow.phase,
            OperatorAccountConnectionPhase::AwaitingSecretInApp
        );
        assert!(flow.secret.is_none());
    }
    assert!(platform.pending_browser.lock().unwrap().is_empty());
    assert!(platform.vault.list(None).unwrap().is_empty());

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn browser_probe_failure_has_zero_connection_side_effects() {
    let (platform, directory) = platform();
    let error = platform
        .browser_account_connection_start(BrowserAccountConnectionStartInput {
            platform: OperatorAccountPlatform::X,
            profile_name: "x-test".into(),
            proxy: ProxyConfig {
                kind: ProxyKind::Http,
                host: String::new(),
                port: 0,
                username: Some("proxy-user-secret".into()),
                password: Some("proxy-password-secret".into()),
            },
            x_chat_preference: XChatConnectionPreference::Disabled,
            x_chat_passcode: None,
            idempotency_key: "browser-probe-failure".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error, "proxy host and port are required");
    assert!(
        platform
            .pending_account_connections
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(platform.pending_browser.lock().unwrap().is_empty());
    assert!(platform.vault.list(None).unwrap().is_empty());
    for forbidden in [
        "proxy-user-secret",
        "proxy-password-secret",
        "chat-passcode-secret",
    ] {
        assert!(!error.contains(forbidden));
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn completed_browser_complete_replays_without_requiring_a_live_browser_capability() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &dummy_secret())
        .unwrap();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: None,
            label: Some("x-test".into()),
        })
        .await
        .unwrap();
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&prepared.flow_id).unwrap();
        complete_flow(flow, &profile);
    }
    let input = BrowserAccountConnectionCompleteInput {
        operation_id: prepared.flow_id,
        login_completed: true,
        idempotency_key: "complete-once".into(),
    };
    let first = platform
        .browser_account_connection_complete(input.clone())
        .await
        .unwrap();
    let replay = platform
        .browser_account_connection_complete(input)
        .await
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(first.phase, DirectAccountConnectionPhase::Completed);
    assert_eq!(first.browser_capability, replay.browser_capability);
    let wire = serde_json::to_string(&first).unwrap();
    for forbidden in [
        "must-not-leak",
        "proxy.internal",
        "proxy-user",
        "proxy-password",
        "private fingerprint",
    ] {
        assert!(
            !wire.contains(forbidden),
            "public result leaked {forbidden}"
        );
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
