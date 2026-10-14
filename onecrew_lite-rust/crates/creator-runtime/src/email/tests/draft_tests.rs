use super::*;

#[test]
fn email_folder_counts_include_local_drafts() {
    let (_directory, runtime, _) = runtime(TestGateway::default());
    let mailbox = mailbox(&runtime, vec![EmailCapability::Send]);
    let mut changes = runtime.subscribe_changes();
    runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Count this draft".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let change = changes.try_recv().unwrap();
    assert!(change.scopes.contains(&CreatorChangeScope::EmailDrafts));
    assert!(change.scopes.contains(&CreatorChangeScope::EmailFolders));

    let folders = runtime
        .list_email_folders(ListEmailFoldersInput {
            mailbox_ref_id: Some(mailbox.mailbox_ref_id),
        })
        .unwrap();
    assert_eq!(
        folders
            .iter()
            .find(|summary| summary.folder == EmailFolder::Drafts)
            .map(|summary| summary.count),
        Some(1)
    );
}

#[tokio::test]
async fn draft_lifecycle_send_and_replay_use_verified_email_path() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            to: String::new(),
            creator_id: Some(creator_id.clone()),
            contact_route_id: Some(contact_route_id.clone()),
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(draft.revision, 1);
    assert_eq!(draft.state, EmailDraftState::Draft);
    let trashed = runtime
        .trash_email_draft(EmailDraftMutationInput {
            draft_id: draft.id.clone(),
            expected_revision: draft.revision,
        })
        .unwrap();
    assert_eq!(trashed.state, EmailDraftState::Trashed);
    assert_eq!(trashed.revision, 2);
    let stale_restore = runtime
        .restore_email_draft(EmailDraftMutationInput {
            draft_id: draft.id.clone(),
            expected_revision: 1,
        })
        .unwrap_err();
    assert!(stale_restore.contains("revision conflict"));
    let restored = runtime
        .restore_email_draft(EmailDraftMutationInput {
            draft_id: draft.id.clone(),
            expected_revision: trashed.revision,
        })
        .unwrap();
    assert_eq!(restored.state, EmailDraftState::Draft);
    assert_eq!(restored.revision, 3);

    let send = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: restored.revision,
        idempotency_key: "draft-send-1".into(),
    };
    let sent = runtime.send_email_draft(send.clone()).await.unwrap();
    assert!(!sent.replayed);
    assert!(sent.touchpoint.is_some());
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .idempotency_key,
        "draft-send-1"
    );
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .is_err()
    );
    assert!(
        runtime
            .list_email_drafts(ListEmailDraftsInput {
                mailbox_ref_id: None,
                cursor: None,
                state: None,
                limit: None,
            })
            .unwrap()
            .items
            .is_empty()
    );
    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "draft-send-1");
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime.send_email_draft(send).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.email.id, sent.email.id);
    let other_draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: String::new(),
            creator_id: Some(creator_id),
            contact_route_id: Some(contact_route_id),
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let reused_key = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: other_draft.id,
            expected_revision: other_draft.revision,
            idempotency_key: "draft-send-1".into(),
        })
        .await
        .unwrap_err();
    assert!(
        reused_key
            .message()
            .contains("another draft or draft revision")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn draft_remote_started_claim_and_reservation_are_atomic_and_restartable() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-atomic-claim".into(),
    };
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_draft_send_reservation
             BEFORE UPDATE OF sending_idempotency_key ON email_drafts
             WHEN NEW.sending_idempotency_key IS NOT NULL
             BEGIN SELECT RAISE(ABORT, 'draft reservation blocked'); END;",
        )
        .unwrap();

    let error = runtime.send_email_draft(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("draft reservation blocked"),
        "{error}"
    );
    assert!(action_state(&directory, "draft-atomic-claim").is_none());
    let unchanged = runtime
        .get_email_draft(EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    assert!(!unchanged.sending);
    assert!(unchanged.pending_send_idempotency_key.is_none());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    drop(runtime);

    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_draft_send_reservation;")
        .unwrap();
    let runtime = reopen_runtime(&directory, &gateway);
    let sent = runtime.send_email_draft(input).await.unwrap();
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn cancelled_started_draft_claim_persists_reservation_publish_and_same_key_readback() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-claim-cancelled-caller".into(),
    };
    let mut changes = runtime.subscribe_changes();
    let (claim_started, release_claim) = runtime.install_next_repository_write_barrier();
    let send_runtime = runtime.clone();
    let send_input = input.clone();
    let send = tokio::spawn(async move { send_runtime.send_email_draft(send_input).await });
    claim_started
        .await
        .expect("draft claim writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("draft claim blocked the current-thread runtime")
    .unwrap();
    send.abort();
    assert!(send.await.unwrap_err().is_cancelled());
    release_claim.send(()).unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
        .await
        .expect("draft reservation change was lost after caller cancellation")
        .unwrap();
    assert_eq!(
        change.scopes,
        vec![
            CreatorChangeScope::EmailDrafts,
            CreatorChangeScope::EmailFolders
        ]
    );
    assert_eq!(
        action_state(&directory, &input.idempotency_key).as_deref(),
        Some("remote_started")
    );
    let reserved = runtime
        .get_email_draft(EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    assert!(reserved.sending);
    assert_eq!(
        reserved.pending_send_idempotency_key.as_deref(),
        Some(input.idempotency_key.as_str())
    );

    let recovered = runtime.send_email_draft(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn accepted_draft_email_recovers_readback_after_restart_without_resend() {
    let gateway = TestGateway {
        sent_readback: Arc::new(Some(PlatformEmailMessage {
            provider_message_id: "sent-1".into(),
            internet_message_id: Some("<sent-1@example.com>".into()),
            from: "Reach <outreach@example.com>".into(),
            to: vec!["creator@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            subject: "Partnership".into(),
            text: Some("Hello".into()),
            html: None,
            headers: serde_json::json!({ "Message-Id": "<sent-1@example.com>" }),
            last_event: "delivered".into(),
            created_at: "2026-08-05 10:00:00.951732+00".into(),
            inbound: false,
            attachments: Vec::new(),
        })),
        ..TestGateway::default()
    };
    *gateway.next_sent_readback.lock().unwrap() =
        Some(Err("draft sent message is not readable yet".into()));
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-accepted-restart".into(),
    };
    let mut changes = runtime.subscribe_changes();

    let error = runtime.send_email_draft(input.clone()).await.unwrap_err();
    assert!(
        error
            .message()
            .contains("draft sent message is not readable yet"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "draft-accepted-restart").as_deref(),
        Some("remote_accepted")
    );
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .unwrap()
            .sending
    );
    let mut observed = Vec::new();
    while let Ok(change) = changes.try_recv() {
        observed.push(change);
    }
    assert_eq!(observed.len(), 2, "reservation and remote acceptance");
    assert!(observed.iter().all(|change| {
        change.scopes.contains(&CreatorChangeScope::EmailDrafts)
            && change.scopes.contains(&CreatorChangeScope::EmailFolders)
    }));
    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "draft-accepted-restart");

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email_draft(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 2);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn draft_email_retries_same_provider_key_after_pre_accept_stage_crash_window() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-provider-dedupe".into(),
    };
    block_remote_accepted_stage(&directory);

    let error = runtime.send_email_draft(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("remote accepted stage blocked"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "draft-provider-dedupe").as_deref(),
        Some("remote_started")
    );
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .unwrap()
            .sending
    );
    drop(runtime);

    unblock_remote_accepted_stage(&directory);
    assert_current_claim_uses_persisted_shape(&directory, "draft-provider-dedupe");
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email_draft(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .idempotency_key,
        "draft-provider-dedupe"
    );
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn ambiguous_draft_delivery_retries_same_provider_key_after_restart() {
    let gateway = TestGateway::default();
    *gateway.next_ambiguous_after_delivery.lock().unwrap() =
        Some("draft transport response was lost".into());
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-ambiguous-delivery".into(),
    };

    let error = runtime.send_email_draft(input.clone()).await.unwrap_err();
    assert!(error.message().contains("response was lost"), "{error}");
    assert_eq!(
        action_state(&directory, "draft-ambiguous-delivery").as_deref(),
        Some("remote_started")
    );
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .unwrap()
            .sending
    );
    drop(runtime);

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email_draft(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn verified_draft_send_freezes_revision_and_recovers_local_commit_without_resend() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            to: String::new(),
            creator_id: Some(creator_id.clone()),
            contact_route_id: Some(contact_route_id.clone()),
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    assert!(!draft.sending);
    assert!(draft.pending_send_idempotency_key.is_none());
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_recoverable_draft_touchpoint
             BEFORE INSERT ON touchpoints
             BEGIN SELECT RAISE(ABORT, 'recoverable draft commit blocked'); END;",
        )
        .unwrap();
    let send = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-recovery-1".into(),
    };
    let error = runtime.send_email_draft(send.clone()).await.unwrap_err();
    assert!(
        error.message().contains("recoverable draft commit blocked"),
        "{error}"
    );
    assert!(
        error.message().contains("local recovery commit failed"),
        "{error}"
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);

    let reserved = runtime
        .get_email_draft(EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    assert!(reserved.sending);
    assert_eq!(
        reserved.pending_send_idempotency_key.as_deref(),
        Some("draft-recovery-1")
    );
    assert_eq!(reserved.revision, draft.revision);
    let save_error = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: Some(draft.id.clone()),
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: reserved.to.clone(),
            creator_id: Some(creator_id),
            contact_route_id: Some(contact_route_id),
            reply_to_email_id: None,
            subject: "Changed while sending".into(),
            text: reserved.text.clone(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: Some(reserved.revision),
        })
        .unwrap_err();
    assert!(save_error.contains("reserved for sending"));
    for mutation in [EmailDraftState::Trashed, EmailDraftState::Draft] {
        let error = runtime
            .repository
            .set_email_draft_state(
                EmailDraftMutationInput {
                    draft_id: draft.id.clone(),
                    expected_revision: draft.revision,
                },
                mutation,
            )
            .unwrap_err();
        assert!(error.contains("reserved for sending"));
    }

    drop(runtime);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_recoverable_draft_touchpoint;")
        .unwrap();
    assert_current_claim_uses_persisted_shape(&directory, "draft-recovery-1");
    let restarted_gateway = Arc::new(gateway.clone());
    let restarted_gateways = CreatorGateways::default().with_email(restarted_gateway);
    let runtime = crate::CreatorRuntime::open(directory.path(), restarted_gateways).unwrap();
    let listed = runtime
        .list_email_drafts(ListEmailDraftsInput {
            mailbox_ref_id: Some(draft.mailbox_ref_id.clone()),
            cursor: None,
            state: Some(EmailDraftState::Draft),
            limit: None,
        })
        .unwrap();
    let listed_draft = listed
        .items
        .iter()
        .find(|listed| listed.id == draft.id)
        .unwrap();
    assert!(listed_draft.sending);
    let stored_draft = runtime
        .get_email_draft(crate::EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    assert_eq!(
        stored_draft.pending_send_idempotency_key.as_deref(),
        Some("draft-recovery-1")
    );
    let restarted_draft = runtime
        .get_email_draft(EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    let recovery_key = restarted_draft
        .pending_send_idempotency_key
        .clone()
        .unwrap();
    let recovered = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: restarted_draft.id,
            expected_revision: restarted_draft.revision,
            idempotency_key: recovery_key,
        })
        .await
        .unwrap();
    assert_eq!(recovered.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .is_err()
    );
}

#[tokio::test]
async fn failed_draft_idempotency_is_bound_to_draft_and_revision() {
    let gateway = TestGateway {
        send_error: Arc::new(Some(PlatformEmailSendError::Rejected {
            status: 422,
            message: "provider unavailable".into(),
        })),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft_input = |id, expected_revision| SaveEmailDraftInput {
        id,
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        to: "creator@example.com".into(),
        creator_id: None,
        contact_route_id: None,
        reply_to_email_id: None,
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        template_id: None,
        template_variables: std::collections::BTreeMap::new(),
        expected_revision,
    };
    let draft = runtime.save_email_draft(draft_input(None, None)).unwrap();
    let failed = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-failed-fingerprint".into(),
    };
    assert!(runtime.send_email_draft(failed.clone()).await.is_err());
    assert!(
        !runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .unwrap()
            .sending
    );
    assert!(
        runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone()
            })
            .unwrap()
            .pending_send_idempotency_key
            .is_none()
    );
    let same_failure = runtime.send_email_draft(failed).await.unwrap_err();
    assert!(same_failure.message().contains("provider unavailable"));

    let updated = runtime
        .save_email_draft(draft_input(Some(draft.id.clone()), Some(draft.revision)))
        .unwrap();
    let revision_mismatch = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: updated.id.clone(),
            expected_revision: updated.revision,
            idempotency_key: "draft-failed-fingerprint".into(),
        })
        .await
        .unwrap_err();
    assert!(
        revision_mismatch
            .message()
            .contains("another draft or draft revision")
    );
    let other = runtime.save_email_draft(draft_input(None, None)).unwrap();
    let draft_mismatch = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: other.id,
            expected_revision: other.revision,
            idempotency_key: "draft-failed-fingerprint".into(),
        })
        .await
        .unwrap_err();
    assert!(
        draft_mismatch
            .message()
            .contains("another draft or draft revision")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn draft_failure_persist_fallback_atomically_releases_claim_and_reservation() {
    let gateway = TestGateway {
        send_error: Arc::new(Some(PlatformEmailSendError::Rejected {
            status: 422,
            message: "provider unavailable".into(),
        })),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER fail_draft_terminal_failure
             BEFORE UPDATE OF state ON idempotency_records
             WHEN NEW.state = 'failed'
             BEGIN SELECT RAISE(ABORT, 'forced draft failure persistence error'); END;",
        )
        .unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id.clone(),
        expected_revision: draft.revision,
        idempotency_key: "draft-failure-atomic-fallback".into(),
    };

    let error = runtime.send_email_draft(input.clone()).await.unwrap_err();
    assert!(
        error
            .message()
            .contains("claim and draft reservation were released")
    );
    assert!(action_state(&directory, &input.idempotency_key).is_none());
    assert!(
        !runtime
            .get_email_draft(EmailDraftGetInput {
                draft_id: draft.id.clone(),
            })
            .unwrap()
            .sending
    );

    drop(runtime);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER fail_draft_terminal_failure")
        .unwrap();
    let runtime = reopen_runtime(&directory, &gateway);
    let retry = runtime.send_email_draft(input).await.unwrap_err();
    assert!(retry.message().contains("provider unavailable"));
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert!(
        !runtime
            .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
            .unwrap()
            .sending
    );
}
