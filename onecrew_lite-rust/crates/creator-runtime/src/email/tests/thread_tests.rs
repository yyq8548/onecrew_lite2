use super::*;

#[tokio::test]
async fn manual_draft_reply_reuses_unowned_thread_and_provider_reply_headers() {
    let inbound = PlatformEmailMessage {
        provider_message_id: "manual-inbound-1".into(),
        internet_message_id: Some("<manual-inbound-1@example.net>".into()),
        from: "Creator <creator@example.com>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Manual inbound".into(),
        text: Some("Hello".into()),
        html: None,
        headers: serde_json::json!({ "Message-Id": "<manual-inbound-1@example.net>" }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
    );
    let synced = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let original_thread = runtime
        .repository
        .get_email_thread_record(&synced.changed_thread_ids[0])
        .unwrap();
    let inbound_email = runtime
        .repository
        .get_email_thread_messages(&original_thread.id, 10, None)
        .unwrap()
        .items
        .remove(0);
    assert!(original_thread.creator_id.is_none());

    let draft = runtime
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: mailbox.mailbox_ref_id,
            to: String::new(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: Some(inbound_email.id),
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            template_id: None,
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .unwrap();
    assert_eq!(draft.to, "creator@example.com");
    let sent = runtime
        .send_email_draft(SendEmailDraftInput {
            draft_id: draft.id,
            expected_revision: draft.revision,
            idempotency_key: "manual-draft-reply".into(),
        })
        .await
        .unwrap();
    assert_eq!(sent.thread.id, original_thread.id);
    assert!(sent.touchpoint.is_none());
    assert_eq!(
        sent.email.in_reply_to.as_deref(),
        Some("<manual-inbound-1@example.net>")
    );
    assert_eq!(
        sent.email.references,
        vec!["<manual-inbound-1@example.net>".to_string()]
    );
    let request = gateway.last_email_request.lock().unwrap().clone().unwrap();
    assert_eq!(
        request.in_reply_to.as_deref(),
        Some("<manual-inbound-1@example.net>")
    );
    assert_eq!(
        request.references,
        vec!["<manual-inbound-1@example.net>".to_string()]
    );
    let thread = runtime
        .get_email_thread(EmailThreadGetInput {
            thread_id: original_thread.id,
            cursor: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(thread.messages.len(), 2);
}
