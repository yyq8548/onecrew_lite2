mod instagram;
mod mailboxes;
mod resolution;
mod telegram;
mod x;
mod youtube;

use std::collections::BTreeSet;

use onecrew_local_domain::{ProfileSecret, ServiceCredentialSecret, ServiceProvider};
use onecrew_local_vault::{EmailSecretRecordKind, EmailSecretRecordSecret};
use schemars::schema_for;

use super::*;

pub(super) fn platform() -> (LocalPlatform, std::path::PathBuf) {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-connection-inventory-test-{}",
        uuid::Uuid::new_v4()
    ));
    (LocalPlatform::open(&directory).unwrap(), directory)
}

pub(super) fn cleanup(platform: LocalPlatform, directory: std::path::PathBuf) {
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

pub(super) fn legacy_mailbox(platform: &LocalPlatform, label: &str, email: &str, display_name: Option<String>, secret: &str) -> uuid::Uuid {
    let record = platform.vault.upsert_email_secret_record(
        EmailSecretRecordKind::ResendApiKey,
        label,
        serde_json::json!({"email_address": email, "display_name": display_name}),
        &EmailSecretRecordSecret::new(secret),
    ).unwrap();
    platform.creators.sync_mailbox_ref(onecrew_creator_runtime::MailboxRefInput {
        id: onecrew_creator_runtime::MailboxRefId::new(record.id.to_string()),
        provider: "resend".into(),
        label: label.into(),
        email_address: email.into(),
        display_name,
        capabilities: vec![onecrew_creator_runtime::EmailCapability::Inbox],
        available: false,
        last_error: Some("lite_mailbox_provider_unsupported".into()),
    }).unwrap();
    record.id
}

pub(super) fn profile_secret(provider_account_id: &str, label: &str) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: provider_account_id.into(),
        cookies: Vec::new(),
        x_chat_passcode: Some("x-chat-secret-must-not-leak".into()),
        instagram_session_settings: Some("instagram-secret-must-not-leak".into()),
        telegram_api_id: Some(12345),
        telegram_api_hash: Some("telegram-hash-must-not-leak".into()),
        telegram_session: Some("telegram-session-must-not-leak".into()),
        profile_name: label.into(),
        browser_profile_dir: "/private/profile/path".into(),
        fingerprint_seed: "fingerprint-must-not-leak".into(),
        proxy: None,
        proxy_egress: None,
    }
}

fn object_keys(value: &serde_json::Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn empty_inventory_has_named_empty_groups() {
    let (platform, directory) = platform();

    assert_eq!(
        platform.platform_connection_list().unwrap(),
        PlatformConnectionInventory {
            x: Vec::new(),
            instagram: Vec::new(),
            telegram: Vec::new(),
            youtube: Vec::new(),
            mailboxes: Vec::new(),
        }
    );

    cleanup(platform, directory);
}

#[test]
fn wire_and_schema_keep_groups_and_platform_ids_independent() {
    let (platform, directory) = platform();
    platform
        .vault
        .upsert(
            onecrew_local_domain::Platform::X,
            "x_operator",
            &profile_secret("x-provider", "X Operator"),
        )
        .unwrap();
    platform
        .vault
        .upsert(
            onecrew_local_domain::Platform::Instagram,
            "instagram_operator",
            &profile_secret("instagram-provider", "Instagram Operator"),
        )
        .unwrap();
    platform
        .vault
        .upsert(
            onecrew_local_domain::Platform::Telegram,
            "telegram_operator",
            &profile_secret("telegram-provider", "Telegram Operator"),
        )
        .unwrap();
    platform
        .put_youtube_api_key("YouTube", None, "youtube-secret")
        .unwrap();
    legacy_mailbox(
            &platform,
            "Mailbox",
            "sender@example.com",
            Some("Sender".into()),
            "resend-secret",
        );

    let value = serde_json::to_value(platform.platform_connection_list().unwrap()).unwrap();
    assert_eq!(
        object_keys(&value),
        BTreeSet::from(["instagram", "mailboxes", "telegram", "x", "youtube"])
    );
    for group in ["x", "instagram", "telegram"] {
        assert_eq!(
            object_keys(&value[group][0]),
            BTreeSet::from(["availability", "label", "operator_account_id", "username"])
        );
    }
    assert_eq!(
        object_keys(&value["youtube"][0]),
        BTreeSet::from(["availability", "credential_id", "label"])
    );
    assert_eq!(
        object_keys(&value["mailboxes"][0]),
        BTreeSet::from([
            "availability",
            "display_label",
            "email_address",
            "mailbox_ref_id",
            "provider",
        ])
    );
    serde_json::from_value::<PlatformConnectionInventory>(value.clone()).unwrap();

    let mut unknown_top_level = value.clone();
    unknown_top_level
        .as_object_mut()
        .unwrap()
        .insert("connection_id".into(), serde_json::json!("generic"));
    assert!(serde_json::from_value::<PlatformConnectionInventory>(unknown_top_level).is_err());
    let mut unknown_platform_field = value;
    unknown_platform_field["youtube"][0]
        .as_object_mut()
        .unwrap()
        .insert("config".into(), serde_json::json!({}));
    assert!(serde_json::from_value::<PlatformConnectionInventory>(unknown_platform_field).is_err());

    let schema = serde_json::to_string(&schema_for!(PlatformConnectionInventory)).unwrap();
    for required in [
        "XConnectionSummary",
        "InstagramConnectionSummary",
        "TelegramConnectionSummary",
        "YoutubeCredentialSummary",
        "MailboxAccountSummary",
        "operator_account_id",
        "credential_id",
        "mailbox_ref_id",
    ] {
        assert!(schema.contains(required), "schema omitted {required}");
    }
    for forbidden in ["connection_id", "capabilities", "metadata", "config"] {
        assert!(!schema.contains(forbidden), "schema exposed {forbidden}");
    }

    cleanup(platform, directory);
}

#[test]
fn aggregate_wire_omits_secrets_metadata_and_owner_errors() {
    const YOUTUBE_KEY: &str = "AIza-secret-shaped-fixture";
    const RESEND_KEY: &str = "re_secret-shaped-fixture";
    const OWNER_ERROR: &str = "provider rejected re_secret at ?key=AIza-secret";
    let (platform, directory) = platform();
    platform
        .vault
        .upsert(
            onecrew_local_domain::Platform::X,
            "operator",
            &profile_secret("provider-operator", "Operator"),
        )
        .unwrap();
    let youtube = platform
        .vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "YouTube",
            serde_json::json!({
                "api": "youtube_data_api_v3",
                "api_key": YOUTUBE_KEY,
                "raw_metadata": {"token": YOUTUBE_KEY},
            }),
            &ServiceCredentialSecret::new(YOUTUBE_KEY),
        )
        .unwrap();
    let resend = platform
        .vault
        .upsert_email_secret_record(
            EmailSecretRecordKind::ResendApiKey,
            "Mailbox",
            serde_json::json!({
                "email_address": "sender@example.com",
                "display_name": "Sender",
            }),
            &EmailSecretRecordSecret::new(RESEND_KEY),
        )
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(youtube.id, OWNER_ERROR, false, None)
        .unwrap();
    platform
        .vault
        .mark_email_secret_record_failure(
            resend.id,
            EmailSecretRecordKind::ResendApiKey,
            OWNER_ERROR,
            false,
            None,
        )
        .unwrap();
    platform.creators.sync_mailbox_ref(onecrew_creator_runtime::MailboxRefInput {
        id: onecrew_creator_runtime::MailboxRefId::new(resend.id.to_string()),
        provider: "resend".into(),
        label: "Mailbox".into(),
        email_address: "sender@example.com".into(),
        display_name: Some("Sender".into()),
        capabilities: vec![onecrew_creator_runtime::EmailCapability::Inbox],
        available: false,
        last_error: Some(OWNER_ERROR.into()),
    }).unwrap();

    let wire = serde_json::to_string(&platform.platform_connection_list().unwrap()).unwrap();
    for forbidden in [
        YOUTUBE_KEY,
        RESEND_KEY,
        OWNER_ERROR,
        "x-chat-secret-must-not-leak",
        "instagram-secret-must-not-leak",
        "telegram-hash-must-not-leak",
        "telegram-session-must-not-leak",
        "fingerprint-must-not-leak",
        "/private/profile/path",
        "raw_metadata",
        "metadata",
        "last_error",
        "api_key",
        "encrypted_secret",
        "cookies",
        "connection_id",
        "config",
        "timestamps",
        "issue",
        "capabilities",
    ] {
        assert!(!wire.contains(forbidden), "wire leaked {forbidden}");
    }

    cleanup(platform, directory);
}
