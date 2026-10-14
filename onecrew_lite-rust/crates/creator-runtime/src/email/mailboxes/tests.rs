use crate::{
    CreatorChangeScope, CreatorGateways, CreatorRepositoryDispatchFailure, CreatorRuntime,
    EmailAuthConnectionId, EmailCapability, EmailProviderKind, EmailSecretRecordId,
    MailboxIdentityMappingInput, MailboxProjectionKind, MailboxRefId, MailboxRefInput,
};

fn mailbox_runtime() -> (tempfile::TempDir, CreatorRuntime, MailboxRefId) {
    let directory = tempfile::tempdir().unwrap();
    let runtime = CreatorRuntime::open(directory.path(), CreatorGateways::default()).unwrap();
    let mailbox = MailboxRefId::new("11111111-1111-4111-8111-111111111111");
    runtime
        .sync_mailbox_ref(MailboxRefInput {
            id: mailbox.clone(),
            provider: "resend".into(),
            label: "Mailbox fixture".into(),
            email_address: "sender@example.com".into(),
            display_name: Some("Sender".into()),
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .unwrap();
    runtime
        .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: mailbox.clone(),
            auth_connection_id: EmailAuthConnectionId::new("auth-fixture"),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new("secret-fixture"),
            connection_label: "Connection fixture".into(),
            projection_kind: MailboxProjectionKind::Mailbox,
            capability_projection_revision: 1,
            checkpoint_namespace: "checkpoint-fixture".into(),
        })
        .unwrap();
    (directory, runtime, mailbox)
}

#[test]
fn mailbox_projection_failure_rolls_back_before_publishing_a_commit() {
    let (directory, runtime, mailbox) = mailbox_runtime();
    let connection = rusqlite::Connection::open(directory.path().join("crm.db")).unwrap();
    // SMTP is retained by the storage format but unsupported by the current
    // public mailbox projection. It makes the existing projection fail after
    // the reconciliation has already attempted its writes.
    connection.execute(
        "INSERT INTO mailbox_refs(id,provider,label,email_address,capabilities_json,available,created_at,updated_at)
         VALUES ('22222222-2222-4222-8222-222222222222','smtp','Retained mailbox','old@example.com','[]',1,1,1)",
        [],
    ).unwrap();
    let mut receiver = runtime.subscribe_changes();
    let result = runtime.reconcile_mailbox_refs(vec![MailboxRefInput {
        id: mailbox.clone(),
        provider: "resend".into(),
        label: "Must roll back".into(),
        email_address: "sender@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Send],
        available: true,
        last_error: None,
    }]);
    assert!(result.is_err());
    let label: String = connection
        .query_row(
            "SELECT label FROM mailbox_refs WHERE id=?1",
            [mailbox.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let available: bool = connection
        .query_row(
            "SELECT available FROM mailbox_refs WHERE provider='smtp'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(label, "Mailbox fixture");
    assert!(available);
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[test]
fn committed_mailbox_reconciliation_is_published_even_if_a_later_mapping_fails() {
    let (_directory, runtime, mailbox) = mailbox_runtime();
    let mut receiver = runtime.subscribe_changes();
    runtime
        .reconcile_mailbox_refs(vec![MailboxRefInput {
            id: mailbox.clone(),
            provider: "resend".into(),
            label: "Committed label".into(),
            email_address: "sender@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        }])
        .unwrap();
    assert_eq!(
        receiver.try_recv().unwrap().scopes,
        vec![CreatorChangeScope::EmailFolders]
    );
    assert!(
        runtime
            .upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
                mailbox_ref_id: mailbox,
                auth_connection_id: EmailAuthConnectionId::new("auth-fixture"),
                provider_kind: EmailProviderKind::Resend,
                secret_record_id: EmailSecretRecordId::new("conflicting-secret"),
                connection_label: "Rejected mapping".into(),
                projection_kind: MailboxProjectionKind::Mailbox,
                capability_projection_revision: 1,
                checkpoint_namespace: "checkpoint-fixture".into(),
            })
            .is_err()
    );
    assert_eq!(
        runtime.list_mailboxes().unwrap()[0].label,
        "Committed label"
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn admitted_mailbox_binding_matches_blocking_and_reports_current_availability() {
    let (_directory, runtime, mailbox) = mailbox_runtime();
    let binding = runtime
        .resolve_mailbox_auth_binding(&mailbox)
        .await
        .unwrap();
    assert_eq!(
        binding,
        runtime
            .resolve_mailbox_auth_binding_blocking(&mailbox)
            .unwrap()
    );
    assert_eq!(binding.email_address, "sender@example.com");
    assert_eq!(
        binding.auth_connection.secret_record_id.as_str(),
        "secret-fixture"
    );
    assert!(binding.available && binding.auth_connection.available);
    runtime.reconcile_mailbox_refs(Vec::new()).unwrap();
    let changed = runtime
        .resolve_mailbox_auth_binding(&mailbox)
        .await
        .unwrap();
    assert!(!changed.available || !changed.auth_connection.available);
    assert!(
        runtime
            .resolve_mailbox_auth_binding(&MailboxRefId::new("missing"))
            .await
            .is_err()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn mailbox_binding_waits_for_creator_admission_before_repository_dispatch() {
    let (_directory, runtime, mailbox) = mailbox_runtime();
    let mut releases = Vec::new();
    let mut workers = Vec::new();
    for _ in 0..crate::repository::SQLITE_READ_POOL_SIZE {
        let dispatch = runtime.repository_dispatch.clone();
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, blocked) = std::sync::mpsc::channel::<()>();
        workers.push(tokio::spawn(async move {
            dispatch
                .read(move || {
                    entered.send(()).unwrap();
                    assert!(
                        blocked.recv().is_err(),
                        "release closes the admission blocker"
                    );
                })
                .await
        }));
        started.await.unwrap();
        releases.push(release);
    }
    // The pool itself is idle. A direct synchronous read would succeed here,
    // while the real Creator admission is fully occupied by bounded workers.
    let result = runtime.resolve_mailbox_auth_binding(&mailbox).await;
    drop(releases);
    for worker in workers {
        worker.await.unwrap().unwrap();
    }
    assert_eq!(
        result.unwrap_err(),
        CreatorRepositoryDispatchFailure::Overloaded.to_string()
    );
    assert!(runtime.resolve_mailbox_auth_binding(&mailbox).await.is_ok());
}
