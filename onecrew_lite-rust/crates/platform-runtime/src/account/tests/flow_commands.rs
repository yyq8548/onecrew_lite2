use super::*;

#[tokio::test]
async fn new_connections_are_typed_awaiting_secret_without_secret_fields() {
    let (platform, directory) = platform();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Telegram,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::TelegramQr,
            operator_account_id: None,
            label: Some("Community operator".into()),
        })
        .await
        .unwrap();
    assert_eq!(
        prepared.phase,
        OperatorAccountConnectionPhase::AwaitingSecretInApp
    );
    assert_eq!(
        prepared.kind,
        OperatorAccountConnectionKind::TelegramQrConnect
    );
    assert!(prepared.expires_at > 1_000_000_000_000);
    assert_eq!(
        prepared.handoff.as_ref().map(|value| value.kind),
        Some(OperatorAccountConnectionHandoffKind::ConfigureConnectionInDesktop)
    );

    let begin = platform
        .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
            idempotency_key: "begin-1".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        begin.phase,
        OperatorAccountConnectionPhase::AwaitingSecretInApp
    );
    let wire = serde_json::to_value(begin).unwrap();
    for forbidden in ["api_hash", "otp", "2fa", "password", "token", "cookie"] {
        assert!(
            !wire.to_string().to_ascii_lowercase().contains(forbidden),
            "public status leaked or named forbidden secret field {forbidden}: {wire}"
        );
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn begin_requires_confirmation_and_an_idempotency_key() {
    let (platform, directory) = platform();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: None,
            label: None,
        })
        .await
        .unwrap();
    let error = platform
        .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: false,
            idempotency_key: "begin-1".into(),
        })
        .await
        .unwrap_err();
    assert!(error.contains("confirmed=true"));
    let error = platform
        .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id,
            confirmed: true,
            idempotency_key: " ".into(),
        })
        .await
        .unwrap_err();
    assert!(error.contains("idempotency_key"));
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn cancel_is_confirmed_and_idempotent_without_starting_external_work() {
    let (platform, directory) = platform();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Instagram,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: None,
            label: Some("IG operator".into()),
        })
        .await
        .unwrap();
    let error = platform
        .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: false,
        })
        .await
        .unwrap_err();
    assert!(error.contains("confirmed=true"));

    for _ in 0..2 {
        let canceled = platform
            .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
                flow_id: prepared.flow_id.clone(),
                confirmed: true,
            })
            .await
            .unwrap();
        assert_eq!(canceled.phase, OperatorAccountConnectionPhase::Canceled);
        assert_eq!(
            canceled.allowed_actions,
            vec![OperatorAccountConnectionAction::Status]
        );
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn telegram_submit_and_cancel_are_mutually_exclusive() {
    let (platform, directory) = platform();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Telegram,
            intent: OperatorAccountConnectionIntent::Connect,
            method: OperatorAccountConnectionMethod::TelegramPhone,
            operator_account_id: None,
            label: None,
        })
        .await
        .unwrap();
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&prepared.flow_id).unwrap();
        flow.live = Some(LiveConnection::TelegramPhone {
            token: "pending".into(),
        });
        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
    }
    let (operation_token, _) = platform
        .begin_telegram_operation(&prepared.flow_id, false)
        .unwrap();
    let cancellation = platform
        .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
        })
        .await
        .unwrap();
    assert_eq!(cancellation.phase, OperatorAccountConnectionPhase::Starting);
    assert_eq!(
        cancellation.error_code.as_deref(),
        Some("cancellation_requested")
    );
    {
        let flows = platform.pending_account_connections.lock().unwrap();
        ensure_telegram_operation(flows.get(&prepared.flow_id).unwrap(), &operation_token).unwrap();
    }

    platform
        .restore_telegram_operation(&prepared.flow_id, &operation_token)
        .unwrap();
    let canceled = platform
        .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
        })
        .await
        .unwrap();
    assert_eq!(canceled.phase, OperatorAccountConnectionPhase::Canceled);
    assert!(
        platform
            .begin_telegram_operation(&prepared.flow_id, false)
            .is_err()
    );
    assert_eq!(
        platform
            .operator_account_connection_snapshot(&prepared.flow_id)
            .unwrap()
            .phase,
        OperatorAccountConnectionPhase::Canceled
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
