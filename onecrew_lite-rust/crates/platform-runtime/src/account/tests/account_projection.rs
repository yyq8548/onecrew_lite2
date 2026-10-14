use super::*;

#[test]
fn account_list_is_a_public_projection_only() {
    let (platform, directory) = platform();
    let profile = platform
        .vault
        .upsert(Platform::X, "operator", &dummy_secret())
        .unwrap();
    let accounts = platform.operator_account_list().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].operator_account_id, profile.id.to_string());
    assert_eq!(accounts[0].provider_account_id, "remote-operator-1");
    assert!(accounts[0].configured);
    assert_eq!(
        accounts[0].credential_availability,
        OperatorAccountCredentialAvailability::Available
    );
    assert_eq!(
        accounts[0].x_chat_passcode_status,
        XChatPasscodeStatus::Configured
    );
    let wire = serde_json::to_string(&accounts).unwrap();
    for forbidden in [
        "cookie",
        "must-not-leak",
        "proxy.internal",
        "proxy-user",
        "proxy-password",
        "fingerprint",
        "203.0.113.1",
    ] {
        assert!(
            !wire.contains(forbidden),
            "public projection leaked {forbidden}"
        );
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn telegram_projection_never_uses_phone_profile_name_or_phone_shaped_username() {
    let (platform, directory) = platform();
    let mut secret = dummy_secret();
    secret.profile_name = "Telegram · +852 9123 4567".into();
    secret.telegram_api_id = Some(12345);
    secret.telegram_api_hash = Some("private-api-hash".into());
    secret.telegram_session = Some("private-session".into());
    let profile = platform
        .vault
        .upsert(Platform::Telegram, "+85291234567", &secret)
        .unwrap();

    let account = platform
        .operator_account_list()
        .unwrap()
        .into_iter()
        .find(|account| account.operator_account_id == profile.id.to_string())
        .unwrap();
    assert!(account.username.is_empty());
    assert!(account.label.starts_with("Telegram account "));
    assert_eq!(
        account.x_chat_passcode_status,
        XChatPasscodeStatus::NotApplicable
    );
    let wire = serde_json::to_string(&account).unwrap();
    for forbidden in ["91234567", "+852", "private-api-hash", "private-session"] {
        assert!(!wire.contains(forbidden), "projection leaked {forbidden}");
    }
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unreadable_vault_secret_projects_typed_unavailable_not_configured() {
    let profile = LocalProfile {
        id: Uuid::new_v4(),
        platform: Platform::X,
        provider_account_id: "remote-public-operator".into(),
        username: "public_operator".into(),
        created_at: "1".into(),
        updated_at: "2".into(),
    };
    let account = project_operator_account(profile, None);
    assert!(!account.configured);
    assert_eq!(
        account.credential_availability,
        OperatorAccountCredentialAvailability::Unavailable
    );
    assert_eq!(
        account.x_chat_passcode_status,
        XChatPasscodeStatus::Unavailable
    );
    assert_eq!(
        serde_json::to_value(account).unwrap()["credential_availability"],
        "unavailable"
    );
}
