use super::*;

#[tokio::test]
async fn lazy_sweep_removes_expired_plaintext_flow_state() {
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
    platform
        .operator_account_connection_configure_browser_in_app(
            &prepared.flow_id,
            "operator".into(),
            dummy_secret().proxy.unwrap().into(),
            XChatConnectionPreference::Enabled,
            Some("plaintext-passcode".into()),
        )
        .await
        .unwrap();
    platform
        .pending_account_connections
        .lock()
        .unwrap()
        .get_mut(&prepared.flow_id)
        .unwrap()
        .expires_at = Instant::now() - Duration::from_secs(1);

    platform.cleanup_connection_flows_async().await.unwrap();
    assert!(
        !platform
            .pending_account_connections
            .lock()
            .unwrap()
            .contains_key(&prepared.flow_id)
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn lazy_sweep_does_not_remove_an_executing_flow() {
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
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&prepared.flow_id).unwrap();
        flow.phase = OperatorAccountConnectionPhase::Starting;
        flow.expires_at = Instant::now() - Duration::from_secs(1);
    }

    platform.cleanup_connection_flows_async().await.unwrap();
    assert!(
        platform
            .pending_account_connections
            .lock()
            .unwrap()
            .contains_key(&prepared.flow_id)
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn terminal_failure_erases_attached_plaintext_secret() {
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
    platform
        .operator_account_connection_configure_browser_in_app(
            &prepared.flow_id,
            "operator".into(),
            dummy_secret().proxy.unwrap().into(),
            XChatConnectionPreference::Enabled,
            Some("plaintext-passcode".into()),
        )
        .await
        .unwrap();
    let failed = platform
        .fail_connection(&prepared.flow_id, "test_failure")
        .await
        .unwrap();
    assert_eq!(failed.phase, OperatorAccountConnectionPhase::Failed);
    {
        let flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get(&prepared.flow_id).unwrap();
        assert!(flow.secret.is_none());
        assert!(flow.live.is_none());
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn async_lazy_sweep_removes_expired_live_flow_and_handle() {
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
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&prepared.flow_id).unwrap();
        flow.live = Some(LiveConnection::Browser {
            token: "already-gone".into(),
        });
        flow.expires_at = Instant::now() - Duration::from_secs(1);
    }
    let error = platform
        .operator_account_connection_status(OperatorAccountConnectionStatusInput {
            flow_id: prepared.flow_id.clone(),
        })
        .await
        .unwrap_err();
    assert!(error.contains("flow not found"));
    assert!(
        !platform
            .pending_account_connections
            .lock()
            .unwrap()
            .contains_key(&prepared.flow_id)
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn expired_continuation_handle_fails_closed_for_begin_and_later_status() {
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
        .pending_account_connections
        .lock()
        .unwrap()
        .get_mut(&prepared.flow_id)
        .unwrap()
        .expires_at = Instant::now() - Duration::from_secs(1);

    let begin_error = platform
        .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
            idempotency_key: "expired-begin".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(begin_error, "operator account connection flow not found");

    let status_error = platform
        .operator_account_connection_status(OperatorAccountConnectionStatusInput {
            flow_id: prepared.flow_id,
        })
        .await
        .unwrap_err();
    assert_eq!(status_error, "operator account connection flow not found");

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
