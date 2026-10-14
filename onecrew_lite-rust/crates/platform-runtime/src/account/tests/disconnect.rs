use super::*;

#[test]
fn disconnect_deactivates_creator_and_deletes_vault_profile() {
    use onecrew_creator_runtime::AccountAvailability;

    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &dummy_secret())
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    let creators = platform.creators().unwrap();
    let input = OperatorAccountDisconnectInput {
        operator_account_id: profile.id.to_string(),
        confirmed: true,
        idempotency_key: "disconnect-profile".into(),
    };
    let mut changes = platform.subscribe_changes();
    let error = platform
        .operator_account_disconnect(OperatorAccountDisconnectInput {
            confirmed: false,
            ..input.clone()
        })
        .unwrap_err();
    assert_eq!(error, OperatorAccountDisconnectError::ConfirmationRequired);
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let result = platform.operator_account_disconnect(input.clone()).unwrap();
    let change = changes.try_recv().expect("disconnect change");
    assert_eq!(
        change.scopes,
        vec![crate::PlatformChangeScope::OperatorAccounts]
    );
    assert_eq!(result.operator_account_id, profile.id.to_string());
    assert!(!result.replayed);
    let account = creators.get_account_ref(&account_ref_id).unwrap();
    assert!(!account.active);
    assert_eq!(account.availability, AccountAvailability::Unavailable);
    // The public outcome names the operator account and the replay flag only:
    // no Creator reference, no availability, and no binding impact.
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::json!({
            "operator_account_id": profile.id.to_string(),
            "replayed": false
        })
    );
    assert!(platform.vault.get(profile.id).is_err());
    let replay = platform.operator_account_disconnect(input).unwrap();
    assert!(replay.replayed);
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    drop(creators);
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn startup_recovers_disconnect_after_creator_deactivation_before_vault_delete() {
    use onecrew_creator_runtime::AccountAvailability;

    let (platform, directory) = platform();
    let browser_profile_dir = directory.join("profiles/recovery-operator");
    std::fs::create_dir_all(&browser_profile_dir).unwrap();
    let mut secret = dummy_secret();
    secret.browser_profile_dir = browser_profile_dir.to_string_lossy().into_owned();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &secret)
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    platform
        .creators
        .begin_operator_account_disconnect("recovery-key", &account_ref_id)
        .unwrap();
    assert!(platform.vault.get(profile.id).is_ok());
    drop(platform);

    let reopened = LocalPlatform::open(&directory).unwrap();
    assert!(reopened.vault.get(profile.id).is_err());
    let account = reopened
        .creators()
        .unwrap()
        .get_account_ref(&account_ref_id)
        .unwrap();
    assert!(!account.active);
    assert_eq!(account.availability, AccountAvailability::Unavailable);
    assert!(
        reopened
            .creators
            .pending_operator_account_disconnects()
            .unwrap()
            .is_empty()
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn startup_recovers_disconnect_after_vault_delete_before_creator_verification() {
    use onecrew_creator_runtime::AccountAvailability;

    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &dummy_secret())
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    platform
        .creators
        .begin_operator_account_disconnect("remote-started-recovery", &account_ref_id)
        .unwrap();
    platform
        .creators
        .mark_operator_account_disconnect_remote_started("remote-started-recovery", &account_ref_id)
        .unwrap();
    assert!(platform.vault.delete(profile.id).unwrap());
    drop(platform);

    let reopened = LocalPlatform::open(&directory).unwrap();
    let account = reopened
        .creators()
        .unwrap()
        .get_account_ref(&account_ref_id)
        .unwrap();
    assert!(!account.active);
    assert_eq!(account.availability, AccountAvailability::Unavailable);
    assert!(
        reopened
            .creators
            .pending_operator_account_disconnects()
            .unwrap()
            .is_empty()
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn missing_vault_profile_is_an_idempotent_successful_disconnect() {
    let (platform, directory) = platform();
    let account_ref_id = AccountRefId::new(Uuid::new_v4().to_string());
    platform
        .creators
        .sync_account_ref(
            account_ref_id.clone(),
            onecrew_creator_runtime::Platform::X,
            "profile",
            "@missing",
        )
        .unwrap();
    let input = OperatorAccountDisconnectInput {
        operator_account_id: account_ref_id.as_str().to_string(),
        confirmed: true,
        idempotency_key: "missing-vault-profile".into(),
    };
    let first = platform.operator_account_disconnect(input.clone()).unwrap();
    assert!(!first.replayed);
    assert_eq!(first.operator_account_id, account_ref_id.as_str());
    let replay = platform.operator_account_disconnect(input).unwrap();
    assert!(replay.replayed);
    let account = platform.creators.get_account_ref(&account_ref_id).unwrap();
    assert!(!account.active);
    assert_eq!(
        account.availability,
        onecrew_creator_runtime::AccountAvailability::Unavailable
    );
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn vault_sync_never_reactivates_inactive_account_refs() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &dummy_secret())
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let account_ref_id = AccountRefId::new(profile.id.to_string());
    platform
        .creators
        .begin_operator_account_disconnect("inactive-sync", &account_ref_id)
        .unwrap();
    platform.reconcile_vault_projections().unwrap();
    let account = platform.creators.get_account_ref(&account_ref_id).unwrap();
    assert!(!account.active);
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn disconnect_input_is_strict_and_rejects_a_non_operator_id_before_claiming() {
    let parsed: OperatorAccountDisconnectInput = serde_json::from_value(serde_json::json!({
        "operator_account_id": Uuid::new_v4().to_string(),
        "confirmed": true,
        "idempotency_key": "strict-operator-id"
    }))
    .unwrap();
    assert!(
        serde_json::from_value::<OperatorAccountDisconnectInput>(serde_json::json!({
            "account_ref_id": parsed.operator_account_id,
            "confirmed": true,
            "idempotency_key": "retired-field"
        }))
        .is_err()
    );

    let (validation_platform, validation_directory) = platform();
    assert_eq!(
        validation_platform
            .operator_account_disconnect(OperatorAccountDisconnectInput {
                operator_account_id: "not-a-uuid".into(),
                confirmed: true,
                idempotency_key: "invalid-id".into(),
            })
            .unwrap_err(),
        OperatorAccountDisconnectError::InvalidOperatorAccountId
    );
    assert_eq!(
        validation_platform
            .operator_account_disconnect(OperatorAccountDisconnectInput {
                operator_account_id: Uuid::new_v4().to_string(),
                confirmed: true,
                idempotency_key: String::new(),
            })
            .unwrap_err(),
        OperatorAccountDisconnectError::IdempotencyKeyInvalid
    );
    drop(validation_platform);
    std::fs::remove_dir_all(validation_directory).unwrap();

    let (platform, directory) = platform();
    let credential = platform
        .put_youtube_api_key("Not an operator", None, "AIza-operator-boundary")
        .unwrap();
    let error = platform
        .operator_account_disconnect(OperatorAccountDisconnectInput {
            operator_account_id: credential.id.to_string(),
            confirmed: true,
            idempotency_key: "wrong-kind".into(),
        })
        .unwrap_err();
    assert_eq!(
        error,
        OperatorAccountDisconnectError::NotFoundOrIdentityMismatch
    );
    assert!(
        platform
            .creators
            .pending_operator_account_disconnects()
            .unwrap()
            .is_empty()
    );
    assert!(platform.vault.get_service_credential(credential.id).is_ok());
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn disconnect_reports_a_typed_idempotency_conflict_for_a_different_account() {
    let (platform, directory) = platform();
    let first = platform
        .vault
        .upsert(Platform::X, "first", &dummy_secret())
        .unwrap();
    let mut second_secret = dummy_secret();
    second_secret.provider_account_id = "remote-operator-2".into();
    let second = platform
        .vault
        .upsert(Platform::X, "second", &second_secret)
        .unwrap();
    platform.sync_profile_account_ref(&first).unwrap();
    platform.sync_profile_account_ref(&second).unwrap();
    let key = "same-disconnect-key";
    platform
        .operator_account_disconnect(OperatorAccountDisconnectInput {
            operator_account_id: first.id.to_string(),
            confirmed: true,
            idempotency_key: key.into(),
        })
        .unwrap();
    assert_eq!(
        platform
            .operator_account_disconnect(OperatorAccountDisconnectInput {
                operator_account_id: second.id.to_string(),
                confirmed: true,
                idempotency_key: key.into(),
            })
            .unwrap_err(),
        OperatorAccountDisconnectError::IdempotencyConflict
    );
    assert!(platform.vault.get(second.id).is_ok());
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn disconnect_forgets_the_live_x_session_of_that_account_only() {
    let (platform, directory) = platform();
    let x_cookies = |auth_token: &str| {
        let mut secret = dummy_secret();
        // Direct egress: building the session opens no connection either way.
        secret.proxy = None;
        secret.cookies = ["auth_token", "ct0"]
            .into_iter()
            .map(|name| BrowserCookie {
                name: name.into(),
                value: format!("{auth_token}-{name}"),
                domain: ".x.com".into(),
                path: "/".into(),
                secure: true,
                http_only: true,
            })
            .collect();
        secret
    };
    let disconnected = platform
        .vault
        .upsert(Platform::X, "disconnected", &x_cookies("first"))
        .unwrap();
    let mut kept_secret = x_cookies("second");
    kept_secret.provider_account_id = "remote-operator-2".into();
    let kept = platform
        .vault
        .upsert(Platform::X, "kept", &kept_secret)
        .unwrap();
    platform.sync_profile_account_ref(&disconnected).unwrap();
    platform.sync_profile_account_ref(&kept).unwrap();
    for profile in [&disconnected, &kept] {
        let profile_id = profile.id.to_string();
        platform.sessions.x(&profile_id).unwrap();
        assert!(platform.sessions.caches_x_account(&profile_id));
    }

    platform
        .operator_account_disconnect(OperatorAccountDisconnectInput {
            operator_account_id: disconnected.id.to_string(),
            confirmed: true,
            idempotency_key: "disconnect-live-x-session".into(),
        })
        .unwrap();

    assert!(
        !platform
            .sessions
            .caches_x_account(&disconnected.id.to_string())
    );
    assert!(platform.sessions.caches_x_account(&kept.id.to_string()));
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
