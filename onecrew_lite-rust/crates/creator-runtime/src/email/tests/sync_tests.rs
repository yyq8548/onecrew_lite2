use super::*;
use crate::{EmailFolder, EmailThreadSort, ListEmailThreadsInput};

#[tokio::test]
async fn mailbox_sync_keeps_unmatched_inbound_email_unowned() {
    let inbound = PlatformEmailMessage {
        provider_message_id: "received-1".into(),
        internet_message_id: Some("<received-1@example.net>".into()),
        from: "Unknown <unknown@example.net>".into(),
        to: vec!["forwarder@example.net".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: vec!["outreach@example.com".into()],
        reply_to: Vec::new(),
        subject: "Hello".into(),
        text: Some("Inbound".into()),
        html: None,
        headers: serde_json::json!({ "Message-Id": "<received-1@example.net>" }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let mut changes = runtime.subscribe_changes();
    let result = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(result.matched, 0);
    assert_eq!(result.unmatched, 1);
    assert_eq!(result.changed_thread_ids.len(), 1);
    let changed_thread = runtime
        .repository
        .get_email_thread_record(&result.changed_thread_ids[0])
        .unwrap();
    assert!(changed_thread.creator_id.is_none());
    assert!(changed_thread.contact_route_id.is_none());
    let messages = runtime
        .repository
        .get_email_thread_messages(&changed_thread.id, 10, None)
        .unwrap()
        .items;
    let inbound = runtime
        .repository
        .get_email_message(&messages[0].id)
        .unwrap();
    assert!(inbound.bcc.is_empty());
    assert_eq!(inbound.received_for, ["outreach@example.com"]);
    let change = changes.try_recv().unwrap();
    assert!(change.scopes.contains(&CreatorChangeScope::EmailFolders));
    assert!(change.scopes.contains(&CreatorChangeScope::EmailThreads));
}

#[tokio::test]
async fn mailbox_sync_rejects_an_adapter_leak_from_another_mailbox() {
    let gateway = TestGateway {
        received: Arc::new(vec![PlatformEmailMessage {
            provider_message_id: "other-mailbox-message".into(),
            internet_message_id: Some("<other-mailbox-message@example.net>".into()),
            from: "Unknown <unknown@example.net>".into(),
            to: vec!["billing@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            subject: "Billing".into(),
            text: Some("Not for outreach".into()),
            html: None,
            headers: serde_json::json!({}),
            last_event: "received".into(),
            created_at: "2026-08-05T11:00:00Z".into(),
            inbound: true,
            attachments: Vec::new(),
        }]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(
        report.missing_provider_message_ids,
        vec!["other-mailbox-message"],
        "the leaked message is isolated as missing work, not imported"
    );
    let failure = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .unwrap()
        .state
        .unwrap()
        .failure
        .expect("the leak is durable, resumable failure state");
    assert!(
        failure
            .message
            .contains("not addressed to the selected mailbox")
    );
}

#[tokio::test]
async fn conflicting_inbound_owner_is_isolated_from_referenced_creator_thread() {
    let inbound = PlatformEmailMessage {
        provider_message_id: "received-conflict-1".into(),
        internet_message_id: Some("<received-conflict-1@example.net>".into()),
        from: "Creator <creator@example.com>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Re: Partnership".into(),
        text: Some("Which Creator owns this address?".into()),
        html: None,
        headers: serde_json::json!({
            "Message-Id": "<received-conflict-1@example.net>",
            "References": "<sent-1@example.com>",
            "In-Reply-To": "<sent-1@example.com>"
        }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let (creator_id, contact_route_id) = creator_email_route(&runtime);
    let first_creator = runtime.repository.get_creator(&creator_id).unwrap();
    let locator_id = first_creator
        .contact_routes
        .iter()
        .find(|record| record.route.id == contact_route_id)
        .unwrap()
        .locator
        .id
        .clone();
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
    );
    let sent = runtime
        .send_email(SendEmailInput {
            creator_id: creator_id.clone(),
            contact_route_id: contact_route_id.clone(),
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            reply_to_email_id: None,
            subject: "Partnership".into(),
            text: "Hello".into(),
            html: None,
            idempotency_key: "email-conflict-seed".into(),
        })
        .await
        .unwrap();

    let second_creator = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Conflicting Creator".into(),
            notes: None,
        })
        .unwrap();
    runtime
        .repository
        .insert_historical_shared_contact_route_for_test(
            &second_creator.creator.id,
            &locator_id,
            Confidence::Exact,
        )
        .unwrap();

    let result = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(result.matched, 0);
    assert_eq!(result.unmatched, 1);

    // The identity map makes the second sync skip the already-complete
    // message instead of importing it again.
    let resynced = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(resynced.summaries_listed, 1);
    assert_eq!(resynced.skipped_complete, 1);
    assert_eq!(resynced.matched, 0);
    assert_eq!(resynced.unmatched, 0);

    let original_messages = runtime
        .repository
        .get_email_thread_messages(&sent.thread.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(original_messages.len(), 1);
    assert_eq!(original_messages[0].provider_message_id, "sent-1");

    let threads = runtime
        .repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Newest,
            100,
            None,
        )
        .unwrap();
    let isolated = threads
        .items
        .iter()
        .find(|summary| summary.thread.provider_thread_key == "resend:unowned:received-conflict-1")
        .expect("conflicting inbound thread");
    assert!(isolated.thread.creator_id.is_none());
    assert!(isolated.thread.contact_route_id.is_none());
    let isolated_messages = runtime
        .repository
        .get_email_thread_messages(&isolated.thread.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(isolated_messages.len(), 1);
    assert_eq!(
        isolated_messages[0].provider_message_id,
        "received-conflict-1"
    );
}
#[tokio::test]
async fn mailbox_sync_keeps_archived_thread_archived() {
    let inbound = PlatformEmailMessage {
        provider_message_id: "received-archive-1".into(),
        internet_message_id: Some("<received-archive-1@example.net>".into()),
        from: "Creator <creator@example.com>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Re: Partnership".into(),
        text: Some("Reply".into()),
        html: None,
        headers: serde_json::json!({
            "Message-Id": "<received-archive-1@example.net>",
            "References": "<sent-1@example.com>",
            "In-Reply-To": "<sent-1@example.com>"
        }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
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
    let sent = runtime
        .send_email(send_input(
            creator_id,
            contact_route_id,
            mailbox.mailbox_ref_id.clone(),
            "archive-sync-seed",
        ))
        .await
        .unwrap();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let archived = runtime
        .archive_email_thread(EmailThreadMutationInput {
            thread_id: sent.thread.id.clone(),
        })
        .unwrap();
    assert_eq!(archived.thread.folder, crate::EmailThreadFolder::Archive);

    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let synced = runtime
        .get_email_thread(EmailThreadGetInput {
            thread_id: sent.thread.id,
            cursor: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(synced.thread.folder, crate::EmailThreadFolder::Archive);
    assert_eq!(synced.messages.len(), 2);
    assert_eq!(
        runtime
            .list_email_threads(ListEmailThreadsInput {
                mailbox_ref_id: Some(mailbox.mailbox_ref_id.clone()),
                folder: EmailFolder::Sent,
                sort: EmailThreadSort::Newest,
                limit: None,
                cursor: None,
            })
            .unwrap()
            .items
            .len(),
        1,
        "archived mixed thread remains in Sent"
    );
    assert_eq!(
        runtime
            .list_email_threads(ListEmailThreadsInput {
                mailbox_ref_id: Some(mailbox.mailbox_ref_id),
                folder: EmailFolder::Archive,
                sort: EmailThreadSort::Newest,
                limit: None,
                cursor: None,
            })
            .unwrap()
            .items
            .len(),
        1,
        "archived mixed thread remains in Archive"
    );
}

#[tokio::test]
async fn a_sync_reports_what_it_still_owes_a_verdict_on() {
    // 提醒放在指令里只是请求,放在返回值里就是事实:同步结果自己说出
    // 还有哪几封没人判断过,判断完再同步一次,这份清单会空掉。
    let inbound = PlatformEmailMessage {
        provider_message_id: "received-1".into(),
        internet_message_id: Some("<received-1@example.net>".into()),
        from: "Creator <creator@example.com>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: vec!["outreach@example.com".into()],
        reply_to: Vec::new(),
        subject: "Re: Partnership".into(),
        text: Some("能寄样品吗？".into()),
        html: None,
        headers: serde_json::json!({ "Message-Id": "<received-1@example.net>" }),
        last_event: "received".into(),
        created_at: "2026-08-05T11:00:00Z".into(),
        inbound: true,
        attachments: Vec::new(),
    };
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let synced = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let pending = synced
        .pending_triage
        .as_ref()
        .expect("triage debt reported");
    assert_eq!(pending.total, 1, "刚收进来的信还没人判断");
    let owed = &pending.items[0];
    assert_eq!(owed.channel, crate::ReplyChannel::Email);
    assert_eq!(owed.from, "Creator <creator@example.com>");
    assert_eq!(owed.subject.as_deref(), Some("Re: Partnership"));
    assert!(owed.excerpt.contains("能寄样品吗"), "{}", owed.excerpt);
    let email_id = owed.email_id.clone().expect("邮箱回信要指名是哪封信");

    runtime
        .record_reply_triage(crate::RecordReplyTriageInput {
            email_id: Some(email_id),
            message_id: None,
            intent: crate::ReplyIntent::Question,
            summary: "问能不能寄样品".into(),
            suggested_reply: None,
        })
        .unwrap();

    let again = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let cleared = again.pending_triage.as_ref().expect("triage debt reported");
    assert_eq!(cleared.total, 0, "判断过了就不再欠着");
    assert!(cleared.items.is_empty());
}
