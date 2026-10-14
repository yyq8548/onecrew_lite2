//! The canonical pull-only received engine: backfill to the provider floor,
//! bounded-overlap incremental runs, dedupe through the identity map,
//! partial-page failure isolation, and page-level publication.

use super::*;
use crate::EmailDirection;

#[tokio::test]
async fn terminal_sync_failure_replaces_old_failure_without_advancing_cursor() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    for (error, expected) in [("gmail_read_http_403_rateLimitExceeded", "gmail_read_http_403_rateLimitExceeded"),
        ("private token provider body", "mailbox_sync_failed")] {
        gateway.received_list_errors.lock().unwrap().push_back(error.into());
        assert!(runtime.sync_mailbox_full(crate::SyncMailboxRequest { mailbox_ref_id: mailbox.mailbox_ref_id.clone(), mode: crate::EmailSyncRunMode::Incremental }).await.is_err());
        let state = runtime.mailbox_sync_status(&mailbox.mailbox_ref_id).unwrap();
        assert!(!state.running);
        let inbound = state.inbound.unwrap();
        assert_eq!(inbound.failure.unwrap().message, expected);
        assert!(inbound.last_full_sync_at.is_none());
        assert!(inbound.last_page_at.is_none());
    }
}

#[tokio::test]
async fn gmail_sync_never_reads_external_history_even_with_legacy_inbox_capability() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let mailbox = runtime.sync_mailbox_ref(MailboxRefInput {
        id: crate::MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "gmail".into(), label: "Gmail".into(), email_address: "outreach@example.com".into(),
        display_name: None, capabilities: vec![EmailCapability::Inbox, EmailCapability::Send, EmailCapability::SentReadback], available: true, last_error: None,
    }).unwrap();
    assert_eq!(runtime.sync_mailbox_received(&mailbox.mailbox_ref_id).await.unwrap().imported, 0);
    let full = runtime.sync_mailbox_full(crate::SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(), mode: crate::EmailSyncRunMode::Incremental,
    }).await.unwrap();
    assert_eq!(full.inbound.pages_listed, 0);
    assert_eq!(full.outbound.pages_listed, 0);
    assert!(gateway.list_cursors.lock().unwrap().is_empty());
    assert!(gateway.sent_list_cursors.lock().unwrap().is_empty());
    assert!(gateway.detail_calls.lock().unwrap().is_empty());
    assert_eq!(gateway.send_calls.load(Ordering::SeqCst), 0);
    assert!(!runtime.mailbox_sync_status(&mailbox.mailbox_ref_id).unwrap().running);
}

fn inbound(provider_message_id: &str, created_at: &str) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(format!("<{provider_message_id}@example.net>")),
        from: format!("Sender <{provider_message_id}@example.net>"),
        to: vec!["outreach@example.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: vec!["outreach@example.com".into()],
        reply_to: Vec::new(),
        subject: format!("Message {provider_message_id}"),
        text: Some("body".into()),
        html: None,
        headers: serde_json::json!({}),
        last_event: "received".into(),
        created_at: created_at.into(),
        inbound: true,
        attachments: Vec::new(),
    }
}

fn summary(message: &PlatformEmailMessage) -> PlatformEmailSummary {
    PlatformEmailSummary {
        provider_message_id: message.provider_message_id.clone(),
        created_at: message.created_at.clone(),
    }
}

fn page(messages: &[&PlatformEmailMessage], has_more: bool) -> PlatformEmailPage {
    PlatformEmailPage {
        messages: messages.iter().map(|message| summary(message)).collect(),
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

fn drain_changes(changes: &mut tokio::sync::broadcast::Receiver<crate::CreatorChange>) -> usize {
    let mut count = 0;
    while changes.try_recv().is_ok() {
        count += 1;
    }
    count
}

#[tokio::test]
async fn received_repository_segments_keep_current_thread_responsive_and_commit_after_cancel() {
    let message = inbound("received-segment", "2026-08-05T12:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![message.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(&[&message], true)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let mut changes = runtime.subscribe_changes();
    let (preflight_started, release_preflight) = runtime.install_next_repository_read_barrier();
    let sync_runtime = runtime.clone();
    let mailbox_ref_id = mailbox.mailbox_ref_id.clone();
    let sync =
        tokio::spawn(async move { sync_runtime.sync_mailbox_received(&mailbox_ref_id).await });
    preflight_started
        .await
        .expect("preflight read did not start");
    assert!(gateway.list_cursors.lock().unwrap().is_empty());
    let (dedupe_started, release_dedupe) = runtime.install_next_repository_read_barrier();
    release_preflight.send(()).unwrap();
    dedupe_started
        .await
        .expect("page dedupe read did not start");
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 1);
    assert!(gateway.detail_calls.lock().unwrap().is_empty());
    let (write_started, release_write) = runtime.install_next_repository_write_barrier();
    release_dedupe.send(()).unwrap();
    write_started.await.expect("page writer did not start");
    assert_eq!(gateway.detail_calls.lock().unwrap().len(), 1);
    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("received page writer blocked current-thread runtime")
    .unwrap();
    sync.abort();
    assert!(sync.await.unwrap_err().is_cancelled());
    release_write.send(()).unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
        .await
        .expect("cancelled caller lost received-page invalidation")
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
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_before.newest_provider_message_id.as_deref(),
        Some("received-segment")
    );
    assert_eq!(
        state_before.oldest_backfill_cursor.as_deref(),
        Some("received-segment")
    );
    assert!(!state_before.backfill_complete);
    assert!(state_before.last_full_sync_at.is_none());
    assert!(state_before.failure.is_none());
    let continuation = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.incremental_continuation)
        .unwrap()
        .expect("cancelled caller lost the committed Phase A continuation");
    assert_eq!(continuation.next_cursor, "received-segment");
    assert_eq!(continuation.newest_provider_message_id, "received-segment");
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert!(report.full_sync_recorded);
    assert_eq!(gateway.detail_calls.lock().unwrap().len(), 1);
    assert_eq!(
        gateway.list_cursors.lock().unwrap()[1].as_deref(),
        Some("received-segment"),
        "the retry did not resume from the committed continuation"
    );
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound,)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .is_none()
    );
    let state_completed = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state_completed.backfill_complete);
    assert!(state_completed.last_full_sync_at.is_some());
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert!(!report.full_sync_recorded);
    let state_after = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_after, state_completed,
        "quiet overlap rewrote checkpoint"
    );
}

#[tokio::test]
async fn received_nonterminal_page_preserves_prior_failure_until_full_recovery() {
    let failed = inbound("received-failed-old", "2026-08-05T10:00:00Z");
    let newer = inbound("received-new-before-recovery", "2026-08-05T12:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![failed.clone(), newer.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(&[&failed], false)]))),
        failing_details: Arc::new(Mutex::new(HashSet::from([failed
            .provider_message_id
            .clone()]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap()
            .failure
            .is_some()
    );

    gateway.failing_details.lock().unwrap().clear();
    gateway
        .received_pages
        .lock()
        .unwrap()
        .push_back(page(&[&newer], true));
    let (write_started, release_write) = runtime.install_next_repository_write_barrier();
    let sync_runtime = runtime.clone();
    let mailbox_ref_id = mailbox.mailbox_ref_id.clone();
    let sync =
        tokio::spawn(async move { sync_runtime.sync_mailbox_received(&mailbox_ref_id).await });
    write_started
        .await
        .expect("nonterminal page writer did not start");
    sync.abort();
    assert!(sync.await.unwrap_err().is_cancelled());
    release_write.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let state = runtime
                .repository
                .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
                .map(|checkpoint| checkpoint.state)
                .unwrap()
                .unwrap();
            if fact_count(&directory, "emails") == 1 {
                assert!(state.failure.is_some());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("nonterminal page did not commit");
}

#[tokio::test]
async fn received_existing_unfinished_state_completes_when_phase_a_reaches_floor() {
    let old = inbound("received-old-watermark", "2026-08-05T10:00:00Z");
    let newer = inbound("received-floor-new", "2026-08-05T12:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![old.clone(), newer.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(&[&old], false)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE mailbox_sync_state
             SET oldest_backfill_cursor = 'stale-cursor', backfill_complete = 0
             WHERE mailbox_ref_id = ?1 AND direction = 'inbound'",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();
    gateway
        .received_pages
        .lock()
        .unwrap()
        .push_back(page(&[&newer], false));
    let listed_before = gateway.list_cursors.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert!(report.backfill_complete);
    assert_eq!(
        gateway.list_cursors.lock().unwrap().len(),
        listed_before + 1
    );
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("received-floor-new")
    );
}

#[tokio::test]
async fn received_existing_unfinished_state_reaching_floor_on_page_200_is_not_budget_failure() {
    let old = inbound("received-page-200-watermark", "2026-08-05T10:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![old.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(&[&old], false)]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE mailbox_sync_state
             SET oldest_backfill_cursor = 'stale-cursor', backfill_complete = 0
             WHERE mailbox_ref_id = ?1 AND direction = 'inbound'",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();
    let pages = (1..=200)
        .map(|index| PlatformEmailPage {
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
    *gateway.received_pages.lock().unwrap() = pages;

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 200);
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);
    assert!(report.missing_provider_message_ids.is_empty());
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.backfill_complete);
    assert!(state.last_full_sync_at.is_some());
    assert!(state.failure.is_none());
}

#[tokio::test]
async fn a_fresh_mailbox_backfills_to_the_provider_floor() {
    let newest = inbound("received-3", "2026-08-05T12:00:00Z");
    let middle = inbound("received-2", "2026-08-05T11:00:00Z");
    let oldest = inbound("received-1", "2026-08-05T10:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![newest.clone(), middle.clone(), oldest.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            page(&[&newest, &middle], true),
            page(&[&oldest], false),
        ]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let mut changes = runtime.subscribe_changes();

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2);
    assert_eq!(report.summaries_listed, 3);
    assert_eq!(report.details_fetched, 3);
    assert_eq!(report.imported, 3);
    assert_eq!(report.skipped_complete, 0);
    assert!(report.backfill_complete);
    assert!(report.full_sync_recorded);
    assert!(report.missing_provider_message_ids.is_empty());
    assert_eq!(
        drain_changes(&mut changes),
        2,
        "one publication per changed page, never per message"
    );

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .expect("durable inbound state");
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("received-3")
    );
    assert!(state.backfill_complete);
    assert!(state.last_full_sync_at.is_some());
    assert_eq!(state.failure, None);
}

#[tokio::test]
async fn a_quiet_second_sync_fetches_no_details_and_writes_nothing() {
    let newest = inbound("received-2", "2026-08-05T11:00:00Z");
    let oldest = inbound("received-1", "2026-08-05T10:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![newest.clone(), oldest.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            page(&[&newest, &oldest], false),
            // The rewalk lists the same overlap page.
            page(&[&newest, &oldest], true),
        ]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let state_before = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    let details_before = gateway.detail_calls.lock().unwrap().len();
    let mut changes = runtime.subscribe_changes();

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 1, "the walk stops at the watermark");
    assert_eq!(report.skipped_complete, 2);
    assert_eq!(report.imported, 0);
    assert_eq!(
        gateway.detail_calls.lock().unwrap().len(),
        details_before,
        "no detail GET for messages already complete locally"
    );
    assert_eq!(
        drain_changes(&mut changes),
        0,
        "nothing changed, no publication"
    );

    let state_after = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state_after, state_before,
        "a quiet run makes no SQLite write"
    );
}

#[tokio::test]
async fn new_mail_since_the_last_run_is_imported_exactly_once() {
    let first = inbound("received-1", "2026-08-05T10:00:00Z");
    let newer = inbound("received-2", "2026-08-05T12:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![first.clone(), newer.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            page(&[&first], false),
            // Next run: new mail on top, the watermark item inside the page.
            page(&[&newer, &first], true),
        ]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.skipped_complete, 1);
    assert_eq!(
        report.pages_listed, 1,
        "crossing the watermark ends the walk"
    );

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("received-2")
    );

    // Exactly one local fact per provider message.
    let complete = runtime
        .repository
        .complete_email_provider_ids(
            &mailbox.mailbox_ref_id,
            &["received-1".into(), "received-2".into()],
        )
        .unwrap();
    assert_eq!(complete.len(), 2);
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
    let total_messages: usize = threads
        .items
        .iter()
        .map(|summary| {
            runtime
                .repository
                .get_email_thread_messages(&summary.thread.id, 50, None)
                .unwrap()
                .items
                .len()
        })
        .sum();
    assert_eq!(total_messages, 2, "the watermark overlap deduplicates");
}

#[tokio::test]
async fn a_failed_detail_commits_the_subset_and_the_retry_fetches_only_missing_work() {
    let good = inbound("received-good", "2026-08-05T10:00:00Z");
    let bad = inbound("received-bad", "2026-08-05T11:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![good.clone(), bad.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            page(&[&bad, &good], false),
            page(&[&bad, &good], false),
        ]))),
        failing_details: Arc::new(Mutex::new(HashSet::from(["received-bad".to_owned()]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert_eq!(report.missing_provider_message_ids, vec!["received-bad"]);
    assert!(!report.full_sync_recorded);

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.newest_provider_message_id, None,
        "a failed page never advances the boundary"
    );
    let failure = state.failure.expect("sanitized failure state is durable");
    assert_eq!(failure.missing_provider_message_ids, vec!["received-bad"]);
    assert_eq!(
        runtime
            .repository
            .complete_email_provider_ids(&mailbox.mailbox_ref_id, &["received-good".into()])
            .unwrap(),
        vec!["received-good".to_owned()],
        "the successful subset committed once"
    );

    // The provider recovers; the retry fetches only the missing message.
    gateway.failing_details.lock().unwrap().clear();
    gateway.detail_calls.lock().unwrap().clear();
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 1);
    assert!(report.missing_provider_message_ids.is_empty());
    assert!(report.full_sync_recorded);
    assert_eq!(
        *gateway.detail_calls.lock().unwrap(),
        vec!["received-bad".to_owned()],
        "already-complete work is never re-fetched"
    );
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.failure, None,
        "recovery clears the durable failure state"
    );
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("received-bad"),
        "the recovered walk owns the boundary"
    );
}

#[tokio::test]
async fn replies_on_one_page_join_their_root_imported_by_the_same_page() {
    let root = inbound("received-root", "2026-08-05T10:00:00Z");
    let mut reply = inbound("received-reply", "2026-08-05T11:00:00Z");
    reply.headers = serde_json::json!({
        "in-reply-to": "<received-root@example.net>",
        "references": "<received-root@example.net>"
    });
    let gateway = TestGateway {
        received: Arc::new(vec![root.clone(), reply.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(
            &[&reply, &root],
            false,
        )]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 2);
    assert_eq!(
        report.changed_thread_ids.len(),
        1,
        "the reply attaches to the thread its root created on the same page"
    );
}

#[tokio::test]
async fn a_same_page_owner_conflict_isolates_the_reply_instead_of_aborting_the_run() {
    // Root from a SAVED route (owned thread) plus a reply-all from an
    // unsaved colleague referencing it — both on one page. The reply must
    // land isolated and unowned; the run must not abort at commit.
    let mut root = inbound("root-c", "2026-08-05T10:00:00Z");
    root.from = "Creator <creator@example.com>".into();
    root.internet_message_id = Some("<root-c@example.net>".into());
    let mut reply = inbound("reply-c", "2026-08-05T11:00:00Z");
    reply.from = "Colleague <colleague@example.net>".into();
    reply.headers = serde_json::json!({
        "In-Reply-To": "<root-c@example.net>",
        "References": "<root-c@example.net>"
    });

    let gateway = TestGateway {
        received: Arc::new(vec![root.clone(), reply.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![page(
            &[&reply, &root],
            false,
        )]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    creator_email_route(&runtime);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert!(report.missing_provider_message_ids.is_empty());
    assert_eq!(report.imported, 2);
    assert_eq!(report.matched, 1, "the saved-route root stays owned");
    assert_eq!(report.unmatched, 1, "the conflicting reply imports unowned");
    assert_eq!(
        report.changed_thread_ids.len(),
        2,
        "the reply is isolated into its own thread, not the owned one"
    );

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.failure.is_none(), "no abort, no failure state");
    assert!(state.backfill_complete);
}

#[tokio::test]
async fn page_budget_exhaustion_persists_the_cursor_and_the_next_run_resumes() {
    // 201 team messages, one per page: the first run stops at the 200-page
    // budget but keeps its watermark and cursor; the second run crosses at
    // page one and finishes the backfill from the durable cursor.
    let mut messages = Vec::new();
    for index in 0..201_u32 {
        let t = 50_000 - index;
        let created_at = format!(
            "2026-08-04T{:02}:{:02}:{:02}Z",
            t / 3600,
            (t % 3600) / 60,
            t % 60
        );
        messages.push(inbound(&format!("m-{index:03}"), &created_at));
    }
    let first_run_pages: VecDeque<PlatformEmailPage> = messages
        .iter()
        .take(200)
        .map(|message| page(&[message], true))
        .collect();

    let gateway = TestGateway {
        received: Arc::new(messages.clone()),
        received_pages: Arc::new(Mutex::new(first_run_pages)),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 200);
    assert_eq!(report.imported, 200);
    assert!(!report.backfill_complete);

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    let failure = state.failure.as_ref().expect("budget failure recorded");
    assert_eq!(failure.error_class, "page_budget_exhausted");
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("m-000"),
        "the initial walk's page-one watermark is durable"
    );
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("m-199"),
        "the cursor after the last listed page is durable"
    );
    assert!(!state.backfill_complete);

    // Second run: the fresh-mailbox Phase A lineage resumes directly from
    // its independent continuation and reaches the retention floor.
    gateway
        .received_pages
        .lock()
        .unwrap()
        .extend([page(&[&messages[0]], true), page(&[&messages[200]], false)]);
    let listed_before = gateway.list_cursors.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2, "no restart from scratch");
    assert_eq!(report.imported, 1, "only the message beyond the budget");
    assert!(report.backfill_complete);
    let cursors = gateway.list_cursors.lock().unwrap();
    assert_eq!(
        cursors[listed_before].as_deref(),
        Some("m-199"),
        "Phase A did not resume from its durable continuation"
    );
    assert_eq!(
        cursors[listed_before + 1].as_deref(),
        Some("m-000"),
        "the resumed page advanced its continuation"
    );
    drop(cursors);

    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(state.failure.is_none());
    assert!(state.backfill_complete);
}

async fn assert_received_existing_watermark_phase_a_continues(backfill_complete: bool) {
    let mut messages = (0..=200_u32)
        .map(|index| inbound(&format!("received-new-{index:03}"), "2026-08-05T12:00:00Z"))
        .collect::<Vec<_>>();
    let old_watermark = inbound("received-existing-watermark", "2026-08-05T10:00:00Z");
    messages.push(old_watermark.clone());
    let first_run_pages = messages
        .iter()
        .take(200)
        .map(|message| page(&[message], true))
        .collect::<VecDeque<_>>();
    let gateway = TestGateway {
        received: Arc::new(messages.clone()),
        received_pages: Arc::new(Mutex::new(first_run_pages)),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, newest_provider_message_id,
                newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                last_page_at, last_full_sync_at, failure_json, updated_at)
             VALUES (?1, 'inbound', 'received-existing-watermark', 1, ?2, ?3,
                     1, ?4, NULL, 1)",
            rusqlite::params![
                mailbox.mailbox_ref_id.as_str(),
                (!backfill_complete).then_some("received-phase-b-cursor"),
                backfill_complete,
                backfill_complete.then_some(1_u64),
            ],
        )
        .unwrap();

    let first = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(first.pages_listed, 200);
    assert_eq!(first.imported, 200);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("received-existing-watermark"),
        "an unfinished Phase A walk moved the old watermark"
    );
    assert_eq!(state.backfill_complete, backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        (!backfill_complete).then_some("received-phase-b-cursor")
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
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.incremental_continuation)
        .unwrap()
        .expect("page 200 continuation");
    assert_eq!(continuation.next_cursor, "received-new-199");
    assert_eq!(
        continuation.newest_provider_message_id, "received-new-000",
        "the original first-page anchor was not preserved"
    );

    let mut remainder = VecDeque::from(vec![
        page(&[&messages[200]], true),
        page(&[&old_watermark], true),
    ]);
    if !backfill_complete {
        remainder.push_back(PlatformEmailPage {
            messages: Vec::new(),
            has_more: false,
            next_cursor: None,
            bounds: None,
        });
    }
    *gateway.received_pages.lock().unwrap() = remainder;
    let listed_before = gateway.list_cursors.lock().unwrap().len();
    let second = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(second.pages_listed, if backfill_complete { 2 } else { 3 });
    assert_eq!(second.imported, 2);
    assert!(second.backfill_complete);
    let cursors = gateway.list_cursors.lock().unwrap();
    assert_eq!(
        cursors[listed_before].as_deref(),
        Some("received-new-199"),
        "Phase A restarted from the newest page"
    );
    assert_eq!(
        cursors[listed_before + 1].as_deref(),
        Some("received-new-200")
    );
    if !backfill_complete {
        assert_eq!(
            cursors[listed_before + 2].as_deref(),
            Some("received-phase-b-cursor"),
            "Phase B did not retain its independent cursor"
        );
    }
    drop(cursors);
    let final_state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        final_state.newest_provider_message_id.as_deref(),
        Some("received-new-000")
    );
    assert!(final_state.backfill_complete);
    assert!(final_state.failure.is_none());
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound,)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .is_none()
    );
    let details = gateway.detail_calls.lock().unwrap();
    assert_eq!(details.len(), 202);
    assert_eq!(
        details.iter().collect::<HashSet<_>>().len(),
        details.len(),
        "resumed Phase A fetched a message detail twice"
    );
}

#[tokio::test]
async fn received_existing_complete_watermark_resumes_phase_a_after_page_200() {
    assert_received_existing_watermark_phase_a_continues(true).await;
}

#[tokio::test]
async fn received_existing_unfinished_watermark_resumes_phase_a_before_phase_b() {
    assert_received_existing_watermark_phase_a_continues(false).await;
}

#[tokio::test]
async fn received_missing_next_cursor_is_a_durable_failure_for_every_start_state() {
    for (label, backfill_complete, invalid_cursor) in [
        ("initial", None, None),
        ("existing-complete", Some(true), Some("")),
        ("existing-unfinished", Some(false), Some(" \t\r\n")),
    ] {
        let message = inbound(
            &format!("received-null-cursor-{label}"),
            "2026-08-05T12:00:00Z",
        );
        let gateway = TestGateway {
            received: Arc::new(vec![message.clone()]),
            received_pages: Arc::new(Mutex::new(VecDeque::from(vec![PlatformEmailPage {
                messages: vec![summary(&message)],
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
        let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
        if let Some(backfill_complete) = backfill_complete {
            Connection::open(directory.path().join("crm.db"))
                .unwrap()
                .execute(
                    "INSERT INTO mailbox_sync_state
                       (mailbox_ref_id, direction, newest_provider_message_id,
                        newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                        last_page_at, last_full_sync_at, failure_json, updated_at)
                     VALUES (?1, 'inbound', 'received-old-watermark', 1, ?2, ?3,
                             1, ?4, NULL, 1)",
                    rusqlite::params![
                        mailbox.mailbox_ref_id.as_str(),
                        (!backfill_complete).then_some("received-old-phase-b"),
                        backfill_complete,
                        backfill_complete.then_some(1_u64),
                    ],
                )
                .unwrap();
        }

        let report = runtime
            .sync_mailbox_received(&mailbox.mailbox_ref_id)
            .await
            .unwrap();
        assert_eq!(report.pages_listed, 1, "{label}");
        assert_eq!(gateway.list_cursors.lock().unwrap().len(), 1, "{label}");
        let state = runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
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
            backfill_complete.map(|_| "received-old-watermark"),
            "{label} moved its watermark"
        );
        assert_eq!(
            state.oldest_backfill_cursor.as_deref(),
            backfill_complete
                .filter(|complete| !complete)
                .map(|_| "received-old-phase-b"),
            "{label} moved its Phase B cursor"
        );
        assert!(
            runtime
                .repository
                .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound,)
                .map(|checkpoint| checkpoint.incremental_continuation)
                .unwrap()
                .is_none(),
            "{label} invented a continuation without a next cursor"
        );
    }
}

#[tokio::test]
async fn received_nonterminal_page_requires_a_nonblank_newest_anchor() {
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
        let message = inbound(&format!("received-anchor-{label}"), "2026-08-05T12:00:00Z");
        let gateway = TestGateway {
            received: Arc::new(vec![message.clone()]),
            received_pages: Arc::new(Mutex::new(VecDeque::from(vec![PlatformEmailPage {
                messages: vec![summary(&message)],
                has_more: true,
                next_cursor: Some(format!("received-anchor-cursor-{label}")),
                bounds,
            }]))),
            ..TestGateway::default()
        };
        let (_directory, runtime, _) = runtime(gateway);
        let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

        runtime
            .sync_mailbox_received(&mailbox.mailbox_ref_id)
            .await
            .unwrap();
        let checkpoint = runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
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
async fn received_phase_b_missing_next_cursor_is_not_a_retention_floor() {
    let watermark = inbound("received-phase-b-null-watermark", "2026-08-05T10:00:00Z");
    let gateway = TestGateway {
        received: Arc::new(vec![watermark.clone()]),
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![
            page(&[&watermark], true),
            PlatformEmailPage {
                messages: Vec::new(),
                has_more: true,
                next_cursor: None,
                bounds: None,
            },
        ]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "INSERT INTO mailbox_sync_state
               (mailbox_ref_id, direction, newest_provider_message_id,
                newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                last_page_at, last_full_sync_at, failure_json, updated_at)
             VALUES (?1, 'inbound', 'received-phase-b-null-watermark', 1,
                     'received-phase-b-null-cursor', 0, 1, NULL, NULL, 1)",
            [mailbox.mailbox_ref_id.as_str()],
        )
        .unwrap();

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.pages_listed, 2);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert!(!state.backfill_complete);
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("received-phase-b-null-cursor")
    );
    assert_eq!(
        state.failure.unwrap().error_class,
        "pagination_cursor_missing"
    );
    assert_eq!(
        gateway.list_cursors.lock().unwrap()[1].as_deref(),
        Some("received-phase-b-null-cursor")
    );
}

#[tokio::test]
async fn a_mailbox_matching_nothing_still_anchors_its_watermark_on_the_raw_page() {
    // An active team whose items all belong to other mailboxes: filtering
    // empties every page, but the raw boundary still yields a watermark, so
    // the next run stops at page one instead of re-walking the team history.
    let foreign_page = |newest_id: &str, has_more: bool| PlatformEmailPage {
        messages: Vec::new(),
        has_more,
        next_cursor: Some(format!("{newest_id}-cursor")),
        bounds: Some(onecrew_email_contracts::PlatformPageBounds {
            newest_provider_message_id: newest_id.into(),
            newest_created_at: "2026-08-05T10:00:00Z".into(),
            oldest_created_at: "2026-08-05T09:00:00Z".into(),
        }),
    };
    let gateway = TestGateway {
        received_pages: Arc::new(Mutex::new(VecDeque::from(vec![foreign_page(
            "team-item-9",
            false,
        )]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert!(report.backfill_complete);
    let state = runtime
        .repository
        .email_sync_checkpoint(&mailbox.mailbox_ref_id, crate::EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("team-item-9"),
        "the watermark anchors on the raw team item even with zero matches"
    );

    // Next run: page one's raw newest equals the watermark — crossed after
    // ONE list call even though the provider reports more pages below.
    gateway
        .received_pages
        .lock()
        .unwrap()
        .push_back(foreign_page("team-item-9", true));
    let listed_before = gateway.list_cursors.lock().unwrap().len();
    let report = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();
    assert_eq!(report.imported, 0);
    assert_eq!(
        gateway.list_cursors.lock().unwrap().len() - listed_before,
        1,
        "a zero-match mailbox must not re-walk the team history"
    );
}

#[test]
fn backfill_percent_estimates_from_the_time_window() {
    use crate::email::sync::backfill_progress_percent;
    let bounds = |oldest: &str| onecrew_email_contracts::PlatformPageBounds {
        newest_provider_message_id: "n".into(),
        newest_created_at: "2026-08-24T00:00:00Z".into(),
        oldest_created_at: oldest.into(),
    };
    let anchor = crate::email::message::parse_provider_time("2026-08-24T00:00:00Z").unwrap();

    // Three days into the 30-day retention window ≈ 10%.
    let three_days = bounds("2026-08-21T00:00:00Z");
    assert_eq!(
        backfill_progress_percent(Some(anchor), Some(&three_days)),
        Some(10)
    );
    // Beyond the window clamps to 100, never overflows.
    let ancient = bounds("2026-07-01T00:00:00Z");
    assert_eq!(
        backfill_progress_percent(Some(anchor), Some(&ancient)),
        Some(100)
    );
    // Without an anchor or bounds there is no estimate — the bar stays
    // indeterminate rather than showing a made-up number.
    assert_eq!(backfill_progress_percent(None, Some(&ancient)), None);
    assert_eq!(backfill_progress_percent(Some(anchor), None), None);
    // A page newer than the anchor saturates at zero.
    let newer = bounds("2026-08-25T00:00:00Z");
    assert_eq!(
        backfill_progress_percent(Some(anchor), Some(&newer)),
        Some(0)
    );
}

#[tokio::test]
async fn invalid_sync_rate_limit_metadata_stops_before_checkpoint_advance() {
    let gateway = TestGateway {
        received_list_errors: Arc::new(Mutex::new(VecDeque::from([
            "Resend sync response metadata invalid: missing ratelimit-reset".into(),
        ]))),
        ..TestGateway::default()
    };
    let (directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let error = runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap_err();
    assert!(error.contains("metadata invalid"), "{error}");
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 1);
    assert!(
        runtime
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)
            .unwrap()
            .state
            .is_none()
    );
    assert_eq!(fact_count(&directory, "emails"), 0);
}

#[tokio::test]
async fn gmail_outreach_replies_sync_without_inbox_crawl_and_deduplicate() {
    let reply = inbound("reply-1", "2026-10-12T20:00:00Z");
    let (_directory, runtime, gateway) = runtime(TestGateway {
        received: Arc::new(vec![reply]), ..TestGateway::default()
    });
    let mailbox = runtime.sync_mailbox_ref(MailboxRefInput {
        id: crate::MailboxRefId::new("11111111-1111-4111-8111-111111111111"),
        provider: "gmail".into(), label: "Gmail".into(), email_address: "outreach@example.com".into(),
        display_name: None, capabilities: vec![EmailCapability::Inbox, EmailCapability::Send, EmailCapability::SentReadback], available: true, last_error: None,
    }).unwrap();
    let sent = runtime.send_manual_email(SendManualEmailInput {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(), to: "creator@example.com".into(),
        subject: "Partnership".into(), text: "Hello".into(), html: None, idempotency_key: "reply-scope".into(),
    }).await.unwrap();
    for expected in [1, 0] {
        let result = runtime.sync_mailbox_full(crate::SyncMailboxRequest {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(), mode: crate::EmailSyncRunMode::Incremental,
        }).await.unwrap();
        assert_eq!(result.inbound.imported, expected);
        if expected == 1 {
            runtime.mark_email_thread_read(crate::EmailThreadMutationInput { thread_id: sent.thread.id.clone() }).unwrap();
        }
    }
    assert_eq!(runtime.repository.get_email_thread_record(&sent.thread.id).unwrap().unread_count, 0);
    let messages = runtime.repository.get_email_thread_messages(&sent.thread.id, 100, None).unwrap();
    assert_eq!(messages.items.len(), 2);
    assert!(messages.items.iter().any(|m| m.provider_message_id == "reply-1" && m.direction == EmailDirection::Inbound));
    assert!(gateway.list_cursors.lock().unwrap().is_empty());
    assert!(gateway.sent_list_cursors.lock().unwrap().is_empty());
}
