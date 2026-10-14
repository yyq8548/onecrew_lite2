use super::*;

#[tokio::test]
async fn failed_thread_sync_preserves_the_safe_provider_diagnostic_and_stage() {
    let (_directory, runtime, _) = runtime(TestGateway {
        conversation_unavailable: Arc::new(AtomicBool::new(true)),
        ..TestGateway::default()
    });
    let ids = outreach_ids(&runtime);
    let error = runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0,
                contact_route_id: ids.1,
                resolved_target_id: ids.2,
                operator_account_ref_id: ids.3,
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .unwrap_err();
    let crate::ConversationPlatformError::ProviderRead(failure) = error else {
        panic!("the thread read error must retain its provider evidence")
    };
    assert_eq!(failure.stage, "thread_sync");
    assert_eq!(failure.diagnostic.platform, "test");
    assert_eq!(failure.diagnostic.protocol, "test_thread_reader");
    assert_eq!(failure.diagnostic.code, None);
    assert_eq!(failure.diagnostic.message, "conversation read failed");
}

#[tokio::test]
async fn absent_conversation_is_a_normal_empty_read() {
    let (_directory, runtime, _) = runtime(TestGateway {
        conversation_absent: Arc::new(AtomicBool::new(true)),
        ..TestGateway::default()
    });
    let ids = outreach_ids(&runtime);

    let local = runtime
        .get_conversation(ConversationGetInput {
            conversation_id: ConversationId::new("conversation-that-was-never-recorded"),
        })
        .unwrap();
    assert!(local.is_none());

    let remote = runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0.clone(),
                contact_route_id: ids.1.clone(),
                resolved_target_id: ids.2.clone(),
                operator_account_ref_id: ids.3.clone(),
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .unwrap();
    assert!(remote.is_none());
}

#[tokio::test]
async fn sync_reuses_the_locally_persisted_provider_thread_id() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    sync_fixture(&runtime, &ids).await.unwrap();

    let remote = runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0.clone(),
                contact_route_id: ids.1.clone(),
                resolved_target_id: ids.2.clone(),
                operator_account_ref_id: ids.3.clone(),
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .unwrap();
    assert!(remote.is_some());
    {
        let requests = gateway.conversation_requests.lock().unwrap();
        assert_eq!(
            requests.last().and_then(|thread_id| thread_id.as_deref()),
            Some("thread-1")
        );
    }

    gateway.conversation_absent.store(true, Ordering::SeqCst);
    let fallback = runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0,
                contact_route_id: ids.1,
                resolved_target_id: ids.2,
                operator_account_ref_id: ids.3,
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .unwrap()
        .expect("local conversation should remain visible after a remote lookup miss");
    assert_eq!(fallback.conversation.provider_thread_id, "thread-1");
    assert_eq!(fallback.messages.len(), 1);
}

#[tokio::test]
async fn a_stored_message_keeps_the_platform_payload_exactly_as_it_was_returned() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let sent = sync_fixture(&runtime, &ids).await.unwrap();

    // Whatever the platform said about the message is stored under one key and
    // is never read, reshaped or trimmed on the way in.
    let expected = serde_json::json!({
        "id": "message-1",
        "sender": { "id": "operator", "verified": true },
        "reactions": ["🔥"],
    });
    assert_eq!(message_public_json(&directory)["provider"], expected);
    assert_eq!(sent.messages[0].public_data["provider"], expected);
}

#[tokio::test]
async fn a_message_stored_before_the_payload_was_kept_still_reads_as_an_absence() {
    let (directory, runtime, _) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let sent = sync_fixture(&runtime, &ids).await.unwrap();
    // A row written by an earlier release: same DDL, the older content.
    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE messages SET public_json = ?1",
            [r#"{"sender_id":"operator"}"#],
        )
        .unwrap();

    let view = runtime
        .get_conversation(ConversationGetInput {
            conversation_id: sent.conversation.id.clone(),
        })
        .expect("the legacy row still reads")
        .expect("the conversation is present");
    let message = view.messages.first().expect("the legacy message is listed");
    assert!(
        message.public_data.get("provider").is_none(),
        "an older row carries no platform payload, and nothing invents one"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_sync_repository_segments_keep_two_worker_runtime_responsive() {
    use tokio::sync::broadcast::error::TryRecvError;

    let barrier = ConversationBarrier {
        entered: Arc::new(tokio::sync::Barrier::new(2)),
        release: Arc::new(tokio::sync::Barrier::new(2)),
    };
    let (_directory, runtime, gateway) = runtime(TestGateway {
        conversation_barrier: Arc::new(Mutex::new(Some(barrier.clone()))),
        ..TestGateway::default()
    });
    let ids = outreach_ids(&runtime);
    let mut changes = runtime.subscribe_changes();
    let (first_read, release_first_read) = hold_repository_read(&runtime).await;
    let (second_read, release_second_read) = hold_repository_read(&runtime).await;

    let sync_runtime = runtime.clone();
    let sync_input = ConversationSyncInput {
        creator_id: ids.0.clone(),
        contact_route_id: ids.1.clone(),
        resolved_target_id: ids.2.clone(),
        operator_account_ref_id: ids.3.clone(),
        external_thread_id: None,
        limit: 100,
    };
    let sync = tokio::spawn(async move { sync_runtime.sync_conversation(sync_input, None).await });
    assert_runtime_health().await;
    assert!(gateway.conversation_requests.lock().unwrap().is_empty());

    release_first_read.send(()).unwrap();
    first_read.await.unwrap().unwrap();
    barrier.entered.wait().await;
    assert_eq!(gateway.conversation_requests.lock().unwrap().len(), 1);
    let (third_read, release_third_read) = hold_repository_read(&runtime).await;

    let (writer, release_writer) = hold_repository_write(&runtime).await;
    barrier.release.wait().await;
    assert_runtime_health().await;
    assert!(!sync.is_finished());
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));
    release_writer.send(()).unwrap();
    writer.await.unwrap().unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(1), changes.recv())
        .await
        .expect("conversation commit did not publish")
        .unwrap();
    assert_eq!(
        change.scopes,
        vec![crate::CreatorChangeScope::Conversations]
    );
    assert!(
        !sync.is_finished(),
        "sync bypassed saturated read admission for triage readback"
    );
    release_second_read.send(()).unwrap();
    second_read.await.unwrap().unwrap();

    let view = sync.await.unwrap().unwrap().expect("remote conversation");
    assert_eq!(view.conversation.provider_thread_id, "thread-1");
    assert_eq!(view.messages.len(), 1);
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));

    release_third_read.send(()).unwrap();
    third_read.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_sync_remote_miss_fallback_uses_read_admission_without_change() {
    use tokio::sync::broadcast::error::TryRecvError;

    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    sync_fixture(&runtime, &ids).await.unwrap();
    gateway.conversation_absent.store(true, Ordering::SeqCst);
    let barrier = ConversationBarrier {
        entered: Arc::new(tokio::sync::Barrier::new(2)),
        release: Arc::new(tokio::sync::Barrier::new(2)),
    };
    *gateway.conversation_barrier.lock().unwrap() = Some(barrier.clone());
    let mut changes = runtime.subscribe_changes();

    let sync_runtime = runtime.clone();
    let sync_input = ConversationSyncInput {
        creator_id: ids.0,
        contact_route_id: ids.1,
        resolved_target_id: ids.2,
        operator_account_ref_id: ids.3,
        external_thread_id: None,
        limit: 100,
    };
    let sync = tokio::spawn(async move { sync_runtime.sync_conversation(sync_input, None).await });
    barrier.entered.wait().await;
    let (first_read, release_first_read) = hold_repository_read(&runtime).await;
    let (second_read, release_second_read) = hold_repository_read(&runtime).await;
    barrier.release.wait().await;
    assert_runtime_health().await;
    assert!(
        !sync.is_finished(),
        "remote-miss fallback bypassed saturated read admission"
    );
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));

    release_first_read.send(()).unwrap();
    first_read.await.unwrap().unwrap();
    let view = sync
        .await
        .unwrap()
        .unwrap()
        .expect("local fallback conversation");
    assert_eq!(view.conversation.provider_thread_id, "thread-1");
    assert_eq!(view.messages.len(), 1);
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));

    release_second_read.send(()).unwrap();
    second_read.await.unwrap().unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conversation_sync_started_commit_survives_caller_cancellation_for_readback() {
    use tokio::sync::broadcast::error::TryRecvError;

    let barrier = ConversationBarrier {
        entered: Arc::new(tokio::sync::Barrier::new(2)),
        release: Arc::new(tokio::sync::Barrier::new(2)),
    };
    let (_directory, runtime, gateway) = runtime(TestGateway {
        conversation_barrier: Arc::new(Mutex::new(Some(barrier.clone()))),
        ..TestGateway::default()
    });
    let ids = outreach_ids(&runtime);
    let mut changes = runtime.subscribe_changes();
    let sync_input = ConversationSyncInput {
        creator_id: ids.0.clone(),
        contact_route_id: ids.1.clone(),
        resolved_target_id: ids.2.clone(),
        operator_account_ref_id: ids.3.clone(),
        external_thread_id: None,
        limit: 100,
    };
    let sync_runtime = runtime.clone();
    let sync = tokio::spawn(async move { sync_runtime.sync_conversation(sync_input, None).await });
    barrier.entered.wait().await;

    let (write_started, release_write) = runtime.install_next_repository_write_barrier();
    barrier.release.wait().await;
    write_started
        .await
        .expect("conversation commit never entered writer closure");
    assert_runtime_health().await;
    sync.abort();
    assert!(sync.await.unwrap_err().is_cancelled());
    release_write.send(()).unwrap();

    let change = tokio::time::timeout(std::time::Duration::from_secs(2), changes.recv())
        .await
        .expect("cancelled caller stranded committed conversation change")
        .unwrap();
    assert_eq!(
        change.scopes,
        vec![crate::CreatorChangeScope::Conversations]
    );
    assert!(matches!(changes.try_recv(), Err(TryRecvError::Empty)));

    let readback_runtime = runtime.clone();
    let view = tokio::time::timeout(std::time::Duration::from_secs(2), async move {
        loop {
            let runtime = readback_runtime.clone();
            let view = tokio::task::spawn_blocking(move || {
                let listed = runtime
                    .list_dm_conversations(crate::ListConversationsInput::default())
                    .unwrap();
                listed.items.first().and_then(|summary| {
                    runtime
                        .get_conversation(ConversationGetInput {
                            conversation_id: summary.conversation_id.clone(),
                        })
                        .unwrap()
                })
            })
            .await
            .unwrap();
            if let Some(view) = view {
                break view;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("started conversation commit was not durable for readback");
    assert_eq!(view.conversation.provider_thread_id, "thread-1");
    assert_eq!(view.messages.len(), 1);
    assert_eq!(gateway.conversation_requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn archived_creator_can_read_existing_local_conversation_history() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let sent = sync_fixture(&runtime, &ids).await.unwrap();
    runtime.archive_creator(&ids.0).unwrap();

    let view = runtime
        .get_conversation(ConversationGetInput {
            conversation_id: sent.conversation.id.clone(),
        })
        .unwrap()
        .unwrap();
    assert_eq!(view.conversation.id, sent.conversation.id);
    assert_eq!(view.messages.len(), 1);
    assert_eq!(view.messages[0].body, "hello");

    let sync_error = runtime
        .sync_conversation(
            ConversationSyncInput {
                creator_id: ids.0.clone(),
                contact_route_id: ids.1.clone(),
                resolved_target_id: ids.2.clone(),
                operator_account_ref_id: ids.3.clone(),
                external_thread_id: None,
                limit: 100,
            },
            None,
        )
        .await
        .unwrap_err();
    // Archiving is the only change since the fixture sync succeeded. The
    // active-Creator rejection is a context read failure, which the typed
    // error reports by stage without echoing repository text, and it stops
    // the sync before any provider thread read.
    assert_eq!(
        sync_error,
        crate::ConversationPlatformError::LocalRead("context")
    );
    assert_eq!(gateway.conversation_requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn inbox_listing_carries_the_full_route_context_and_latest_preview() {
    let (directory, runtime, _gateway) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);

    let empty = runtime
        .list_dm_conversations(crate::ListConversationsInput::default())
        .unwrap();
    assert!(empty.items.is_empty());
    assert!(empty.next_cursor.is_none());

    sync_fixture(&runtime, &ids).await.unwrap();
    sync_fixture(&runtime, &ids).await.unwrap();

    let page = runtime
        .list_dm_conversations(crate::ListConversationsInput::default())
        .unwrap();
    assert_eq!(
        page.items.len(),
        1,
        "one thread per (account, target, channel)"
    );
    let row = &page.items[0];
    assert_eq!(row.creator_id, ids.0);
    assert_eq!(row.contact_route_id, ids.1);
    assert_eq!(row.resolved_target_id, ids.2);
    assert_eq!(row.operator_account_ref_id, ids.3);
    assert!(!row.creator_display_name.is_empty());
    assert!(!row.operator_account_label.is_empty());
    assert!(!row.provider_thread_id.is_empty());
    // The latest preview comes from the normal persisted provider snapshot.
    let last = row.last_message.as_ref().expect("latest message preview");
    assert!(!last.preview.is_empty());
    assert!(!last.direction.is_empty());

    // 分页:limit 钳到最小也返回同一行并终止。
    let bounded = runtime
        .list_dm_conversations(crate::ListConversationsInput {
            limit: Some(1),
            cursor: None,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(bounded.items.len(), 1);
    assert!(
        bounded.next_cursor.is_none(),
        "single thread has no continuation"
    );

    let replied = runtime
        .list_dm_conversations(crate::ListConversationsInput {
            query: Some("@operator".into()),
            status: Some(crate::ConversationListStatus::Replied),
            channel: Some(ContactChannel::X),
            ..Default::default()
        })
        .expect("matching reply filters");
    assert_eq!(replied.items.len(), 1);
    for input in [
        crate::ListConversationsInput {
            status: Some(crate::ConversationListStatus::WaitingReply),
            ..Default::default()
        },
        crate::ListConversationsInput {
            channel: Some(ContactChannel::Instagram),
            ..Default::default()
        },
        crate::ListConversationsInput {
            query: Some("no-such-conversation".into()),
            ..Default::default()
        },
    ] {
        assert!(
            runtime
                .list_dm_conversations(input)
                .unwrap()
                .items
                .is_empty(),
            "non-matching conversation filter must return no rows"
        );
    }

    Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .execute("UPDATE messages SET direction = 'inbound'", [])
        .unwrap();
    assert_eq!(
        runtime
            .list_dm_conversations(crate::ListConversationsInput {
                status: Some(crate::ConversationListStatus::WaitingReply),
                ..Default::default()
            })
            .unwrap()
            .items
            .len(),
        1
    );
    assert!(
        runtime
            .list_dm_conversations(crate::ListConversationsInput {
                status: Some(crate::ConversationListStatus::Replied),
                ..Default::default()
            })
            .unwrap()
            .items
            .is_empty()
    );
}

#[tokio::test]
async fn a_sync_named_by_another_toolkit_is_refused_before_the_provider_is_read() {
    let (_directory, runtime, gateway) = runtime(TestGateway::default());
    let ids = outreach_ids(&runtime);
    let input = ConversationSyncInput {
        creator_id: ids.0.clone(),
        contact_route_id: ids.1.clone(),
        resolved_target_id: ids.2.clone(),
        operator_account_ref_id: ids.3.clone(),
        external_thread_id: None,
        limit: 100,
    };

    let error = runtime
        .sync_conversation(input.clone(), Some(Platform::Instagram))
        .await
        .unwrap_err();
    assert_eq!(error, crate::ConversationPlatformError::WrongPlatform);
    assert!(
        gateway.conversation_requests.lock().unwrap().is_empty(),
        "a refused toolkit must not reach the provider"
    );

    // The account's own toolkit reads normally.
    runtime
        .sync_conversation(input, Some(Platform::X))
        .await
        .expect("the account's own toolkit syncs");
    assert_eq!(gateway.conversation_requests.lock().unwrap().len(), 1);
}
