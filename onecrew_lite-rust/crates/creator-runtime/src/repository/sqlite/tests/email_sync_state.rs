use super::*;
use crate::{
    EmailAttachmentInput, EmailMessageInput, EmailOrigin, EmailSnapshotInput, EmailSyncAdvance,
    EmailSyncFailure, EmailSyncIncrementalContinuationMutation, EmailSyncPageCommit,
    EmailSyncSentMutations, EmailSyncThreadMerge,
};

fn mailbox(repository: &SqliteRepository, id: &str) -> MailboxRefId {
    repository
        .upsert_mailbox_ref(MailboxRefInput {
            id: MailboxRefId::new(id.to_owned()),
            provider: "resend".into(),
            label: "Mail".into(),
            email_address: "hello@example-team.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox, EmailCapability::Send],
            available: true,
            last_error: None,
        })
        .expect("mailbox")
        .mailbox_ref_id
}

fn inbound_message(provider_message_id: &str, occurred_at: u64) -> EmailMessageInput {
    EmailMessageInput {
        provider_message_id: provider_message_id.into(),
        internet_message_id: Some(format!("<{provider_message_id}@mailer.example.net>")),
        direction: EmailDirection::Inbound,
        from: "Creator <creator@example.net>".into(),
        to: vec!["hello@example-team.com".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        received_for: vec!["hello@example-team.com".into()],
        reply_to: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        subject: "Reply".into(),
        text: Some("Hi".into()),
        html: None,
        delivery_status: "unread".into(),
        headers: serde_json::json!({}),
        occurred_at,
        origin: EmailOrigin::ProviderImport,
        attachments: vec![EmailAttachmentInput {
            provider_attachment_id: format!("attachment-{provider_message_id}"),
            filename: "media-kit.pdf".into(),
            content_type: "application/pdf".into(),
            content_id: None,
            content_disposition: Some("attachment".into()),
            size_bytes: Some(482_133),
            available: true,
        }],
    }
}

fn outbound_message(provider_message_id: &str, occurred_at: u64) -> EmailMessageInput {
    let mut message = inbound_message(provider_message_id, occurred_at);
    message.direction = EmailDirection::Outbound;
    message.delivery_status = "sent".into();
    message
}

fn snapshot(mailbox_ref_id: &MailboxRefId, message: EmailMessageInput) -> EmailSnapshotInput {
    EmailSnapshotInput {
        mailbox_ref_id: mailbox_ref_id.clone(),
        creator_id: None,
        contact_route_id: None,
        provider_thread_key: format!("resend:message:{}", message.provider_message_id),
        subject: message.subject.clone(),
        messages: vec![message],
    }
}

#[test]
fn a_complete_page_commits_facts_and_watermark_together() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "11111111-1111-4111-8111-111111111111");

    let outcome = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![
                snapshot(&mailbox_id, inbound_message("provider-1", 200)),
                snapshot(&mailbox_id, inbound_message("provider-2", 100)),
            ],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-1".into()),
                newest_occurred_at: Some(200),
                oldest_backfill_cursor: Some("provider-2".into()),
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .expect("page commit");
    assert!(outcome.advanced);
    assert_eq!(outcome.changed_thread_ids.len(), 2);

    let state = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .expect("state read")
        .expect("state row");
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("provider-1")
    );
    assert_eq!(state.newest_occurred_at, Some(200));
    assert_eq!(state.oldest_backfill_cursor.as_deref(), Some("provider-2"));
    assert!(!state.backfill_complete);
    assert!(state.last_page_at.is_some());
    assert_eq!(state.last_full_sync_at, None);
    assert_eq!(state.failure, None);

    // The identity map answers the dedupe question without a detail fetch.
    let complete = repository
        .complete_email_provider_ids(
            &mailbox_id,
            &[
                "provider-1".into(),
                "provider-2".into(),
                "provider-3".into(),
            ],
        )
        .expect("dedupe read");
    assert_eq!(
        complete,
        vec!["provider-1".to_owned(), "provider-2".to_owned()]
    );

    // Attachment metadata persisted without any URL.
    let connection = Connection::open(_directory.path().join("crm.db")).expect("open db");
    let (count, urls): (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN filename LIKE '%http%' THEN 1 ELSE 0 END)
             FROM email_attachments",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("attachment rows");
    assert_eq!(count, 2);
    assert_eq!(urls, 0);
}

#[test]
fn a_partial_page_commits_its_subset_once_without_advancing_the_boundary() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "22222222-2222-4222-8222-222222222222");

    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![snapshot(&mailbox_id, inbound_message("provider-old", 50))],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-old".into()),
                newest_occurred_at: Some(50),
                oldest_backfill_cursor: Some("provider-old".into()),
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .expect("first page");

    // The next page fails one required detail: its successful subset commits
    // once, the watermark stays, and sanitized failure state is durable.
    let outcome = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![snapshot(&mailbox_id, inbound_message("provider-new", 300))],
            advance: None,
            failure: Some(EmailSyncFailure {
                missing_provider_message_ids: vec!["provider-missing".into()],
                error_class: "rate_limited".into(),
                message: "detail fetch was rate limited".into(),
            }),
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .expect("partial page");
    assert!(!outcome.advanced);
    assert_eq!(outcome.changed_thread_ids.len(), 1);

    let state = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .expect("state read")
        .expect("state row");
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("provider-old"),
        "a failed page never advances the boundary"
    );
    assert!(state.backfill_complete);
    assert!(state.last_full_sync_at.is_some());
    let failure = state.failure.expect("failure state is durable");
    assert_eq!(
        failure.missing_provider_message_ids,
        vec!["provider-missing"]
    );

    // The successful subset is durable and skippable next run.
    let complete = repository
        .complete_email_provider_ids(&mailbox_id, &["provider-new".into()])
        .expect("dedupe read");
    assert_eq!(complete, vec!["provider-new".to_owned()]);

    // Recovery: the retried page completes and clears the failure state.
    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![snapshot(
                &mailbox_id,
                inbound_message("provider-missing", 310),
            )],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-new".into()),
                newest_occurred_at: Some(300),
                oldest_backfill_cursor: Some("provider-old".into()),
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .expect("retried page");
    let state = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .expect("state read")
        .expect("state row");
    assert_eq!(state.failure, None);
    assert_eq!(
        state.newest_provider_message_id.as_deref(),
        Some("provider-new")
    );
}

#[test]
fn a_failing_snapshot_rolls_back_the_whole_page() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "33333333-3333-4333-8333-333333333333");
    let foreign_mailbox = MailboxRefId::new("99999999-9999-4999-8999-999999999999");

    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-watermark".into()),
                newest_occurred_at: Some(5),
                oldest_backfill_cursor: Some("phase-b-cursor".into()),
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: EmailSyncIncrementalContinuationMutation::Set {
                next_cursor: "phase-a-before".into(),
                newest_provider_message_id: "phase-a-anchor".into(),
                newest_occurred_at: 30,
            },
            sent_mutations: Default::default(),
        })
        .expect("seed independent Phase A and Phase B cursors");

    let error = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![
                snapshot(&mailbox_id, inbound_message("provider-a", 10)),
                snapshot(&foreign_mailbox, inbound_message("provider-b", 20)),
            ],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-a".into()),
                newest_occurred_at: Some(10),
                oldest_backfill_cursor: None,
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: EmailSyncIncrementalContinuationMutation::Set {
                next_cursor: "phase-a-after".into(),
                newest_provider_message_id: "provider-a".into(),
                newest_occurred_at: 10,
            },
            sent_mutations: Default::default(),
        })
        .expect_err("a snapshot outside the committing mailbox fails the page");
    assert!(error.contains("committing mailbox"));

    // Nothing from the failed page is durable: not the first snapshot, not
    // either boundary.
    assert_eq!(
        repository
            .complete_email_provider_ids(&mailbox_id, &["provider-a".into()])
            .expect("dedupe read"),
        Vec::<String>::new()
    );
    let state = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.state)
        .expect("state read")
        .expect("seeded state");
    assert_eq!(
        state.oldest_backfill_cursor.as_deref(),
        Some("phase-b-cursor")
    );
    let continuation = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .map(|checkpoint| checkpoint.incremental_continuation)
        .expect("continuation read")
        .expect("seeded continuation");
    assert_eq!(
        continuation.next_cursor, "phase-a-before",
        "a rolled-back page moved its Phase A continuation"
    );
}

#[test]
fn phase_a_continuation_coexists_with_phase_b_cursor_and_clears_atomically() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "34343434-3434-4434-8434-343434343434");

    let set = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("old-watermark".into()),
                newest_occurred_at: Some(10),
                oldest_backfill_cursor: Some("phase-b-cursor".into()),
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: EmailSyncIncrementalContinuationMutation::Set {
                next_cursor: "  phase-a-cursor  ".into(),
                newest_provider_message_id: "new-anchor".into(),
                newest_occurred_at: 30,
            },
            sent_mutations: Default::default(),
        })
        .expect("atomic Phase A page");
    assert!(set.advanced);
    assert!(set.continuation_changed);
    assert_eq!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap()
            .oldest_backfill_cursor
            .as_deref(),
        Some("phase-b-cursor")
    );
    assert_eq!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .unwrap()
            .next_cursor,
        "  phase-a-cursor  "
    );

    let clear = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("new-anchor".into()),
                newest_occurred_at: Some(30),
                oldest_backfill_cursor: Some("phase-b-cursor".into()),
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: EmailSyncIncrementalContinuationMutation::Clear,
            sent_mutations: Default::default(),
        })
        .expect("watermark and continuation clear share the page commit");
    assert!(clear.advanced);
    assert!(clear.continuation_changed);
    assert!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap()
            .newest_provider_message_id
            .as_deref(),
        Some("new-anchor")
    );
}

#[test]
fn page_commit_owner_rejects_blank_continuation_boundaries_without_writes() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "35353535-3535-4535-8535-353535353535");

    for mutation in [
        EmailSyncIncrementalContinuationMutation::Set {
            next_cursor: " \t\r\n".into(),
            newest_provider_message_id: "anchor".into(),
            newest_occurred_at: 20,
        },
        EmailSyncIncrementalContinuationMutation::Set {
            next_cursor: "next".into(),
            newest_provider_message_id: " \t\r\n".into(),
            newest_occurred_at: 20,
        },
    ] {
        let error = repository
            .commit_email_sync_page(EmailSyncPageCommit {
                mailbox_ref_id: mailbox_id.clone(),
                direction: EmailDirection::Inbound,
                snapshots: Vec::new(),
                advance: Some(EmailSyncAdvance {
                    newest_provider_message_id: Some("would-advance".into()),
                    newest_occurred_at: Some(20),
                    oldest_backfill_cursor: None,
                    backfill_complete: false,
                    mark_full_sync: false,
                }),
                failure: None,
                incremental_continuation: mutation,
                sent_mutations: Default::default(),
            })
            .expect_err("blank private boundary must fail at the page owner");
        assert!(error.contains("must not be blank"), "{error}");
    }
    let checkpoint = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
        .unwrap();
    assert!(checkpoint.state.is_none());
    assert!(checkpoint.incremental_continuation.is_none());
}

#[test]
fn sent_status_merge_snapshot_and_checkpoint_share_one_transaction() {
    let (directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "55555555-5555-4555-8555-555555555555");
    let (target, _) = repository
        .persist_email_snapshot(snapshot(
            &mailbox_id,
            outbound_message("provider-status", 10),
        ))
        .unwrap();
    repository
        .persist_email_snapshot(EmailSnapshotInput {
            mailbox_ref_id: mailbox_id.clone(),
            creator_id: None,
            contact_route_id: None,
            provider_thread_key: target.provider_thread_key.clone(),
            subject: "Reply".into(),
            messages: vec![outbound_message("provider-status-2", 11)],
        })
        .unwrap();
    let (source, _) = repository
        .persist_email_snapshot(snapshot(
            &mailbox_id,
            inbound_message("provider-source", 20),
        ))
        .unwrap();
    let foreign_mailbox = MailboxRefId::new("99999999-9999-4999-8999-999999999999");
    let mutations = EmailSyncSentMutations {
        status_updates: vec![
            ("provider-status".into(), "delivered".into()),
            ("provider-status-2".into(), "delivered".into()),
        ],
        thread_merges: vec![EmailSyncThreadMerge {
            target: target.id.clone(),
            sources: vec![source.id.clone(), source.id.clone()],
        }],
    };

    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: vec![snapshot(
                &foreign_mailbox,
                outbound_message("provider-invalid", 30),
            )],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-invalid".into()),
                newest_occurred_at: Some(30),
                oldest_backfill_cursor: None,
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: mutations.clone(),
        })
        .expect_err("invalid snapshot must roll back every sent mutation");
    let connection = Connection::open(directory.path().join("crm.db")).unwrap();
    let status: String = connection
        .query_row(
            "SELECT delivery_status FROM emails WHERE provider_message_id = 'provider-status'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "sent");
    let source_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM email_threads WHERE id = ?1)",
            [source.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(source_exists);
    assert!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .is_none()
    );

    let outcome = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("provider-status".into()),
                newest_occurred_at: Some(10),
                oldest_backfill_cursor: None,
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: mutations,
        })
        .unwrap();
    assert_eq!(outcome.reconciled_statuses, 2);
    assert_eq!(outcome.merged_threads, 1);
    assert!(outcome.changed_thread_ids.contains(&target.id));
    assert!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap()
            .backfill_complete
    );
    let source_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM email_threads WHERE id = ?1)",
            [source.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!source_exists);
}

#[test]
fn a_quiet_sent_overlap_does_not_write_sqlite_or_refresh_checkpoint_state() {
    let (directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "66666666-6666-4666-8666-666666666666");
    let (target, _) = repository
        .persist_email_snapshot(snapshot(
            &mailbox_id,
            outbound_message("provider-quiet", 10),
        ))
        .unwrap();
    let advance = EmailSyncAdvance {
        newest_provider_message_id: Some("provider-quiet".into()),
        newest_occurred_at: Some(10),
        oldest_backfill_cursor: None,
        backfill_complete: true,
        mark_full_sync: true,
    };
    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: Some(advance.clone()),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .unwrap();

    let state_before = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    let target_updated_before: u64 = Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT updated_at FROM email_threads WHERE id = ?1",
            [target.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    let observer = Connection::open(directory.path().join("crm.db")).unwrap();
    let data_version_before: u64 = observer
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();

    let outcome = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: Some(advance),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: EmailSyncSentMutations {
                status_updates: vec![("provider-quiet".into(), "sent".into())],
                thread_merges: vec![EmailSyncThreadMerge {
                    target: target.id.clone(),
                    sources: vec![target.id.clone(), target.id.clone()],
                }],
            },
        })
        .unwrap();

    assert!(!outcome.checkpoint_changed);
    assert!(!outcome.advanced);
    assert_eq!(outcome.reconciled_statuses, 0);
    assert_eq!(outcome.merged_threads, 0);
    assert!(outcome.changed_thread_ids.is_empty());
    let data_version_after: u64 = observer
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        data_version_after, data_version_before,
        "a quiet overlap committed a SQLite write"
    );
    assert_eq!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .unwrap()
            .unwrap(),
        state_before,
        "a quiet overlap refreshed the durable checkpoint"
    );
    let target_updated_after: u64 = Connection::open(directory.path().join("crm.db"))
        .unwrap()
        .query_row(
            "SELECT updated_at FROM email_threads WHERE id = ?1",
            [target.id.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(target_updated_after, target_updated_before);
}

#[test]
fn a_failure_only_page_reports_its_checkpoint_commit_without_advancing() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "77777777-7777-4777-8777-777777777777");
    let failure = EmailSyncFailure {
        missing_provider_message_ids: vec!["provider-missing".into()],
        error_class: "detail_retrieval_failed".into(),
        message: "detail unavailable".into(),
    };

    let outcome = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: None,
            failure: Some(failure.clone()),
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .unwrap();
    assert!(outcome.checkpoint_changed);
    assert!(!outcome.advanced);
    assert!(outcome.changed_thread_ids.is_empty());
    let state = repository
        .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
        .map(|checkpoint| checkpoint.state)
        .unwrap()
        .unwrap();
    assert_eq!(state.newest_provider_message_id, None);
    assert_eq!(state.failure.as_ref(), Some(&failure));

    let replay = repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id,
            direction: EmailDirection::Outbound,
            snapshots: Vec::new(),
            advance: None,
            failure: Some(failure),
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .unwrap();
    assert!(!replay.checkpoint_changed);
    assert!(!replay.advanced);
}

#[test]
fn directions_keep_independent_state() {
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "44444444-4444-4444-8444-444444444444");

    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("inbound-newest".into()),
                newest_occurred_at: Some(10),
                oldest_backfill_cursor: None,
                backfill_complete: false,
                mark_full_sync: false,
            }),
            failure: None,
            incremental_continuation: EmailSyncIncrementalContinuationMutation::Set {
                next_cursor: "inbound-next".into(),
                newest_provider_message_id: "inbound-anchor".into(),
                newest_occurred_at: 20,
            },
            sent_mutations: Default::default(),
        })
        .expect("inbound state");

    assert!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.state)
            .expect("state read")
            .is_none(),
        "sent and received never share a watermark field"
    );
    assert!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Outbound)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .expect("continuation read")
            .is_none(),
        "sent and received never share a Phase A continuation"
    );
    assert_eq!(
        repository
            .email_sync_checkpoint(&mailbox_id, EmailDirection::Inbound)
            .map(|checkpoint| checkpoint.incremental_continuation)
            .expect("continuation read")
            .unwrap()
            .next_cursor,
        "inbound-next"
    );
}

static MAILBOX_READBACK_TRACE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static MAILBOX_READBACK_TRACE_PAUSE: std::sync::Mutex<
    Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
> = std::sync::Mutex::new(None);

fn pause_mailbox_readback_after_inbound_snapshot(event: rusqlite::trace::TraceEvent<'_>) {
    let rusqlite::trace::TraceEvent::Stmt(statement, sql) = event else {
        return;
    };
    let expanded = statement.expanded_sql().unwrap_or_else(|| sql.to_owned());
    if !expanded.contains("FROM mailbox_sync_state") || !expanded.contains("'outbound'") {
        return;
    }
    let Some((entered, release)) = MAILBOX_READBACK_TRACE_PAUSE.lock().unwrap().take() else {
        return;
    };
    entered.send(()).unwrap();
    release.recv().unwrap();
}

fn arm_mailbox_readback_traces(repository: &SqliteRepository) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    let events = rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT;
    first.trace_v2(events, Some(pause_mailbox_readback_after_inbound_snapshot));
    second.trace_v2(events, Some(pause_mailbox_readback_after_inbound_snapshot));
}

fn clear_mailbox_readback_traces(repository: &SqliteRepository) {
    let first = repository.read_connection().unwrap();
    let second = repository.read_connection().unwrap();
    first.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
    second.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
}

#[test]
fn mailbox_sync_readback_uses_one_committed_snapshot_during_concurrent_write() {
    use std::{sync::mpsc, time::Duration};

    let _trace_guard = MAILBOX_READBACK_TRACE_TEST_LOCK.lock().unwrap();
    let (_directory, repository) = repository();
    let mailbox_id = mailbox(&repository, "55555555-5555-4555-8555-555555555555");
    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![snapshot(
                &mailbox_id,
                inbound_message("before-readback", 100),
            )],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("before-readback".into()),
                newest_occurred_at: Some(100),
                oldest_backfill_cursor: None,
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .unwrap();

    arm_mailbox_readback_traces(&repository);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    *MAILBOX_READBACK_TRACE_PAUSE.lock().unwrap() = Some((entered_tx, release_rx));
    let read_repository = repository.clone();
    let read_mailbox_id = mailbox_id.clone();
    let readback =
        std::thread::spawn(move || read_repository.mailbox_sync_readback(&read_mailbox_id, 10));
    entered_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("composite readback did not establish its snapshot");

    repository
        .commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: mailbox_id.clone(),
            direction: EmailDirection::Inbound,
            snapshots: vec![snapshot(
                &mailbox_id,
                inbound_message("committed-during-readback", 200),
            )],
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: Some("committed-during-readback".into()),
                newest_occurred_at: Some(200),
                oldest_backfill_cursor: None,
                backfill_complete: true,
                mark_full_sync: true,
            }),
            failure: None,
            incremental_continuation: Default::default(),
            sent_mutations: Default::default(),
        })
        .expect("WAL writer should commit beside the read snapshot");
    release_tx.send(()).unwrap();

    let (inbound, outbound, pending_triage) = readback.join().unwrap().unwrap();
    assert_eq!(
        inbound.unwrap().newest_provider_message_id.as_deref(),
        Some("before-readback")
    );
    assert!(outbound.is_none());
    assert_eq!(pending_triage.total, 1);

    let (inbound, outbound, pending_triage) =
        repository.mailbox_sync_readback(&mailbox_id, 10).unwrap();
    assert_eq!(
        inbound.unwrap().newest_provider_message_id.as_deref(),
        Some("committed-during-readback")
    );
    assert!(outbound.is_none());
    assert_eq!(pending_triage.total, 2);
    clear_mailbox_readback_traces(&repository);
}
