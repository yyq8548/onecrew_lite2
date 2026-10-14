use super::*;
use crate::{MailboxGetInput, MailboxGetOutcome, MailboxIssue};

#[test]
fn mailbox_get_distinguishes_found_and_not_found_without_error_text() {
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let found = runtime
        .get_mailbox(MailboxGetInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        })
        .unwrap();
    assert!(matches!(found, MailboxGetOutcome::Found { .. }));

    let missing_id = crate::MailboxRefId::new("99999999-9999-4999-8999-999999999999");
    let missing = runtime
        .get_mailbox(MailboxGetInput {
            mailbox_ref_id: missing_id.clone(),
        })
        .unwrap();
    assert!(matches!(
        missing,
        MailboxGetOutcome::NotFound { mailbox_ref_id } if mailbox_ref_id == missing_id
    ));
}

#[test]
fn mailbox_snapshot_reconcile_replaces_missing_refs_and_keeps_unavailable_refs() {
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let first = MailboxRefInput {
        id: crate::MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "resend".into(),
        label: "First".into(),
        email_address: "first@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    let second = MailboxRefInput {
        id: crate::MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
        provider: "resend".into(),
        label: "Second".into(),
        email_address: "second@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        last_error: None,
    };
    runtime
        .reconcile_mailbox_refs(vec![first, second.clone()])
        .expect("initial mailbox snapshot");

    let mut unavailable = second;
    unavailable.available = false;
    unavailable.last_error = Some("credential temporarily disabled".into());
    let reconciled = runtime
        .reconcile_mailbox_refs(vec![unavailable])
        .expect("replacement mailbox snapshot");

    assert_eq!(reconciled.len(), 2);
    let second = reconciled
        .iter()
        .find(|mailbox| mailbox.label == "Second")
        .unwrap();
    assert!(!second.available);
    assert_eq!(
        second.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );
    let first = reconciled
        .iter()
        .find(|mailbox| mailbox.label == "First")
        .unwrap();
    assert!(!first.available);
    assert_eq!(
        first.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );
    assert_eq!(runtime.list_mailboxes().expect("mailboxes").len(), 2);
}

#[tokio::test]
async fn missing_mailbox_credential_preserves_history_facts_and_drafts_but_blocks_sends() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let sent = runtime
        .send_email(send_input(
            creator_id.clone(),
            contact_route_id.clone(),
            mailbox.mailbox_ref_id.clone(),
            "history-before-credential-loss",
        ))
        .await
        .unwrap();
    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            to: String::new(),
            creator_id: Some(creator_id.clone()),
            contact_route_id: Some(contact_route_id.clone()),
            reply_to_email_id: None,
            subject: "Preserved draft".into(),
            text: "Keep me".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    let business_counts_before = (
        fact_count(&directory, "creators"),
        fact_count(&directory, "email_threads"),
        fact_count(&directory, "emails"),
        fact_count(&directory, "touchpoints"),
        fact_count(&directory, "audit_events"),
        fact_count(&directory, "email_drafts"),
        fact_count(&directory, "idempotency_records"),
    );

    let reconciled = runtime.reconcile_mailbox_refs(Vec::new()).unwrap();
    let retained_mailbox = reconciled
        .iter()
        .find(|candidate| candidate.mailbox_ref_id == mailbox.mailbox_ref_id)
        .expect("missing credential mailbox remains projected");
    assert!(!retained_mailbox.available);
    assert_eq!(
        retained_mailbox.issues,
        vec![MailboxIssue::Unavailable, MailboxIssue::RecordedFailure]
    );
    assert_eq!(
        business_counts_before,
        (
            fact_count(&directory, "creators"),
            fact_count(&directory, "email_threads"),
            fact_count(&directory, "emails"),
            fact_count(&directory, "touchpoints"),
            fact_count(&directory, "audit_events"),
            fact_count(&directory, "email_drafts"),
            fact_count(&directory, "idempotency_records"),
        )
    );
    let history = runtime
        .get_email_thread(EmailThreadGetInput {
            thread_id: sent.thread.id.clone(),
            limit: None,
            cursor: None,
        })
        .unwrap();
    assert_eq!(history.messages.len(), 1);
    assert_eq!(history.messages[0].id, sent.email.id);
    assert!(!history.mailbox.available);
    let retained_draft = runtime
        .get_email_draft(EmailDraftGetInput {
            draft_id: draft.id.clone(),
        })
        .unwrap();
    assert_eq!(retained_draft.subject, "Preserved draft");
    assert_eq!(
        action_state(&directory, "history-before-credential-loss").as_deref(),
        Some("completed")
    );

    let blocked = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id,
            "blocked-after-credential-loss",
        ))
        .await
        .unwrap_err();
    assert_eq!(blocked.message(), "mailbox is unavailable");
    let blocked_draft = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: draft.id.clone(),
            expected_revision: draft.revision,
            idempotency_key: "blocked-draft-after-credential-loss".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(blocked_draft.message(), "mailbox is unavailable");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        fact_count(&directory, "email_threads"),
        business_counts_before.1
    );
    assert_eq!(fact_count(&directory, "emails"), business_counts_before.2);
    assert_eq!(
        fact_count(&directory, "touchpoints"),
        business_counts_before.3
    );
    assert_eq!(
        fact_count(&directory, "audit_events"),
        business_counts_before.4
    );
    assert_eq!(
        fact_count(&directory, "email_drafts"),
        business_counts_before.5
    );
    let retained_draft = runtime
        .get_email_draft(EmailDraftGetInput { draft_id: draft.id })
        .unwrap();
    assert_eq!(retained_draft.revision, draft.revision);
    assert!(!retained_draft.sending);
    assert!(retained_draft.pending_send_idempotency_key.is_none());
}
