use super::*;

#[tokio::test]
async fn direct_prepare_is_idempotent_and_reserves_one_operation() {
    let (platform, directory) = platform();
    let input = OperatorAccountConnectionPrepareInput {
        platform: OperatorAccountPlatform::X,
        intent: OperatorAccountConnectionIntent::Connect,
        method: OperatorAccountConnectionMethod::Browser,
        operator_account_id: None,
        label: Some("x-test".into()),
    };
    let (first, first_created) = platform
        .operator_account_connection_prepare_internal(
            input.clone(),
            Some(DirectRequestReservation {
                idempotency_key: "same-start-key".into(),
                fingerprint: [7; 32],
            }),
        )
        .await
        .unwrap();
    let (second, second_created) = platform
        .operator_account_connection_prepare_internal(
            input,
            Some(DirectRequestReservation {
                idempotency_key: "same-start-key".into(),
                fingerprint: [7; 32],
            }),
        )
        .await
        .unwrap();
    assert!(first_created);
    assert!(!second_created);
    assert_eq!(first.flow_id, second.flow_id);
    assert_eq!(
        platform.pending_account_connections.lock().unwrap().len(),
        1
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn browser_request_key_conflict_precedes_proxy_validation_and_keeps_secrets_out_of_error() {
    let (platform, directory) = platform();
    let proxy = ProxyConfig {
        kind: ProxyKind::Http,
        host: String::new(),
        port: 0,
        username: Some("private-user".into()),
        password: Some("private-password".into()),
    };
    let original_fingerprint = browser_connection_request_fingerprint(
        OperatorAccountPlatform::X,
        "operator",
        &proxy,
        XChatConnectionPreference::Enabled,
        Some("original-passcode"),
    );
    platform
        .operator_account_connection_prepare_internal(
            OperatorAccountConnectionPrepareInput {
                platform: OperatorAccountPlatform::X,
                intent: OperatorAccountConnectionIntent::Connect,
                method: OperatorAccountConnectionMethod::Browser,
                operator_account_id: None,
                label: Some("operator".into()),
            },
            Some(DirectRequestReservation {
                idempotency_key: "browser-request-key".into(),
                fingerprint: original_fingerprint,
            }),
        )
        .await
        .unwrap();

    let error = platform
        .browser_account_connection_start(BrowserAccountConnectionStartInput {
            platform: OperatorAccountPlatform::X,
            profile_name: "operator".into(),
            proxy,
            x_chat_preference: XChatConnectionPreference::Enabled,
            x_chat_passcode: Some("changed-passcode".into()),
            idempotency_key: "browser-request-key".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(
        error,
        "idempotency_key is already used by another connection request"
    );
    for secret in [
        "private-user",
        "private-password",
        "original-passcode",
        "changed-passcode",
    ] {
        assert!(!error.contains(secret));
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn direct_request_fingerprints_bind_every_secret_bearing_input_field() {
    let proxy = ProxyConfig {
        kind: ProxyKind::Socks5,
        host: "proxy.example".into(),
        port: 1080,
        username: Some("user".into()),
        password: Some("password".into()),
    };
    let browser = browser_connection_request_fingerprint(
        OperatorAccountPlatform::X,
        "profile",
        &proxy,
        XChatConnectionPreference::Enabled,
        Some("passcode"),
    );
    for changed in [
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &proxy,
            XChatConnectionPreference::Disabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::Instagram,
            "profile",
            &proxy,
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "other-profile",
            &proxy,
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &ProxyConfig {
                host: "other.example".into(),
                ..proxy.clone()
            },
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &ProxyConfig {
                port: 1081,
                ..proxy.clone()
            },
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &ProxyConfig {
                username: Some("other-user".into()),
                ..proxy.clone()
            },
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &ProxyConfig {
                password: Some("other-password".into()),
                ..proxy.clone()
            },
            XChatConnectionPreference::Enabled,
            Some("passcode"),
        ),
        browser_connection_request_fingerprint(
            OperatorAccountPlatform::X,
            "profile",
            &proxy,
            XChatConnectionPreference::Enabled,
            Some("other-passcode"),
        ),
    ] {
        assert_ne!(browser, changed);
    }

    let telegram = telegram_connection_request_fingerprint(
        12345,
        "api-hash",
        &proxy,
        TelegramAccountConnectionMethod::Phone,
        Some("+85291234567"),
    );
    for changed in [
        telegram_connection_request_fingerprint(
            12346,
            "api-hash",
            &proxy,
            TelegramAccountConnectionMethod::Phone,
            Some("+85291234567"),
        ),
        telegram_connection_request_fingerprint(
            12345,
            "other-api-hash",
            &proxy,
            TelegramAccountConnectionMethod::Phone,
            Some("+85291234567"),
        ),
        telegram_connection_request_fingerprint(
            12345,
            "api-hash",
            &ProxyConfig {
                kind: ProxyKind::Http,
                ..proxy.clone()
            },
            TelegramAccountConnectionMethod::Phone,
            Some("+85291234567"),
        ),
        telegram_connection_request_fingerprint(
            12345,
            "api-hash",
            &proxy,
            TelegramAccountConnectionMethod::Qr,
            Some("+85291234567"),
        ),
        telegram_connection_request_fingerprint(
            12345,
            "api-hash",
            &proxy,
            TelegramAccountConnectionMethod::Phone,
            Some("+85291234568"),
        ),
    ] {
        assert_ne!(telegram, changed);
    }
}

#[tokio::test]
async fn telegram_request_key_binds_api_proxy_method_and_phone_before_probe() {
    let (platform, directory) = platform();
    let proxy = ProxyConfig {
        kind: ProxyKind::Socks5,
        host: String::new(),
        port: 0,
        username: Some("private-user".into()),
        password: Some("private-password".into()),
    };
    let original_fingerprint = telegram_connection_request_fingerprint(
        12345,
        "original-api-hash",
        &proxy,
        TelegramAccountConnectionMethod::Phone,
        Some("+85291234567"),
    );
    platform
        .operator_account_connection_prepare_internal(
            OperatorAccountConnectionPrepareInput {
                platform: OperatorAccountPlatform::Telegram,
                intent: OperatorAccountConnectionIntent::Connect,
                method: OperatorAccountConnectionMethod::TelegramPhone,
                operator_account_id: None,
                label: None,
            },
            Some(DirectRequestReservation {
                idempotency_key: "telegram-request-key".into(),
                fingerprint: original_fingerprint,
            }),
        )
        .await
        .unwrap();

    let error = platform
        .telegram_account_connection_start(TelegramAccountConnectionStartInput {
            api_id: 12345,
            api_hash: "changed-api-hash".into(),
            proxy,
            method: TelegramAccountConnectionMethod::Phone,
            phone: Some("+85291234567".into()),
            idempotency_key: "telegram-request-key".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(
        error,
        "idempotency_key is already used by another connection request"
    );
    for secret in [
        "private-user",
        "private-password",
        "original-api-hash",
        "changed-api-hash",
        "+85291234567",
    ] {
        assert!(!error.contains(secret));
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
