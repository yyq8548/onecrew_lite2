use super::*;

#[tokio::test]
async fn gmail_draft_send_reaches_gateway_and_replays_without_another_send() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = runtime.sync_mailbox_ref(MailboxRefInput {
        id: MailboxRefId::new("99999999-9999-4999-8999-999999999999"),
        provider: "gmail".into(),
        label: "Gmail".into(),
        email_address: "outreach@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Send, EmailCapability::SentReadback],
        available: true,
        last_error: None,
    }).unwrap();
    assert!(ensure_mailbox(&mailbox, EmailCapability::Reply).is_err());
    let draft = runtime.save_email_draft(SaveEmailDraftInput {
        id: None,
        mailbox_ref_id: mailbox.mailbox_ref_id,
        to: "creator@example.com".into(),
        creator_id: None,
        contact_route_id: None,
        reply_to_email_id: None,
        subject: "Gmail test".into(),
        text: "Test".into(),
        html: None,
        template_id: None,
        template_variables: Default::default(),
        expected_revision: None,
    }).unwrap();
    let input = SendEmailDraftInput {
        draft_id: draft.id,
        expected_revision: draft.revision,
        idempotency_key: "gmail-draft-test".into(),
    };
    let sent = runtime.send_email_draft(input.clone()).await.unwrap();
    assert_eq!(sent.email.provider_message_id, "sent-1");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 1);
    let replayed = runtime.send_email_draft(input).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(sent.email.id, replayed.email.id);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn completed_creator_reply_replays_on_its_real_thread_without_duplicate_effects() {
    let CreatorReplyFixture {
        directory,
        runtime,
        gateway,
        input,
        thread_id,
        provider_thread_key,
        seed_email_id,
        inbound_email_id,
    } = creator_reply_fixture("email-send-1", false, true).await;
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(sent_readback_message(
        "creator-reply-provider",
        "<creator-reply@example.com>",
    )));
    let sent = runtime.send_email(input.clone()).await.unwrap();
    assert!(!sent.replayed);
    assert_eq!(sent.thread.id, thread_id);
    assert_eq!(sent.thread.provider_thread_key, provider_thread_key);
    assert_eq!(sent.email.provider_message_id, "creator-reply-provider");
    assert_creator_reply_facts(
        &runtime,
        &directory,
        &thread_id,
        &provider_thread_key,
        &seed_email_id,
        &inbound_email_id,
        &sent.email.id,
    );
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .idempotency_key,
        "email-send-1"
    );
    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "email-send-1");
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime.send_email(input.clone()).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.thread.id, thread_id);
    assert_eq!(replayed.thread.provider_thread_key, provider_thread_key);
    assert_eq!(replayed.email.id, sent.email.id);
    assert_creator_reply_facts(
        &runtime,
        &directory,
        &thread_id,
        &provider_thread_key,
        &seed_email_id,
        &inbound_email_id,
        &replayed.email.id,
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
    assert_eq!(gateway.provider_deliveries.load(Ordering::SeqCst), 2);
    let mut changed = input;
    changed.subject = "Different request".into();
    let error = runtime.send_email(changed).await.unwrap_err();
    assert!(error.message().contains("a different request"), "{error}");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn manual_email_send_persists_unbound_thread_and_replays_without_creator_facts() {
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
        idempotency_key: "manual-email-send-1".into(),
    };

    let sent = runtime.send_manual_email(input.clone()).await.unwrap();
    assert!(!sent.replayed);
    assert!(sent.thread.creator_id.is_none());
    assert!(sent.thread.contact_route_id.is_none());
    assert!(sent.audit_event.creator_id.is_none());
    assert!(sent.audit_event.contact_locator_id.is_none());
    assert_eq!(sent.audit_event.kind, "manual_email_sent");
    assert_eq!(
        gateway
            .last_email_request
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .idempotency_key,
        "manual-email-send-1"
    );
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "touchpoints"), 0);
    assert_eq!(fact_count(&directory, "audit_events"), 1);

    drop(runtime);
    assert_current_claim_uses_persisted_shape(&directory, "manual-email-send-1");
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime.send_manual_email(input).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.thread.id, sent.thread.id);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

fn batch_input(mailbox_ref_id: MailboxRefId, idempotency_key: &str) -> SendManyEmailInput {
    SendManyEmailInput {
        mailbox_ref_id,
        idempotency_key: idempotency_key.into(),
        validation: EmailBatchValidation::Strict,
        items: vec![EmailBatchItemInput {
            recipient: EmailBatchRecipient::ExplicitAddress {
                to: vec!["creator@example.com".into()],
            },
            from_name: Some("Reach".into()),
            cc: Vec::new(),
            bcc: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            content: EmailBatchContentInput::InlineRendered {
                subject: "Partnership".into(),
                text: Some("Hello".into()),
                html: None,
            },
        }],
    }
}

#[tokio::test]
async fn email_batch_persists_verified_items_and_replays_without_provider_resend() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = batch_input(mailbox.mailbox_ref_id.clone(), "batch-send-1");
    let preparation = runtime.prepare_email_send_many(input.clone()).unwrap();
    assert_eq!(preparation.item_count, 1);
    assert!(preparation.confirmation_required);

    let sent = runtime.send_email_many(input.clone()).await.unwrap();
    assert!(!sent.replayed);
    assert!(matches!(
        sent.outcomes[0].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert!(sent.outcomes[0].creator_id.is_none());
    assert!(sent.outcomes[0].contact_route_id.is_none());
    assert!(!sent.outcomes[0].payload_fingerprint.is_empty());
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);

    drop(runtime);
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime.send_email_many(input).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);
}

#[tokio::test]
async fn email_batch_readback_failure_keeps_remote_acceptance_for_same_key_recovery() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = batch_input(mailbox.mailbox_ref_id, "batch-readback-retry");
    *gateway.next_sent_readback.lock().unwrap() = Some(Err("readback unavailable".into()));
    let error = runtime.send_email_many(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("readback is incomplete"),
        "{error}"
    );
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 0);

    drop(runtime);
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email_many(input).await.unwrap();
    assert!(recovered.replayed);
    assert!(matches!(
        recovered.outcomes[0].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
}

#[tokio::test]
async fn email_batch_provider_unknown_stays_recoverable_without_blind_resend() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = batch_input(mailbox.mailbox_ref_id, "batch-provider-unknown");
    let preparation = runtime.prepare_email_send_many(input.clone()).unwrap();
    *gateway.next_batch_result.lock().unwrap() = Some(Ok(EmailBatchResult {
        logical_fingerprint: "unrelated-fingerprint".into(),
        chunks: Vec::new(),
        outcomes: vec![ProviderEmailBatchItemResult {
            input_index: 0,
            outcome: EmailBatchItemOutcome::Unknown {
                message: "provider response could not be correlated".into(),
            },
        }],
    }));
    let error = runtime.send_email_many(input.clone()).await.unwrap_err();
    assert!(error.message().contains("manual reconciliation"), "{error}");
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "idempotency_records"), 1);
    assert_eq!(fact_count(&directory, "emails"), 0);

    let reconciled = runtime
        .reconcile_email_send_many(ReconcileEmailBatchInput {
            mailbox_ref_id: input.mailbox_ref_id.clone(),
            idempotency_key: input.idempotency_key.clone(),
            logical_fingerprint: preparation.logical_fingerprint.clone(),
            items: vec![EmailBatchReconciliationItem {
                input_index: 0,
                outcome: EmailBatchReconciliationOutcome::Accepted {
                    provider_message_id: "batch-sent-0".into(),
                },
            }],
        })
        .await
        .unwrap();
    assert!(reconciled.replayed);
    assert!(matches!(
        reconciled.outcomes[0].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert_eq!(fact_count(&directory, "emails"), 1);

    let reconciled_replay = runtime
        .reconcile_email_send_many(ReconcileEmailBatchInput {
            mailbox_ref_id: input.mailbox_ref_id.clone(),
            idempotency_key: input.idempotency_key.clone(),
            logical_fingerprint: preparation.logical_fingerprint,
            items: Vec::new(),
        })
        .await
        .unwrap();
    assert!(reconciled_replay.replayed);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);

    drop(runtime);
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime.send_email_many(input).await.unwrap();
    assert!(replayed.replayed);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
}

/// Resend echoes back what it normalized, so the request fields are no longer
/// compared. A readback answering for the right message id completes the batch
/// even when every field it carries differs from what was submitted — which is
/// what a published template does, since the provider owns that body.
#[tokio::test]
async fn published_template_batch_completes_although_the_provider_rewrote_every_field() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-published-template");
    input.items[0].content = EmailBatchContentInput::PublishedTemplate {
        template_id_or_alias: "welcome-template".into(),
        variables: std::collections::BTreeMap::new(),
        from_override: Some("Alt <alt@example.com>".into()),
        subject_override: None,
        reply_to_override: Vec::new(),
    };
    let mut readback = sent_readback_message("batch-sent-0", "<published-template@example.com>");
    readback.from = "Someone Else <elsewhere@example.com>".into();
    readback.to = vec!["another-recipient@example.com".into()];
    readback.subject = "A subject the request never carried".into();
    readback.text = Some("a different body".into());
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(readback));

    let sent = runtime.send_email_many(input).await.unwrap();
    assert!(matches!(
        sent.outcomes[0].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
}

/// The provider has not finished with this item, so staying remote_accepted is
/// the wait it asked for: the same key picks the item up once its status
/// advances, with no second batch call.
#[tokio::test]
async fn batch_item_still_advancing_keeps_remote_acceptance_for_the_same_key() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = batch_input(mailbox.mailbox_ref_id, "batch-queued");
    let mut readback = sent_readback_message("batch-sent-0", "<queued@example.com>");
    readback.last_event = "queued".into();
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(readback));

    let error = runtime.send_email_many(input.clone()).await.unwrap_err();
    assert!(
        error.message().contains("readback is incomplete"),
        "{error}"
    );
    assert_eq!(error.effect(), crate::EmailSendEffect::Sent);
    assert_eq!(fact_count(&directory, "emails"), 0);

    drop(runtime);
    let runtime = reopen_runtime(&directory, &gateway);
    let recovered = runtime.send_email_many(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
}

/// `bounced` is the provider's own terminal answer: it accepted the item and
/// then did not deliver it. Reading the status again cannot change that, so the
/// batch stops and says so rather than asking for the same key forever. The
/// item is not recorded as rejected, because it was in fact sent.
#[tokio::test]
async fn batch_item_the_provider_did_not_deliver_stops_without_claiming_it_was_never_sent() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = batch_input(mailbox.mailbox_ref_id, "batch-bounced");
    let mut readback = sent_readback_message("batch-sent-0", "<bounced@example.com>");
    readback.last_event = "bounced".into();
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(readback));

    let error = runtime.send_email_many(input).await.unwrap_err();
    assert!(error.message().contains("did not deliver"), "{error}");
    assert!(error.message().contains("reconcile"), "{error}");
    // The mail did leave OneCrew, so a bounce is never reported as a send that
    // never happened.
    assert_eq!(error.effect(), crate::EmailSendEffect::Sent);
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 0);
}

#[test]
fn batch_action_fingerprint_distinguishes_explicit_and_creator_targets() {
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let explicit = runtime
        .prepare_email_send_many(batch_input(
            mailbox.mailbox_ref_id.clone(),
            "same-action-key",
        ))
        .unwrap();
    let mut creator_input = batch_input(mailbox.mailbox_ref_id, "same-action-key");
    creator_input.items[0].recipient = EmailBatchRecipient::CreatorRoute {
        creator_id,
        contact_route_id,
    };
    let creator = runtime.prepare_email_send_many(creator_input).unwrap();
    assert_ne!(
        explicit.logical_fingerprint, creator.logical_fingerprint,
        "Creator action identity must include target provenance"
    );
    assert_eq!(
        explicit.provider_payload_fingerprint, creator.provider_payload_fingerprint,
        "same rendered provider payload may share a provider fingerprint"
    );
}

#[tokio::test]
async fn creator_route_local_template_resolves_builtins_before_provider_call() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let template = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: None,
            name: "Creator greeting".into(),
            subject: "Hello {{name}}".into(),
            text: "Hi {{first_name}}, reach me at {{email}}".into(),
            html: None,
            expected_revision: None,
        })
        .unwrap();
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-creator-template");
    input.items[0].recipient = EmailBatchRecipient::CreatorRoute {
        creator_id: creator_id.clone(),
        contact_route_id: contact_route_id.clone(),
    };
    input.items[0].content = EmailBatchContentInput::LocalTemplate {
        template_id: template.id,
        revision: template.revision,
        variables: std::collections::BTreeMap::new(),
    };
    let mut readback = sent_readback_message("batch-sent-0", "<batch-template@example.com>");
    readback.subject = "Hello Creator".into();
    readback.text = Some("Hi Creator, reach me at creator@example.com".into());
    *gateway.next_sent_readback.lock().unwrap() = Some(Ok(readback));

    let sent = runtime.send_email_many(input).await.unwrap();
    assert!(matches!(
        sent.outcomes[0].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert_eq!(sent.outcomes[0].creator_id.as_ref(), Some(&creator_id));
    assert_eq!(
        sent.outcomes[0].contact_route_id.as_ref(),
        Some(&contact_route_id)
    );
    assert!(!sent.outcomes[0].payload_fingerprint.is_empty());
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let (stored_creator, stored_route): (String, String) = connection
        .query_row(
            "SELECT creator_id, contact_route_id FROM email_threads LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored_creator, creator_id.as_str());
    assert_eq!(stored_route, contact_route_id.as_str());
}

#[tokio::test]
async fn unresolved_creator_builtin_fails_strict_preflight_without_provider_call() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let template = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: None,
            name: "Needs handle".into(),
            subject: "Hello {{handle}}".into(),
            text: "Body".into(),
            html: None,
            expected_revision: None,
        })
        .unwrap();
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-unresolved-handle");
    input.items[0].recipient = EmailBatchRecipient::CreatorRoute {
        creator_id,
        contact_route_id,
    };
    input.items[0].content = EmailBatchContentInput::LocalTemplate {
        template_id: template.id,
        revision: template.revision,
        variables: std::collections::BTreeMap::new(),
    };
    let error = runtime.prepare_email_send_many(input).unwrap_err();
    assert!(error.contains("unresolved variables"), "{error}");
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fact_count(&directory, "idempotency_records"), 0);
}

#[test]
fn batch_template_rejects_invalid_variable_keys_before_provider_call() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let template = runtime
        .save_email_template(SaveEmailTemplateInput {
            id: None,
            name: "Variable validation".into(),
            subject: "Hello {{name}}".into(),
            text: "Body".into(),
            html: None,
            expected_revision: None,
        })
        .unwrap();
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-invalid-variable");
    input.items[0].content = EmailBatchContentInput::LocalTemplate {
        template_id: template.id,
        revision: template.revision,
        variables: std::collections::BTreeMap::from([("bad-key".into(), "value".into())]),
    };
    let error = runtime.prepare_email_send_many(input).unwrap_err();
    assert!(error.contains("invalid email template variable"), "{error}");
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(fact_count(&directory, "idempotency_records"), 0);
}

#[test]
fn batch_preflight_1000_creator_items_uses_one_snapshot_path() {
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-1000-snapshot");
    let mut items = Vec::with_capacity(1000);
    for index in 0..1000 {
        let creator = runtime
            .repository
            .create_creator(CreateCreatorInput {
                tags: None,
                display_name: format!("Creator {index}"),
                notes: None,
            })
            .unwrap();
        let address = format!("creator-{index}@example.com");
        let locator = runtime
            .repository
            .upsert_contact_locator(ContactLocatorInput {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                normalized_value: address.clone(),
                display_value: address,
            })
            .unwrap();
        let route = runtime
            .repository
            .link_creator_contact_route(&creator.creator.id, &locator.id, Confidence::Exact)
            .unwrap();
        let mut item = input.items[0].clone();
        item.recipient = EmailBatchRecipient::CreatorRoute {
            creator_id: creator.creator.id,
            contact_route_id: route.id,
        };
        items.push(item);
    }
    input.items = items;
    let preparation = runtime.prepare_email_send_many(input).unwrap();
    assert_eq!(preparation.item_count, 1000);
    assert_eq!(
        1000_usize.div_ceil(crate::email::batch::PREFLIGHT_CREATOR_CHUNK_SIZE),
        3,
        "the 1000 unique Creator snapshot must use three bounded IN-query chunks"
    );
}

#[tokio::test]
async fn permissive_batch_rejects_missing_template_item_and_sends_valid_item() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let mut input = batch_input(mailbox.mailbox_ref_id, "batch-permissive-missing");
    input.validation = EmailBatchValidation::Permissive;
    let mut invalid = input.items[0].clone();
    invalid.content = EmailBatchContentInput::LocalTemplate {
        template_id: EmailTemplateId::new("missing-template"),
        revision: 1,
        variables: std::collections::BTreeMap::new(),
    };
    input.items = vec![invalid, input.items[0].clone()];
    let result = runtime.send_email_many(input).await.unwrap();
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchOutcome::Rejected { ref message }
            if message.contains("email template not found")
    ));
    assert!(matches!(
        result.outcomes[1].outcome,
        EmailBatchOutcome::Accepted { .. }
    ));
    assert_eq!(gateway.batch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(fact_count(&directory, "emails"), 1);
}

#[tokio::test]
async fn manual_send_recovery_write_segments_keep_current_thread_runtime_responsive() {
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
        idempotency_key: "manual-email-segment-dispatch".into(),
    };

    let (claim_started, release_claim) = runtime.install_next_repository_write_barrier();
    let send_runtime = runtime.clone();
    let send_input = input.clone();
    let send = tokio::spawn(async move { send_runtime.send_manual_email(send_input).await });
    claim_started.await.expect("claim writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 0);
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("claim repository work starved Tokio")
    .unwrap();

    let (accepted_started, release_accepted) = runtime.install_next_repository_write_barrier();
    release_claim.send(()).unwrap();
    accepted_started
        .await
        .expect("accepted-stage writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    let (verified_started, release_verified) = runtime.install_next_repository_write_barrier();
    release_accepted.send(()).unwrap();
    verified_started
        .await
        .expect("verified-stage writer did not start");
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 1);
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("verified-stage repository work starved Tokio")
    .unwrap();

    release_verified.send(()).unwrap();
    let result = send.await.unwrap().unwrap();
    assert!(!result.replayed);
    assert_eq!(fact_count(&directory, "emails"), 1);
    assert_eq!(fact_count(&directory, "audit_events"), 1);

    let replay = runtime.send_manual_email(input).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn creator_email_claim_finishes_before_provider_send() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let input = send_input(
        creator_id,
        contact_route_id,
        mailbox.mailbox_ref_id,
        "creator-email-claim-dispatch",
    );
    let (claim_started, release_claim) = runtime.install_next_repository_write_barrier();
    let send_runtime = runtime.clone();
    let send = tokio::spawn(async move { send_runtime.send_email(input).await });
    claim_started
        .await
        .expect("Creator claim writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    release_claim.send(()).unwrap();
    send.await.unwrap().unwrap();
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelled_manual_send_keeps_remote_accepted_stage_for_same_key_recovery() {
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
        idempotency_key: "manual-email-cancel-after-accept".into(),
    };
    let mut changes = runtime.subscribe_changes();
    let (claim_started, release_claim) = runtime.install_next_repository_write_barrier();
    let send_runtime = runtime.clone();
    let send_input = input.clone();
    let send = tokio::spawn(async move { send_runtime.send_manual_email(send_input).await });
    claim_started.await.expect("claim writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    let (accepted_started, release_accepted) = runtime.install_next_repository_write_barrier();
    release_claim.send(()).unwrap();
    accepted_started
        .await
        .expect("accepted-stage writer did not start");
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    send.abort();
    assert!(send.await.unwrap_err().is_cancelled());
    release_accepted.send(()).unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while action_state(&directory, &input.idempotency_key).as_deref() != Some("remote_accepted")
        {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("accepted email stage was not durable after caller cancellation");
    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .expect("accepted stage did not publish after caller cancellation")
        .unwrap();
    assert_eq!(
        change.scopes,
        vec![
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads,
        ]
    );
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 0);

    let recovered = runtime.send_manual_email(input).await.unwrap();
    assert!(recovered.replayed);
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 1);
    assert_eq!(gateway.sent_readback_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn historical_manual_email_completed_result_replays_without_exposing_mailbox_failure() {
    let (directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );
    let mailbox_ref_id = mailbox.mailbox_ref_id.clone();
    drop(runtime);

    let key = "historical-manual-email-completed";
    let fingerprint = format!(
        "{{\"kind\":\"manual\",\"mailbox_ref_id\":\"{}\",\"to\":\"creator@example.com\",\"subject\":\"Partnership\",\"text\":\"Hello\",\"html\":null}}",
        mailbox_ref_id.as_str()
    );
    let payload = serde_json::json!({
        "preparation": {
            "mailbox": {
                "id": mailbox_ref_id.as_str(),
                "provider": "resend",
                "label": "Outreach",
                "email_address": "outreach@example.com",
                "display_name": "Reach",
                "capabilities": ["send", "sent_readback"],
                "available": false,
                "last_error": "historical recorded failure",
                "created_at": 10,
                "updated_at": 11
            },
            "to": "creator@example.com",
            "confirmation_required": true,
            "confirmation_reason": "external write"
        },
        "thread": {
            "id": "historical-thread",
            "mailbox_ref_id": mailbox_ref_id.as_str(),
            "creator_id": null,
            "contact_route_id": null,
            "provider_thread_key": "historical-thread-key",
            "subject": "Partnership",
            "unread_count": 0,
            "folder": "sent",
            "created_at": 20,
            "updated_at": 21
        },
        "email": {
            "id": "historical-email",
            "thread_id": "historical-thread",
            "provider_message_id": "historical-provider-message",
            "internet_message_id": null,
            "direction": "outbound",
            "from": "outreach@example.com",
            "to": ["creator@example.com"],
            "cc": [],
            "reply_to": [],
            "in_reply_to": null,
            "references": [],
            "subject": "Partnership",
            "text": "Hello",
            "html": null,
            "delivery_status": "sent",
            "occurred_at": 20
        },
        "audit_event": {
            "id": "historical-audit",
            "kind": "manual_email_sent",
            "creator_id": null,
            "account_ref_id": null,
            "resolved_target_id": null,
            "mailbox_ref_id": mailbox_ref_id.as_str(),
            "contact_locator_id": null,
            "email_id": "historical-email",
            // Released builds through v35 recorded this key on audit events. The
            // 35 -> 36 edge leaves email replay payloads without retired
            // provenance untouched, so an upgraded database still carries it.
            "campaign_id": null,
            "payload": {"verified": true},
            "created_at": 20
        },
        "replayed": false
    });
    let persisted_payload = payload.to_string();
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        crate::CREATOR_SCHEMA_VERSION
    );
    connection
        .execute(
            "INSERT INTO idempotency_records
               (key, action, mailbox_ref_id, contact_route_id, request_fingerprint,
                state, result_json, created_at, updated_at)
             VALUES (?1, 'manual_email_send', ?2, NULL, ?3, 'completed', ?4, 1, 2)",
            rusqlite::params![
                key,
                mailbox_ref_id.as_str(),
                fingerprint,
                &persisted_payload
            ],
        )
        .unwrap();
    drop(connection);

    assert_eq!(
        crate::CreatorRuntime::inspect_database(directory.path().join("crm.db")),
        crate::CreatorDatabaseStatus::Ready
    );
    let runtime = reopen_runtime(&directory, &gateway);
    let replayed = runtime
        .send_manual_email(SendManualEmailInput {
            mailbox_ref_id,
            to: "creator@example.com".into(),
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            idempotency_key: key.into(),
        })
        .await
        .unwrap();
    assert!(replayed.replayed);
    assert_eq!(replayed.email.id.as_str(), "historical-email");
    assert!(
        replayed
            .preparation
            .mailbox
            .issues
            .contains(&crate::MailboxIssue::RecordedFailure)
    );
    let visible = serde_json::to_string(&replayed).unwrap();
    assert!(!visible.contains("historical recorded failure"));
    assert!(!visible.contains("last_error"));
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        Connection::open(directory.path().join("crm.db"))
            .unwrap()
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key = ?1",
                [key],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        persisted_payload
    );
}

#[tokio::test]
async fn a_second_outreach_starts_its_own_thread() {
    // 同一个人、同一条路由,连主题都一样,但没有指明在回哪一封——那就
    // 是一次新的触达。RFC 5322 里线程关系由 `In-Reply-To`/`References`
    // 决定,收件人相同**从来不是**判据。
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
    );
    let first = runtime
        .send_email(send_input(
            creator_id.clone(),
            contact_route_id.clone(),
            mailbox.mailbox_ref_id.clone(),
            "outreach-1",
        ))
        .await
        .unwrap();
    let second = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id,
            "outreach-2",
        ))
        .await
        .unwrap();
    assert_ne!(
        first.thread.id, second.thread.id,
        "一次新的触达要归它自己的线程"
    );
}

#[tokio::test]
async fn a_new_outreach_is_not_dragged_into_a_deleted_thread() {
    // 用户撞上的那件事:上一条线程删掉之后再写一封新的信,它被吞进那条
    // 线程,于是刚发出去的信直接躺在回收站里,而「已发送」是空的。
    let (_directory, runtime, _gateway) = runtime(TestGateway::default());
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
    );
    let first = runtime
        .send_email(send_input(
            creator_id.clone(),
            contact_route_id.clone(),
            mailbox.mailbox_ref_id.clone(),
            "outreach-before-delete",
        ))
        .await
        .unwrap();
    let trashed = runtime
        .trash_email_thread(EmailThreadMutationInput {
            thread_id: first.thread.id.clone(),
        })
        .unwrap();
    assert_eq!(trashed.thread.folder, crate::EmailThreadFolder::Deleted);

    let second = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id,
            "outreach-after-delete",
        ))
        .await
        .unwrap();
    assert_ne!(second.thread.id, first.thread.id);
    assert_eq!(
        second.thread.folder,
        crate::EmailThreadFolder::Sent,
        "新发出去的信要落在已发送,而不是继承上一条线程的回收站状态"
    );
}
