use crate::PlatformChangeScope;
use onecrew_creator_runtime::{AccountAvailability, AccountRefId};
use onecrew_local_domain::{ServiceCredentialSecret, ServiceProvider};

use crate::{
    LocalPlatform, YoutubeCredentialDeleteError, YoutubeCredentialDeleteInput,
    YoutubeCredentialRejectionIssue, YoutubePlatformTestError, YoutubePlatformTestResult,
};

#[test]
fn youtube_credential_test_results_publish_known_health_commits_even_when_reconciliation_fails() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let mut changes = platform.subscribe_changes();

    platform
        .finish_youtube_platform_test(
            Ok(YoutubePlatformTestResult {
                credential_id: "youtube-success".into(),
                ok: true,
                checked_at: 2,
                channel_id: Some("UC-test".into()),
                issue: None,
                message: "valid".into(),
            }),
            Ok(Vec::new()),
        )
        .unwrap();
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );

    let rejected = platform
        .finish_youtube_platform_test(
            Ok(YoutubePlatformTestResult {
                credential_id: "youtube-rejected".into(),
                ok: false,
                checked_at: 3,
                channel_id: None,
                issue: Some(YoutubeCredentialRejectionIssue::ApiKeyInvalid),
                message: "provider rejected".into(),
            }),
            Ok(Vec::new()),
        )
        .unwrap();
    assert!(!rejected.ok);
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );

    let error = platform
        .finish_youtube_platform_test(
            Ok(YoutubePlatformTestResult {
                credential_id: "youtube-reconcile-failed".into(),
                ok: true,
                checked_at: 3,
                channel_id: None,
                issue: None,
                message: "valid".into(),
            }),
            Err("Creator runtime unavailable".into()),
        )
        .unwrap_err();
    assert_eq!(error, YoutubePlatformTestError::Infrastructure);
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
    for error in [
        YoutubePlatformTestError::InvalidCredentialId,
        YoutubePlatformTestError::CredentialUnavailable,
        YoutubePlatformTestError::Infrastructure,
    ] {
        assert_eq!(
            platform.finish_youtube_platform_test(Err(error), Ok(Vec::new())),
            Err(error)
        );
    }
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    assert_eq!(
        platform.finish_youtube_platform_test(
            Err(YoutubePlatformTestError::RateLimited),
            Err("reconcile failed".into())
        ),
        Err(YoutubePlatformTestError::Infrastructure)
    );
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
}

#[test]
fn youtube_mcp_projection_sanitizes_historical_vault_errors() {
    const KEY: &str = "FAKE_YOUTUBE_SECRET_DO_NOT_LEAK";
    let directory = std::env::temp_dir().join(format!(
        "onecrew-youtube-sanitization-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).unwrap();
    let credential = platform
        .vault
        .upsert_service_credential(
            ServiceProvider::Youtube,
            "leak-test",
            serde_json::json!({"api": "youtube_data_api_v3"}),
            &ServiceCredentialSecret::new(KEY),
        )
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(
            credential.id,
            &format!("https://www.googleapis.com/youtube/v3/search?key={KEY}&part=id {KEY}"),
            true,
            None,
        )
        .unwrap();
    platform.reconcile_vault_youtube_refs().unwrap();

    let projected = platform.list_service_credentials().unwrap();
    let wire = serde_json::to_string(&projected).unwrap();
    assert!(!wire.contains(KEY));
    assert!(!wire.contains("?key="));
    assert!(!wire.contains("googleapis.com"));
    assert!(wire.contains("YouTube credential has a recorded failure"));
    assert!(!wire.contains("part=id"));

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn youtube_state_and_delete_reconcile_vault_and_historical_ref() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-youtube-reconcile-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).expect("platform");
    let credential = platform
        .put_youtube_api_key("Discovery", Some("project".into()), "AIza-test")
        .expect("credential");
    let account_ref_id = AccountRefId::new(credential.id.to_string());
    let creators = platform.creators().expect("creators");

    platform
        .vault
        .mark_service_credential_failure(credential.id, "quota cooling down", false, Some(u64::MAX))
        .expect("mark failure");
    // Reconciliation is an explicit lifecycle step now; the list itself is a
    // pure read of the vault.
    platform
        .reconcile_vault_youtube_refs()
        .expect("explicit reconciliation");
    let projected = platform
        .list_service_credentials()
        .expect("credential list");
    assert_eq!(projected.len(), 1);
    assert!(!projected[0].available);
    assert_eq!(
        creators
            .get_account_ref(&account_ref_id)
            .unwrap()
            .availability,
        AccountAvailability::Unavailable
    );

    // Replacing the key clears provider failure metadata and makes it
    // operable again before the explicit deletion lifecycle.
    platform
        .put_youtube_api_key("Discovery", Some("project".into()), "AIza-replaced")
        .expect("replace key");
    assert_eq!(
        creators
            .get_account_ref(&account_ref_id)
            .unwrap()
            .availability,
        AccountAvailability::Available
    );
    let input = YoutubeCredentialDeleteInput {
        credential_id: credential.id.to_string(),
        confirmed: true,
        idempotency_key: "delete-youtube-runtime".into(),
    };
    let mut changes = platform.subscribe_changes();
    let result = platform.delete_youtube_credential(input.clone()).unwrap();
    let change = changes.try_recv().expect("YouTube delete change");
    assert_eq!(change.scopes, vec![PlatformChangeScope::ServiceCredentials]);
    assert_eq!(result.credential_id, credential.id.to_string());
    // The public outcome names the credential and the replay flag only: no
    // Creator reference, no availability, and no binding impact.
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        serde_json::json!({
            "credential_id": credential.id.to_string(),
            "replayed": false
        })
    );
    assert!(
        platform
            .vault
            .get_service_credential(credential.id)
            .is_err()
    );
    assert!(platform.delete_youtube_credential(input).unwrap().replayed);
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    assert_eq!(
        creators
            .get_account_ref(&account_ref_id)
            .unwrap()
            .availability,
        AccountAvailability::Unavailable
    );
    let historical = platform.list_service_credentials().unwrap();
    assert_eq!(historical.len(), 1);
    assert_eq!(historical[0].account_ref_id, account_ref_id);
    assert!(!historical[0].active);
    assert!(!historical[0].available);
    assert!(
        historical[0]
            .last_error
            .as_deref()
            .unwrap()
            .contains("missing")
    );

    drop(creators);
    drop(platform);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn youtube_delete_input_is_strict_and_rejects_a_non_credential_id_before_claiming() {
    let credential_id = uuid::Uuid::new_v4().to_string();
    assert!(
        serde_json::from_value::<YoutubeCredentialDeleteInput>(serde_json::json!({
            "credential_id": credential_id,
            "confirmed": true,
            "idempotency_key": "strict-credential-id"
        }))
        .is_ok()
    );
    assert!(
        serde_json::from_value::<YoutubeCredentialDeleteInput>(serde_json::json!({
            "account_ref_id": credential_id,
            "confirmed": true,
            "idempotency_key": "retired-field"
        }))
        .is_err()
    );

    let validation_directory = tempfile::tempdir().unwrap();
    let validation_platform = LocalPlatform::open(validation_directory.path()).unwrap();
    assert_eq!(
        validation_platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id: uuid::Uuid::new_v4().to_string(),
                confirmed: false,
                idempotency_key: "not-confirmed".into(),
            })
            .unwrap_err(),
        YoutubeCredentialDeleteError::ConfirmationRequired
    );
    assert_eq!(
        validation_platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id: "not-a-uuid".into(),
                confirmed: true,
                idempotency_key: "invalid-id".into(),
            })
            .unwrap_err(),
        YoutubeCredentialDeleteError::InvalidCredentialId
    );
    assert_eq!(
        validation_platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id: uuid::Uuid::new_v4().to_string(),
                confirmed: true,
                idempotency_key: String::new(),
            })
            .unwrap_err(),
        YoutubeCredentialDeleteError::IdempotencyKeyInvalid
    );
    assert_eq!(
        validation_platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id: uuid::Uuid::new_v4().to_string(),
                confirmed: true,
                idempotency_key: "missing".into(),
            })
            .unwrap_err(),
        YoutubeCredentialDeleteError::NotFound
    );

    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let profile = platform
        .vault
        .upsert(
            onecrew_local_domain::Platform::X,
            "not_a_credential",
            &onecrew_local_domain::ProfileSecret {
                provider_account_id: "x-provider".into(),
                cookies: Vec::new(),
                x_chat_passcode: None,
                instagram_session_settings: None,
                telegram_api_id: None,
                telegram_api_hash: None,
                telegram_session: None,
                profile_name: "X".into(),
                browser_profile_dir: String::new(),
                fingerprint_seed: String::new(),
                proxy: None,
                proxy_egress: None,
            },
        )
        .unwrap();
    platform.sync_profile_account_ref(&profile).unwrap();
    let error = platform
        .delete_youtube_credential(YoutubeCredentialDeleteInput {
            credential_id: profile.id.to_string(),
            confirmed: true,
            idempotency_key: "wrong-kind".into(),
        })
        .unwrap_err();
    assert_eq!(error, YoutubeCredentialDeleteError::WrongKind);
    assert!(
        platform
            .creators
            .pending_service_credential_deletes()
            .unwrap()
            .is_empty()
    );
    assert!(platform.vault.get(profile.id).is_ok());
}

#[test]
fn youtube_delete_reports_typed_idempotency_conflict_for_another_credential() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let first = platform
        .put_youtube_api_key("First", None, "AIza-first")
        .unwrap();
    let second = platform
        .put_youtube_api_key("Second", None, "AIza-second")
        .unwrap();
    let key = "same-youtube-delete-key";
    platform
        .delete_youtube_credential(YoutubeCredentialDeleteInput {
            credential_id: first.id.to_string(),
            confirmed: true,
            idempotency_key: key.into(),
        })
        .unwrap();
    assert_eq!(
        platform
            .delete_youtube_credential(YoutubeCredentialDeleteInput {
                credential_id: second.id.to_string(),
                confirmed: true,
                idempotency_key: key.into(),
            })
            .unwrap_err(),
        YoutubeCredentialDeleteError::IdempotencyConflict
    );
    assert!(platform.vault.get_service_credential(second.id).is_ok());
}
