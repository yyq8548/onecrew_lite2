use crate::PlatformChangeScope;
use crate::credentials::services::{ResendCredentialValidation, YoutubeCredentialValidation};
use onecrew_creator_runtime::AccountRefId;
use onecrew_local_domain::ServiceProvider;

use super::{
    ServiceCredentialReconciler, ServiceCredentialValidator, connect_reconciliation_error,
};
use crate::{
    LocalPlatform, RenameYoutubeCredentialInput, ResendCredentialConnectInput,
    YoutubeApiKeyConnectInput,
};

struct NoopCredentialReconciler;

impl ServiceCredentialReconciler for NoopCredentialReconciler {
    fn reconcile_youtube(&self, _platform: &LocalPlatform) -> Result<(), String> {
        Ok(())
    }
    fn reconcile_resend(&self, _platform: &LocalPlatform) -> Result<(), String> {
        Ok(())
    }
}

struct FakeCredentialValidator {
    youtube: Result<YoutubeCredentialValidation, String>,
    resend: Result<ResendCredentialValidation, String>,
}

#[async_trait::async_trait]
impl ServiceCredentialValidator for FakeCredentialValidator {
    async fn validate_youtube(
        &self,
        _api_key: &str,
    ) -> Result<YoutubeCredentialValidation, String> {
        self.youtube.clone()
    }

    async fn validate_resend(
        &self,
        _email_address: &str,
        _display_name: Option<&str>,
        _api_key: &str,
    ) -> Result<ResendCredentialValidation, String> {
        self.resend.clone()
    }
}

struct FailingCredentialReconciler {
    youtube_error: String,
    resend_error: String,
}

#[tokio::test]
async fn failed_credential_rollback_invalidates_the_committed_vault_row() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let connection =
        rusqlite::Connection::open(directory.path().join("agent-platform.db")).unwrap();
    connection.execute_batch("CREATE TRIGGER fail_credential_rollback BEFORE DELETE ON service_credentials BEGIN SELECT RAISE(ABORT, 'forced rollback failure'); END;").unwrap();
    let mut changes = platform.subscribe_changes();
    let error = platform
        .connect_youtube_api_key_with_dependencies(
            YoutubeApiKeyConnectInput {
                label: "Committed candidate".into(),
                project: None,
                api_key: "fake-offline-key".into(),
            },
            &successful_validator(),
            &FailingCredentialReconciler {
                youtube_error: "forced reconcile failure".into(),
                resend_error: "unused".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(error.contains("rollback failed"));
    assert!(!error.contains("rolled back"));
    assert_eq!(
        platform
            .vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

impl ServiceCredentialReconciler for FailingCredentialReconciler {
    fn reconcile_youtube(&self, _platform: &LocalPlatform) -> Result<(), String> {
        Err(self.youtube_error.clone())
    }

    fn reconcile_resend(&self, _platform: &LocalPlatform) -> Result<(), String> {
        Err(self.resend_error.clone())
    }
}

fn successful_validator() -> FakeCredentialValidator {
    FakeCredentialValidator {
        youtube: Ok(YoutubeCredentialValidation {
            checked_at: 42,
            channel_id: Some("UC-test".into()),
            message: "YouTube key is valid".into(),
        }),
        resend: Ok(ResendCredentialValidation {
            checked_at: 43,
            email_address: "sender@example.com".into(),
            display_name: Some("Sender".into()),
            sender_domain: "example.com".into(),
            sender_domain_verified: true,
            message: "Resend key, receiving access and sender domain are valid".into(),
        }),
    }
}

#[test]
fn credential_connection_inputs_redact_keys_from_debug_and_json() {
    const KEY: &str = "SECRET_MUST_NOT_APPEAR";
    let youtube: YoutubeApiKeyConnectInput = serde_json::from_value(serde_json::json!({
        "label": "Discovery",
        "project": "project",
        "api_key": KEY
    }))
    .unwrap();
    let resend: ResendCredentialConnectInput = serde_json::from_value(serde_json::json!({
        "label": "Mailbox",
        "email_address": "sender@example.com",
        "display_name": "Sender",
        "api_key": KEY
    }))
    .unwrap();
    for wire in [
        format!("{youtube:?}"),
        serde_json::to_string(&youtube).unwrap(),
        format!("{resend:?}"),
        serde_json::to_string(&resend).unwrap(),
    ] {
        assert!(!wire.contains(KEY), "secret leaked in {wire}");
    }
    let youtube_schema =
        serde_json::to_string(&schemars::schema_for!(YoutubeApiKeyConnectInput)).unwrap();
    let resend_schema =
        serde_json::to_string(&schemars::schema_for!(ResendCredentialConnectInput)).unwrap();
    assert!(youtube_schema.contains("api_key"));
    assert!(resend_schema.contains("api_key"));
    let combined = connect_reconciliation_error(
        &format!("reconcile exposed {KEY}"),
        Some(&format!("rollback exposed {KEY}")),
        KEY,
    );
    assert!(combined.contains("credential reconciliation failed"));
    assert!(combined.contains("credential rollback failed"));
    assert!(!combined.contains(KEY));
}

#[tokio::test]
async fn youtube_validation_failure_preserves_same_label_credential_and_creator_ref() {
    const OLD_KEY: &str = "AIza-old-good";
    const CANDIDATE_KEY: &str = "AIza-invalid-candidate";
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let old = platform
        .put_youtube_api_key("Discovery", Some("old-project".into()), OLD_KEY)
        .unwrap();
    let account_ref_id = AccountRefId::new(old.id.to_string());
    let before = platform.creators.get_account_ref(&account_ref_id).unwrap();
    let validator = FakeCredentialValidator {
        youtube: Err(format!("provider rejected key {CANDIDATE_KEY}")),
        ..successful_validator()
    };
    let mut changes = platform.subscribe_changes();

    let error = platform
        .connect_youtube_api_key_with_validator(
            YoutubeApiKeyConnectInput {
                label: "Discovery".into(),
                project: Some("new-project".into()),
                api_key: CANDIDATE_KEY.into(),
            },
            &validator,
        )
        .await
        .unwrap_err();
    assert!(!error.contains(CANDIDATE_KEY));
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let (stored, secret) = platform.vault.get_service_credential(old.id).unwrap();
    assert_eq!(stored.metadata["project"], "old-project");
    assert_eq!(secret.expose_secret(), OLD_KEY);
    let after = platform.creators.get_account_ref(&account_ref_id).unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.platform, before.platform);
    assert_eq!(after.kind, before.kind);
    assert_eq!(after.label, before.label);
    assert_eq!(after.active, before.active);
    assert_eq!(after.availability, before.availability);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
}

#[tokio::test]
async fn resend_validation_failure_preserves_same_label_credential_and_mailbox_ref() {
    const OLD_KEY: &str = "re_old_good";
    const CANDIDATE_KEY: &str = "re_invalid_candidate";
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let old = platform
        .put_resend_credential(
            "Mailbox",
            "old@example.com",
            Some("Old Sender".into()),
            OLD_KEY,
        )
        .unwrap();
    let before = platform.creators.list_mailboxes().unwrap();
    let validator = FakeCredentialValidator {
        resend: Err(format!("provider rejected key {CANDIDATE_KEY}")),
        ..successful_validator()
    };
    let mut changes = platform.subscribe_changes();

    let error = platform
        .connect_resend_credential_with_validator(
            ResendCredentialConnectInput {
                label: "Mailbox".into(),
                email_address: "new@example.com".into(),
                display_name: Some("New Sender".into()),
                api_key: CANDIDATE_KEY.into(),
            },
            &validator,
        )
        .await
        .unwrap_err();
    assert!(!error.contains(CANDIDATE_KEY));
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    let (stored, secret) = platform
        .credential_services
        .get_resend_credential(&old.id.to_string())
        .unwrap();
    assert_eq!(stored.metadata["email_address"], "old@example.com");
    assert_eq!(secret.expose_secret(), OLD_KEY);
    let after = platform.creators.list_mailboxes().unwrap();
    assert_eq!(after.len(), before.len());
    let before = &before[0];
    let after = &after[0];
    assert_eq!(after.mailbox_ref_id, before.mailbox_ref_id);
    assert_eq!(after.provider, before.provider);
    assert_eq!(after.label, before.label);
    assert_eq!(after.email_address, before.email_address);
    assert_eq!(after.display_name, before.display_name);
    assert_eq!(after.capabilities, before.capabilities);
    assert_eq!(after.available, before.available);
    assert_eq!(after.issues, before.issues);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
}

#[tokio::test]
async fn youtube_reconcile_failure_restores_old_snapshot_and_removes_new_candidate() {
    const OLD_KEY: &str = "AIza-old-good";
    const CANDIDATE_KEY: &str = "AIza-candidate-secret";
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let old = platform
        .put_youtube_api_key("Discovery", Some("old-project".into()), OLD_KEY)
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(old.id, "old health", true, Some(u64::MAX))
        .unwrap();
    let (before, before_secret) = platform.vault.get_service_credential(old.id).unwrap();
    let reconciler = FailingCredentialReconciler {
        youtube_error: format!("Creator runtime rejected candidate {CANDIDATE_KEY}"),
        resend_error: "unused".into(),
    };
    let mut changes = platform.subscribe_changes();

    let error = platform
        .rotate_youtube_credential_with_dependencies(
            crate::RotateYoutubeCredentialInput {
                credential_id: old.id.to_string(),
                api_key: CANDIDATE_KEY.into(),
                confirmed: true,
            },
            &successful_validator(),
            &reconciler,
        )
        .await
        .unwrap_err();
    assert!(error.contains("rolled back"));
    assert!(!error.contains(CANDIDATE_KEY));
    let (after, after_secret) = platform.vault.get_service_credential(old.id).unwrap();
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(after.disabled, before.disabled);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.last_used_at, before.last_used_at);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after_secret.expose_secret(), before_secret.expose_secret());

    let error = platform
        .connect_youtube_api_key_with_dependencies(
            YoutubeApiKeyConnectInput {
                label: "New Discovery".into(),
                project: None,
                api_key: CANDIDATE_KEY.into(),
            },
            &successful_validator(),
            &reconciler,
        )
        .await
        .unwrap_err();
    assert!(error.contains("rolled back"));
    assert!(
        platform
            .vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .unwrap()
            .iter()
            .all(|credential| credential.label != "New Discovery")
    );
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn resend_reconcile_failure_restores_old_snapshot_and_removes_new_candidate() {
    const OLD_KEY: &str = "re_old_good";
    const CANDIDATE_KEY: &str = "re_candidate_secret";
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let old = platform
        .put_resend_credential("Mailbox", "old@example.com", Some("Old".into()), OLD_KEY)
        .unwrap();
    platform
        .vault
        .mark_email_secret_record_failure(
            old.id,
            onecrew_local_vault::EmailSecretRecordKind::ResendApiKey,
            "old health",
            true,
            Some(u64::MAX),
        )
        .unwrap();
    let (before, before_secret) = platform
        .credential_services
        .get_resend_credential(&old.id.to_string())
        .unwrap();
    let reconciler = FailingCredentialReconciler {
        youtube_error: "unused".into(),
        resend_error: format!("Creator runtime rejected candidate {CANDIDATE_KEY}"),
    };

    let error = platform
        .rotate_resend_credential_with_dependencies(
            crate::RotateResendCredentialInput {
                credential_id: old.id.to_string(),
                api_key: CANDIDATE_KEY.into(),
                confirmed: true,
            },
            &successful_validator(),
            &reconciler,
        )
        .await
        .unwrap_err();
    assert!(error.contains("rolled back"), "{error}");
    assert!(!error.contains(CANDIDATE_KEY));
    let (after, after_secret) = platform
        .credential_services
        .get_resend_credential(&old.id.to_string())
        .unwrap();
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(after.disabled, before.disabled);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.last_used_at, before.last_used_at);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after.created_at, before.created_at);
    assert_eq!(after.updated_at, before.updated_at);
    assert_eq!(after_secret.expose_secret(), before_secret.expose_secret());

    let error = platform
        .connect_resend_credential_with_dependencies(
            ResendCredentialConnectInput {
                label: "New Mailbox".into(),
                email_address: "sender@example.com".into(),
                display_name: None,
                api_key: CANDIDATE_KEY.into(),
            },
            &successful_validator(),
            &reconciler,
        )
        .await
        .unwrap_err();
    assert!(error.contains("rolled back"), "{error}");
    assert!(
        platform
            .vault
            .list_service_credentials(ServiceProvider::Resend, false)
            .unwrap()
            .iter()
            .all(|credential| credential.label != "New Mailbox")
    );
}

#[tokio::test]
async fn validated_connections_persist_and_return_only_safe_projections() {
    const YOUTUBE_KEY: &str = "AIza-new-secret";
    const RESEND_KEY: &str = "re_new_secret";
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let validator = successful_validator();
    let mut changes = platform.subscribe_changes();

    let youtube = platform
        .connect_youtube_api_key_with_validator(
            YoutubeApiKeyConnectInput {
                label: "Discovery".into(),
                project: Some("project".into()),
                api_key: YOUTUBE_KEY.into(),
            },
            &validator,
        )
        .await
        .unwrap();
    let youtube_change = changes.try_recv().expect("YouTube connection change");
    assert_eq!(
        youtube_change.scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
    let mut creator_changes = platform.creators.subscribe_changes();
    let resend = platform
        .connect_resend_credential_with_validator(
            ResendCredentialConnectInput {
                label: "Mailbox".into(),
                email_address: "sender@example.com".into(),
                display_name: Some("Sender".into()),
                api_key: RESEND_KEY.into(),
            },
            &validator,
        )
        .await
        .unwrap();
    let resend_change = changes.try_recv().expect("Resend connection change");
    assert_eq!(
        resend_change.scopes,
        vec![PlatformChangeScope::ServiceCredentials,]
    );
    assert_eq!(
        creator_changes
            .try_recv()
            .expect("Creator mailbox commit")
            .scopes,
        vec![onecrew_creator_runtime::CreatorChangeScope::EmailFolders]
    );

    let wire = serde_json::to_string(&(youtube.clone(), resend.clone())).unwrap();
    assert!(!wire.contains(YOUTUBE_KEY));
    assert!(!wire.contains(RESEND_KEY));
    assert!(youtube.credential.available);
    assert!(resend.credential.available);
    assert_eq!(
        youtube.health.provider_resource_id.as_deref(),
        Some("UC-test")
    );
    let youtube_id = uuid::Uuid::parse_str(&youtube.credential.credential_id).unwrap();
    let resend_id = uuid::Uuid::parse_str(&resend.credential.credential_id).unwrap();
    assert_eq!(
        platform
            .vault
            .get_service_credential(youtube_id)
            .unwrap()
            .1
            .expose_secret(),
        YOUTUBE_KEY
    );
    assert_eq!(
        platform
            .credential_services
            .get_resend_credential(&resend_id.to_string())
            .unwrap()
            .1
            .expose_secret(),
        RESEND_KEY
    );
}

#[tokio::test]
async fn connect_rejects_an_existing_label_and_rotate_replaces_the_key_in_place() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let first = platform
        .connect_youtube_api_key_with_dependencies(
            YoutubeApiKeyConnectInput {
                label: "Discovery".into(),
                project: Some("project-a".into()),
                api_key: "AIza_first_key".into(),
            },
            &successful_validator(),
            &NoopCredentialReconciler,
        )
        .await
        .expect("first connect");
    assert_eq!(
        first
            .credential
            .youtube
            .as_ref()
            .and_then(|details| details.project.as_deref()),
        Some("project-a"),
        "connect result carries typed provider details"
    );

    let conflict = platform
        .connect_youtube_api_key_with_dependencies(
            YoutubeApiKeyConnectInput {
                label: "Discovery".into(),
                project: None,
                api_key: "AIza_second_key".into(),
            },
            &successful_validator(),
            &NoopCredentialReconciler,
        )
        .await
        .expect_err("same label must conflict instead of silently replacing");
    assert!(conflict.contains("already exists"), "{conflict}");
    assert!(conflict.contains("rotate"), "{conflict}");
    assert!(!conflict.contains("AIza_second_key"), "{conflict}");

    let unconfirmed = platform
        .rotate_youtube_credential_with_dependencies(
            crate::RotateYoutubeCredentialInput {
                credential_id: first.credential.credential_id.clone(),
                api_key: "AIza_rotated_key".into(),
                confirmed: false,
            },
            &successful_validator(),
            &NoopCredentialReconciler,
        )
        .await
        .expect_err("rotation requires confirmation");
    assert!(unconfirmed.contains("confirmation"), "{unconfirmed}");

    let rotated = platform
        .rotate_youtube_credential_with_dependencies(
            crate::RotateYoutubeCredentialInput {
                credential_id: first.credential.credential_id.clone(),
                api_key: "AIza_rotated_key".into(),
                confirmed: true,
            },
            &successful_validator(),
            &NoopCredentialReconciler,
        )
        .await
        .expect("rotation succeeds");
    assert_eq!(
        rotated.credential.credential_id,
        first.credential.credential_id
    );
    assert_eq!(rotated.credential.label, "Discovery");
    assert_eq!(
        rotated
            .credential
            .youtube
            .as_ref()
            .and_then(|details| details.project.as_deref()),
        Some("project-a"),
        "rotation keeps the stored project"
    );
    let credential_id = uuid::Uuid::parse_str(&first.credential.credential_id).unwrap();
    let (_, secret) = platform
        .vault
        .get_service_credential(credential_id)
        .unwrap();
    assert_eq!(secret.expose_secret(), "AIza_rotated_key");

    let wrong_provider = platform
        .rotate_resend_credential_with_dependencies(
            crate::RotateResendCredentialInput {
                credential_id: first.credential.credential_id.clone(),
                api_key: "re_key".into(),
                confirmed: true,
            },
            &successful_validator(),
            &NoopCredentialReconciler,
        )
        .await
        .expect_err("provider mismatch must fail");
    assert!(
        wrong_provider.contains("not a Resend credential"),
        "{wrong_provider}"
    );
}

#[test]
fn youtube_rename_preserves_stable_id_secret_and_health_and_updates_creator_ref() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let credential = platform
        .put_youtube_api_key("Discovery", Some("project".into()), "AIza-rename-secret")
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(credential.id, "old health", true, Some(u64::MAX))
        .unwrap();
    platform.reconcile_vault_youtube_refs().unwrap();
    let account_ref_id = AccountRefId::new(credential.id.to_string());
    let before = platform.creators.get_account_ref(&account_ref_id).unwrap();
    let (before_credential, before_secret) = platform
        .vault
        .get_service_credential(credential.id)
        .unwrap();

    let renamed = platform
        .rename_youtube_credential(RenameYoutubeCredentialInput {
            credential_id: credential.id.to_string(),
            label: "  Renamed Discovery  ".into(),
            confirmed: true,
        })
        .unwrap();
    let (after_credential, after_secret) = platform
        .vault
        .get_service_credential(credential.id)
        .unwrap();
    let after = platform.creators.get_account_ref(&account_ref_id).unwrap();
    assert_eq!(renamed.account_ref_id.as_str(), credential.id.to_string());
    assert_eq!(renamed.label, "Renamed Discovery");
    assert_eq!(after_credential.id, before_credential.id);
    assert_eq!(after_credential.label, "Renamed Discovery");
    assert_eq!(after_credential.metadata, before_credential.metadata);
    assert_eq!(after_credential.disabled, before_credential.disabled);
    assert_eq!(
        after_credential.cooldown_until,
        before_credential.cooldown_until
    );
    assert_eq!(after_credential.last_error, before_credential.last_error);
    assert_eq!(after_secret.expose_secret(), before_secret.expose_secret());
    assert_eq!(after.id, before.id);
    assert_eq!(after.label, "Renamed Discovery");
    assert_eq!(after.availability, before.availability);
}

#[test]
fn youtube_rename_rejects_unconfirmed_and_duplicate_labels_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let source = platform
        .put_youtube_api_key("Source", None, "AIza-source-secret")
        .unwrap();
    platform
        .put_youtube_api_key("Taken", None, "AIza-taken-secret")
        .unwrap();

    let unconfirmed = platform
        .rename_youtube_credential(RenameYoutubeCredentialInput {
            credential_id: source.id.to_string(),
            label: "New Source".into(),
            confirmed: false,
        })
        .unwrap_err();
    assert!(unconfirmed.contains("confirmation"));

    let duplicate = platform
        .rename_youtube_credential(RenameYoutubeCredentialInput {
            credential_id: source.id.to_string(),
            label: "Taken".into(),
            confirmed: true,
        })
        .unwrap_err();
    assert!(duplicate.contains("already exists"));
    let (stored, secret) = platform.vault.get_service_credential(source.id).unwrap();
    assert_eq!(stored.label, "Source");
    assert_eq!(secret.expose_secret(), "AIza-source-secret");
}

#[test]
fn youtube_rename_reconcile_failure_restores_old_vault_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let credential = platform
        .put_youtube_api_key("Discovery", Some("project".into()), "AIza-rename-secret")
        .unwrap();
    platform
        .vault
        .mark_service_credential_failure(credential.id, "old health", true, Some(u64::MAX))
        .unwrap();
    let (before, before_secret) = platform
        .vault
        .get_service_credential(credential.id)
        .unwrap();
    let reconciler = FailingCredentialReconciler {
        youtube_error: "Creator runtime rejected renamed label".into(),
        resend_error: "unused".into(),
    };

    let error = platform
        .rename_youtube_credential_with_dependencies(
            RenameYoutubeCredentialInput {
                credential_id: credential.id.to_string(),
                label: "Renamed Discovery".into(),
                confirmed: true,
            },
            &reconciler,
        )
        .unwrap_err();
    assert!(error.contains("reconciliation failed"));
    let (after, after_secret) = platform
        .vault
        .get_service_credential(credential.id)
        .unwrap();
    assert_eq!(after.id, before.id);
    assert_eq!(after.label, before.label);
    assert_eq!(after.metadata, before.metadata);
    assert_eq!(after.disabled, before.disabled);
    assert_eq!(after.cooldown_until, before.cooldown_until);
    assert_eq!(after.last_error, before.last_error);
    assert_eq!(after_secret.expose_secret(), before_secret.expose_secret());
    assert!(error.contains("vault rename rolled back"));
    assert!(error.contains("Creator reference rollback failed"));
    assert!(!error.contains("Creator reference rollback completed"));
}

#[test]
fn youtube_rename_rollback_diagnostics_only_claim_attempted_outcomes() {
    let vault_failure =
        super::rename_reconciliation_error("provider sync failed", Some("disk full"), false, None);
    assert!(vault_failure.contains("vault rollback failed: disk full"));
    assert!(vault_failure.contains("Creator reference rollback was not attempted"));
    assert!(!vault_failure.contains("vault rename rolled back"));
    assert!(!vault_failure.contains("Creator reference rollback completed"));

    let creator_success =
        super::rename_reconciliation_error("provider sync failed", None, true, None);
    assert!(creator_success.contains("vault rename rolled back"));
    assert!(creator_success.contains("Creator reference rollback completed"));
    assert!(!creator_success.contains("rollback failed"));

    let creator_failure = super::rename_reconciliation_error(
        "provider sync failed",
        None,
        true,
        Some("database unavailable"),
    );
    assert!(creator_failure.contains("vault rename rolled back"));
    assert!(creator_failure.contains("Creator reference rollback failed: database unavailable"));
    assert!(!creator_failure.contains("Creator reference rollback completed"));
}
