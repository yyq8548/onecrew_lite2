use crate::PlatformChangeScope;
use crate::credentials::services::{ResendCredentialTest, ResendCredentialTestError};
use onecrew_creator_runtime::{
    EmailCapability, MailboxIdentityMappingInput, MailboxIssue, MailboxProjectionKind,
    MailboxRefId, MailboxRefInput, SaveEmailDraftInput,
};
use onecrew_local_vault::{EmailSecretRecordKind, EmailSecretRecordSecret};

use super::acquire_resend_credential_deletion_lock;
use crate::{LocalPlatform, ResendCredentialDeletionError};
use onecrew_creator_runtime::resend_auth_connection_id;

#[test]
fn resend_credential_test_results_distinguish_committed_health_from_preflight_failure() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let mut changes = platform.subscribe_changes();

    platform
        .finish_resend_credential_test(
            Ok(ResendCredentialTest {
                credential_id: "resend-success".into(),
                ok: true,
                checked_at: 1,
                email_address: "sender@example.com".into(),
                message: "valid".into(),
            }),
            Ok(Vec::new()),
        )
        .unwrap();
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );

    assert_eq!(
        platform
            .finish_resend_credential_test(
                Err(ResendCredentialTestError::MailboxUnavailable(
                    "resend rejected".into()
                )),
                Ok(Vec::new())
            )
            .unwrap_err()
            .to_string(),
        "resend rejected"
    );
    assert!(matches!(
        changes.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    platform
        .finish_resend_credential_test(
            Ok(ResendCredentialTest {
                credential_id: "committed".into(),
                ok: true,
                checked_at: 2,
                email_address: "sender@example.com".into(),
                message: "valid".into(),
            }),
            Err("reconcile failed".into()),
        )
        .unwrap_err();
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
    platform
        .finish_resend_credential_test(
            Err(ResendCredentialTestError::Provider(
                onecrew_email_contracts::EmailError::Transport("offline".into()),
            )),
            Err("reconcile failed".into()),
        )
        .unwrap_err();
    assert_eq!(
        changes.try_recv().unwrap().scopes,
        vec![PlatformChangeScope::ServiceCredentials]
    );
}

#[test]
fn resend_credential_deletion_workflows_are_serialized_by_an_os_lock() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-mailbox-delete-lock-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let first = acquire_resend_credential_deletion_lock(&directory).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (acquired_tx, acquired_rx) = std::sync::mpsc::channel();
    let second_directory = directory.clone();
    let thread = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _second = acquire_resend_credential_deletion_lock(&second_directory).unwrap();
        acquired_tx.send(()).unwrap();
    });
    started_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    assert_eq!(
        acquired_rx.recv_timeout(std::time::Duration::from_millis(100)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    );
    drop(first);
    acquired_rx
        .recv_timeout(std::time::Duration::from_secs(1))
        .unwrap();
    thread.join().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn resend_credential_paths_reconcile_the_complete_vault_snapshot() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-mailbox-reconcile-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).expect("platform");
    let first = platform
        .put_resend_credential(
            "First",
            "first@example.com",
            Some("First sender".into()),
            "re_first",
        )
        .expect("first credential");
    let second = platform
        .put_resend_credential("Second", "second@example.com", None, "re_second")
        .expect("second credential");
    let binding = platform
        .creators
        .resolve_mailbox_auth_binding_blocking(&MailboxRefId::new(first.id.to_string()))
        .expect("fresh Resend connect creates typed mailbox binding");
    assert_eq!(
        binding.auth_connection.provider_kind,
        onecrew_creator_runtime::EmailProviderKind::Resend
    );
    assert_eq!(
        binding.auth_connection.secret_record_id.as_str(),
        first.id.to_string()
    );
    assert_eq!(binding.email_address, "first@example.com");
    platform
        .vault
        .mark_email_secret_record_failure(
            second.id,
            EmailSecretRecordKind::ResendApiKey,
            "temporarily unavailable",
            true,
            None,
        )
        .expect("disable second credential");
    let generic_error = platform
        .vault
        .delete_service_credential(second.id)
        .expect_err("generic credential API must reject typed Email rows");
    assert!(
        generic_error
            .to_string()
            .contains("typed Email secret records require the dedicated vault API")
    );

    let creators = platform.creators().expect("creators");
    creators
        .sync_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("33333333-3333-4333-8333-333333333333"),
            provider: "resend".into(),
            label: "Stale".into(),
            email_address: "stale@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox],
            available: false,
            last_error: Some("deleted credential".into()),
        })
        .expect("stale projection");
    assert_eq!(creators.list_mailboxes().expect("mailboxes").len(), 3);

    // Reconciliation is an explicit lifecycle step now; the list itself is a
    // pure read of the vault.
    platform
        .reconcile_vault_mailbox_refs()
        .expect("explicit reconciliation");
    let credentials = platform.list_resend_credentials().expect("credential list");
    assert_eq!(credentials.len(), 2);
    let mailboxes = creators.list_mailboxes().expect("reconciled mailboxes");
    assert_eq!(mailboxes.len(), 3);
    let stale = mailboxes
        .iter()
        .find(|mailbox| mailbox.mailbox_ref_id.as_str() == "33333333-3333-4333-8333-333333333333")
        .expect("stale mailbox remains as unavailable history");
    assert!(!stale.available);
    assert_eq!(
        stale.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );
    let disabled = mailboxes
        .iter()
        .find(|mailbox| mailbox.mailbox_ref_id.as_str() == second.id.to_string())
        .expect("disabled mailbox remains projected");
    assert!(!disabled.available);
    assert_eq!(
        disabled.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );

    assert!(
        platform
            .delete_resend_credential(crate::ResendCredentialDeleteInput {
                credential_id: first.id.to_string().clone(),
                confirmed: true,
            })
            .expect("delete credential")
    );
    let remaining = creators
        .list_mailboxes()
        .expect("remaining mailbox projection");
    assert_eq!(remaining.len(), 3);
    let deleted = remaining
        .iter()
        .find(|mailbox| mailbox.mailbox_ref_id.as_str() == first.id.to_string())
        .expect("deleted credential mailbox remains as unavailable history");
    assert!(!deleted.available);
    assert_eq!(
        deleted.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );
    let disabled = remaining
        .iter()
        .find(|mailbox| mailbox.mailbox_ref_id.as_str() == second.id.to_string())
        .expect("disabled credential remains projected");
    assert!(!disabled.available);
    assert_eq!(
        disabled.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );

    drop(creators);
    drop(platform);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resend_credential_delete_is_typed_and_fail_closed_for_active_email_recovery() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-mailbox-delete-guard-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).expect("platform");
    let credential = platform
        .put_resend_credential(
            "Recovery mailbox",
            "recovery@example.com",
            None,
            "re_recovery",
        )
        .unwrap();
    let mailbox_id = credential.id.to_string();
    let secret_id = credential.id.to_string();
    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    connection
        .execute(
            "INSERT INTO idempotency_records
               (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                created_at, updated_at)
             VALUES ('active-recovery', 'manual_email_send', ?1, 'fingerprint',
                     'remote_started', '{}', 1, 1)",
            [&mailbox_id],
        )
        .unwrap();

    let error = platform
        .delete_resend_credential(crate::ResendCredentialDeleteInput {
            credential_id: secret_id.clone(),
            confirmed: true,
        })
        .unwrap_err();
    let ResendCredentialDeletionError::Blocked { blockers } = error else {
        panic!("expected typed deletion blocker")
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].mailbox_ref_id.as_str(), mailbox_id);
    assert_eq!(blockers[0].recoveries.len(), 1);
    assert!(blockers[0].sending_drafts.is_empty());
    assert!(
        platform
            .list_resend_credentials()
            .unwrap()
            .iter()
            .any(|candidate| candidate.id == credential.id)
    );
    connection
        .execute(
            "DELETE FROM idempotency_records WHERE key = 'active-recovery'",
            [],
        )
        .unwrap();
    let creators = platform.creators().unwrap();
    let draft = creators
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: MailboxRefId::new(mailbox_id.clone()),
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Preserve".into(),
            text: "Recovery required".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    connection
        .execute(
            "UPDATE email_drafts
             SET sending_idempotency_key = 'sending-draft', send_reserved_at = 1
             WHERE id = ?1",
            [draft.id.as_str()],
        )
        .unwrap();
    let error = platform
        .delete_resend_credential(crate::ResendCredentialDeleteInput {
            credential_id: secret_id.clone(),
            confirmed: true,
        })
        .unwrap_err();
    let ResendCredentialDeletionError::Blocked { blockers } = error else {
        panic!("expected sending draft blocker")
    };
    assert_eq!(blockers.len(), 1);
    assert!(blockers[0].recoveries.is_empty());
    assert_eq!(blockers[0].sending_drafts.len(), 1);

    connection
        .execute(
            "UPDATE email_drafts
             SET sending_idempotency_key = NULL, send_reserved_at = NULL
             WHERE id = ?1",
            [draft.id.as_str()],
        )
        .unwrap();
    assert!(
        platform
            .delete_resend_credential(crate::ResendCredentialDeleteInput {
                credential_id: secret_id.clone(),
                confirmed: true,
            })
            .unwrap()
    );
    drop(connection);
    drop(creators);
    drop(platform);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn same_label_recreation_does_not_rebind_an_old_mailbox_recovery() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-mailbox-label-recovery-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).expect("platform");
    let original = platform
        .put_resend_credential(
            "Recovery mailbox",
            "original@example.com",
            None,
            "re_original",
        )
        .unwrap();
    let original_id = original.id.to_string();
    let original_mailbox_id = original.id.to_string();

    // Simulate an out-of-band legacy removal. The supported deletion path
    // below must still bind recovery to the stable mailbox id, not label.
    assert!(
        platform
            .credential_services
            .delete_resend_credential(&original_id)
            .unwrap()
    );
    let replacement = platform
        .put_resend_credential(
            "Recovery mailbox",
            "replacement@example.com",
            None,
            "re_replacement",
        )
        .unwrap();
    assert_ne!(replacement.id, original.id);

    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    connection
        .execute(
            "INSERT INTO idempotency_records
               (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                created_at, updated_at)
             VALUES ('old-mailbox-recovery', 'manual_email_send', ?1, 'fingerprint',
                     'remote_started', '{}', 1, 1)",
            [&original_mailbox_id],
        )
        .unwrap();
    let error = platform
        .delete_resend_credential(crate::ResendCredentialDeleteInput {
            credential_id: original_id.clone(),
            confirmed: true,
        })
        .unwrap_err();
    let ResendCredentialDeletionError::Blocked { blockers } = error else {
        panic!("expected stable-id recovery blocker")
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].mailbox_ref_id.as_str(), original_mailbox_id);
    assert_eq!(blockers[0].recoveries.len(), 1);
    assert!(
        platform
            .list_resend_credentials()
            .unwrap()
            .iter()
            .any(|candidate| candidate.id == replacement.id)
    );

    drop(connection);
    drop(platform);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn resend_delete_claims_all_mailbox_projections_and_releases_all_guards() {
    let directory = std::env::temp_dir().join(format!(
        "onecrew-mailbox-multi-projection-delete-test-{}",
        uuid::Uuid::new_v4()
    ));
    let platform = LocalPlatform::open(&directory).expect("platform");
    let credential = platform
        .put_resend_credential("Shared Resend", "primary@example.com", None, "re_shared")
        .unwrap();
    let primary_mailbox = MailboxRefId::new(credential.id.to_string());
    let alias_mailbox = MailboxRefId::new(uuid::Uuid::new_v4().to_string());
    platform
        .creators
        .sync_mailbox_ref(MailboxRefInput {
            id: alias_mailbox.clone(),
            provider: "resend".into(),
            label: "Shared alias".into(),
            email_address: "alias@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .unwrap();
    let auth_connection_id = resend_auth_connection_id(credential.id);
    platform
        .creators
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: alias_mailbox.clone(),
            auth_connection_id: auth_connection_id.clone(),
            provider_kind: onecrew_creator_runtime::EmailProviderKind::Resend,
            secret_record_id: onecrew_creator_runtime::EmailSecretRecordId::new(
                credential.id.to_string(),
            ),
            connection_label: "Shared Resend".into(),
            projection_kind: MailboxProjectionKind::SendAs,
            capability_projection_revision: 1,
            checkpoint_namespace: "email-checkpoint-alias".into(),
        })
        .unwrap();

    let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
    connection
        .execute(
            "INSERT INTO idempotency_records
               (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                created_at, updated_at)
             VALUES ('alias-recovery', 'manual_email_send', ?1, 'fingerprint',
                     'remote_started', '{}', 1, 1)",
            [alias_mailbox.as_str()],
        )
        .unwrap();
    let error = platform
        .delete_resend_credential(crate::ResendCredentialDeleteInput {
            credential_id: credential.id.to_string(),
            confirmed: true,
        })
        .unwrap_err();
    let ResendCredentialDeletionError::Blocked { blockers } = error else {
        panic!("expected all-projection blocker")
    };
    assert_eq!(blockers.len(), 1);
    assert_eq!(blockers[0].mailbox_ref_id, alias_mailbox);

    connection
        .execute(
            "DELETE FROM idempotency_records WHERE key = 'alias-recovery'",
            [],
        )
        .unwrap();
    assert!(
        platform
            .delete_resend_credential(crate::ResendCredentialDeleteInput {
                credential_id: credential.id.to_string(),
                confirmed: true,
            })
            .unwrap()
    );
    let guard_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM mailbox_credential_deletion_guards
             WHERE mailbox_ref_id IN (?1, ?2)",
            rusqlite::params![primary_mailbox.as_str(), alias_mailbox.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(guard_count, 0, "all projection guards must be released");
    drop(connection);
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn explicit_vault_projection_reconciliation_surfaces_invalid_credentials() {
    let directory =
        std::env::temp_dir().join(format!("onecrew-projection-test-{}", uuid::Uuid::new_v4()));
    let platform = LocalPlatform::open(&directory).unwrap();
    platform
        .vault
        .upsert_email_secret_record(
            EmailSecretRecordKind::ResendApiKey,
            "invalid-mailbox",
            serde_json::json!({}),
            &EmailSecretRecordSecret::new("re_test"),
        )
        .unwrap();

    let reconciliation_error = match platform.reconcile_vault_projections() {
        Ok(()) => panic!("explicit reconciliation must reject an invalid projection"),
        Err(error) => error,
    };
    // Startup runs this same explicit lifecycle owner before exposing the
    // platform. The pure `creators()` accessor deliberately performs no validation
    // or write-back of an unrelated Vault snapshot.
    assert!(reconciliation_error.contains("email_address"));
    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}
