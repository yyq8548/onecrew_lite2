//! Durable bidirectional sync state: the page-commit contract, the
//! mailbox-direction watermarks, and the provider-identity dedupe queries.
//! One SQLite transaction covers a page's snapshots and its eligible
//! progress; provider network work never happens inside it.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::{
    EmailDirection, EmailSyncAdvance, EmailSyncCheckpoint, EmailSyncFailure,
    EmailSyncIncrementalContinuation, EmailSyncIncrementalContinuationMutation,
    EmailSyncPageCommit, EmailSyncPageOutcome, EmailSyncState, MailboxRefId,
};

use super::super::{SqliteRepository, now};
use super::snapshots::persist_snapshot;

fn load_email_sync_state(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
    direction: EmailDirection,
) -> Result<Option<EmailSyncState>, String> {
    connection
        .query_row(
            "SELECT newest_provider_message_id, newest_occurred_at, oldest_backfill_cursor,
                    backfill_complete, last_page_at, last_full_sync_at, failure_json,
                    updated_at
             FROM mailbox_sync_state WHERE mailbox_ref_id = ?1 AND direction = ?2",
            params![mailbox_ref_id.as_str(), direction.as_str()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<u64>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, Option<u64>>(4)?,
                    row.get::<_, Option<u64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, u64>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .map(
            |(
                newest_provider_message_id,
                newest_occurred_at,
                oldest_backfill_cursor,
                backfill_complete,
                last_page_at,
                last_full_sync_at,
                failure_json,
                updated_at,
            )| {
                let failure = failure_json
                    .as_deref()
                    .map(serde_json::from_str::<EmailSyncFailure>)
                    .transpose()
                    .map_err(|error| error.to_string())?;
                Ok(EmailSyncState {
                    mailbox_ref_id: mailbox_ref_id.clone(),
                    direction,
                    newest_provider_message_id,
                    newest_occurred_at,
                    oldest_backfill_cursor,
                    backfill_complete,
                    last_page_at,
                    last_full_sync_at,
                    failure,
                    updated_at,
                })
            },
        )
        .transpose()
}

fn load_email_sync_incremental_continuation(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
    direction: EmailDirection,
) -> Result<Option<EmailSyncIncrementalContinuation>, String> {
    connection
        .query_row(
            "SELECT next_cursor, newest_provider_message_id, newest_occurred_at, updated_at
             FROM mailbox_sync_incremental_continuations
             WHERE mailbox_ref_id = ?1 AND direction = ?2",
            params![mailbox_ref_id.as_str(), direction.as_str()],
            |row| {
                Ok(EmailSyncIncrementalContinuation {
                    mailbox_ref_id: mailbox_ref_id.clone(),
                    direction,
                    next_cursor: row.get(0)?,
                    newest_provider_message_id: row.get(1)?,
                    newest_occurred_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn load_email_sync_checkpoint(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
    direction: EmailDirection,
) -> Result<EmailSyncCheckpoint, String> {
    let (
        state_mailbox_ref_id,
        newest_provider_message_id,
        newest_occurred_at,
        oldest_backfill_cursor,
        backfill_complete,
        last_page_at,
        last_full_sync_at,
        failure_json,
        state_updated_at,
        continuation_mailbox_ref_id,
        next_cursor,
        continuation_newest_provider_message_id,
        continuation_newest_occurred_at,
        continuation_updated_at,
    ) = connection
        .query_row(
            "SELECT state.mailbox_ref_id,
                    state.newest_provider_message_id, state.newest_occurred_at,
                    state.oldest_backfill_cursor, state.backfill_complete,
                    state.last_page_at, state.last_full_sync_at, state.failure_json,
                    state.updated_at,
                    continuation.mailbox_ref_id, continuation.next_cursor,
                    continuation.newest_provider_message_id,
                    continuation.newest_occurred_at, continuation.updated_at
             FROM (SELECT 1) singleton
             LEFT JOIN mailbox_sync_state state
               ON state.mailbox_ref_id = ?1 AND state.direction = ?2
             LEFT JOIN mailbox_sync_incremental_continuations continuation
               ON continuation.mailbox_ref_id = ?1 AND continuation.direction = ?2",
            params![mailbox_ref_id.as_str(), direction.as_str()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<u64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<bool>>(4)?,
                    row.get::<_, Option<u64>>(5)?,
                    row.get::<_, Option<u64>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<u64>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<u64>>(12)?,
                    row.get::<_, Option<u64>>(13)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;

    let state = state_mailbox_ref_id
        .map(|_| -> Result<EmailSyncState, String> {
            let failure = failure_json
                .as_deref()
                .map(serde_json::from_str::<EmailSyncFailure>)
                .transpose()
                .map_err(|error| error.to_string())?;
            Ok(EmailSyncState {
                mailbox_ref_id: mailbox_ref_id.clone(),
                direction,
                newest_provider_message_id,
                newest_occurred_at,
                oldest_backfill_cursor,
                backfill_complete: backfill_complete
                    .ok_or_else(|| "mailbox sync state has no completion flag".to_string())?,
                last_page_at,
                last_full_sync_at,
                failure,
                updated_at: state_updated_at
                    .ok_or_else(|| "mailbox sync state has no update timestamp".to_string())?,
            })
        })
        .transpose()?;
    let incremental_continuation = continuation_mailbox_ref_id
        .map(|_| -> Result<EmailSyncIncrementalContinuation, String> {
            Ok(EmailSyncIncrementalContinuation {
                mailbox_ref_id: mailbox_ref_id.clone(),
                direction,
                next_cursor: next_cursor
                    .ok_or_else(|| "Phase A continuation has no next cursor".to_string())?,
                newest_provider_message_id: continuation_newest_provider_message_id
                    .ok_or_else(|| "Phase A continuation has no newest anchor".to_string())?,
                newest_occurred_at: continuation_newest_occurred_at
                    .ok_or_else(|| "Phase A continuation has no anchor timestamp".to_string())?,
                updated_at: continuation_updated_at
                    .ok_or_else(|| "Phase A continuation has no update timestamp".to_string())?,
            })
        })
        .transpose()?;
    Ok(EmailSyncCheckpoint {
        state,
        incremental_continuation,
    })
}

fn reconcile_sent_statuses_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    mailbox_ref_id: &MailboxRefId,
    updates: &[(String, String)],
) -> Result<(Vec<crate::EmailThreadId>, u32), String> {
    let mut changed = Vec::new();
    let mut updated = 0_u32;
    let mut statement = transaction
        .prepare_cached(
            "UPDATE emails SET delivery_status = ?3
             WHERE id = (SELECT email_id FROM email_provider_identities
                         WHERE mailbox_ref_id = ?1 AND provider_message_id = ?2)
               AND direction = 'outbound' AND delivery_status <> ?3
             RETURNING thread_id",
        )
        .map_err(|error| error.to_string())?;
    for (provider_message_id, last_event) in updates {
        let thread_id = statement
            .query_row(
                params![mailbox_ref_id.as_str(), provider_message_id, last_event],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(thread_id) = thread_id {
            updated += 1;
            let thread_id = crate::EmailThreadId::new(thread_id);
            if !changed.contains(&thread_id) {
                changed.push(thread_id);
            }
        }
    }
    Ok((changed, updated))
}

struct MergeOutcome {
    merged_sources: u32,
    target_changed: bool,
}

fn validate_incremental_continuation_mutation(
    mutation: &EmailSyncIncrementalContinuationMutation,
) -> Result<(), String> {
    let EmailSyncIncrementalContinuationMutation::Set {
        next_cursor,
        newest_provider_message_id,
        ..
    } = mutation
    else {
        return Ok(());
    };
    if next_cursor.trim().is_empty() {
        return Err("Phase A continuation next cursor must not be blank".into());
    }
    if newest_provider_message_id.trim().is_empty() {
        return Err("Phase A continuation newest anchor must not be blank".into());
    }
    Ok(())
}

fn merge_email_threads_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    target: &crate::EmailThreadId,
    sources: &[crate::EmailThreadId],
    expected_mailbox: Option<&MailboxRefId>,
) -> Result<MergeOutcome, String> {
    if let Some(mailbox) = expected_mailbox {
        for thread_id in std::iter::once(target).chain(sources) {
            let valid = transaction
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM email_threads
                     WHERE id = ?1 AND mailbox_ref_id = ?2)",
                    params![thread_id.as_str(), mailbox.as_str()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| error.to_string())?;
            if !valid {
                return Err("sent page thread merge is outside the committing mailbox".into());
            }
        }
    }
    let mut merged_sources = 0_u32;
    let mut target_changed = false;
    for source in sources {
        if source == target {
            continue;
        }
        let moved = transaction
            .execute(
                "UPDATE emails SET thread_id = ?1
                 WHERE thread_id = ?2
                   AND provider_message_id NOT IN
                       (SELECT provider_message_id FROM emails WHERE thread_id = ?1)",
                params![target.as_str(), source.as_str()],
            )
            .map_err(|error| error.to_string())?;
        let deleted = transaction
            .execute(
                "DELETE FROM email_threads
                 WHERE id = ?1
                   AND NOT EXISTS (SELECT 1 FROM emails WHERE thread_id = ?1)",
                [source.as_str()],
            )
            .map_err(|error| error.to_string())?;
        target_changed |= moved > 0 || deleted > 0;
        if deleted > 0 {
            merged_sources += 1;
        }
    }
    if target_changed {
        transaction
            .execute(
                "UPDATE email_threads SET unread_count =
                   (SELECT COUNT(*) FROM emails
                    WHERE thread_id = ?1 AND direction = 'inbound'
                      AND delivery_status = 'unread'),
                 updated_at = MAX(updated_at,
                   COALESCE((SELECT MAX(occurred_at) FROM emails WHERE thread_id = ?1), 0))
                 WHERE id = ?1",
                [target.as_str()],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(MergeOutcome {
        merged_sources,
        target_changed,
    })
}

impl SqliteRepository {
    /// Commits one provider page atomically: every snapshot (messages,
    /// recipients, attachments, identity rows) plus, only when `advance` is
    /// present, the direction's durable watermark/backfill movement. A page
    /// with failed required items commits its successful subset once and
    /// records sanitized failure state, leaving the boundary untouched so
    /// the next run retries only the missing work.
    pub(crate) fn commit_email_sync_page(
        &self,
        input: EmailSyncPageCommit,
    ) -> Result<EmailSyncPageOutcome, String> {
        validate_incremental_continuation_mutation(&input.incremental_continuation)?;
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;

        let mut changed_thread_ids = Vec::new();
        if input.direction != EmailDirection::Outbound
            && (!input.sent_mutations.status_updates.is_empty()
                || !input.sent_mutations.thread_merges.is_empty())
        {
            return Err("sent page mutations require the outbound direction".into());
        }
        let mut merged_threads = 0_u32;
        for merge in &input.sent_mutations.thread_merges {
            let merged = merge_email_threads_in_transaction(
                &transaction,
                &merge.target,
                &merge.sources,
                Some(&input.mailbox_ref_id),
            )?;
            merged_threads += merged.merged_sources;
            if merged.target_changed && !changed_thread_ids.contains(&merge.target) {
                changed_thread_ids.push(merge.target.clone());
            }
        }
        let (status_threads, reconciled_statuses) = reconcile_sent_statuses_in_transaction(
            &transaction,
            &input.mailbox_ref_id,
            &input.sent_mutations.status_updates,
        )?;
        for thread_id in status_threads {
            if !changed_thread_ids.contains(&thread_id) {
                changed_thread_ids.push(thread_id);
            }
        }
        for snapshot in &input.snapshots {
            if snapshot.mailbox_ref_id != input.mailbox_ref_id {
                return Err("sync page snapshots must belong to the committing mailbox".into());
            }
            let (thread, _) = persist_snapshot(&transaction, snapshot)?;
            if !changed_thread_ids.contains(&thread.id) {
                changed_thread_ids.push(thread.id);
            }
        }

        let timestamp = now();
        let current_continuation = load_email_sync_incremental_continuation(
            &transaction,
            &input.mailbox_ref_id,
            input.direction,
        )?;
        let continuation_changed = match &input.incremental_continuation {
            EmailSyncIncrementalContinuationMutation::Preserve => false,
            EmailSyncIncrementalContinuationMutation::Set {
                next_cursor,
                newest_provider_message_id,
                newest_occurred_at,
            } => current_continuation.as_ref().is_none_or(|current| {
                current.next_cursor != *next_cursor
                    || current.newest_provider_message_id != *newest_provider_message_id
                    || current.newest_occurred_at != *newest_occurred_at
            }),
            EmailSyncIncrementalContinuationMutation::Clear => current_continuation.is_some(),
        };
        let failure_json = input
            .failure
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| error.to_string())?;
        let current = transaction
            .query_row(
                "SELECT newest_provider_message_id, newest_occurred_at,
                        oldest_backfill_cursor, backfill_complete,
                        last_full_sync_at, failure_json
                 FROM mailbox_sync_state
                 WHERE mailbox_ref_id = ?1 AND direction = ?2",
                params![input.mailbox_ref_id.as_str(), input.direction.as_str()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<u64>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, bool>(3)?,
                        row.get::<_, Option<u64>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let current_failure = current
            .as_ref()
            .and_then(|(_, _, _, _, _, failure_json)| failure_json.as_deref())
            .map(serde_json::from_str::<EmailSyncFailure>)
            .transpose()
            .map_err(|error| error.to_string())?;
        let facts_changed = !changed_thread_ids.is_empty();
        let checkpoint_changed = match (&input.advance, current.as_ref()) {
            (Some(_), None) | (None, None) => true,
            (
                Some(advance),
                Some((
                    newest_provider_message_id,
                    newest_occurred_at,
                    oldest_backfill_cursor,
                    backfill_complete,
                    last_full_sync_at,
                    _,
                )),
            ) => {
                facts_changed
                    || current_failure.as_ref() != input.failure.as_ref()
                    || newest_provider_message_id != &advance.newest_provider_message_id
                    || newest_occurred_at != &advance.newest_occurred_at
                    || oldest_backfill_cursor != &advance.oldest_backfill_cursor
                    || backfill_complete != &advance.backfill_complete
                    || (advance.mark_full_sync && last_full_sync_at.is_none())
            }
            (None, Some(_)) => facts_changed || current_failure.as_ref() != input.failure.as_ref(),
        };
        if checkpoint_changed {
            match &input.advance {
                Some(EmailSyncAdvance {
                    newest_provider_message_id,
                    newest_occurred_at,
                    oldest_backfill_cursor,
                    backfill_complete,
                    mark_full_sync,
                }) => {
                    transaction
                        .execute(
                            "INSERT INTO mailbox_sync_state
                           (mailbox_ref_id, direction, newest_provider_message_id,
                            newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                            last_page_at, last_full_sync_at, failure_json, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?7)
                         ON CONFLICT(mailbox_ref_id, direction) DO UPDATE SET
                           newest_provider_message_id = excluded.newest_provider_message_id,
                           newest_occurred_at = excluded.newest_occurred_at,
                           oldest_backfill_cursor = excluded.oldest_backfill_cursor,
                           backfill_complete = excluded.backfill_complete,
                           last_page_at = excluded.last_page_at,
                           last_full_sync_at = CASE WHEN ?10 THEN excluded.last_page_at
                                                    ELSE mailbox_sync_state.last_full_sync_at END,
                           failure_json = excluded.failure_json,
                           updated_at = excluded.updated_at",
                            params![
                                input.mailbox_ref_id.as_str(),
                                input.direction.as_str(),
                                newest_provider_message_id,
                                newest_occurred_at,
                                oldest_backfill_cursor,
                                backfill_complete,
                                timestamp,
                                mark_full_sync.then_some(timestamp),
                                failure_json,
                                mark_full_sync
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                }
                None => {
                    // No boundary movement: create the row if absent so
                    // failure state is durable, but never touch existing
                    // watermarks.
                    transaction
                        .execute(
                            "INSERT INTO mailbox_sync_state
                               (mailbox_ref_id, direction, newest_provider_message_id,
                                newest_occurred_at, oldest_backfill_cursor, backfill_complete,
                                last_page_at, last_full_sync_at, failure_json, updated_at)
                             VALUES (?1, ?2, NULL, NULL, NULL, 0, NULL, NULL, ?3, ?4)
                             ON CONFLICT(mailbox_ref_id, direction) DO UPDATE SET
                               failure_json = excluded.failure_json,
                               updated_at = excluded.updated_at",
                            params![
                                input.mailbox_ref_id.as_str(),
                                input.direction.as_str(),
                                failure_json,
                                timestamp
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }

        if continuation_changed {
            match &input.incremental_continuation {
                EmailSyncIncrementalContinuationMutation::Preserve => {}
                EmailSyncIncrementalContinuationMutation::Set {
                    next_cursor,
                    newest_provider_message_id,
                    newest_occurred_at,
                } => {
                    transaction
                        .execute(
                            "INSERT INTO mailbox_sync_incremental_continuations
                               (mailbox_ref_id, direction, next_cursor,
                                newest_provider_message_id, newest_occurred_at, updated_at)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                             ON CONFLICT(mailbox_ref_id, direction) DO UPDATE SET
                               next_cursor = excluded.next_cursor,
                               newest_provider_message_id = excluded.newest_provider_message_id,
                               newest_occurred_at = excluded.newest_occurred_at,
                               updated_at = excluded.updated_at",
                            params![
                                input.mailbox_ref_id.as_str(),
                                input.direction.as_str(),
                                next_cursor,
                                newest_provider_message_id,
                                newest_occurred_at,
                                timestamp
                            ],
                        )
                        .map_err(|error| error.to_string())?;
                }
                EmailSyncIncrementalContinuationMutation::Clear => {
                    transaction
                        .execute(
                            "DELETE FROM mailbox_sync_incremental_continuations
                             WHERE mailbox_ref_id = ?1 AND direction = ?2",
                            params![input.mailbox_ref_id.as_str(), input.direction.as_str()],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }

        transaction.commit().map_err(|error| error.to_string())?;
        Ok(EmailSyncPageOutcome {
            changed_thread_ids,
            checkpoint_changed,
            advanced: checkpoint_changed && input.advance.is_some(),
            continuation_changed,
            reconciled_statuses,
            merged_threads,
        })
    }

    pub(crate) fn email_sync_checkpoint(
        &self,
        mailbox_ref_id: &MailboxRefId,
        direction: EmailDirection,
    ) -> Result<EmailSyncCheckpoint, String> {
        let connection = self.read_connection()?;
        load_email_sync_checkpoint(&connection, mailbox_ref_id, direction)
    }

    pub(crate) fn mailbox_sync_readback(
        &self,
        mailbox_ref_id: &MailboxRefId,
        triage_limit: u32,
    ) -> Result<
        (
            Option<EmailSyncState>,
            Option<EmailSyncState>,
            crate::PendingReplyTriageDebt,
        ),
        String,
    > {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let inbound = load_email_sync_state(&transaction, mailbox_ref_id, EmailDirection::Inbound)?;
        let outbound =
            load_email_sync_state(&transaction, mailbox_ref_id, EmailDirection::Outbound)?;
        let pending_triage = super::super::reply_triage::pending_email_triage_from_connection(
            &transaction,
            mailbox_ref_id,
            triage_limit,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok((inbound, outbound, pending_triage))
    }

    /// The subset of `provider_message_ids` whose full detail is already
    /// local for this mailbox — the dedupe read that keeps a re-sync from
    /// fetching immutable complete messages again.
    pub(crate) fn complete_email_provider_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<String>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare_cached(
                "SELECT 1 FROM email_provider_identities
                 WHERE mailbox_ref_id = ?1 AND provider_message_id = ?2
                   AND detail_complete = 1",
            )
            .map_err(|error| error.to_string())?;
        let mut complete = Vec::new();
        for provider_message_id in provider_message_ids {
            let known = statement
                .query_row(
                    params![mailbox_ref_id.as_str(), provider_message_id],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if known.is_some() {
                complete.push(provider_message_id.clone());
            }
        }
        drop(statement);
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(complete)
    }
}

impl SqliteRepository {
    /// Applies sent list-metadata delivery-status updates without body
    /// re-fetch. Only outbound rows change, only when the status actually
    /// differs; returns the affected thread ids for page-level publication.
    pub(crate) fn reconcile_sent_statuses(
        &self,
        mailbox_ref_id: &MailboxRefId,
        updates: &[(String, String)],
    ) -> Result<Vec<crate::EmailThreadId>, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let (changed, _) =
            reconcile_sent_statuses_in_transaction(&transaction, mailbox_ref_id, updates)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(changed)
    }

    /// Threads in this mailbox containing a message whose RFC evidence
    /// (`In-Reply-To` or `References`) names the given Message-ID, oldest
    /// first. This is how a newly imported outbound root finds replies that
    /// arrived before it.
    pub(crate) fn find_email_threads_referencing(
        &self,
        mailbox_ref_id: &MailboxRefId,
        internet_message_id: &str,
    ) -> Result<Vec<crate::EmailThreadId>, String> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare_cached(
                "SELECT DISTINCT thread.id, thread.created_at FROM emails email
                 JOIN email_threads thread ON thread.id = email.thread_id
                 WHERE thread.mailbox_ref_id = ?1
                   AND (email.in_reply_to = ?2
                        OR EXISTS (SELECT 1 FROM json_each(email.references_json)
                                   WHERE json_each.value = ?2))
                 ORDER BY thread.created_at ASC, thread.id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                rusqlite::params![mailbox_ref_id.as_str(), internet_message_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        Ok(rows.into_iter().map(crate::EmailThreadId::new).collect())
    }

    /// Merges RFC-proven sibling threads into one. Messages move with their
    /// stable ids, so read state, triage verdicts, touchpoints, drafts, and
    /// audit references survive untouched; the target thread's disposition
    /// wins, and a drained source thread is removed. A same-provider-message
    /// duplicate (a pre-v24 legacy artifact) stays where it is rather than
    /// colliding.
    pub(crate) fn merge_email_threads(
        &self,
        target: &crate::EmailThreadId,
        sources: &[crate::EmailThreadId],
    ) -> Result<(), String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        merge_email_threads_in_transaction(&transaction, target, sources, None)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl SqliteRepository {
    pub(crate) fn email_attachments(
        &self,
        email_id: &crate::EmailId,
    ) -> Result<Vec<crate::EmailAttachmentView>, String> {
        let connection = self.read_connection()?;
        let mut statement = connection
            .prepare_cached(
                "SELECT provider_attachment_id, filename, content_type, content_id,
                        content_disposition, size_bytes, direction, available
                 FROM email_attachments WHERE email_id = ?1
                 ORDER BY filename ASC, provider_attachment_id ASC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([email_id.as_str()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<u64>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, bool>(7)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        rows.into_iter()
            .map(
                |(
                    provider_attachment_id,
                    filename,
                    content_type,
                    content_id,
                    content_disposition,
                    size_bytes,
                    direction,
                    available,
                )| {
                    Ok(crate::EmailAttachmentView {
                        provider_attachment_id,
                        filename,
                        content_type,
                        content_id,
                        content_disposition,
                        size_bytes,
                        direction: crate::EmailDirection::parse(&direction)?,
                        available,
                    })
                },
            )
            .collect()
    }

    pub(crate) fn email_provider_coordinates(
        &self,
        email_id: &crate::EmailId,
    ) -> Result<(MailboxRefId, String, EmailDirection), String> {
        let connection = self.read_connection()?;
        let row = connection
            .query_row(
                "SELECT thread.mailbox_ref_id, email.provider_message_id, email.direction
                 FROM emails email
                 JOIN email_threads thread ON thread.id = email.thread_id
                 WHERE email.id = ?1",
                [email_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("email {} does not exist", email_id.as_str()))?;
        Ok((
            MailboxRefId::new(row.0),
            row.1,
            EmailDirection::parse(&row.2)?,
        ))
    }

    pub(crate) fn record_email_attachments(
        &self,
        email_id: &crate::EmailId,
        direction: EmailDirection,
        metadata: &[onecrew_email_contracts::PlatformEmailAttachment],
    ) -> Result<(), String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        for attachment in metadata {
            transaction
                .execute(
                    "INSERT INTO email_attachments
                       (id, email_id, provider_attachment_id, filename, content_type,
                        content_id, content_disposition, size_bytes, direction, available,
                        created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?10)
                     ON CONFLICT(email_id, provider_attachment_id) DO UPDATE SET
                       filename = excluded.filename,
                       content_type = excluded.content_type,
                       content_id = excluded.content_id,
                       content_disposition = excluded.content_disposition,
                       size_bytes = excluded.size_bytes,
                       available = 1,
                       updated_at = excluded.updated_at",
                    params![
                        uuid::Uuid::new_v4().to_string(),
                        email_id.as_str(),
                        attachment.provider_attachment_id,
                        attachment.filename,
                        attachment.content_type,
                        attachment.content_id,
                        attachment.content_disposition,
                        attachment.size_bytes,
                        direction.as_str(),
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(crate) fn mark_email_attachment_unavailable(
        &self,
        email_id: &crate::EmailId,
        provider_attachment_id: &str,
    ) -> Result<(), String> {
        let connection = self.lock()?;
        connection
            .execute(
                "UPDATE email_attachments SET available = 0, updated_at = ?3
                 WHERE email_id = ?1 AND provider_attachment_id = ?2",
                params![email_id.as_str(), provider_attachment_id, now()],
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}
