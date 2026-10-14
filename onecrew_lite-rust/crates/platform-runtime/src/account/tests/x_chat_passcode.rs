use super::*;

#[test]
fn x_chat_passcode_set_and_clear_replace_only_the_encrypted_secret_field() {
    let (platform, directory) = platform();
    let mut original_secret = dummy_secret();
    original_secret.x_chat_passcode = None;
    original_secret.cookies = vec![BrowserCookie {
        name: "auth_token".into(),
        value: "cookie-secret".into(),
        domain: ".x.com".into(),
        path: "/".into(),
        secure: true,
        http_only: true,
    }];
    let original_profile = platform
        .vault
        .upsert(Platform::X, "operator", &original_secret)
        .unwrap();
    let mut changes = platform.subscribe_changes();

    let set = platform
        .operator_account_x_chat_passcode_set(SetXChatPasscodeInput {
            operator_account_id: original_profile.id.to_string(),
            x_chat_passcode: "new-chat-passcode-secret".into(),
        })
        .unwrap();
    let set_change = changes.try_recv().expect("passcode set change");
    assert_eq!(
        set_change.scopes,
        vec![crate::PlatformChangeScope::OperatorAccounts]
    );
    let change_wire = serde_json::to_string(&set_change).unwrap();
    assert!(!change_wire.contains("new-chat-passcode-secret"));
    assert!(!change_wire.contains("cookie-secret"));
    assert_eq!(set.operator_account_id, original_profile.id.to_string());
    assert_eq!(
        set.provider_account_id,
        original_profile.provider_account_id
    );
    assert_eq!(set.x_chat_passcode_status, XChatPasscodeStatus::Configured);
    let set_wire = serde_json::to_string(&set).unwrap();
    assert!(!set_wire.contains("new-chat-passcode-secret"));
    assert!(!set_wire.contains("cookie-secret"));

    let (stored_profile, stored_secret) = platform.vault.get(original_profile.id).unwrap();
    assert_eq!(stored_profile.id, original_profile.id);
    assert_eq!(
        stored_profile.provider_account_id,
        original_profile.provider_account_id
    );
    let mut expected_secret = original_secret.clone();
    expected_secret.x_chat_passcode = Some("new-chat-passcode-secret".into());
    assert_eq!(
        serde_json::to_value(&stored_secret).unwrap(),
        serde_json::to_value(&expected_secret).unwrap()
    );
    assert_eq!(
        platform
            .list_profiles()
            .unwrap()
            .into_iter()
            .find(|profile| profile.id == original_profile.id.to_string())
            .unwrap()
            .x_chat_passcode_status,
        XChatPasscodeStatus::Configured
    );

    let confirmation_error = platform
        .operator_account_x_chat_passcode_clear(ClearXChatPasscodeInput {
            operator_account_id: original_profile.id.to_string(),
            confirmed: false,
        })
        .unwrap_err();
    assert_eq!(confirmation_error, "confirmed=true is required");
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    assert_eq!(
        platform
            .vault
            .get(original_profile.id)
            .unwrap()
            .1
            .x_chat_passcode
            .as_deref(),
        Some("new-chat-passcode-secret")
    );

    let cleared = platform
        .operator_account_x_chat_passcode_clear(ClearXChatPasscodeInput {
            operator_account_id: original_profile.id.to_string(),
            confirmed: true,
        })
        .unwrap();
    let clear_change = changes.try_recv().expect("passcode clear change");
    assert_eq!(
        clear_change.scopes,
        vec![crate::PlatformChangeScope::OperatorAccounts]
    );
    assert_eq!(
        cleared.x_chat_passcode_status,
        XChatPasscodeStatus::NotConfigured
    );
    let (cleared_profile, cleared_secret) = platform.vault.get(original_profile.id).unwrap();
    assert_eq!(cleared_profile.id, original_profile.id);
    assert_eq!(
        cleared_profile.provider_account_id,
        original_profile.provider_account_id
    );
    assert_eq!(
        serde_json::to_value(&cleared_secret).unwrap(),
        serde_json::to_value(&original_secret).unwrap()
    );
    assert_eq!(
        platform
            .list_profiles()
            .unwrap()
            .into_iter()
            .find(|profile| profile.id == original_profile.id.to_string())
            .unwrap()
            .x_chat_passcode_status,
        XChatPasscodeStatus::NotConfigured
    );

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn x_chat_passcode_updates_reject_empty_values_and_non_x_accounts() {
    let (platform, directory) = platform();
    let original_secret = dummy_secret();
    let profile = platform
        .vault
        .upsert(Platform::Instagram, "operator", &original_secret)
        .unwrap();
    let mut changes = platform.subscribe_changes();

    let empty_error = platform
        .operator_account_x_chat_passcode_set(SetXChatPasscodeInput {
            operator_account_id: profile.id.to_string(),
            x_chat_passcode: "   ".into(),
        })
        .unwrap_err();
    assert_eq!(empty_error, "x_chat_passcode must be non-empty");
    let set_error = platform
        .operator_account_x_chat_passcode_set(SetXChatPasscodeInput {
            operator_account_id: profile.id.to_string(),
            x_chat_passcode: "must-not-be-stored".into(),
        })
        .unwrap_err();
    assert_eq!(
        set_error,
        "X Chat passcode can only be changed for an X operator account"
    );
    let clear_error = platform
        .operator_account_x_chat_passcode_clear(ClearXChatPasscodeInput {
            operator_account_id: profile.id.to_string(),
            confirmed: true,
        })
        .unwrap_err();
    assert_eq!(
        clear_error,
        "X Chat passcode can only be changed for an X operator account"
    );
    let (_, unchanged_secret) = platform.vault.get(profile.id).unwrap();
    assert_eq!(
        serde_json::to_value(unchanged_secret).unwrap(),
        serde_json::to_value(original_secret).unwrap()
    );
    assert_eq!(
        platform
            .operator_account_list()
            .unwrap()
            .into_iter()
            .find(|account| account.operator_account_id == profile.id.to_string())
            .unwrap()
            .x_chat_passcode_status,
        XChatPasscodeStatus::NotApplicable
    );
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
