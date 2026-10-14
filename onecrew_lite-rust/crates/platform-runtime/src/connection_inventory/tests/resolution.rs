use onecrew_creator_runtime::{AccountAvailability, AccountKind, AccountRefId, Platform};
use onecrew_local_domain::{
    Platform as LocalProfilePlatform, ServiceCredentialSecret, ServiceProvider,
};

use super::{cleanup, platform, profile_secret};
use crate::{
    OperatorAccountPlatform, PlatformConnectionResolutionError, PlatformConnectionSelector,
};

fn operator_account(
    platform: &crate::LocalPlatform,
    local_platform: LocalProfilePlatform,
    username: &str,
) -> String {
    let profile = platform
        .vault
        .upsert(
            local_platform,
            username,
            &profile_secret(&format!("provider-{username}"), username),
        )
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    profile.id.to_string()
}

fn youtube_credential(platform: &crate::LocalPlatform, label: &str, secret: &str) -> String {
    let credential = platform
        .vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            label,
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new(secret),
        )
        .unwrap();
    platform.reconcile_vault_youtube_refs().unwrap();
    credential.id.to_string()
}

#[tokio::test]
async fn resolves_available_operator_and_youtube_connections_to_canonical_creator_refs() {
    let (platform, directory) = platform();
    let operator_account_id =
        operator_account(&platform, LocalProfilePlatform::X, "available-operator");
    let credential_id = youtube_credential(&platform, "Available YouTube", "youtube-secret");

    assert_eq!(
        platform
            .resolve_platform_connection(PlatformConnectionSelector::OperatorAccount {
                platform: OperatorAccountPlatform::X,
                operator_account_id: operator_account_id.clone(),
            })
            .await
            .unwrap(),
        AccountRefId::new(operator_account_id)
    );
    assert_eq!(
        platform
            .resolve_platform_connection(PlatformConnectionSelector::YoutubeCredential {
                credential_id: credential_id.clone(),
            })
            .await
            .unwrap(),
        AccountRefId::new(credential_id)
    );

    cleanup(platform, directory);
}

#[tokio::test]
async fn rejects_selector_platform_and_cross_kind_mismatches() {
    let (platform, directory) = platform();
    let operator_account_id =
        operator_account(&platform, LocalProfilePlatform::X, "mismatch-operator");
    let credential_id = youtube_credential(&platform, "Mismatch YouTube", "youtube-secret");

    for selector in [
        PlatformConnectionSelector::OperatorAccount {
            platform: OperatorAccountPlatform::Instagram,
            operator_account_id: operator_account_id.clone(),
        },
        PlatformConnectionSelector::OperatorAccount {
            platform: OperatorAccountPlatform::X,
            operator_account_id: credential_id.clone(),
        },
        PlatformConnectionSelector::YoutubeCredential {
            credential_id: operator_account_id,
        },
    ] {
        assert_eq!(
            platform.resolve_platform_connection(selector).await,
            Err(PlatformConnectionResolutionError::NotFoundOrMismatch)
        );
    }

    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    connection
        .execute(
            "UPDATE account_refs SET kind = 'profile' WHERE id = ?1",
            [&credential_id],
        )
        .unwrap();
    drop(connection);
    assert_eq!(
        platform
            .resolve_platform_connection(PlatformConnectionSelector::YoutubeCredential {
                credential_id,
            })
            .await,
        Err(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );

    cleanup(platform, directory);
}

#[tokio::test]
async fn rejects_invalid_and_missing_identifiers_without_guessing_an_id_domain() {
    let (platform, directory) = platform();

    assert_eq!(
        platform
            .resolve_platform_connection(PlatformConnectionSelector::OperatorAccount {
                platform: OperatorAccountPlatform::Telegram,
                operator_account_id: "not-a-uuid".into(),
            })
            .await,
        Err(PlatformConnectionResolutionError::InvalidIdentifier)
    );
    assert_eq!(
        platform
            .resolve_platform_connection(PlatformConnectionSelector::YoutubeCredential {
                credential_id: uuid::Uuid::new_v4().to_string(),
            })
            .await,
        Err(PlatformConnectionResolutionError::NotFoundOrMismatch)
    );

    cleanup(platform, directory);
}

#[tokio::test]
async fn rejects_inactive_or_creator_unavailable_operator_accounts() {
    let (platform, directory) = platform();
    let inactive_id = operator_account(
        &platform,
        LocalProfilePlatform::Instagram,
        "inactive-operator",
    );
    let unavailable_id = operator_account(
        &platform,
        LocalProfilePlatform::Telegram,
        "unavailable-operator",
    );
    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    connection
        .execute(
            "UPDATE account_refs SET active = 0 WHERE id = ?1",
            [&inactive_id],
        )
        .unwrap();
    drop(connection);
    platform
        .creators
        .sync_account_ref_with_state(
            AccountRefId::new(unavailable_id.clone()),
            Platform::Telegram,
            AccountKind::Profile,
            "unavailable-operator",
            AccountAvailability::Unavailable,
        )
        .unwrap();

    for selector in [
        PlatformConnectionSelector::OperatorAccount {
            platform: OperatorAccountPlatform::Instagram,
            operator_account_id: inactive_id,
        },
        PlatformConnectionSelector::OperatorAccount {
            platform: OperatorAccountPlatform::Telegram,
            operator_account_id: unavailable_id,
        },
    ] {
        assert_eq!(
            platform.resolve_platform_connection(selector).await,
            Err(PlatformConnectionResolutionError::Unavailable)
        );
    }

    cleanup(platform, directory);
}

#[tokio::test]
async fn rejects_locally_unavailable_youtube_credentials_with_secret_free_errors() {
    const SECRET: &str = "youtube-secret-must-not-leak";
    const OWNER_ERROR: &str = "provider error includes youtube-secret-must-not-leak";
    let (platform, directory) = platform();
    let credential_id = youtube_credential(&platform, "Unavailable YouTube", SECRET);
    let parsed_id = uuid::Uuid::parse_str(&credential_id).unwrap();
    platform
        .vault
        .mark_service_credential_failure(parsed_id, OWNER_ERROR, true, None)
        .unwrap();

    let error = platform
        .resolve_platform_connection(PlatformConnectionSelector::YoutubeCredential {
            credential_id,
        })
        .await
        .unwrap_err();
    assert_eq!(error, PlatformConnectionResolutionError::Unavailable);
    let public_error = format!("{error:?}: {}", error.safe_message());
    assert!(!public_error.contains(SECRET));
    assert!(!public_error.contains(OWNER_ERROR));

    cleanup(platform, directory);
}
