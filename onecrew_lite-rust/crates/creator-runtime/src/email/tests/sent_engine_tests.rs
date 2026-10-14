//! The sent-history engine: fresh-computer import, in-place reconciliation
//! of locally confirmed sends, RFC-only threading in both processing orders,
//! and the explicit refusal to merge by subject or recipient.

use super::*;
use crate::{EmailDirection, EmailMessageInput, EmailSnapshotInput};

fn sent(
    provider_message_id: &str,
    internet_message_id: Option<&str>,
    created_at: &str,
    to: &str,
    last_event: &str,
) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: provider_message_id.into(),
        internet_message_id: internet_message_id.map(str::to_owned),
        from: "Reach <outreach@example.com>".into(),
        to: vec![to.into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: Vec::new(),
        reply_to: Vec::new(),
        subject: "Partnership".into(),
        text: Some("Hello there".into()),
        html: None,
        headers: serde_json::json!({}),
        last_event: last_event.into(),
        created_at: created_at.into(),
        inbound: false,
        attachments: Vec::new(),
    }
}

fn sent_summary(message: &PlatformEmailMessage) -> PlatformSentEmailSummary {
    PlatformSentEmailSummary {
        provider_message_id: message.provider_message_id.clone(),
        internet_message_id: message.internet_message_id.clone(),
        from: message.from.clone(),
        to: message.to.clone(),
        cc: message.cc.clone(),
        bcc: message.bcc.clone(),
        reply_to: message.reply_to.clone(),
        subject: message.subject.clone(),
        last_event: message.last_event.clone(),
        created_at: message.created_at.clone(),
    }
}

fn sent_page(messages: &[&PlatformEmailMessage], has_more: bool) -> PlatformSentEmailPage {
    PlatformSentEmailPage {
        messages: messages
            .iter()
            .map(|message| sent_summary(message))
            .collect(),
        has_more,
        next_cursor: messages
            .last()
            .map(|message| message.provider_message_id.clone()),
        bounds: messages
            .first()
            .zip(messages.last())
            .map(
                |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                    newest_provider_message_id: newest.provider_message_id.clone(),
                    newest_created_at: newest.created_at.clone(),
                    oldest_created_at: oldest.created_at.clone(),
                },
            ),
    }
}

fn inbound_reply(
    provider_message_id: &str,
    created_at: &str,
    references: &str,
) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(format!("<{provider_message_id}@example.net>")),
        from: "Creator <creator@example.net>".into(),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: vec!["outreach@example.com".into()],
        reply_to: Vec::new(),
        subject: "Re: Partnership".into(),
        text: Some("Sounds good".into()),
        html: None,
        headers: serde_json::json!({
            "in-reply-to": references,
            "references": references
        }),
        last_event: "received".into(),
        created_at: created_at.into(),
        inbound: true,
        attachments: Vec::new(),
    }
}

fn received_page(messages: &[&PlatformEmailMessage], has_more: bool) -> PlatformEmailPage {
    PlatformEmailPage {
        messages: messages
            .iter()
            .map(|message| PlatformEmailSummary {
                provider_message_id: message.provider_message_id.clone(),
                created_at: message.created_at.clone(),
            })
            .collect(),
        has_more,
        next_cursor: messages
            .last()
            .map(|message| message.provider_message_id.clone()),
        bounds: messages
            .first()
            .zip(messages.last())
            .map(
                |(newest, oldest)| onecrew_email_contracts::PlatformPageBounds {
                    newest_provider_message_id: newest.provider_message_id.clone(),
                    newest_created_at: newest.created_at.clone(),
                    oldest_created_at: oldest.created_at.clone(),
                },
            ),
    }
}

fn side_counts(directory: &tempfile::TempDir) -> (i64, i64, i64) {
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let touchpoints: i64 = connection
        .query_row("SELECT COUNT(*) FROM touchpoints", [], |row| row.get(0))
        .unwrap();
    let audits: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .unwrap();
    let idempotency: i64 = connection
        .query_row("SELECT COUNT(*) FROM idempotency_records", [], |row| {
            row.get(0)
        })
        .unwrap();
    (touchpoints, audits, idempotency)
}

fn referencing_snapshot(
    mailbox_ref_id: &crate::MailboxRefId,
    provider_message_id: &str,
    provider_thread_key: &str,
    occurred_at: u64,
    references: &[&str],
) -> EmailSnapshotInput {
    EmailSnapshotInput {
        mailbox_ref_id: mailbox_ref_id.clone(),
        creator_id: None,
        contact_route_id: None,
        provider_thread_key: provider_thread_key.into(),
        subject: "Re: Partnership".into(),
        messages: vec![EmailMessageInput {
            provider_message_id: provider_message_id.into(),
            internet_message_id: Some(format!("<{provider_message_id}@example.net>")),
            direction: EmailDirection::Inbound,
            from: "Creator <creator@example.net>".into(),
            to: vec!["outreach@example.com".into()],
            cc: Vec::new(),
            bcc: Vec::new(),
            received_for: vec!["outreach@example.com".into()],
            reply_to: Vec::new(),
            in_reply_to: None,
            references: references.iter().map(|value| (*value).to_owned()).collect(),
            subject: "Re: Partnership".into(),
            text: Some("reply".into()),
            html: None,
            delivery_status: "unread".into(),
            headers: serde_json::json!({}),
            occurred_at,
            origin: crate::models::EmailOrigin::ProviderImport,
            attachments: Vec::new(),
        }],
    }
}

#[tokio::test]
async fn sent_repository_segments_keep_current_thread_responsive_and_commit_after_cancel() {
    let message = sent(
        "sent-segment",
        Some("<sent-segment@mail.example.com>"),
        "2026-08-05T12:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![message.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(
            &[&message],
            true,
        )]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    let mut changes = runtime.subscribe_changes();
    let (preflight_started, release_preflight) = runtime.install_next_repository_read_barrier();
    let sync_runtime = runtime.clone();
    let mailbox_ref_id = mailbox.mailbox_ref_id.clone();
    let sync = tokio::spawn(async move { sync_runtime.sync_mailbox_sent(&mailbox_ref_id).await });
    preflight_started
        .await
        .expect("preflight read did not start");
    assert!(gateway.sent_list_cursors.lock().unwrap().is_empty());
    let (dedupe_started, release_dedupe) = runtime.install_next_repository_read_barrier();
    release_preflight.send(()).unwrap();
    dedupe_started
        .await
        .expect("page dedupe read did not start");
    assert_eq!(gateway.sent_list_cursors.lock().unwrap().len(), 1);
    assert!(gateway.sent_detail_calls.lock().unwrap().is_empty());
    let (write_started, release_write) = runtime.install_next_repository_write_barrier();
    release_dedupe.send(()).unwrap();
    write_started.await.expect("page writer did not start");
    assert_eq!(gateway.sent_detail_calls.lock().unwrap().len(), 1);
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("sent page writer blocked current-thread runtime")
    .unwrap();
    sync.abort();
    assert!(sync.await.unwrap_err().is_cancelled());
    release_write.send(()).unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
        .await
        .expect("cancelled caller lost sent-page invalidation")
        .unwrap();
    assert_eq!(
        change.scopes,
        vec![
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads
        ]
    );
    assert_eq!(fact_count(&directory, "emails"), 1);
    let state_before = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_before.newest_provider_message_id.as_deref(),
        Some("sent-segment")
    );
    assert!(!state_before.backfill_complete);
    assert!(state_before.last_full_sync_at.is_none());
    assert!(state_before.failure.is_none());
    let continuation = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.incremental_continuation)
        .unwrap()
        .expect("cancelled caller lost the committed Phase A continuation");
    assert_eq!(continuation.next_cursor, "sent-segment");
    assert_eq!(continuation.newest_provider_message_id, "sent-segment");
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert!(report.full_sync_recorded);
    assert_eq!(gateway.sent_detail_calls.lock().unwrap().len(), 1);
    assert_eq!(
        gateway.sent_list_cursors.lock().unwrap()[1].as_deref(),
        Some("sent-segment"),
        "the retry did not resume from the committed continuation"
    );
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound,)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .is_none()
    );
    let state_completed = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state_completed.backfill_complete);
    assert!(state_completed.last_full_sync_at.is_some());
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert!(!report.full_sync_recorded);
    let state_after = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_after, state_completed,
        "quiet overlap rewrote checkpoint"
    );
    assert!(
        changes.try_recv().is_err(),
        "quiet overlap published a change"
    );
}

#[tokio::test]
async fn a_fresh_computer_imports_sent_history_idempotently() {
    let newer = sent(
        "sent-2",
        Some("<sent-2@mail.example.com>"),
        "2026-08-05T12:00:00Z",
        "creator-two@example.net",
        "delivered",
    );
    let older = sent(
        "sent-1",
        Some("<sent-1@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator-one@example.net",
        "opened",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![newer.clone(), older.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            sent_page(&[&newer], true),
            sent_page(&[&older], false),
            // Second run: one overlap page containing the watermark item.
            sent_page(&[&newer, &older], true),
        ]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2);
    assert_eq!(report.imported, 2);
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);

    // Bodies, identities, and statuses are canonical local facts now.
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
    assert_eq!(threads.items.len(), 2);
    let mut statuses = Vec::new();
    for summary in &threads.items {
        let messages = runtime
            .repository
            .get_email_thread_messages(&summary.thread.id, 10, None)
            .unwrap()
            .items;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text.as_deref(), Some("Hello there"));
        statuses.push(messages[0].delivery_status.clone());
    }
    statuses.sort();
    assert_eq!(statuses, vec!["delivered".to_owned(), "opened".to_owned()]);
    // Imported history is not a local outreach action: no touchpoint,
    // idempotency result, or audit event exists.
    assert_eq!(side_counts(&directory), (0, 0, 0));

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(state.newest_provider_message_id.as_deref(), Some("sent-2"));

    // Replaying the same pages produces nothing new.
    let details_before = gateway.sent_detail_calls.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped_complete, 2);
    assert_eq!(report.pages_listed, 1, "the walk stops at the watermark");
    assert_eq!(
        gateway.sent_detail_calls.lock().unwrap().len(),
        details_before,
        "immutable complete sent bodies are never retrieved again"
    );
    assert_eq!(side_counts(&directory), (0, 0, 0));
}

#[tokio::test]
async fn sent_existing_unfinished_state_reaching_floor_on_page_200_is_not_budget_failure() {
    let old = sent(
        "sent-page-200-watermark",
        Some("<sent-page-200-watermark@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![old.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&old], false)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE mailbox_sync_state
             SET oldest_backfill_cursor = 'stale-cursor', backfill_complete = 0
             WHERE mailbox_ref_id = ?1 AND direction = 'outbound'",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();
    let pages = (1..=200)
        .map(|index| PlatformSentEmailPage {
            messages: Vec::new(),
            has_more: index < 200,
            next_cursor: Some(format!("floor-{index}")),
            bounds: Some(onecrew_email_contracts::PlatformPageBounds {
                newest_provider_message_id: "team-newest".into(),
                newest_created_at: "2026-08-05T12:00:00Z".into(),
                oldest_created_at: "2026-08-05T11:00:00Z".into(),
            }),
        })
        .collect::<VecDeque<_>>();
    *gateway.sent_pages.lock().unwrap() = pages;

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 200);
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);
    assert!(report.missing_provider_message_ids.is_empty());
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.backfill_complete);
    assert!(state.last_full_sync_at.is_some());
    assert!(state.failure.is_none());
}

#[tokio::test]
async fn sent_phase_b_reaching_floor_on_total_page_200_is_not_budget_failure() {
    let old = sent(
        "sent-phase-b-page-200",
        Some("<sent-phase-b-page-200@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![old.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&old], false)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE mailbox_sync_state
             SET oldest_backfill_cursor = 'phase-b-stale', backfill_complete = 0
             WHERE mailbox_ref_id = ?1 AND direction = 'outbound'",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();
    let mut pages = VecDeque::from(vec![sent_page(&[&old], true)]);
    pages.extend((2..=200).map(|index| PlatformSentEmailPage {
        messages: Vec::new(),
        has_more: index < 200,
        next_cursor: Some(format!("phase-b-floor-{index}")),
        bounds: Some(onecrew_email_contracts::PlatformPageBounds {
            newest_provider_message_id: "team-old-history".into(),
            newest_created_at: "2026-08-04T12:00:00Z".into(),
            oldest_created_at: "2026-08-04T11:00:00Z".into(),
        }),
    }));
    *gateway.sent_pages.lock().unwrap() = pages;

    let listed_before = gateway.sent_list_cursors.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 200);
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);
    assert!(report.missing_provider_message_ids.is_empty());
    let cursors = gateway.sent_list_cursors.lock().unwrap();
    assert_eq!(cursors[listed_before], None);
    assert_eq!(cursors[listed_before + 1].as_deref(), Some("phase-b-stale"));
    drop(cursors);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("phase-b-floor-200")
    );
    assert!(state.failure.is_none());
}

#[tokio::test]
async fn sent_page_budget_persists_cursor_and_the_next_run_resumes() {
    let mut messages = Vec::new();
    for index in 0..201_u32 {
        let t = 50_000 - index;
        let created_at = format!(
            "2026-08-04T{:02}:{:02}:{:02}Z",
            t / 3600,
            (t % 3600) / 60,
            t % 60
        );
        messages.push(sent(
            &format!("sent-{index:03}"),
            Some(&format!("<sent-{index:03}@mail.example.com>")),
            &created_at,
            "creator@example.net",
            "delivered",
        ));
    }
    let first_run_pages = messages
        .iter()
        .take(200)
        .map(|message| sent_page(&[message], true))
        .collect::<VecDeque<_>>();
    let gateway = TestGateway {
        sent_history: Arc::new(messages.clone()),
        sent_pages: Arc::new(Mutex::new(first_run_pages)),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 200);
    assert_eq!(report.imported, 200);
    assert!(!report.backfill_complete);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.failure.as_ref().unwrap().error_class,
        "page_budget_exhausted"
    );
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("sent-000")
    );
    assert_eq!(state.oldest_backfill_cursor.as_deref(), Some("sent-199"));
    assert!(!state.backfill_complete);

    gateway.sent_pages.lock().unwrap().extend([
        sent_page(&[&messages[0]], true),
        sent_page(&[&messages[200]], false),
    ]);
    let listed_before = gateway.sent_list_cursors.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2);
    assert_eq!(report.imported, 1);
    assert!(report.backfill_complete);
    let cursors = gateway.sent_list_cursors.lock().unwrap();
    assert_eq!(cursors[listed_before].as_deref(), Some("sent-199"));
    assert_eq!(cursors[listed_before + 1].as_deref(), Some("sent-000"));
    drop(cursors);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.failure.is_none());
    assert!(state.backfill_complete);
}

async fn assert_sent_existing_watermark_phase_a_continues(backfill_complete: bool) {
    let mut messages = (0..=200_u32)
        .map(|index| {
            sent(
                &format!("sent-new-{index:03}"),
                Some(&format!("<sent-new-{index:03}@mail.example.com>")),
                "2026-08-05T12:00:00Z",
                "creator@example.net",
                "delivered",
            )
        })
        .collect::<Vec<_>>();
    let old_watermark = sent(
        "sent-existing-watermark",
        Some("<sent-existing-watermark@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    messages.push(old_watermark.clone());
    let first_run_pages = messages
        .iter()
        .take(200)
        .map(|message| sent_page(&[message], true))
        .collect::<VecDeque<_>>();
    let gateway = TestGateway {
        sent_history: Arc::new(messages.clone()),
        sent_pages: Arc::new(Mutex::new(first_run_pages)),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, newest_provider_message_id,
                newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                last_page_at, last_full_sync_at, failure_json, updated_at)
             VALUES (?1, 'outbound', 'sent-existing-watermark', 1, ?2, ?3,
                     1, ?4, NULL, 1)",
            rusqlite::params![
                mailbox.mailbox_ref_id.as_str(),
                (!backfill_complete).then_some("sent-phase-b-cursor"),
                backfill_complete,
                backfill_complete.then_some(1_u64),
            ],
        )
        .unwrap();

    let first = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(first.pages_listed, 200);
    assert_eq!(first.imported, 200);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("sent-existing-watermark"),
        "an unfinished Phase A walk moved the old watermark"
    );
    assert_eq!(state.backfill_complete, backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        (!backfill_complete).then_some("sent-phase-b-cursor")
    );
    assert_eq!(
        state
            .failure
            .as_ref()
            .map(|failure| failure.error_class.as_str()),
        Some("page_budget_exhausted")
    );
    let continuation = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.incremental_continuation)
        .unwrap()
        .expect("page 200 continuation");
    assert_eq!(continuation.next_cursor, "sent-new-199");
    assert_eq!(
        continuation.newest_provider_message_id, "sent-new-000",
        "the original first-page anchor was not preserved"
    );

    let mut remainder = VecDeque::from(vec![
        sent_page(&[&messages[200]], true),
        sent_page(&[&old_watermark], true),
    ]);
    if !backfill_complete {
        remainder.push_back(PlatformSentEmailPage {
            messages: Vec::new(),
            has_more: false,
            next_cursor: None,
            bounds: None,
        });
    }
    *gateway.sent_pages.lock().unwrap() = remainder;
    let listed_before = gateway.sent_list_cursors.lock().unwrap().len();
    let second = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(second.pages_listed, if backfill_complete { 2 } else { 3 });
    assert_eq!(second.imported, 2);
    assert!(second.backfill_complete);
    let cursors = gateway.sent_list_cursors.lock().unwrap();
    assert_eq!(
        cursors[listed_before].as_deref(),
        Some("sent-new-199"),
        "Phase A restarted from the newest page"
    );
    assert_eq!(cursors[listed_before + 1].as_deref(), Some("sent-new-200"));
    if !backfill_complete {
        assert_eq!(
            cursors[listed_before + 2].as_deref(),
            Some("sent-phase-b-cursor"),
            "Phase B did not retain its independent cursor"
        );
    }
    drop(cursors);
    let final_state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        final_state.newest_provider_message_id.as_deref(),
        Some("sent-new-000")
    );
    assert!(final_state.backfill_complete);
    assert!(final_state.failure.is_none());
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound,)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .is_none()
    );
    let details = gateway.sent_detail_calls.lock().unwrap();
    assert_eq!(details.len(), 202);
    assert_eq!(
        details
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        details.len(),
        "resumed Phase A fetched a message detail twice"
    );
}

#[tokio::test]
async fn sent_existing_complete_watermark_resumes_phase_a_after_page_200() {
    assert_sent_existing_watermark_phase_a_continues(true).await;
}

#[tokio::test]
async fn sent_existing_unfinished_watermark_resumes_phase_a_before_phase_b() {
    assert_sent_existing_watermark_phase_a_continues(false).await;
}

#[tokio::test]
async fn sent_missing_next_cursor_is_a_durable_failure_for_every_start_state() {
    for (label, backfill_complete, invalid_cursor) in [
        ("initial", None, None),
        ("existing-complete", Some(true), Some("")),
        ("existing-unfinished", Some(false), Some(" \t\r\n")),
    ] {
        let message = sent(
            &format!("sent-null-cursor-{label}"),
            Some(&format!("<sent-null-cursor-{label}@mail.example.com>")),
            "2026-08-05T12:00:00Z",
            "creator@example.net",
            "delivered",
        );
        let gateway = TestGateway {
            sent_history: Arc::new(vec![message.clone()]),
            sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![PlatformSentEmailPage {
                messages: vec![sent_summary(&message)],
                has_more: true,
                next_cursor: invalid_cursor.map(str::to_owned),
                bounds: Some(onecrew_email_contracts::PlatformPageBounds {
                    newest_provider_message_id: message.provider_message_id.clone(),
                    newest_created_at: message.created_at.clone(),
                    oldest_created_at: message.created_at.clone(),
                }),
            }]))),
            ..TestGateway::default()
        };
        let (directory, runtime, gateway) = runtime(gateway);
        let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
        if let Some(backfill_complete) = backfill_complete {
            Connection::open(directory.path().join("crm.db"))
                .unwrap()
                .execute(
                    "INSERT INTO mailbox_sync_state
                       (mailbox_ref_id, direction, newest_provider_message_id,
                        newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                        last_page_at, last_full_sync_at, failure_json, updated_at)
                     VALUES (?1, 'outbound', 'sent-old-watermark', 1, ?2, ?3,
                             1, ?4, NULL, 1)",
                    rusqlite::params![
                        mailbox.mailbox_ref_id.as_str(),
                        (!backfill_complete).then_some("sent-old-phase-b"),
                        backfill_complete,
                        backfill_complete.then_some(1_u64),
                    ],
                )
                .unwrap();
        }

        let report = runtime
            .sync_mailbox_sent(&mailbox.mailbox_ref_id)
            .await
            .unwrap();
        assert_eq!(report.pages_listed, 1, "{label}");
        assert_eq!(
            gateway.sent_list_cursors.lock().unwrap().len(),
            1,
            "{label}"
        );
        let state = runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap();
        assert_eq!(
            state
                .failure
                .as_ref()
                .map(|failure| failure.error_class.as_str()),
            Some("pagination_cursor_missing"),
            "{label}"
        );
        assert_eq!(
            state.newest_provider_message_id.as_deref(),
            backfill_complete.map(|_| "sent-old-watermark"),
            "{label} moved its watermark"
        );
        assert_eq!(
            state.oldest_backfill_cursor.as_deref(),
            backfill_complete
                .filter(|complete| !complete)
                .map(|_| "sent-old-phase-b"),
            "{label} moved its Phase B cursor"
        );
        assert!(
            runtime
                .repository
                .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound,)
                .map(|checkpoint| checkpoint.incremental_continuation)
                .unwrap()
                .is_none(),
            "{label} invented a continuation without a next cursor"
        );
    }
}

#[tokio::test]
async fn sent_nonterminal_page_requires_a_nonblank_newest_anchor() {
    for (label, bounds) in [
        ("missing", None),
        (
            "blank",
            Some(onecrew_email_contracts::PlatformPageBounds {
                newest_provider_message_id: " \t\r\n".into(),
                newest_created_at: "2026-08-05T12:00:00Z".into(),
                oldest_created_at: "2026-08-05T12:00:00Z".into(),
            }),
        ),
    ] {
        let message = sent(
            &format!("sent-anchor-{label}"),
            Some(&format!("<sent-anchor-{label}@mail.example.com>")),
            "2026-08-05T12:00:00Z",
            "creator@example.net",
            "delivered",
        );
        let gateway = TestGateway {
            sent_history: Arc::new(vec![message.clone()]),
            sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![PlatformSentEmailPage {
                messages: vec![sent_summary(&message)],
                has_more: true,
                next_cursor: Some(format!("sent-anchor-cursor-{label}")),
                bounds,
            }]))),
            ..TestGateway::default()
        };
        let (_directory, runtime, _) = runtime(gateway);
        let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);

        runtime
            .sync_mailbox_sent(&mailbox.mailbox_ref_id)
            .await
            .unwrap();
        let checkpoint = runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
            .unwrap();
        assert_eq!(
            checkpoint
                .state
                .as_ref()
                .unwrap()
                .failure
                .as_ref()
                .unwrap()
                .error_class,
            "pagination_anchor_missing",
            "{label}"
        );
        assert!(checkpoint.incremental_continuation.is_none(), "{label}");
    }
}

#[tokio::test]
async fn sent_phase_b_missing_next_cursor_is_not_a_retention_floor() {
    let watermark = sent(
        "sent-phase-b-null-watermark",
        Some("<sent-phase-b-null-watermark@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![watermark.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            sent_page(&[&watermark], true),
            PlatformSentEmailPage {
                messages: Vec::new(),
                has_more: true,
                next_cursor: None,
                bounds: None,
            },
        ]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, newest_provider_message_id,
                newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                last_page_at, last_full_sync_at, failure_json, updated_at)
             VALUES (?1, 'outbound', 'sent-phase-b-null-watermark', 1,
                     'sent-phase-b-null-cursor', 0, 1, NULL, NULL, 1)",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(!state.backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("sent-phase-b-null-cursor")
    );
    assert_eq!(
        state.failure.unwrap().error_class,
        "pagination_cursor_missing"
    );
    assert_eq!(
        gateway.sent_list_cursors.lock().unwrap()[1].as_deref(),
        Some("sent-phase-b-null-cursor")
    );
}

#[tokio::test]
async fn a_locally_confirmed_send_reconciles_status_in_place() {
    let listed = sent(
        "sent-confirmed",
        Some("<confirmed@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![listed.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(
            &[&listed],
            false,
        )]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );

    // The confirmed-send path already persisted this provider id.
    runtime
        .repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: "creator:1:route:1:send:key-1".into(),
            subject: "Partnership".into(),
            messages: vec![EmailMessageInput {
                provider_message_id: "sent-confirmed".into(),
                internet_message_id: Some("<confirmed@mail.example.com>".into()),
                direction: EmailDirection::Outbound,
                from: "Reach <outreach@example.com>".into(),
                to: vec!["creator@example.net".into()],
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: None,
                references: Vec::new(),
                subject: "Partnership".into(),
                text: Some("Hello".into()),
                html: None,
                delivery_status: "sent".into(),
                headers: serde_json::json!({}),
                occurred_at: 100,
                origin: crate::models::EmailOrigin::ConfirmedSend,
                attachments: Vec::new(),
            }],
        })
        .unwrap();

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(
        report.imported, 0,
        "no second email for a known provider id"
    );
    assert_eq!(report.skipped_complete, 1);
    assert_eq!(report.reconciled_statuses, 1);
    assert!(gateway.sent_detail_calls.lock().unwrap().is_empty());

    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let (count, status, origin): (i64, String, String) = connection
        .query_row(
            "SELECT COUNT(*), MAX(delivery_status), MAX(origin) FROM emails",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert_eq!(status, "delivered", "list metadata updated the status");
    assert_eq!(
        origin, "confirmed_send",
        "reconciliation never downgrades confirmed-send provenance"
    );

    // A second listing with the same status writes nothing.
    gateway
        .sent_pages
        .lock()
        .unwrap()
        .push_back(sent_page(&[&listed], false));
    let state_before = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.reconciled_statuses, 0);
    let state_after = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_after, state_before,
        "a quiet run makes no SQLite write"
    );
}

#[tokio::test]
async fn a_detail_batch_error_preserves_status_only_reconciliation_without_checkpoint_advance() {
    let known = sent(
        "sent-known-before-batch-error",
        Some("<known-before-batch-error@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let unknown = sent(
        "sent-unknown-batch-error",
        Some("<unknown-batch-error@mail.example.com>"),
        "2026-08-05T11:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![unknown.clone(), known.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(
            &[&unknown, &known],
            false,
        )]))),
        sent_detail_batch_error: Arc::new(Mutex::new(Some("scripted batch failure".into()))),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    runtime
        .repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: "sent-known-before-batch-error".into(),
            subject: "Partnership".into(),
            messages: vec![EmailMessageInput {
                provider_message_id: known.provider_message_id.clone(),
                internet_message_id: known.internet_message_id.clone(),
                direction: EmailDirection::Outbound,
                from: known.from.clone(),
                to: known.to.clone(),
                cc: Vec::new(),
                bcc: Vec::new(),
                received_for: Vec::new(),
                reply_to: Vec::new(),
                in_reply_to: None,
                references: Vec::new(),
                subject: known.subject.clone(),
                text: known.text.clone(),
                html: None,
                delivery_status: "sent".into(),
                headers: serde_json::json!({}),
                occurred_at: 10,
                origin: crate::models::EmailOrigin::ConfirmedSend,
                attachments: Vec::new(),
            }],
        })
        .unwrap();
    let mut changes = runtime.subscribe_changes();

    let error = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap_err();
    assert_eq!(error, "scripted batch failure");
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let known_status: String = connection
        .query_row(
            "SELECT delivery_status FROM emails WHERE provider_message_id = ?1",
            [&known.provider_message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(known_status, "delivered");
    let unknown_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM emails WHERE provider_message_id = ?1",
            [&unknown.provider_message_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(unknown_count, 0);
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .is_none(),
        "a failed detail batch must not create or advance a checkpoint"
    );
    let change = changes
        .try_recv()
        .expect("status reconciliation publication");
    assert_eq!(
        change.scopes,
        vec![
            CreatorChangeScope::EmailFolders,
            CreatorChangeScope::EmailThreads
        ]
    );
    assert!(
        changes.try_recv().is_err(),
        "status path published more than once"
    );
}

#[tokio::test]
async fn an_imported_root_adopts_earlier_replies_and_merges_proven_siblings() {
    let root = sent(
        "sent-root",
        Some("<root@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let reply_one = inbound_reply("reply-1", "2026-08-05T10:00:00Z", "<root@mail.example.com>");
    // The deeper reply cites its sibling first (client-dependent ordering)
    // and arrives in an earlier run than that sibling: the worst-case
    // fragmentation, where neither key nor lookup can converge them yet.
    let mut reply_two = inbound_reply("reply-2", "2026-08-05T11:00:00Z", "");
    reply_two.headers = serde_json::json!({
        "in-reply-to": "<reply-1@example.net>",
        "references": "<reply-1@example.net> <root@mail.example.com>"
    });
    let gateway = TestGateway {
        received: Arc::new(vec![reply_one.clone(), reply_two.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            received_page(&[&reply_two], false),
            received_page(&[&reply_one, &reply_two], false),
        ]))),
        sent_history: Arc::new(vec![root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&root], false)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Inbox,
            EmailCapability::Send,
            EmailCapability::SentReadback,
        ],
    );

    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let fragmented = runtime
        .repository
        .list_email_threads(
            Some(&mailbox.mailbox_ref_id),
            EmailFolder::All,
            crate::EmailThreadSort::Newest,
            100,
            None,
        )
        .unwrap();
    assert_eq!(
        fragmented.items.len(),
        2,
        "replies citing only the missing root cannot converge yet"
    );

    // Local attention state set before the merge must survive it.
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    connection
        .execute(
            "UPDATE emails SET delivery_status = 'read' WHERE provider_message_id = 'reply-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(
        report.merged_threads, 1,
        "the RFC graph proved one sibling merge"
    );

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
    assert_eq!(
        threads.items.len(),
        1,
        "root and both replies share one thread"
    );
    let messages = runtime
        .repository
        .get_email_thread_messages(&threads.items[0].thread.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(messages.len(), 3);
    let read_state: String = Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT delivery_status FROM emails WHERE provider_message_id = 'reply-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(read_state, "read", "read state survives the merge");
}

#[tokio::test]
async fn same_page_chained_merges_normalize_snapshots_to_the_final_canonical_thread() {
    let first_root = sent(
        "sent-root-a",
        Some("<root-a@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let second_root = sent(
        "sent-root-c",
        Some("<root-c@mail.example.com>"),
        "2026-08-05T09:30:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![second_root.clone(), first_root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(
            &[&second_root, &first_root],
            false,
        )]))),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    let (thread_c, _) = runtime
        .repository
        .persist_email_snapshot(referencing_snapshot(
            &mailbox.mailbox_ref_id,
            "reply-c",
            "reply-thread-c",
            5,
            &["<root-c@mail.example.com>"],
        ))
        .unwrap();
    let (thread_a, _) = runtime
        .repository
        .persist_email_snapshot(referencing_snapshot(
            &mailbox.mailbox_ref_id,
            "reply-a",
            "reply-thread-a",
            10,
            &["<root-a@mail.example.com>", "<root-c@mail.example.com>"],
        ))
        .unwrap();
    let (thread_b, _) = runtime
        .repository
        .persist_email_snapshot(referencing_snapshot(
            &mailbox.mailbox_ref_id,
            "reply-b",
            "reply-thread-b",
            20,
            &["<root-a@mail.example.com>", "<root-c@mail.example.com>"],
        ))
        .unwrap();
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    for (thread, created_at) in [(&thread_c, 5_u64), (&thread_a, 10), (&thread_b, 20)] {
        connection
            .execute(
                "UPDATE email_threads SET created_at = ?2 WHERE id = ?1",
                rusqlite::params![thread.id.as_str(), created_at],
            )
            .unwrap();
    }
    drop(connection);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(report.merged_threads, 2);
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
    assert_eq!(threads.items.len(), 1, "a deleted alias was recreated");
    assert_eq!(threads.items[0].thread.id, thread_c.id);
    let messages = runtime
        .repository
        .get_email_thread_messages(&thread_c.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(messages.len(), 5, "a chained merge lost a message");
    let mut provider_ids = messages
        .into_iter()
        .map(|message| message.provider_message_id)
        .collect::<Vec<_>>();
    provider_ids.sort();
    assert_eq!(
        provider_ids,
        vec![
            "reply-a",
            "reply-b",
            "reply-c",
            "sent-root-a",
            "sent-root-c"
        ]
    );
}

#[tokio::test]
async fn same_page_chained_merges_stop_at_a_planned_owner_boundary() {
    let first_root = sent(
        "sent-owned-root-a",
        Some("<owned-root-a@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.com",
        "delivered",
    );
    let second_root = sent(
        "sent-owned-root-c",
        Some("<owned-root-c@mail.example.com>"),
        "2026-08-05T09:30:00Z",
        "creator-b@example.com",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![second_root.clone(), first_root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(
            &[&second_root, &first_root],
            false,
        )]))),
        ..TestGateway::default()
    };
    let (directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);
    let (creator_a, route_a) = creator_email_route(&runtime);
    let creator_b = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator B".into(),
            notes: None,
        })
        .unwrap();
    let locator_b = runtime
        .repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            normalized_value: "creator-b@example.com".into(),
            display_value: "creator-b@example.com".into(),
        })
        .unwrap();
    let route_b = runtime
        .repository
        .link_creator_contact_route(&creator_b.creator.id, &locator_b.id, Confidence::Exact)
        .unwrap();
    let mut snapshot_c = referencing_snapshot(
        &mailbox.mailbox_ref_id,
        "owned-reply-c",
        "owned-reply-thread-c",
        5,
        &["<owned-root-c@mail.example.com>"],
    );
    snapshot_c.creator_id = Some(creator_b.creator.id.clone());
    snapshot_c.contact_route_id = Some(route_b.id.clone());
    let (thread_c, _) = runtime
        .repository
        .persist_email_snapshot(snapshot_c)
        .unwrap();
    let mut snapshot_a = referencing_snapshot(
        &mailbox.mailbox_ref_id,
        "owned-reply-a",
        "owned-reply-thread-a",
        10,
        &[
            "<owned-root-a@mail.example.com>",
            "<owned-root-c@mail.example.com>",
        ],
    );
    snapshot_a.creator_id = Some(creator_a.clone());
    snapshot_a.contact_route_id = Some(route_a.clone());
    let (thread_a, _) = runtime
        .repository
        .persist_email_snapshot(snapshot_a)
        .unwrap();
    let mut snapshot_b = referencing_snapshot(
        &mailbox.mailbox_ref_id,
        "owned-reply-b",
        "owned-reply-thread-b",
        20,
        &["<owned-root-a@mail.example.com>"],
    );
    snapshot_b.creator_id = Some(creator_a);
    snapshot_b.contact_route_id = Some(route_a);
    let (thread_b, _) = runtime
        .repository
        .persist_email_snapshot(snapshot_b)
        .unwrap();
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    for (thread, created_at) in [(&thread_c, 5_u64), (&thread_a, 10), (&thread_b, 20)] {
        connection
            .execute(
                "UPDATE email_threads SET created_at = ?2 WHERE id = ?1",
                rusqlite::params![thread.id.as_str(), created_at],
            )
            .unwrap();
    }
    drop(connection);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(report.merged_threads, 1, "only same-owner A <- B merges");
    assert!(report.missing_provider_message_ids.is_empty());
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
    assert_eq!(threads.items.len(), 2, "different owners were merged");
    assert!(
        threads
            .items
            .iter()
            .any(|summary| summary.thread.id == thread_a.id)
    );
    assert!(
        threads
            .items
            .iter()
            .any(|summary| summary.thread.id == thread_c.id)
    );
    let a_messages = runtime
        .repository
        .get_email_thread_messages(&thread_a.id, 10, None)
        .unwrap()
        .items;
    let c_messages = runtime
        .repository
        .get_email_thread_messages(&thread_c.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(a_messages.len(), 3);
    assert_eq!(c_messages.len(), 2);
    assert!(
        a_messages
            .iter()
            .any(|message| message.provider_message_id == "sent-owned-root-a")
    );
    assert!(
        c_messages
            .iter()
            .any(|message| message.provider_message_id == "sent-owned-root-c")
    );
}

#[tokio::test]
async fn a_reply_arriving_after_its_imported_root_joins_the_same_thread() {
    let root = sent(
        "sent-root",
        Some("<root@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let reply = inbound_reply("reply-1", "2026-08-05T10:00:00Z", "<root@mail.example.com>");
    let gateway = TestGateway {
        received: Arc::new(vec![reply.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![received_page(
            &[&reply],
            false,
        )]))),
        sent_history: Arc::new(vec![root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&root], false)]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Inbox,
            EmailCapability::Send,
            EmailCapability::SentReadback,
        ],
    );

    runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();

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
    assert_eq!(threads.items.len(), 1, "both processing orders converge");
    let messages = runtime
        .repository
        .get_email_thread_messages(&threads.items[0].thread.id, 10, None)
        .unwrap()
        .items;
    assert_eq!(messages.len(), 2);
}

#[tokio::test]
async fn same_subject_and_recipient_without_rfc_evidence_never_merge() {
    let root = sent(
        "sent-standalone",
        Some("<standalone@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let mut unrelated = inbound_reply("unrelated-1", "2026-08-05T10:00:00Z", "");
    unrelated.headers = serde_json::json!({});
    unrelated.subject = "Partnership".into();
    let gateway = TestGateway {
        received: Arc::new(vec![unrelated.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![received_page(
            &[&unrelated],
            false,
        )]))),
        sent_history: Arc::new(vec![root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&root], false)]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Inbox,
            EmailCapability::Send,
            EmailCapability::SentReadback,
        ],
    );

    runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();

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
    assert_eq!(
        threads.items.len(),
        2,
        "matching subject and recipient text is not threading evidence"
    );
}

#[tokio::test]
async fn a_failed_sent_detail_keeps_the_boundary_and_the_retry_fetches_only_missing_work() {
    let good = sent(
        "sent-good",
        Some("<good@mail.example.com>"),
        "2026-08-05T09:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let bad = sent(
        "sent-bad",
        Some("<bad@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![good.clone(), bad.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            sent_page(&[&bad, &good], false),
            sent_page(&[&bad, &good], false),
        ]))),
        failing_sent_details: Arc::new(Mutex::new(HashSet::from(["sent-bad".to_owned()]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Send, EmailCapability::SentReadback],
    );

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.missing_provider_message_ids, vec!["sent-bad"]);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(state.newest_provider_message_id, None);

    gateway.failing_sent_details.lock().unwrap().clear();
    gateway.sent_detail_calls.lock().unwrap().clear();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert!(report.missing_provider_message_ids.is_empty());
    assert_eq!(
        *gateway.sent_detail_calls.lock().unwrap(),
        vec!["sent-bad".to_owned()]
    );
}

#[tokio::test]
async fn a_failure_only_sent_page_is_durable_and_recovers_without_false_advance() {
    let missing = sent(
        "sent-only-missing",
        Some("<only-missing@mail.example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.net",
        "delivered",
    );
    let gateway = TestGateway {
        sent_history: Arc::new(vec![missing.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            sent_page(&[&missing], false),
            sent_page(&[&missing], false),
        ]))),
        failing_sent_details: Arc::new(Mutex::new(HashSet::from([missing
            .provider_message_id
            .clone()]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::SentReadback]);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(
        report.missing_provider_message_ids,
        vec!["sent-only-missing"]
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
    let failed_state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(failed_state.newest_provider_message_id, None);
    assert!(!failed_state.backfill_complete);
    assert_eq!(
        failed_state
            .failure
            .as_ref()
            .unwrap()
            .missing_provider_message_ids,
        vec!["sent-only-missing"]
    );

    gateway.failing_sent_details.lock().unwrap().clear();
    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert!(report.missing_provider_message_ids.is_empty());
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);
    let recovered_state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        recovered_state.newest_provider_message_id.as_deref(),
        Some("sent-only-missing")
    );
    assert!(recovered_state.failure.is_none());
}

#[tokio::test]
async fn an_unowned_root_conflicting_with_an_owned_referencing_thread_is_isolated() {
    // A saved creator's reply (owned thread) references an externally sent
    // root; the root's import resolves unowned (two saved routes across its
    // recipients = ambiguous). Adopting the owned thread would abort the
    // page at commit — the root must import standalone and unowned instead.
    let reply = inbound_reply(
        "reply-own-1",
        "2026-08-05T11:00:00Z",
        "<root-x@example.com>",
    );
    let mut reply = reply;
    reply.from = "Creator <creator@example.com>".into();

    let mut root = sent(
        "sent-root-x",
        Some("<root-x@example.com>"),
        "2026-08-05T10:00:00Z",
        "creator@example.com",
        "delivered",
    );
    root.to = vec!["creator@example.com".into(), "creator-b@example.com".into()];

    let gateway = TestGateway {
        received: Arc::new(vec![reply.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![received_page(
            &[&reply],
            false,
        )]))),
        sent_history: Arc::new(vec![root.clone()]),
        sent_pages: Arc::new(Mutex::new(VecDeque::from(vec![sent_page(&[&root], false)]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    creator_email_route(&runtime);
    // Second saved route: makes the sent root's recipient set ambiguous.
    let second = runtime
        .repository
        .create_creator(CreateCreatorInput {
            tags: None,
            display_name: "Creator B".into(),
            notes: None,
        })
        .unwrap();
    let locator = runtime
        .repository
        .upsert_contact_locator(ContactLocatorInput {
            channel: ContactChannel::Email,
            kind: LocatorKind::EmailAddress,
            normalized_value: "creator-b@example.com".into(),
            display_value: "creator-b@example.com".into(),
        })
        .unwrap();
    runtime
        .repository
        .link_creator_contact_route(&second.creator.id, &locator.id, Confidence::Exact)
        .unwrap();
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Inbox, EmailCapability::SentReadback],
    );

    // The saved creator's reply first: it creates the OWNED referencing
    // thread the root would otherwise adopt.
    let received = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(received.imported, 1);
    assert_eq!(received.matched, 1);

    let report = runtime
        .sync_mailbox_sent(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert!(report.missing_provider_message_ids.is_empty());
    assert_eq!(report.imported, 1, "the conflicting root still imports");
    assert_eq!(
        report.merged_threads, 0,
        "no merge on a thread it cannot join"
    );

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.failure.is_none(), "no abort, no failure state");
    assert!(state.backfill_complete);
}
