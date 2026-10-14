use super::*;
#[test]
fn current_schema_contains_strict_local_mail_state() {
    let (directory, _) = repository();
    let connection = Connection::open(directory.path().join("crm.db")).expect("open db");
    let version = connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .expect("schema version");
    assert_eq!(version, schema::CURRENT_SCHEMA_VERSION);
    for table in ["email_templates", "email_drafts"] {
        let exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get::<_, bool>(0),
            )
            .expect("mail table");
        assert!(exists, "missing {table}");
    }
    let columns = |table: &str| {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("table columns");
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .expect("column rows")
            .collect::<Result<Vec<_>, _>>()
            .expect("column names")
    };
    assert!(columns("email_threads").contains(&"active_folder".to_string()));
    assert!(columns("email_threads").contains(&"archived_at".to_string()));
    assert!(columns("email_threads").contains(&"deleted_at".to_string()));
    assert!(columns("email_drafts").contains(&"sending_idempotency_key".to_string()));
    assert!(columns("email_drafts").contains(&"send_reserved_at".to_string()));
    assert!(columns("idempotency_records").contains(&"mailbox_ref_id".to_string()));
    assert!(columns("idempotency_records").contains(&"request_fingerprint".to_string()));
    let idempotency_sql = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'idempotency_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("idempotency schema");
    assert!(idempotency_sql.contains("remote_accepted"));
    assert!(idempotency_sql.contains("remote_verified"));
}

#[test]
fn repository_owns_folder_filter_counts_and_preserves_explicit_disposition() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Mail".into(),
            email_address: "mail@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox, EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("mailbox");
    let snapshot = |key: &str, message_id: &str, direction: EmailDirection| EmailSnapshotInput {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        creator_id: None,
        contact_route_id: None,
        provider_thread_key: key.into(),
        subject: key.into(),
        messages: vec![EmailMessageInput {
            provider_message_id: message_id.into(),
            internet_message_id: Some(format!("<{message_id}@example.com>")),
            direction,
            from: if direction == EmailDirection::Inbound {
                "creator@example.com".into()
            } else {
                mailbox.email_address.clone()
            },
            to: if direction == EmailDirection::Inbound {
                vec![mailbox.email_address.clone()]
            } else {
                vec!["creator@example.com".into()]
            },
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject: key.into(),
            text: Some("body".into()),
            html: None,
            delivery_status: if direction == EmailDirection::Inbound {
                "unread".into()
            } else {
                "sent".into()
            },
            headers: serde_json::json!({}),
            occurred_at: 1,
            origin: crate::models::EmailOrigin::ProviderImport,
            attachments: Vec::new(),
        }],
    };
    let (inbox, _) = repository
        .persist_email_snapshot(snapshot(
            "inbox-thread",
            "inbound-1",
            EmailDirection::Inbound,
        ))
        .expect("inbox thread");
    let (sent, _) = repository
        .persist_email_snapshot(snapshot(
            "sent-thread",
            "outbound-1",
            EmailDirection::Outbound,
        ))
        .expect("sent thread");
    assert_eq!(inbox.folder, EmailThreadFolder::Inbox);
    assert_eq!(sent.folder, EmailThreadFolder::Sent);
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Inbox,
                crate::EmailThreadSort::Newest,
                100,
                None
            )
            .expect("inbox")
            .items
            .len(),
        1
    );
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Sent,
                crate::EmailThreadSort::Newest,
                100,
                None
            )
            .expect("sent")
            .items
            .len(),
        1
    );

    let sent_error = repository
        .archive_email_thread(&sent.id)
        .expect_err("outbound-only thread must not be archived");
    assert!(sent_error.contains("outbound-only"), "{sent_error}");

    let (mixed, _) = repository
        .persist_email_snapshot(snapshot(
            "mixed-thread",
            "mixed-outbound-1",
            EmailDirection::Outbound,
        ))
        .expect("mixed outbound thread");
    repository
        .persist_email_snapshot(snapshot(
            "mixed-thread",
            "mixed-inbound-1",
            EmailDirection::Inbound,
        ))
        .expect("mixed inbound thread");
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Inbox,
                crate::EmailThreadSort::Newest,
                100,
                None,
            )
            .expect("mixed inbox")
            .items
            .len(),
        2
    );
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Sent,
                crate::EmailThreadSort::Newest,
                100,
                None,
            )
            .expect("mixed sent")
            .items
            .len(),
        2
    );
    assert_eq!(
        repository
            .archive_email_thread(&mixed.id)
            .expect("archive mixed thread")
            .folder,
        EmailThreadFolder::Archive
    );
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Inbox,
                crate::EmailThreadSort::Newest,
                100,
                None,
            )
            .expect("archived mixed inbox")
            .items
            .len(),
        1
    );
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Sent,
                crate::EmailThreadSort::Newest,
                100,
                None,
            )
            .expect("archived mixed sent")
            .items
            .len(),
        2
    );
    assert_eq!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Archive,
                crate::EmailThreadSort::Newest,
                100,
                None,
            )
            .expect("mixed archive")
            .items
            .len(),
        1
    );
    let mixed_counts = repository
        .list_email_folder_counts(Some(&mailbox.mailbox_ref_id))
        .expect("mixed folder counts");
    let mixed_count = |folder| {
        mixed_counts
            .iter()
            .find(|summary| summary.folder == folder)
            .map(|summary| summary.count)
            .unwrap()
    };
    assert_eq!(mixed_count(EmailFolder::Inbox), 1);
    assert_eq!(mixed_count(EmailFolder::Sent), 2);
    assert_eq!(mixed_count(EmailFolder::Archive), 1);
    assert_eq!(
        repository
            .unarchive_email_thread(&mixed.id)
            .expect("unarchive mixed thread")
            .folder,
        EmailThreadFolder::Inbox
    );
    repository
        .archive_email_thread(&mixed.id)
        .expect("archive mixed thread for restore");
    repository
        .trash_email_thread(&mixed.id)
        .expect("delete mixed thread");
    assert_eq!(
        repository
            .restore_email_thread(&mixed.id)
            .expect("restore mixed thread")
            .folder,
        EmailThreadFolder::Archive
    );
    assert_eq!(
        repository
            .unarchive_email_thread(&mixed.id)
            .expect("unarchive restored mixed thread")
            .folder,
        EmailThreadFolder::Inbox
    );
    repository
        .trash_email_thread(&sent.id)
        .expect("delete outbound-only thread");
    assert_eq!(
        repository
            .restore_email_thread(&sent.id)
            .expect("restore outbound-only thread")
            .folder,
        EmailThreadFolder::Sent
    );

    repository
        .archive_email_thread(&inbox.id)
        .expect("archive thread");
    repository
        .persist_email_snapshot(snapshot(
            "inbox-thread",
            "inbound-2",
            EmailDirection::Inbound,
        ))
        .expect("sync archived thread");
    let archived = repository
        .get_email_thread_record(&inbox.id)
        .expect("archived thread");
    assert_eq!(archived.folder, EmailThreadFolder::Archive);
    repository
        .trash_email_thread(&inbox.id)
        .expect("trash archived thread");
    assert_eq!(
        repository
            .restore_email_thread(&inbox.id)
            .expect("restore archived thread")
            .folder,
        EmailThreadFolder::Archive
    );
    assert_eq!(
        repository
            .unarchive_email_thread(&inbox.id)
            .expect("unarchive thread")
            .folder,
        EmailThreadFolder::Inbox
    );
    assert!(
        repository
            .list_email_threads(
                Some(&mailbox.mailbox_ref_id),
                EmailFolder::Drafts,
                crate::EmailThreadSort::Newest,
                100,
                None
            )
            .expect("draft thread filter")
            .items
            .is_empty()
    );

    let counts = repository
        .list_email_folder_counts(Some(&mailbox.mailbox_ref_id))
        .expect("folder counts");
    let count = |folder| {
        counts
            .iter()
            .find(|summary| summary.folder == folder)
            .map(|summary| summary.count)
            .unwrap()
    };
    assert_eq!(count(EmailFolder::All), 3);
    assert_eq!(count(EmailFolder::Inbox), 2);
    assert_eq!(count(EmailFolder::Sent), 2);
    assert_eq!(count(EmailFolder::Archive), 0);
    assert_eq!(count(EmailFolder::Deleted), 0);
}

#[test]
fn mailbox_reconcile_preserves_local_drafts_actions_and_templates_for_missing_credential() {
    let (_directory, repository) = repository();
    let stale = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
            provider: "resend".into(),
            label: "Stale".into(),
            email_address: "stale@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("stale mailbox");
    let current = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
            provider: "resend".into(),
            label: "Current".into(),
            email_address: "current@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("current mailbox");
    let template = repository
        .save_email_template(
            SaveEmailTemplateInput {
                id: None,
                name: "Global".into(),
                subject: "Hello".into(),
                text: "Body".into(),
                html: None,
                expected_revision: None,
            },
            Vec::new(),
        )
        .expect("global template");
    let draft = repository
        .save_email_draft(SaveEmailDraftInput {
            id: None,
            mailbox_ref_id: stale.mailbox_ref_id.clone(),
            to: "creator@example.com".into(),
            creator_id: None,
            contact_route_id: None,
            reply_to_email_id: None,
            subject: "Draft".into(),
            text: "Body".into(),
            html: None,
            template_id: Some(template.id.clone()),
            template_variables: std::collections::BTreeMap::new(),
            expected_revision: None,
        })
        .expect("draft");
    {
        let connection = repository.lock().expect("repository lock");
        connection
            .execute(
                "INSERT INTO idempotency_records
                   (key, action, mailbox_ref_id, request_fingerprint, state, result_json,
                    created_at, updated_at)
                 VALUES ('stale-pending', 'email_draft_send', ?1, ?2,
                         'in_progress', NULL, 1, 1)",
                rusqlite::params![
                    stale.mailbox_ref_id.as_str(),
                    serde_json::json!({
                        "mailbox_ref_id": stale.mailbox_ref_id.as_str(),
                        "draft_id": draft.id.as_str(),
                        "expected_revision": draft.revision,
                    })
                    .to_string()
                ],
            )
            .expect("legacy pending draft action");
        connection
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = 'stale-pending', send_reserved_at = 1
                 WHERE id = ?1",
                [draft.id.as_str()],
            )
            .expect("legacy draft reservation");
    }
    repository
        .begin_mailbox_action("current-pending", "email_send", &current.mailbox_ref_id)
        .expect("current action");

    repository
        .reconcile_mailbox_refs(vec![MailboxRefInput {
            id: current.mailbox_ref_id.clone(),
            provider: "resend".into(),
            label: current.label,
            email_address: current.email_address,
            display_name: current.display_name,
            capabilities: current.capabilities,
            available: current.available,
            last_error: None,
        }])
        .expect("mailbox lifecycle reconcile");

    let retained_draft = repository
        .get_email_draft(&draft.id)
        .expect("retained draft");
    assert!(retained_draft.sending);
    assert_eq!(
        retained_draft.pending_send_idempotency_key.as_deref(),
        Some("stale-pending")
    );
    let stale_mailbox = repository
        .find_mailbox_ref(&stale.mailbox_ref_id)
        .expect("read retained stale mailbox")
        .expect("retained stale mailbox");
    assert!(!stale_mailbox.available);
    assert_eq!(
        repository
            .get_email_template(&template.id)
            .expect("global template")
            .name,
        "Global"
    );
    assert!(
        repository
            .find_action("stale-pending", "email_draft_send")
            .is_err()
    );
    assert!(
        repository
            .find_action("current-pending", "email_send")
            .is_err()
    );
}

#[test]
fn mail_lists_are_keyset_paged_with_context_bound_cursors() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("22222222-2222-4222-8222-222222222222"),
            provider: "resend".into(),
            label: "Paging".into(),
            email_address: "paging@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox, EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("mailbox");
    let mut persisted_threads = Vec::new();
    for index in 0..5 {
        let (thread, _) = repository
            .persist_email_snapshot(EmailSnapshotInput {
                mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
                creator_id: None,
                contact_route_id: None,
                provider_thread_key: format!("thread-{index}"),
                subject: format!("Subject {index}"),
                messages: vec![EmailMessageInput {
                    provider_message_id: format!("message-{index}"),
                    internet_message_id: Some(format!("<message-{index}@example.com>")),
                    direction: EmailDirection::Inbound,
                    from: "creator@example.com".into(),
                    to: vec![mailbox.email_address.clone()],
                    cc: Vec::new(),
                    bcc: vec!["hidden@example.com".into()],
                    received_for: Vec::new(),
                    reply_to: Vec::new(),
                    in_reply_to: None,
                    references: Vec::new(),
                    subject: format!("Subject {index}"),
                    text: Some("preview body ".repeat(40)),
                    html: Some("<p>html</p>".into()),
                    delivery_status: "unread".into(),
                    headers: serde_json::json!({"x-raw": "never-public"}),
                    occurred_at: 100 + index,
                    origin: crate::models::EmailOrigin::ProviderImport,
                    attachments: Vec::new(),
                }],
            })
            .expect("thread");
        persisted_threads.push(thread);
    }
    repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: "thread-4".into(),
            subject: "Subject 4".into(),
            messages: vec![EmailMessageInput {
                provider_message_id: "reply-4".into(),
                internet_message_id: Some("<reply-4@example.com>".into()),
                direction: EmailDirection::Outbound,
                from: mailbox.email_address.clone(),
                to: vec!["creator@example.com".into()],
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: Some("<message-4@example.com>".into()),
                references: vec!["<message-4@example.com>".into()],
                subject: "Subject 4".into(),
                text: Some("preview body ".repeat(40)),
                html: Some("<p>Reply</p>".into()),
                delivery_status: "sent".into(),
                headers: serde_json::json!({}),
                occurred_at: 105,
                origin: crate::models::EmailOrigin::ProviderImport,
                attachments: Vec::new(),
            }],
        })
        .expect("reply");
    repository
        .archive_email_thread(&persisted_threads[0].id)
        .expect("archive oldest thread without changing its message time");

    let first = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Newest,
            2,
            None,
        )
        .expect("first page");
    assert_eq!(first.items.len(), 2);
    let cursor = first.next_cursor.expect("continuation");
    let preview = first.items[0].latest_message.as_ref().expect("preview");
    assert_eq!(preview.occurred_at, 105);
    assert_eq!(first.items[0].message_count, 2);
    assert!(first.items[0].has_outbound);
    assert!(preview.has_html);
    assert_eq!(
        preview
            .preview_text
            .as_ref()
            .expect("preview text")
            .chars()
            .count(),
        crate::EMAIL_PREVIEW_MAX_LENGTH
    );

    let second = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Newest,
            2,
            Some(&cursor),
        )
        .expect("second page");
    assert_eq!(second.items.len(), 2);
    assert!(
        second.items[0]
            .latest_message
            .as_ref()
            .expect("latest message")
            .occurred_at
            <= first.items[1]
                .latest_message
                .as_ref()
                .expect("latest message")
                .occurred_at
    );
    let third = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Newest,
            2,
            second.next_cursor.as_deref(),
        )
        .expect("third page");
    assert_eq!(third.items.len(), 1);
    assert!(third.next_cursor.is_none());

    let oldest = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Oldest,
            2,
            None,
        )
        .expect("oldest page");
    assert_eq!(
        oldest.items[0]
            .latest_message
            .as_ref()
            .expect("latest message")
            .occurred_at,
        100
    );
    assert_eq!(
        oldest.items[1]
            .latest_message
            .as_ref()
            .expect("latest message")
            .occurred_at,
        101
    );

    // A cursor is bound to its exact query context.
    let error = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::Inbox,
            crate::EmailThreadSort::Newest,
            2,
            Some(&cursor),
        )
        .expect_err("foreign context");
    assert!(error.contains("not valid for this query"), "{error}");
    let error = repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Oldest,
            2,
            Some(&cursor),
        )
        .expect_err("foreign sort context");
    assert!(error.contains("not valid for this query"), "{error}");

    // Thread messages page newest first with the same cursor discipline.
    let thread_id = first.items[0].thread.id.clone();
    for index in 0..3 {
        repository
            .persist_email_snapshot(EmailSnapshotInput {
                mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
                creator_id: None,
                contact_route_id: None,
                provider_thread_key: first.items[0].thread.provider_thread_key.clone(),
                subject: first.items[0].thread.subject.clone(),
                messages: vec![EmailMessageInput {
                    provider_message_id: format!("extra-{index}"),
                    internet_message_id: Some(format!("<extra-{index}@example.com>")),
                    direction: EmailDirection::Inbound,
                    from: "creator@example.com".into(),
                    to: vec![mailbox.email_address.clone()],
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    received_for: Vec::new(),
                    reply_to: Vec::new(),
                    in_reply_to: None,
                    references: Vec::new(),
                    subject: "extra".into(),
                    text: Some("extra".into()),
                    html: None,
                    delivery_status: "unread".into(),
                    headers: serde_json::json!({}),
                    occurred_at: 200 + index,
                    origin: crate::models::EmailOrigin::ProviderImport,
                    attachments: Vec::new(),
                }],
            })
            .expect("extra message");
    }
    let messages = repository
        .get_email_thread_messages(&thread_id, 2, None)
        .expect("message page");
    assert_eq!(messages.items.len(), 2);
    assert!(messages.items[0].occurred_at >= messages.items[1].occurred_at);
    let rest = repository
        .get_email_thread_messages(&thread_id, 10, messages.next_cursor.as_deref())
        .expect("older messages");
    assert_eq!(rest.items.len(), 3);
    assert!(rest.next_cursor.is_none());
}

#[test]
fn marking_a_thread_read_clears_unread_without_moving_it_and_survives_resync() {
    let (_directory, repository) = repository();
    let mailbox = repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new("44444444-4444-4444-8444-444444444444"),
            provider: "resend".into(),
            label: "Mail".into(),
            email_address: "mail@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox, EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("mailbox");
    let inbound = |message_id: &str| EmailSnapshotInput {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        creator_id: None,
        contact_route_id: None,
        provider_thread_key: "read-thread".into(),
        subject: "Read state".into(),
        messages: vec![EmailMessageInput {
            provider_message_id: message_id.into(),
            internet_message_id: Some(format!("<{message_id}@example.com>")),
            direction: EmailDirection::Inbound,
            from: "creator@example.com".into(),
            to: vec![mailbox.email_address.clone()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: Vec::new(),
            reply_to: Vec::new(),
            in_reply_to: None,
            references: Vec::new(),
            subject: "Read state".into(),
            text: Some("body".into()),
            html: None,
            delivery_status: "unread".into(),
            headers: serde_json::json!({}),
            occurred_at: 1,
            origin: crate::models::EmailOrigin::ProviderImport,
            attachments: Vec::new(),
        }],
    };
    let (thread, _) = repository
        .persist_email_snapshot(inbound("inbound-1"))
        .expect("unread thread");
    assert_eq!(thread.unread_count, 1);

    let read = repository
        .mark_email_thread_read(&thread.id)
        .expect("mark read");
    assert_eq!(read.unread_count, 0);
    assert_eq!(
        read.updated_at, thread.updated_at,
        "read state is attention state and must not reorder the thread list"
    );

    // The provider keeps reporting the same message as unread, so a re-sync must
    // not undo the local read mark.
    let (resynced, _) = repository
        .persist_email_snapshot(inbound("inbound-1"))
        .expect("resync same message");
    assert_eq!(resynced.unread_count, 0);

    // A genuinely new inbound message is still unread.
    let (with_new, _) = repository
        .persist_email_snapshot(inbound("inbound-2"))
        .expect("new message");
    assert_eq!(with_new.unread_count, 1);

    assert!(
        repository
            .mark_email_thread_read(&crate::EmailThreadId::new("missing"))
            .is_err()
    );
}
