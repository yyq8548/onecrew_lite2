//! The coalesced bidirectional operation both peer adapters project:
//! concurrent callers share one run, status separates process-only running
//! state from durable facts, and completion leaves no false running state.

use super::*;
use crate::{CreatorChangeScope, EmailDirection, EmailSyncRunMode, SyncMailboxRequest};

async fn assert_runtime_health() {
    tokio::time::timeout(
        std::time::Duration::from_millis(250),
        tokio::spawn(async { tokio::task::yield_now().await }),
    )
    .await
    .expect("full sync repository segment starved the Tokio workers")
    .unwrap();
}

fn history_message(provider_message_id: &str, inbound: bool) -> PlatformEmailMessage {
    PlatformEmailMessage {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(format!("<{provider_message_id}@example.net>")),
        from: if inbound {
            "Creator <creator@example.net>".into()
        } else {
            "Reach <outreach@example.com>".into()
        },
        to: vec![if inbound {
            "outreach@example.com".into()
        } else {
            "creator@example.net".into()
        }],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: if inbound {
            vec!["outreach@example.com".into()]
        } else {
            Vec::new()
        },
        reply_to: Vec::new(),
        subject: "Partnership".into(),
        text: Some("body".into()),
        html: None,
        headers: serde_json::json!({}),
        last_event: if inbound { "received" } else { "delivered" }.into(),
        created_at: "2026-08-05T10:00:00Z".into(),
        inbound,
        attachments: Vec::new(),
    }
}

#[tokio::test]
async fn concurrent_callers_coalesce_under_one_runtime_operation() {
    let inbound = history_message("received-1", true);
    let sent = history_message("sent-1", false);
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        sent_history: Arc::new(vec![sent]),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Inbox,
            EmailCapability::Send,
            EmailCapability::SentReadback,
        ],
    );

    let request = || SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };
    let (first, second, third) = tokio::join!(
        runtime.sync_mailbox_full(request()),
        runtime.sync_mailbox_full(request()),
        runtime.sync_mailbox_full(request()),
    );
    let first = first.unwrap();
    let second = second.unwrap();
    let third = third.unwrap();

    // One provider walk served every caller.
    assert_eq!(
        gateway.list_cursors.lock().unwrap().len(),
        1,
        "three callers, one received listing"
    );
    assert_eq!(
        gateway.sent_list_cursors.lock().unwrap().len(),
        1,
        "three callers, one sent listing"
    );
    assert_eq!(first.inbound.imported, 1);
    assert_eq!(first.outbound.imported, 1);
    assert_eq!(second.inbound.imported, first.inbound.imported);
    assert_eq!(third.changed_thread_ids, first.changed_thread_ids);

    // Completion leaves no false running state, and durable facts landed.
    let status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!status.running);
    let inbound_state = status.inbound.expect("inbound durable state");
    assert!(inbound_state.backfill_complete);
    assert!(inbound_state.last_full_sync_at.is_some());
    assert!(inbound_state.failure.is_none());
    let outbound_state = status.outbound.expect("outbound durable state");
    assert!(outbound_state.backfill_complete);

    // A later run is a NEW operation, not a stale registry entry.
    let again = runtime.sync_mailbox_full(request()).await.unwrap();
    assert_eq!(again.inbound.imported, 0, "quiet re-run imports nothing");
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn owner_error_is_shared_registry_clears_and_retry_starts_a_new_run() {
    let gateway = TestGateway {
        received: Arc::new(vec![history_message("received-after-error", true)]),
        received_list_errors: Arc::new(Mutex::new(VecDeque::from(vec![
            "scripted received list error".to_owned(),
        ]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let mut changes = runtime.subscribe_changes();
    let request = || SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };

    let (first, second, third) = tokio::join!(
        runtime.sync_mailbox_full(request()),
        runtime.sync_mailbox_full(request()),
        runtime.sync_mailbox_full(request()),
    );
    let first = first.unwrap_err();
    assert_eq!(second.unwrap_err(), first);
    assert_eq!(third.unwrap_err(), first);
    assert_eq!(first, "scripted received list error");
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 1);
    let status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!status.running);
    assert!(status.progress.is_none());
    assert_eq!(
        std::iter::from_fn(|| changes.try_recv().ok())
            .filter(|change| change.scopes.contains(&CreatorChangeScope::EmailSync))
            .count(),
        2,
        "one start and exactly one terminal invalidation"
    );

    let retry = runtime.sync_mailbox_full(request()).await.unwrap();
    assert_eq!(retry.inbound.imported, 1);
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn panicking_owner_wakes_joiners_with_one_redacted_error_and_retry_succeeds() {
    let gateway = TestGateway {
        received: Arc::new(vec![history_message("received-after-panic", true)]),
        received_list_panics: Arc::new(AtomicUsize::new(1)),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let mut changes = runtime.subscribe_changes();
    let request = || SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };
    let wait = std::time::Duration::from_secs(2);

    let (first, second, third) = tokio::join!(
        tokio::time::timeout(wait, runtime.sync_mailbox_full(request())),
        tokio::time::timeout(wait, runtime.sync_mailbox_full(request())),
        tokio::time::timeout(wait, runtime.sync_mailbox_full(request())),
    );
    let first = first
        .expect("first joiner slept through panic")
        .unwrap_err();
    let second = second
        .expect("second joiner slept through panic")
        .unwrap_err();
    let third = third
        .expect("third joiner slept through panic")
        .unwrap_err();
    assert_eq!(first, "mailbox sync task failed unexpectedly");
    assert_eq!(second, first);
    assert_eq!(third, first);
    assert!(!first.contains("secret provider panic payload"));
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 1);
    let status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!status.running);
    assert!(status.progress.is_none());
    assert_eq!(
        std::iter::from_fn(|| changes.try_recv().ok())
            .filter(|change| change.scopes.contains(&CreatorChangeScope::EmailSync))
            .count(),
        2,
        "one start and exactly one terminal invalidation"
    );

    let retry = runtime.sync_mailbox_full(request()).await.unwrap();
    assert_eq!(retry.inbound.imported, 1);
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn received_facts_survive_sent_error_and_retry_dedupes_received_before_sent_continues() {
    let gateway = TestGateway {
        received: Arc::new(vec![history_message("received-before-sent-error", true)]),
        sent_history: Arc::new(vec![history_message("sent-after-retry", false)]),
        sent_list_errors: Arc::new(Mutex::new(VecDeque::from(vec![
            "scripted sent list error".to_owned(),
        ]))),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![EmailCapability::Inbox, EmailCapability::SentReadback],
    );
    let request = || SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };

    assert_eq!(
        runtime.sync_mailbox_full(request()).await.unwrap_err(),
        "scripted sent list error"
    );
    let after_failure = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!after_failure.running);
    assert!(after_failure.progress.is_none());
    assert!(after_failure.inbound.unwrap().backfill_complete);
    assert!(after_failure.outbound.is_none());
    assert_eq!(gateway.detail_calls.lock().unwrap().len(), 1);
    assert_eq!(gateway.sent_list_cursors.lock().unwrap().len(), 1);

    let retry = runtime.sync_mailbox_full(request()).await.unwrap();
    assert_eq!(retry.inbound.imported, 0);
    assert_eq!(retry.inbound.skipped_complete, 1);
    assert_eq!(retry.outbound.imported, 1);
    assert_eq!(
        gateway.detail_calls.lock().unwrap().len(),
        1,
        "durable received detail must not be fetched twice"
    );
    assert_eq!(gateway.list_cursors.lock().unwrap().len(), 2);
    assert_eq!(gateway.sent_list_cursors.lock().unwrap().len(), 2);
    assert_eq!(gateway.sent_detail_calls.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn different_mailboxes_keep_independent_runs_progress_and_results() {
    let first_id = "11111111-1111-4111-8111-111111111111";
    let second_id = "22222222-2222-4222-8222-222222222222";
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let gateway = TestGateway {
        received_list_gate: Some(Arc::clone(&gate)),
        received_by_mailbox: Arc::new(HashMap::from([
            (
                first_id.to_owned(),
                vec![history_message("first-mailbox", true)],
            ),
            (
                second_id.to_owned(),
                vec![
                    history_message("second-mailbox-a", true),
                    history_message("second-mailbox-b", true),
                ],
            ),
        ])),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let first_mailbox = mailbox_with_id(
        &runtime,
        first_id,
        "outreach@example.com",
        vec![EmailCapability::Inbox],
    );
    let second_mailbox = mailbox_with_id(
        &runtime,
        second_id,
        "outreach@example.com",
        vec![EmailCapability::Inbox],
    );
    let first_runtime = runtime.clone();
    let first_ref = first_mailbox.mailbox_ref_id.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .sync_mailbox_full(SyncMailboxRequest {
                mailbox_ref_id: first_ref,
                mode: EmailSyncRunMode::Incremental,
            })
            .await
    });
    let second_runtime = runtime.clone();
    let second_ref = second_mailbox.mailbox_ref_id.clone();
    let second = tokio::spawn(async move {
        second_runtime
            .sync_mailbox_full(SyncMailboxRequest {
                mailbox_ref_id: second_ref,
                mode: EmailSyncRunMode::Incremental,
            })
            .await
    });

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if gateway.received_mailbox_calls.lock().unwrap().len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both mailbox provider walks did not overlap");
    assert!(
        runtime
            .mailbox_sync_status(&first_mailbox.mailbox_ref_id)
            .unwrap()
            .running
    );
    assert!(
        runtime
            .mailbox_sync_status(&second_mailbox.mailbox_ref_id)
            .unwrap()
            .running
    );
    let first_progress = crate::MailboxSyncProgress {
        direction: EmailDirection::Inbound,
        pages_listed: 1,
        summaries_listed: 11,
        details_fetched: 0,
        imported: 0,
        skipped_complete: 0,
        estimated_percent: Some(10),
    };
    runtime.record_sync_progress(&first_mailbox.mailbox_ref_id, first_progress);
    assert_eq!(
        runtime
            .mailbox_sync_status(&first_mailbox.mailbox_ref_id)
            .unwrap()
            .progress,
        Some(first_progress)
    );
    assert!(
        runtime
            .mailbox_sync_status(&second_mailbox.mailbox_ref_id)
            .unwrap()
            .progress
            .is_none()
    );

    gate.add_permits(2);
    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();
    assert_eq!(first.inbound.imported, 1);
    assert_eq!(second.inbound.imported, 2);
    for mailbox in [&first_mailbox, &second_mailbox] {
        let status = runtime
            .mailbox_sync_status(&mailbox.mailbox_ref_id)
            .unwrap();
        assert!(!status.running);
        assert!(status.progress.is_none());
    }
    let calls = gateway.received_mailbox_calls.lock().unwrap();
    assert_eq!(calls.iter().filter(|id| id.as_str() == first_id).count(), 1);
    assert_eq!(
        calls.iter().filter(|id| id.as_str() == second_id).count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn finish_and_replacement_run_keep_status_snapshot_coherent() {
    let gate = Arc::new(tokio::sync::Semaphore::new(1));
    let gateway = TestGateway {
        received: Arc::new(vec![history_message("snapshot-race", true)]),
        received_list_gate: Some(Arc::clone(&gate)),
        ..TestGateway::default()
    };
    let (_directory, runtime, gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let request = || SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };
    let (finish_removed, release_finish) = runtime.install_next_full_sync_finish_barrier();

    let first_runtime = runtime.clone();
    let first_request = request();
    let first = tokio::spawn(async move { first_runtime.sync_mailbox_full(first_request).await });
    finish_removed
        .await
        .expect("first run never reached registry finalization");

    // The completed slot has been removed while the finish hook still owns
    // the registry lock. Queue both a replacement run and a status read at
    // this exact boundary; either legal lock order must return one coherent
    // slot snapshot, never idle state paired with either run's progress.
    let second_runtime = runtime.clone();
    let second_request = request();
    let second =
        tokio::spawn(async move { second_runtime.sync_mailbox_full(second_request).await });
    let status_runtime = runtime.clone();
    let status_mailbox_ref_id = mailbox.mailbox_ref_id.clone();
    let racing_status = tokio::task::spawn_blocking(move || {
        status_runtime.mailbox_sync_status(&status_mailbox_ref_id)
    });
    release_finish.send(()).unwrap();

    first.await.unwrap().unwrap();
    let racing_status = racing_status.await.unwrap().unwrap();
    assert!(racing_status.running || racing_status.progress.is_none());
    if !racing_status.running {
        assert!(racing_status.progress.is_none());
    }

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if gateway.received_mailbox_calls.lock().unwrap().len() == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("replacement run never started");
    let replacement_progress = crate::MailboxSyncProgress {
        direction: EmailDirection::Inbound,
        pages_listed: 9,
        summaries_listed: 99,
        details_fetched: 7,
        imported: 6,
        skipped_complete: 5,
        estimated_percent: Some(44),
    };
    runtime.record_sync_progress(&mailbox.mailbox_ref_id, replacement_progress);
    let replacement_status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(replacement_status.running);
    assert_eq!(replacement_status.progress, Some(replacement_progress));

    gate.add_permits(1);
    second.await.unwrap().unwrap();
    let final_status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!final_status.running);
    assert!(final_status.progress.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn full_preflight_and_composite_readback_leave_two_worker_runtime_responsive() {
    let (_directory, runtime, _) = runtime(TestGateway::default());
    let first_mailbox = mailbox_with_id(
        &runtime,
        "11111111-1111-4111-8111-111111111111",
        "outreach@example.com",
        vec![EmailCapability::Inbox],
    );
    let second_mailbox = mailbox_with_id(
        &runtime,
        "22222222-2222-4222-8222-222222222222",
        "outreach@example.com",
        vec![EmailCapability::Inbox],
    );
    let (first_preflight, release_first_preflight) = runtime.install_next_repository_read_barrier();
    let (second_preflight, release_second_preflight) =
        runtime.install_next_repository_read_barrier();
    let (first_readback_ready, release_first_readback_ready) =
        runtime.install_next_full_sync_readback_barrier();
    let (second_readback_ready, release_second_readback_ready) =
        runtime.install_next_full_sync_readback_barrier();

    let first_runtime = runtime.clone();
    let first_ref = first_mailbox.mailbox_ref_id.clone();
    let first = tokio::spawn(async move {
        first_runtime
            .sync_mailbox_full(SyncMailboxRequest {
                mailbox_ref_id: first_ref,
                mode: EmailSyncRunMode::Incremental,
            })
            .await
    });
    let second_runtime = runtime.clone();
    let second_ref = second_mailbox.mailbox_ref_id.clone();
    let second = tokio::spawn(async move {
        second_runtime
            .sync_mailbox_full(SyncMailboxRequest {
                mailbox_ref_id: second_ref,
                mode: EmailSyncRunMode::Incremental,
            })
            .await
    });

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let (first, second) = tokio::join!(first_preflight, second_preflight);
        first.unwrap();
        second.unwrap();
    })
    .await
    .expect("both Full preflight reads did not enter");
    assert_runtime_health().await;
    release_first_preflight.send(()).unwrap();
    release_second_preflight.send(()).unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let (first, second) = tokio::join!(first_readback_ready, second_readback_ready);
        first.unwrap();
        second.unwrap();
    })
    .await
    .expect("both Full runs did not reach composite readback");
    let (first_readback, release_first_readback) = runtime.install_next_repository_read_barrier();
    let (second_readback, release_second_readback) = runtime.install_next_repository_read_barrier();
    release_first_readback_ready.send(()).unwrap();
    release_second_readback_ready.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let (first, second) = tokio::join!(first_readback, second_readback);
        first.unwrap();
        second.unwrap();
    })
    .await
    .expect("both Full composite readbacks did not enter");
    assert_runtime_health().await;
    release_first_readback.send(()).unwrap();
    release_second_readback.send(()).unwrap();

    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
}

#[tokio::test]
async fn status_before_any_run_reports_idle_with_no_durable_state() {
    let (_directory, runtime, _) = runtime(TestGateway::default());
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);
    let status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!status.running);
    assert!(status.progress.is_none());
    assert!(status.inbound.is_none());
    assert!(status.outbound.is_none());
}

#[tokio::test]
async fn a_dropped_caller_never_abandons_the_run_or_leaks_running_state() {
    let inbound = history_message("received-1", true);
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _gateway) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    // Drop the caller future immediately after starting the operation.
    let request = SyncMailboxRequest {
        mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
        mode: EmailSyncRunMode::Incremental,
    };
    {
        let caller = runtime.sync_mailbox_full(request);
        tokio::pin!(caller);
        // Poll once so the operation spawns, then drop the caller.
        assert!(
            futures_poll_once(caller.as_mut()).await.is_none(),
            "the full sync must still be running when its caller is dropped"
        );
    }

    // The spawned run completes on its own: durable facts appear and the
    // registry clears.
    for _ in 0..200 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let status = runtime
            .mailbox_sync_status(&mailbox.mailbox_ref_id)
            .unwrap();
        if let (false, Some(inbound)) = (status.running, status.inbound) {
            assert!(inbound.backfill_complete);
            return;
        }
    }
    panic!("the dropped caller's run never completed");
}

async fn futures_poll_once<F: std::future::Future + Unpin>(future: F) -> Option<F::Output> {
    struct PollOnce<F>(Option<F>);
    impl<F: std::future::Future + Unpin> std::future::Future for PollOnce<F> {
        type Output = Option<F::Output>;
        fn poll(
            mut self: std::pin::Pin<&mut Self>,
            context: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Self::Output> {
            let mut inner = self.0.take().expect("polled after completion");
            match std::pin::Pin::new(&mut inner).poll(context) {
                std::task::Poll::Ready(output) => std::task::Poll::Ready(Some(output)),
                std::task::Poll::Pending => std::task::Poll::Ready(None),
            }
        }
    }
    PollOnce(Some(future)).await
}

#[tokio::test]
async fn live_progress_publishes_per_page_and_never_outlives_the_run() {
    let inbound = history_message("received-1", true);
    let sent = history_message("sent-1", false);
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        sent_history: Arc::new(vec![sent]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(
        &runtime,
        vec![
            EmailCapability::Inbox,
            EmailCapability::Send,
            EmailCapability::SentReadback,
        ],
    );

    let mut changes = runtime.subscribe_changes();
    runtime
        .sync_mailbox_full(SyncMailboxRequest {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            mode: EmailSyncRunMode::Incremental,
        })
        .await
        .unwrap();

    // Run start + one received page + one sent page + run end: the per-page
    // events are what lets a status reader watch counters move mid-run.
    let mut email_sync_events = 0;
    while let Ok(change) = changes.try_recv() {
        if change.scopes.contains(&CreatorChangeScope::EmailSync) {
            email_sync_events += 1;
        }
    }
    assert!(
        email_sync_events >= 4,
        "per-page progress must publish EmailSync between start and end, saw {email_sync_events}"
    );

    // The registry entry cleared with the run: progress never outlives it.
    let status = runtime
        .mailbox_sync_status(&mailbox.mailbox_ref_id)
        .unwrap();
    assert!(!status.running);
    assert!(status.progress.is_none());
}

#[tokio::test]
async fn a_direct_engine_run_records_no_progress_and_publishes_no_sync_scope() {
    let inbound = history_message("received-1", true);
    let gateway = TestGateway {
        received: Arc::new(vec![inbound]),
        ..TestGateway::default()
    };
    let (_directory, runtime, _) = runtime(gateway);
    let mailbox = mailbox(&runtime, vec![EmailCapability::Inbox]);

    let mut changes = runtime.subscribe_changes();
    runtime
        .sync_mailbox_received(&mailbox.mailbox_ref_id)
        .await
        .unwrap();

    while let Ok(change) = changes.try_recv() {
        assert!(
            !change.scopes.contains(&CreatorChangeScope::EmailSync),
            "an engine run outside sync_mailbox_full must not publish sync progress"
        );
    }
    assert!(
        runtime
            .mailbox_sync_status(&mailbox.mailbox_ref_id)
            .unwrap()
            .progress
            .is_none()
    );
}
