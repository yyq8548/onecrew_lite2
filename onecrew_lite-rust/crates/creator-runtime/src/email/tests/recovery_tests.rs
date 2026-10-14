use super::*;

#[tokio::test]
async fn accepted_creator_reply_recovers_on_its_real_thread_without_resend() {
    let CreatorReplyFixture {
        directory,
        runtime,
        gateway,
        input,
        thread_id,
        provider_thread_key,
        seed_email_id,
        inbound_email_id,
    } = creator_reply_fixture("email-accepted-restart", false, true).await;
    *gateway.next_sent_readback.lock().unwrap() =
        Some(Err("sent message is not readable yet".into()));

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("sent message is not readable yet"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "email-accepted-restart").as_deref(),
        Some("remote_accepted")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 2);
    assert_eq!(fact_count(&directory, "email_threads"), 1);
    assert_eq!(fact_count(&directory, "emails"), 2);
    let mut changed = input.clone();
    changed.text = "Changed while accepted".into();
    let error = runtime.send_email(changed).await.unwrap_err();
    assert!(error.message().contains("a different request"), "{error}");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 2);
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(sent_readback_message(
        "creator-reply-provider",
        "<creator-reply@example.com>",
    )));
    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "email-accepted-restart");

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.thread.id, thread_id);
    assert_eq!(recovered.thread.provider_thread_key, provider_thread_key);
    assert_eq!(
        recovered.email.provider_message_id,
        "creator-reply-provider"
    );
    assert_creator_reply_facts(
        &runtime,
        &directory,
        &thread_id,
        &provider_thread_key,
        &seed_email_id,
        &inbound_email_id,
        &recovered.email.id,
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 3);
    assert_eq!(
        action_state(&directory, "email-accepted-restart").as_deref(),
        Some("completed")
    );
}

#[tokio::test]
async fn started_creator_reply_retries_the_same_provider_key_on_its_real_thread() {
    let CreatorReplyFixture {
        directory,
        runtime,
        gateway,
        input,
        thread_id,
        provider_thread_key,
        seed_email_id,
        inbound_email_id,
    } = creator_reply_fixture("creator-provider-dedupe", true, false).await;
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(sent_readback_message(
        "creator-reply-provider",
        "<creator-reply@example.com>",
    )));
    block_remote_accepted_stage(&directory);

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("remote accepted stage blocked"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "creator-provider-dedupe").as_deref(),
        Some("remote_started")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 2);
    assert_eq!(fact_count(&directory, "email_threads"), 1);
    assert_eq!(fact_count(&directory, "emails"), 2);
    drop(runtime);

    unblock_remote_accepted_stage(&directory);
    assert_current_claim_uses_persisted_shape(&directory, "creator-provider-dedupe");
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.thread.id, thread_id);
    assert_eq!(recovered.thread.provider_thread_key, provider_thread_key);
    assert_eq!(
        recovered.email.provider_message_id,
        "creator-reply-provider"
    );
    assert_creator_reply_facts(
        &runtime,
        &directory,
        &thread_id,
        &provider_thread_key,
        &seed_email_id,
        &inbound_email_id,
        &recovered.email.id,
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 3);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 2);
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .idempotency_key,
        "creator-provider-dedupe"
    );
}

#[tokio::test]
async fn ambiguous_creator_delivery_retries_same_provider_key_after_restart() {
    let gateway = TestGateway::default();
    *gateway.next_ambiguous_after_delivery.lock().unwrap() =
        Some("transport timed out after provider acceptance".into());
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "creator-ambiguous-delivery",
    );

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("transport timed out"), "{error}");
    assert_eq!(
        action_state(&directory, "creator-ambiguous-delivery").as_deref(),
        Some("remote_started")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 0);
    drop(runtime);

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(
        action_state(&directory, "creator-ambiguous-delivery").as_deref(),
        Some("completed")
    );
}

#[tokio::test]
async fn creator_remote_started_claim_failure_is_atomic_and_restartable() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "remote-start-stage-failure",
    );
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_remote_started_stage
             BEFORE INSERT ON idempotency_records
             WHEN NEW.state = 'remote_started'
             BEGIN SELECT RAISE(ABORT, 'remote start stage blocked'); END;",
        )
        .unwrap();

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("remote start stage blocked"),
        "{error}"
    );
    assert!(action_state(&directory, "remote-start-stage-failure").is_none());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    drop(runtime);

    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_remote_started_stage;")
        .unwrap();
    let runtime = reopen_runtime(&directory, &gateway);
    let sent = runtime.send_email(input).await.unwrap();
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn manual_remote_started_claim_failure_is_atomic_and_restartable() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: "manual-atomic-claim".into(),
    };
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_manual_remote_started_claim
             BEFORE INSERT ON idempotency_records
             WHEN NEW.state = 'remote_started'
             BEGIN SELECT RAISE(ABORT, 'manual remote start claim blocked'); END;",
        )
        .unwrap();

    let error = runtime.send_manual_email(input.clone()).await.unwrap_err();
    assert!(
        error
            .message()
            .contains("manual remote start claim blocked"),
        "{error}"
    );
    assert!(action_state(&directory, "manual-atomic-claim").is_none());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    drop(runtime);

    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_manual_remote_started_claim;")
        .unwrap();
    let runtime = reopen_runtime(&directory, &gateway);
    let sent = runtime.send_manual_email(input).await.unwrap();
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn accepted_manual_email_recovers_readback_after_restart_without_resend() {
    let gateway = TestGateway::default();
    *gateway.next_sent_readback.lock().unwrap() =
        Some(Err("manual sent message is not readable yet".into()));
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: "manual-email-accepted-restart".into(),
    };

    let error = runtime.send_manual_email(input.clone()).await.unwrap_err();
    assert!(
        error
            .message()
            .contains("manual sent message is not readable yet"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "manual-email-accepted-restart").as_deref(),
        Some("remote_accepted")
    );
    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "manual-email-accepted-restart");

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_manual_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 2);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn manual_email_retries_same_provider_key_after_pre_accept_stage_crash_window() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: "manual-provider-dedupe".into(),
    };
    block_remote_accepted_stage(&directory);

    let error = runtime.send_manual_email(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("remote accepted stage blocked"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "manual-provider-dedupe").as_deref(),
        Some("remote_started")
    );
    drop(runtime);

    unblock_remote_accepted_stage(&directory);
    assert_current_claim_uses_persisted_shape(&directory, "manual-provider-dedupe");
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_manual_email(input).await.unwrap();
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
        "manual-provider-dedupe"
    );
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn ambiguous_manual_delivery_retries_same_provider_key_after_restart() {
    let gateway = TestGateway::default();
    *gateway.next_ambiguous_after_delivery.lock().unwrap() =
        Some("manual transport response was lost".into());
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: "manual-ambiguous-delivery".into(),
    };

    let error = runtime.send_manual_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("response was lost"), "{error}");
    assert_eq!(
        action_state(&directory, "manual-ambiguous-delivery").as_deref(),
        Some("remote_started")
    );
    drop(runtime);

    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_manual_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn verified_manual_email_recovers_local_commit_after_restart_without_resend() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        subject: "Partnership".into(),
        text: "Hello".into(),
        html: None,
        idempotency_key: "manual-email-local-restart".into(),
    };
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_manual_email_audit
             BEFORE INSERT ON audit_events
             WHEN NEW.kind = 'manual_email_sent'
             BEGIN SELECT RAISE(ABORT, 'manual email local commit blocked'); END;",
        )
        .unwrap();

    let error = runtime.send_manual_email(input.clone()).await.unwrap_err();
    assert!(
        error
            .message()
            .contains("manual email local commit blocked"),
        "{error}"
    );
    assert!(
        error.message().contains("local recovery commit failed"),
        "{error}"
    );
    assert_eq!(
        action_state(&directory, "manual-email-local-restart").as_deref(),
        Some("remote_verified")
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), 0);
    drop(runtime);

    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_manual_email_audit;")
        .unwrap();
    assert_current_claim_uses_persisted_shape(&directory, "manual-email-local-restart");
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_manual_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn email_remote_failure_has_failed_terminal_state_and_no_success_facts() {
    let gateway = TestGateway {
        send_error: Arc::new(Some(PlatformEmailSendError::Rejected {
            status: 422,
            message: "provider send failed".into(),
        })),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "email-remote-failure",
    );

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("provider send failed"));
    assert_eq!(
        action_state(&directory, "email-remote-failure").as_deref(),
        Some("failed")
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), 0);

    let replayed_error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(replayed_error.message().contains("provider send failed"));
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    let mut changed = input;
    changed.text = "Different failed request".into();
    let fingerprint_error = runtime.send_email(changed).await.unwrap_err();
    assert!(
        fingerprint_error.message().contains("a different request"),
        "{fingerprint_error}"
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

/// Identity is the one thing a readback must still prove. Delivery is read
/// from the provider's own status, but a readback answering for *another*
/// message says nothing about this send, so the claim waits rather than
/// commit someone else's mail as this one.
#[tokio::test]
async fn email_readback_for_another_message_remains_remote_accepted_without_local_success_facts() {
    let gateway = TestGateway {
        sent_readback: Arc::new(Some(PlatformEmailMessage {
            provider_message_id: "different-message".into(),
            internet_message_id: Some("<different@example.com>".into()),
            from: "Reach <outreach@example.com>".into(),
            to: vec!["creator@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            subject: "Partnership".into(),
            text: Some("Hello".into()),
            html: None,
            headers: serde_json::json!({}),
            last_event: "delivered".into(),
            created_at: "2026-08-05T10:00:00Z".into(),
            inbound: false,
            attachments: Vec::new(),
        })),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );

    let error = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id,
            "email-readback-mismatch",
        ))
        .await
        .unwrap_err();
    assert!(error.message().contains("readback did not match"));
    assert_eq!(
        action_state(&directory, "email-readback-mismatch").as_deref(),
        Some("remote_accepted")
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
}

#[tokio::test]
async fn email_invalid_readback_and_creator_reply_local_commit_failure_remain_recoverable() {
    let invalid_readback = TestGateway {
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
            headers: serde_json::json!({}),
            last_event: "delivered".into(),
            created_at: "not-a-timestamp".into(),
            inbound: false,
            attachments: Vec::new(),
        })),
        ..TestGateway::default()
    };
    let (invalid_directory, invalid_runtime, _) = runtime(invalid_readback);
    let (creator_id, contact_route_id) = creator_email_route(&invalid_runtime);
    let invalid_mailbox = mailbox(
        &invalid_runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let error = invalid_runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            invalid_mailbox.mailbox_ref_id,
            "email-invalid-readback",
        ))
        .await
        .unwrap_err();
    assert!(error.message().contains("invalid Resend timestamp"));
    assert_eq!(
        action_state(&invalid_directory, "email-invalid-readback").as_deref(),
        Some("remote_accepted")
    );

    let CreatorReplyFixture {
        directory,
        runtime,
        gateway,
        input,
        thread_id,
        provider_thread_key,
        seed_email_id,
        inbound_email_id,
    } = creator_reply_fixture("email-local-commit", false, false).await;
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(sent_readback_message(
        "creator-reply-provider",
        "<creator-reply@example.com>",
    )));
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_email_touchpoint
             BEFORE INSERT ON touchpoints
             BEGIN SELECT RAISE(ABORT, 'email local commit blocked'); END;",
        )
        .unwrap();
    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("local recovery commit failed"));
    assert!(error.message().contains("email local commit blocked"));
    assert_eq!(
        action_state(&directory, "email-local-commit").as_deref(),
        Some("remote_verified")
    );
    assert_eq!(fact_count(&directory, "email_threads"), 1);
    assert_eq!(fact_count(&directory, "emails"), 2);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);

    drop(runtime);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute_batch("DROP TRIGGER reject_email_touchpoint;")
        .unwrap();
    assert_current_claim_uses_persisted_shape(&directory, "email-local-commit");
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(recovered.thread.id, thread_id);
    assert_eq!(recovered.thread.provider_thread_key, provider_thread_key);
    assert_eq!(
        recovered.email.provider_message_id,
        "creator-reply-provider"
    );
    assert_creator_reply_facts(
        &runtime,
        &directory,
        &thread_id,
        &provider_thread_key,
        &seed_email_id,
        &inbound_email_id,
        &recovered.email.id,
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn email_failure_persistence_error_releases_claim_for_retry() {
    let gateway = TestGateway {
        send_error: Arc::new(Some(PlatformEmailSendError::Rejected {
            status: 422,
            message: "provider unavailable".into(),
        })),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "email-failure-persistence",
    );
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_failed_action
             BEFORE UPDATE OF state ON idempotency_records
             WHEN NEW.state = 'failed'
             BEGIN SELECT RAISE(ABORT, 'failure persistence blocked'); END;",
        )
        .unwrap();

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("provider unavailable"));
    assert!(error.message().contains("failure persistence blocked"));
    assert!(error.message().contains("released for retry"));
    assert_eq!(action_state(&directory, "email-failure-persistence"), None);

    connection
        .execute_batch("DROP TRIGGER reject_failed_action;")
        .unwrap();
    let retry_error = runtime.send_email(input).await.unwrap_err();
    assert!(retry_error.message().contains("provider unavailable"));
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        action_state(&directory, "email-failure-persistence").as_deref(),
        Some("failed")
    );
}

/// This claim's own readback, carrying one chosen delivery status. Every
/// other field stays the shape the provider really returns for `sent-1`.
fn readback_with_status(last_event: &str) -> PlatformEmailMessage {
    PlatformEmailMessage {
        last_event: last_event.into(),
        ..sent_readback_message("sent-1", "<sent-1@example.com>")
    }
}

#[tokio::test]
async fn accepted_send_completes_once_the_message_has_left_the_provider() {
    // `sent` already proves the message left Resend; holding out for
    // `delivered` would stall a send that is demonstrably on its way. The
    // subject and recipients coming back differ from the request on purpose:
    // they are no longer compared, because Resend only ever echoed back what
    // it was handed.
    let mut readback = readback_with_status("sent");
    readback.subject = "A subject the request never carried".into();
    readback.to = vec!["someone-else@example.com".into()];
    let gateway = TestGateway {
        sent_readback: Arc::new(Some(readback)),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );

    let sent = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id,
            "email-left-the-provider",
        ))
        .await
        .unwrap();

    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(
        action_state(&directory, "email-left-the-provider").as_deref(),
        Some("completed")
    );
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn accepted_send_records_one_advancing_readback_without_resending() {
    // queued/scheduled/delivery_delayed are positions on the way out, not
    // faults. One real sent-detail read is enough to record the accepted
    // outbound message without claiming it was delivered.
    let gateway = TestGateway {
        next_sent_readback: Arc::new(Mutex::new(Some(Ok(readback_with_status("queued"))))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "email-still-sending",
    );

    let sent = runtime.send_email(input).await.unwrap();
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(sent.email.delivery_status, "queued");
    assert_eq!(
        action_state(&directory, "email-still-sending").as_deref(),
        Some("completed")
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 1);
}

#[tokio::test]
async fn accepted_send_stops_asking_when_the_provider_will_not_deliver_it() {
    // A bounce is an answer, not an unknown: no later read changes it, so the
    // send stops being reported as "not yet verifiable" and says plainly that
    // it was never delivered and that this key cannot be retried into one.
    //
    // The durable claim itself stays remote_accepted. `fail_action` moves a
    // claim out of in_progress/remote_started only, because once the provider
    // has accepted a send the mail did leave OneCrew and the record must not
    // say otherwise. Making this a durable `failed` claim would change
    // idempotency semantics, which is a decision of its own.
    let gateway = TestGateway {
        sent_readback: Arc::new(Some(readback_with_status("bounced"))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "email-bounced",
    );

    let error = runtime.send_email(input.clone()).await.unwrap_err();
    assert!(error.message().contains("bounced"), "{error}");
    assert!(error.message().contains("not delivered"), "{error}");
    assert!(error.message().contains("cannot change that"), "{error}");
    assert_eq!(
        action_state(&directory, "email-bounced").as_deref(),
        Some("remote_accepted")
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), 0);

    // The answer is stable: the same key reads the same status back and never
    // asks the provider to send the message again.
    let replayed = runtime.send_email(input).await.unwrap_err();
    assert!(replayed.message().contains("bounced"), "{replayed}");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}
