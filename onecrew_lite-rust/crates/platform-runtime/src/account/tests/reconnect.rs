use super::*;

#[tokio::test]
async fn unreadable_encrypted_profile_blocks_platform_open_before_reconciliation() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "public_operator", &dummy_secret())
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    drop(platform);

    std::fs::remove_file(directory.join("vault.key")).unwrap();
    let status = LocalPlatform::preflight_data_dir(&directory);
    assert_eq!(status.state, crate::LocalDataState::Blocked);
    assert_eq!(
        status.failure_kind,
        Some(crate::LocalDataFailureKind::UnsafeData)
    );
    assert!(LocalPlatform::open(&directory).is_err());
    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    let active = connection
        .query_row(
            "SELECT active FROM account_refs WHERE id = ?1",
            [account_ref_id.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(
        active, 1,
        "preflight must not reconcile or mutate the Creator store"
    );
    drop(connection);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn telegram_reconnect_with_incomplete_vault_secret_is_typed_terminal_not_dead_handoff() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::Telegram, "public_operator", &dummy_secret())
        .unwrap();
    let status = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Telegram,
            intent: OperatorAccountConnectionIntent::Reconnect,
            method: OperatorAccountConnectionMethod::ExistingAccount,
            operator_account_id: Some(profile.id.to_string()),
            label: None,
        })
        .await
        .unwrap();
    assert_eq!(status.phase, OperatorAccountConnectionPhase::Failed);
    assert_eq!(
        status.error_code.as_deref(),
        Some("telegram_reconnect_unavailable_start_new_connection")
    );
    assert!(status.handoff.is_none());
    assert!(
        !status
            .allowed_actions
            .contains(&OperatorAccountConnectionAction::OpenDesktop)
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn reconnect_flow_rejects_new_connection_secret_configuration() {
    let (platform, directory) = platform();
    let browser_profile_dir = directory.join("existing-browser-profile");
    std::fs::create_dir_all(&browser_profile_dir).unwrap();
    let mut secret = dummy_secret();
    secret.browser_profile_dir = browser_profile_dir.to_string_lossy().into_owned();
    let profile = platform
        .vault
        .upsert(Platform::X, "public_operator", &secret)
        .unwrap();
    let prepared = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Reconnect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: Some(profile.id.to_string()),
            label: None,
        })
        .await
        .unwrap();
    assert_eq!(prepared.phase, OperatorAccountConnectionPhase::ReadyToBegin);
    let error = platform
        .operator_account_connection_configure_browser_in_app(
            &prepared.flow_id,
            "must-not-replace-target".into(),
            dummy_secret().proxy.unwrap().into(),
            XChatConnectionPreference::Disabled,
            None,
        )
        .await
        .unwrap_err();
    assert!(error.contains("does not match this flow"));
    assert_eq!(
        platform
            .operator_account_connection_snapshot(&prepared.flow_id)
            .unwrap()
            .phase,
        OperatorAccountConnectionPhase::ReadyToBegin
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test]
async fn browser_reconnect_without_a_persistent_profile_is_terminal_not_a_dead_handoff() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "public_operator", &dummy_secret())
        .unwrap();
    let status = platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::X,
            intent: OperatorAccountConnectionIntent::Reconnect,
            method: OperatorAccountConnectionMethod::Browser,
            operator_account_id: Some(profile.id.to_string()),
            label: None,
        })
        .await
        .unwrap();
    assert_eq!(status.phase, OperatorAccountConnectionPhase::Failed);
    assert_eq!(
        status.error_code.as_deref(),
        Some("browser_reconnect_unavailable_start_new_connection")
    );
    assert!(status.handoff.is_none());
    assert!(
        !status
            .allowed_actions
            .contains(&OperatorAccountConnectionAction::OpenDesktop)
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
