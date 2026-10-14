use onecrew_tg::QrLoginChallenge;

use super::*;

#[tokio::test]
async fn telegram_direct_start_rejects_non_socks5_without_operation() {
    let (platform, directory) = platform();
    let error = platform
        .telegram_account_connection_start(TelegramAccountConnectionStartInput {
            api_id: 12345,
            api_hash: "api-hash-secret".into(),
            proxy: ProxyConfig {
                kind: ProxyKind::Https,
                host: "proxy.example".into(),
                port: 8443,
                username: Some("proxy-user-secret".into()),
                password: Some("proxy-password-secret".into()),
            },
            method: TelegramAccountConnectionMethod::Qr,
            phone: None,
            idempotency_key: "telegram-wrong-proxy".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error, "Telegram account connection requires a SOCKS5 proxy");
    assert!(
        platform
            .pending_account_connections
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(platform.pending_tg.lock().unwrap().is_empty());
    assert!(platform.pending_tg_qr.lock().unwrap().is_empty());
    for forbidden in [
        "api-hash-secret",
        "proxy-user-secret",
        "proxy-password-secret",
    ] {
        assert!(!error.contains(forbidden));
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn telegram_phone_requires_strict_e164_before_any_operation_is_reserved() {
    let (platform, directory) = platform();
    let error = platform
        .telegram_account_connection_start(TelegramAccountConnectionStartInput {
            api_id: 12345,
            api_hash: "private-api-hash".into(),
            proxy: ProxyConfig {
                kind: ProxyKind::Socks5,
                host: "proxy.example".into(),
                port: 1080,
                username: None,
                password: None,
            },
            method: TelegramAccountConnectionMethod::Phone,
            phone: Some("+852 9123 4567".into()),
            idempotency_key: "invalid-phone".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(error, "Telegram phone must use strict E.164 format");
    assert!(
        platform
            .pending_account_connections
            .lock()
            .unwrap()
            .is_empty()
    );
    assert!(!error.contains("private-api-hash"));
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn telegram_qr_defaults_and_public_result_never_serialize_secrets() {
    let decoded: TelegramAccountConnectionStartInput = serde_json::from_value(serde_json::json!({
        "api_id": 12345,
        "api_hash": "api-hash-secret",
        "proxy": {
            "kind": "socks5",
            "host": "proxy.example",
            "port": 1080,
            "username": "proxy-user-secret",
            "password": "proxy-password-secret"
        },
        "idempotency_key": "telegram-qr-default"
    }))
    .unwrap();
    assert_eq!(decoded.method, TelegramAccountConnectionMethod::Qr);

    let (platform, directory) = platform();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Telegram,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::TelegramQr,
            operator_account_id: None,
            label: None,
        })
        .await
        .unwrap();
    platform
        .operator_account_connection_configure_telegram_in_app(
            &prepared.flow_id,
            decoded.api_id,
            decoded.api_hash,
            None,
            Some(decoded.proxy),
        )
        .await
        .unwrap();
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&prepared.flow_id).unwrap();
        flow.live = Some(LiveConnection::TelegramQr {
            token: "raw-qr-token-secret".into(),
        });
        flow.qr_challenge = Some(QrLoginChallenge {
            image_data_url: "data:image/png;base64,iVBORw0KGgo=".into(),
            terminal_qr: "     \n  █  \n     ".into(),
            expires_at: 1_800_000_000,
        });
        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
    }
    let status = platform
        .telegram_account_connection_snapshot(&prepared.flow_id)
        .unwrap();
    assert_eq!(
        status.phase,
        DirectAccountConnectionPhase::AwaitingTelegramQrScan
    );
    assert!(
        status
            .qr_code
            .as_ref()
            .unwrap()
            .image_data_url
            .starts_with("data:image/png;base64,")
    );
    let wire = serde_json::to_string(&status).unwrap();
    for forbidden in [
        "api-hash-secret",
        "proxy.example",
        "proxy-user-secret",
        "proxy-password-secret",
        "raw-qr-token-secret",
        "terminal_qr",
        "  █  ",
        "otp",
        "password",
    ] {
        assert!(
            !wire.contains(forbidden),
            "public result leaked {forbidden}"
        );
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
