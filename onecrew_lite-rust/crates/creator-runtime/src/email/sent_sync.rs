//! The sent-history import and reconciliation engine.
//!
//! Mirrors the received engine over `GET /emails`: newest-first pages
//! filtered to the mailbox's sender identity, watermark-bounded incremental
//! runs, and backfill to the provider retention floor — with two sent-only
//! behaviors. Delivery status reconciles from list metadata without body
//! re-fetch, and threading connects only through RFC Message-ID evidence:
//! the provider exposes no sent reply parentage, so an imported root joins
//! replies that already reference it (merging RFC-proven sibling threads),
//! and otherwise stands alone. Subject or recipient guessing never happens.

use std::collections::{HashMap, HashSet};

use crate::{
    CreatorChangeScope, EmailCapability, EmailDirection, EmailGateway, EmailSnapshotInput,
    EmailSyncAdvance, EmailSyncFailure, EmailSyncIncrementalContinuationMutation,
    EmailSyncPageCommit, EmailSyncSentMutations, EmailSyncThreadMerge, MailboxDetail,
    SentSyncReport,
};
use onecrew_email_contracts::PlatformSentEmailSummary;

use super::message::parse_provider_time;
use super::sync::{
    DurableSyncCheckpoint, PageCheckpointPlan, PageThreadLedger, RECEIVED_PAGE_LIMIT,
    backfill_progress_percent, pagination_cursor_failure,
};
use super::{ensure_mailbox, isolated_inbound_thread_key, provider_message_input};

const MAX_RUN_PAGES: u32 = 200;

struct SentPageImport {
    changed_thread_ids: Vec<crate::EmailThreadId>,
    imported: u32,
    skipped_complete: u32,
    reconciled_statuses: u32,
    details_fetched: u32,
    merged_threads: u32,
    missing: Vec<(String, String)>,
    committed_advance: Option<EmailSyncAdvance>,
    checkpoint_committed: bool,
    effective_failure: Option<EmailSyncFailure>,
    effective_continuation: EmailSyncIncrementalContinuationMutation,
    terminal_failure: Option<EmailSyncFailure>,
    full_sync_recorded: bool,
}

struct PreparedSentSnapshot {
    snapshot: EmailSnapshotInput,
    adopted_thread_id: Option<crate::EmailThreadId>,
}

type SentThreadOwner = Option<(crate::CreatorId, crate::CreatorContactRouteId)>;

#[derive(Default)]
struct SentPagePlanner {
    aliases: HashMap<crate::EmailThreadId, crate::EmailThreadId>,
    owners: HashMap<crate::EmailThreadId, SentThreadOwner>,
    thread_merges: Vec<EmailSyncThreadMerge>,
}

impl SentPagePlanner {
    fn canonical(&self, thread_id: &crate::EmailThreadId) -> Result<crate::EmailThreadId, String> {
        let mut current = thread_id.clone();
        let mut visited = HashSet::new();
        while let Some(next) = self.aliases.get(&current) {
            if !visited.insert(current.clone()) {
                return Err("sent page thread merge aliases contain a cycle".into());
            }
            if *next == current {
                return Err("sent page thread merge aliases contain a self-cycle".into());
            }
            current = next.clone();
        }
        Ok(current)
    }

    fn effective_owner(
        &self,
        thread_id: &crate::EmailThreadId,
        persisted: &SentThreadOwner,
    ) -> SentThreadOwner {
        self.owners
            .get(thread_id)
            .cloned()
            .unwrap_or_else(|| persisted.clone())
    }

    fn record_owner(&mut self, thread_id: crate::EmailThreadId, owner: SentThreadOwner) {
        self.owners.insert(thread_id, owner);
    }

    fn record_merge(
        &mut self,
        target: crate::EmailThreadId,
        sources: Vec<crate::EmailThreadId>,
    ) -> Result<(), String> {
        let target = self.canonical(&target)?;
        let mut unique_sources = Vec::new();
        let mut seen = HashSet::new();
        for source in sources {
            let source = self.canonical(&source)?;
            if source != target && seen.insert(source.clone()) {
                unique_sources.push(source);
            }
        }
        for source in &unique_sources {
            self.aliases.insert(source.clone(), target.clone());
        }
        if !unique_sources.is_empty() {
            self.thread_merges.push(EmailSyncThreadMerge {
                target,
                sources: unique_sources,
            });
        }
        Ok(())
    }
}

impl crate::CreatorRuntime {
    /// The canonical sent-history engine. Peer adapters project it in
    /// Phase 7 together with the received engine.
    pub async fn sync_mailbox_sent(
        &self,
        mailbox_ref_id: &crate::MailboxRefId,
    ) -> Result<SentSyncReport, String> {
        let gateway = self
            .gateways
            .email
            .as_ref()
            .ok_or_else(|| "email gateway is not configured".to_string())?
            .clone();
        let context_runtime = self.clone();
        let context_mailbox_ref_id = mailbox_ref_id.clone();
        let (mailbox, state, incremental_continuation) = self
            .repository_read_segment(move || {
                let mailbox = context_runtime.require_mailbox_ref(&context_mailbox_ref_id)?;
                ensure_mailbox(&mailbox, EmailCapability::SentReadback)?;
                let checkpoint = context_runtime
                    .repository
                    .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)?;
                Ok((
                    mailbox,
                    checkpoint.state,
                    checkpoint.incremental_continuation,
                ))
            })
            .await
            .map_err(|error| sent_read_segment_error("preflight", error))?;
        let resumed_initial = incremental_continuation
            .as_ref()
            .is_some_and(|continuation| {
                state.as_ref().is_some_and(|state| {
                    !state.backfill_complete
                        && state.newest_provider_message_id.as_ref()
                            == Some(&continuation.newest_provider_message_id)
                        && state.newest_occurred_at == Some(continuation.newest_occurred_at)
                        && state.oldest_backfill_cursor.as_deref()
                            == Some(continuation.next_cursor.as_str())
                })
            });
        let watermark_id = (!resumed_initial)
            .then(|| {
                state
                    .as_ref()
                    .and_then(|state| state.newest_provider_message_id.clone())
            })
            .flatten();
        let watermark_occurred_at = (!resumed_initial)
            .then(|| state.as_ref().and_then(|state| state.newest_occurred_at))
            .flatten();
        let started_initial = state
            .as_ref()
            .and_then(|state| state.newest_provider_message_id.as_ref())
            .is_none()
            || resumed_initial;
        let mut newest_candidate = incremental_continuation.as_ref().map(|continuation| {
            (
                continuation.newest_provider_message_id.clone(),
                continuation.newest_occurred_at,
            )
        });
        let mut cursor = incremental_continuation
            .as_ref()
            .map(|continuation| continuation.next_cursor.clone());
        let mut durable =
            DurableSyncCheckpoint::from_state(state.as_ref(), incremental_continuation);

        let mut report = SentSyncReport::default();
        let mut failure: Option<EmailSyncFailure> = None;

        // Phase A: newest-first reconciliation over the bounded overlap.
        loop {
            let page = gateway
                .sent_emails(
                    &mailbox.mailbox_ref_id,
                    cursor.as_deref(),
                    RECEIVED_PAGE_LIMIT,
                )
                .await?;
            report.pages_listed += 1;
            report.summaries_listed += page.messages.len() as u32;

            let crossed = crossed_watermark(
                &page.messages,
                page.bounds.as_ref(),
                watermark_id.as_deref(),
                watermark_occurred_at,
            )?;
            // Raw team-level anchor: a mailbox whose sender identity matched
            // nothing still stops at page one on the next run.
            if newest_candidate.is_none()
                && let Some(bounds) = page.bounds.as_ref()
                && !bounds.newest_provider_message_id.trim().is_empty()
            {
                newest_candidate = Some((
                    bounds.newest_provider_message_id.clone(),
                    parse_provider_time(&bounds.newest_created_at)?,
                ));
            }

            let phase_a_finished = crossed || !page.has_more;
            let pagination_failure = if page.has_more
                && page
                    .next_cursor
                    .as_ref()
                    .is_none_or(|cursor| cursor.trim().is_empty())
            {
                Some(pagination_cursor_failure())
            } else if !phase_a_finished && newest_candidate.is_none() {
                Some(super::sync::pagination_anchor_failure())
            } else {
                None
            };
            let pagination_failed = pagination_failure.is_some();
            let candidate_advance = if pagination_failed {
                None
            } else if started_initial {
                Some(EmailSyncAdvance {
                    newest_provider_message_id: newest_candidate.as_ref().map(|(id, _)| id.clone()),
                    newest_occurred_at: newest_candidate.as_ref().map(|(_, at)| *at),
                    oldest_backfill_cursor: page.next_cursor.clone(),
                    backfill_complete: !page.has_more,
                    mark_full_sync: !page.has_more,
                })
            } else if !page.has_more && (!crossed || !durable.backfill_complete) {
                Some(EmailSyncAdvance {
                    newest_provider_message_id: newest_candidate
                        .as_ref()
                        .map(|(id, _)| id.clone())
                        .or_else(|| durable.newest_provider_message_id.clone()),
                    newest_occurred_at: newest_candidate
                        .as_ref()
                        .map(|(_, at)| *at)
                        .or(durable.newest_occurred_at),
                    oldest_backfill_cursor: page.next_cursor.clone(),
                    backfill_complete: true,
                    mark_full_sync: true,
                })
            } else if crossed {
                Some(EmailSyncAdvance {
                    newest_provider_message_id: newest_candidate
                        .as_ref()
                        .map(|(id, _)| id.clone())
                        .or_else(|| durable.newest_provider_message_id.clone()),
                    newest_occurred_at: newest_candidate
                        .as_ref()
                        .map(|(_, at)| *at)
                        .or(durable.newest_occurred_at),
                    oldest_backfill_cursor: durable.oldest_backfill_cursor.clone(),
                    backfill_complete: durable.backfill_complete,
                    mark_full_sync: durable.backfill_complete,
                })
            } else {
                None
            };
            let candidate_continuation = if pagination_failed {
                EmailSyncIncrementalContinuationMutation::Preserve
            } else if phase_a_finished {
                EmailSyncIncrementalContinuationMutation::Clear
            } else {
                let (newest_provider_message_id, newest_occurred_at) = newest_candidate
                    .as_ref()
                    .expect("nonterminal Phase A page checked its durable anchor");
                EmailSyncIncrementalContinuationMutation::Set {
                    next_cursor: page
                        .next_cursor
                        .clone()
                        .expect("nonterminal Phase A page checked its next cursor"),
                    newest_provider_message_id: newest_provider_message_id.clone(),
                    newest_occurred_at: *newest_occurred_at,
                }
            };
            let pending_phase_b =
                crossed && page.has_more && !started_initial && !durable.backfill_complete;
            let budget_exhausted = !pagination_failed
                && report.pages_listed >= MAX_RUN_PAGES
                && ((!phase_a_finished && page.has_more) || pending_phase_b);
            let completes_full_sync = candidate_advance
                .as_ref()
                .is_some_and(|advance| advance.mark_full_sync);
            let success_failure = if let Some(pagination_failure) = pagination_failure {
                Some(pagination_failure)
            } else if budget_exhausted {
                Some(page_budget_failure())
            } else if completes_full_sync {
                None
            } else {
                durable.failure.clone()
            };

            let import = self
                .import_sent_page(
                    &mailbox,
                    gateway.as_ref(),
                    &page.messages,
                    PageCheckpointPlan {
                        candidate_advance,
                        success_failure,
                        candidate_continuation,
                        stop_after_success: pagination_failed || budget_exhausted,
                    },
                    durable.clone(),
                )
                .await?;
            accumulate(&mut report, &import);
            let percent = backfill_progress_percent(
                newest_candidate
                    .as_ref()
                    .map(|(_, at)| *at)
                    .or(watermark_occurred_at),
                page.bounds.as_ref(),
            );
            self.record_sync_progress(&mailbox.mailbox_ref_id, sent_progress(&report, percent));
            report.full_sync_recorded |= import.full_sync_recorded;
            if import.checkpoint_committed {
                if let Some(advance) = import.committed_advance.as_ref() {
                    durable.apply(advance, import.effective_failure.clone());
                } else {
                    durable.failure = import.effective_failure.clone();
                }
            }
            durable.apply_incremental_continuation(&import.effective_continuation);
            if let Some(page_failure) = import.terminal_failure.clone() {
                failure = Some(page_failure);
                break;
            }

            if phase_a_finished {
                break;
            }
            cursor = page.next_cursor.clone();
            debug_assert!(cursor.is_some());
        }

        // Phase B: resume an unfinished historical backfill from its durable
        // cursor. Skipped entirely once the retention floor was reached.
        if failure.is_none()
            && durable.incremental_continuation.is_none()
            && watermark_id.is_some()
            && !durable.backfill_complete
        {
            let mut resume_cursor = durable.oldest_backfill_cursor.clone();
            loop {
                let page = gateway
                    .sent_emails(
                        &mailbox.mailbox_ref_id,
                        resume_cursor.as_deref(),
                        RECEIVED_PAGE_LIMIT,
                    )
                    .await?;
                report.pages_listed += 1;
                report.summaries_listed += page.messages.len() as u32;
                let pagination_failed = page.has_more
                    && page
                        .next_cursor
                        .as_ref()
                        .is_none_or(|cursor| cursor.trim().is_empty());
                let reached_floor = !page.has_more;
                let candidate_advance = (!pagination_failed).then(|| EmailSyncAdvance {
                    newest_provider_message_id: durable.newest_provider_message_id.clone(),
                    newest_occurred_at: durable.newest_occurred_at,
                    oldest_backfill_cursor: page.next_cursor.clone(),
                    backfill_complete: reached_floor,
                    mark_full_sync: reached_floor,
                });
                let budget_exhausted =
                    !pagination_failed && report.pages_listed >= MAX_RUN_PAGES && !reached_floor;
                let import = self
                    .import_sent_page(
                        &mailbox,
                        gateway.as_ref(),
                        &page.messages,
                        PageCheckpointPlan {
                            candidate_advance,
                            success_failure: if pagination_failed {
                                Some(pagination_cursor_failure())
                            } else if budget_exhausted {
                                Some(page_budget_failure())
                            } else if reached_floor {
                                None
                            } else {
                                durable.failure.clone()
                            },
                            candidate_continuation:
                                EmailSyncIncrementalContinuationMutation::Preserve,
                            stop_after_success: pagination_failed || budget_exhausted,
                        },
                        durable.clone(),
                    )
                    .await?;
                accumulate(&mut report, &import);
                let percent = backfill_progress_percent(
                    newest_candidate
                        .as_ref()
                        .map(|(_, at)| *at)
                        .or(watermark_occurred_at),
                    page.bounds.as_ref(),
                );
                self.record_sync_progress(&mailbox.mailbox_ref_id, sent_progress(&report, percent));
                report.full_sync_recorded |= import.full_sync_recorded;
                if import.checkpoint_committed {
                    if let Some(advance) = import.committed_advance.as_ref() {
                        durable.apply(advance, import.effective_failure.clone());
                    } else {
                        durable.failure = import.effective_failure.clone();
                    }
                }
                durable.apply_incremental_continuation(&import.effective_continuation);
                if let Some(page_failure) = import.terminal_failure.clone() {
                    failure = Some(page_failure);
                    break;
                }
                if reached_floor {
                    break;
                }
                resume_cursor = page.next_cursor;
                debug_assert!(resume_cursor.is_some());
            }
        }

        report.backfill_complete = durable.backfill_complete;
        report.missing_provider_message_ids = failure
            .map(|failure| failure.missing_provider_message_ids)
            .unwrap_or_default();
        Ok(report)
    }

    /// Imports one sender-filtered sent page: reconcile delivery status for
    /// already-complete messages from list metadata, fetch only missing
    /// details, place each import by RFC evidence (joining and merging
    /// threads that already reference it), and commit once.
    async fn import_sent_page(
        &self,
        mailbox: &MailboxDetail,
        gateway: &dyn EmailGateway,
        summaries: &[PlatformSentEmailSummary],
        checkpoint: PageCheckpointPlan,
        durable: DurableSyncCheckpoint,
    ) -> Result<SentPageImport, String> {
        let mut import = SentPageImport {
            changed_thread_ids: Vec::new(),
            imported: 0,
            skipped_complete: 0,
            reconciled_statuses: 0,
            details_fetched: 0,
            merged_threads: 0,
            missing: Vec::new(),
            committed_advance: None,
            checkpoint_committed: false,
            effective_failure: None,
            effective_continuation: EmailSyncIncrementalContinuationMutation::Preserve,
            terminal_failure: None,
            full_sync_recorded: false,
        };
        let ids: Vec<String> = summaries
            .iter()
            .map(|summary| summary.provider_message_id.clone())
            .collect();
        let repository = std::sync::Arc::clone(&self.repository);
        let dedupe_mailbox_ref_id = mailbox.mailbox_ref_id.clone();
        let dedupe_ids = ids.clone();
        let complete = self
            .repository_read_segment(move || {
                repository.complete_email_provider_ids(&dedupe_mailbox_ref_id, &dedupe_ids)
            })
            .await
            .map_err(|error| sent_read_segment_error("page dedupe", error))?;
        import.skipped_complete = complete.len() as u32;

        // A provider id already written locally (confirmed send or earlier
        // import) reconciles in place: status only, never a second email,
        // thread, touchpoint, idempotency result, or audit event.
        let updates: Vec<(String, String)> = summaries
            .iter()
            .filter(|summary| complete.contains(&summary.provider_message_id))
            .map(|summary| {
                (
                    summary.provider_message_id.clone(),
                    summary.last_event.clone(),
                )
            })
            .collect();
        let to_fetch: Vec<String> = ids
            .into_iter()
            .filter(|id| !complete.contains(id))
            .collect();
        let mut details = Vec::new();
        if !to_fetch.is_empty() {
            let outcomes = match gateway
                .sent_email_details(&mailbox.mailbox_ref_id, &to_fetch)
                .await
            {
                Ok(outcomes) => outcomes,
                Err(provider_error) => {
                    // Preserve the established partial-effect contract: list
                    // metadata is independently sufficient to reconcile
                    // already-complete outbound messages even when the
                    // provider rejects the detail batch itself.
                    if !updates.is_empty() {
                        let update_runtime = self.clone();
                        let update_mailbox_ref_id = mailbox.mailbox_ref_id.clone();
                        self.repository_write_segment(move || {
                            let changed = update_runtime
                                .repository
                                .reconcile_sent_statuses(&update_mailbox_ref_id, &updates)?;
                            if !changed.is_empty() {
                                update_runtime.publish_changes([
                                    CreatorChangeScope::EmailFolders,
                                    CreatorChangeScope::EmailThreads,
                                ]);
                            }
                            Ok(())
                        })
                        .await
                        .map_err(|error| sent_write_segment_error("status-only recovery", error))?;
                    }
                    return Err(provider_error);
                }
            };
            import.details_fetched = outcomes.len() as u32;
            for (id, outcome) in outcomes {
                match outcome {
                    Ok(detail) if detail.provider_message_id == id => details.push(detail),
                    Ok(_) => import
                        .missing
                        .push((id, "detail did not match its list item".into())),
                    Err(error) => import.missing.push((id, error)),
                }
            }
        }
        details.sort_by(|left, right| {
            (left.created_at.as_str(), left.provider_message_id.as_str()).cmp(&(
                right.created_at.as_str(),
                right.provider_message_id.as_str(),
            ))
        });

        let page_runtime = self.clone();
        let mailbox = mailbox.clone();
        self.repository_write_segment(move || {
            let mut prepared_snapshots = Vec::new();
            let mut ledger = PageThreadLedger::default();
            let mut planner = SentPagePlanner::default();
            for detail in details {
                let provider_message_id = detail.provider_message_id.clone();
                match page_runtime.prepare_sent_snapshot(
                    &mailbox,
                    detail,
                    &mut planner,
                    &mut ledger,
                ) {
                    Ok(snapshot) => prepared_snapshots.push(snapshot),
                    Err(error) => import.missing.push((provider_message_id, error)),
                }
            }
            // A later root on the same page may reveal an older canonical
            // thread and redirect an earlier merge target (A <- B, then
            // C <- A). Normalize every adopted snapshot only after the full
            // page's alias graph is known, otherwise merge-first commit order
            // could delete A and snapshot persistence would recreate it.
            let mut snapshots = Vec::with_capacity(prepared_snapshots.len());
            for mut prepared in prepared_snapshots {
                if let Some(adopted_thread_id) = prepared.adopted_thread_id {
                    let canonical = planner.canonical(&adopted_thread_id)?;
                    prepared.snapshot.provider_thread_key = page_runtime
                        .repository
                        .email_thread(&canonical)?
                        .provider_thread_key;
                }
                snapshots.push(prepared.snapshot);
            }
            import.imported = snapshots.len() as u32;
            let (advance, failure, incremental_continuation) = if import.missing.is_empty() {
                (
                    checkpoint.candidate_advance,
                    checkpoint.success_failure,
                    checkpoint.candidate_continuation,
                )
            } else {
                (
                    None,
                    Some(missing_failure(&import.missing)),
                    EmailSyncIncrementalContinuationMutation::Preserve,
                )
            };
            import.effective_failure = failure.clone();
            import.effective_continuation = incremental_continuation.clone();
            import.terminal_failure = if import.missing.is_empty() {
                checkpoint
                    .stop_after_success
                    .then(|| failure.clone())
                    .flatten()
            } else {
                failure.clone()
            };
            if snapshots.is_empty()
                && updates.is_empty()
                && planner.thread_merges.is_empty()
                && !durable.differs(
                    advance.as_ref(),
                    failure.as_ref(),
                    &incremental_continuation,
                )
            {
                return Ok(import);
            }
            let outcome = page_runtime
                .repository
                .commit_email_sync_page(EmailSyncPageCommit {
                    mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
                    direction: EmailDirection::Outbound,
                    snapshots,
                    advance: advance.clone(),
                    failure,
                    incremental_continuation,
                    sent_mutations: EmailSyncSentMutations {
                        status_updates: updates,
                        thread_merges: planner.thread_merges,
                    },
                })?;
            import.checkpoint_committed = outcome.checkpoint_changed;
            import.full_sync_recorded = outcome.advanced
                && advance
                    .as_ref()
                    .is_some_and(|advance| advance.mark_full_sync);
            import.committed_advance = outcome.advanced.then_some(advance).flatten();
            import.reconciled_statuses = outcome.reconciled_statuses;
            import.merged_threads = outcome.merged_threads;
            import.changed_thread_ids = outcome.changed_thread_ids;
            if !import.changed_thread_ids.is_empty() {
                page_runtime.publish_changes([
                    CreatorChangeScope::EmailFolders,
                    CreatorChangeScope::EmailThreads,
                ]);
            }
            Ok(import)
        })
        .await
        .map_err(|error| sent_write_segment_error("page commit", error))
    }

    fn prepare_sent_snapshot(
        &self,
        mailbox: &MailboxDetail,
        detail: onecrew_email_contracts::PlatformEmailMessage,
        planner: &mut SentPagePlanner,
        ledger: &mut PageThreadLedger,
    ) -> Result<PreparedSentSnapshot, String> {
        // Owner attachment mirrors inbound conflict isolation: exactly one
        // Creator route across the recipient set attaches the relationship;
        // anything ambiguous stays unowned. No touchpoint is created —
        // provider presence proves the email fact, not that this computer
        // performed an outreach action.
        let mut route_owner = None;
        let mut ambiguous = false;
        for address in detail.to.iter().chain(detail.cc.iter()) {
            if let Some(record) = self.email_owner(address)? {
                match &route_owner {
                    None => route_owner = Some(record),
                    Some(existing) if existing.route.id == record.route.id => {}
                    Some(_) => {
                        ambiguous = true;
                        break;
                    }
                }
            }
        }
        let mut owner_pair = match (ambiguous, route_owner) {
            (false, Some(record)) => Some((record.route.creator_id, record.route.id)),
            _ => None,
        };

        // RFC placement: replies that arrived before their root already
        // reference its Message-ID. The oldest referencing thread becomes
        // the home — but only when this import's owner may actually join it
        // (the persist-time ownership rules): a conflicting root is isolated
        // unowned instead of aborting the whole page at commit. RFC-proven
        // siblings merge only into a thread this import may join. Without a
        // Message-ID the import is explicitly standalone.
        let mut pending_merge = None;
        let mut pending_owner = None;
        let mut adopted_thread_id = None;
        let mut provider_thread_key = match detail.internet_message_id.as_deref() {
            Some(internet_message_id) => {
                let referencing = self
                    .repository
                    .find_email_threads_referencing(&mailbox.mailbox_ref_id, internet_message_id)?;
                let mut canonical_threads = Vec::new();
                let mut seen = HashSet::new();
                for thread_id in referencing {
                    let thread_id = planner.canonical(&thread_id)?;
                    if seen.insert(thread_id.clone()) {
                        canonical_threads.push(thread_id);
                    }
                }
                let referencing = canonical_threads;
                match referencing.split_first() {
                    Some((target, siblings)) => {
                        let thread = self.repository.email_thread(target)?;
                        let persisted_owner = sent_thread_owner(&thread);
                        let effective_owner = persisted_owner
                            .as_ref()
                            .map(|owner| planner.effective_owner(target, owner));
                        let joinable = effective_owner
                            .as_ref()
                            .is_some_and(|owner| owner_accepts_snapshot(owner, &owner_pair));
                        if joinable {
                            let effective_owner = effective_owner.expect("joinable owner");
                            let owner_after = effective_owner.or_else(|| owner_pair.clone());
                            adopted_thread_id = Some(target.clone());
                            pending_owner = Some((target.clone(), owner_after.clone()));
                            let mut compatible_sources = Vec::new();
                            for source in siblings {
                                let source_thread = self.repository.email_thread(source)?;
                                if let Some(persisted_source_owner) =
                                    sent_thread_owner(&source_thread)
                                {
                                    let source_owner =
                                        planner.effective_owner(source, &persisted_source_owner);
                                    if owners_can_merge(&owner_after, &source_owner) {
                                        compatible_sources.push(source.clone());
                                    }
                                }
                            }
                            if !compatible_sources.is_empty() {
                                pending_merge = Some((target.clone(), compatible_sources));
                            }
                            thread.provider_thread_key
                        } else {
                            owner_pair = None;
                            isolated_inbound_thread_key(&detail)
                        }
                    }
                    None => format!("resend:message:{internet_message_id}"),
                }
            }
            None => format!("resend:sent:{}", detail.provider_message_id),
        };

        // Same-page guard: two imports this page must not shape one thread
        // with incompatible owners (see the received engine's ledger).
        if !ledger.compatible(&provider_thread_key, &owner_pair) {
            provider_thread_key = isolated_inbound_thread_key(&detail);
            owner_pair = None;
            adopted_thread_id = None;
            pending_owner = None;
            pending_merge = None;
        }
        ledger.record(
            detail.internet_message_id.clone(),
            &provider_thread_key,
            owner_pair.clone(),
        );
        let (creator_id, contact_route_id) = match owner_pair {
            Some((creator_id, contact_route_id)) => (Some(creator_id), Some(contact_route_id)),
            None => (None, None),
        };

        let last_event = detail.last_event.clone();
        let snapshot = EmailSnapshotInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            creator_id,
            contact_route_id,
            provider_thread_key,
            subject: detail.subject.clone(),
            messages: vec![provider_message_input(
                detail,
                EmailDirection::Outbound,
                crate::models::EmailOrigin::ProviderImport,
                None,
                Vec::new(),
                &last_event,
            )?],
        };
        if let Some((thread_id, owner)) = pending_owner {
            planner.record_owner(thread_id, owner);
        }
        if let Some((target, sources)) = pending_merge {
            planner.record_merge(target, sources)?;
        }
        Ok(PreparedSentSnapshot {
            snapshot,
            adopted_thread_id,
        })
    }
}

fn sent_thread_owner(thread: &crate::EmailThread) -> Option<SentThreadOwner> {
    match (&thread.creator_id, &thread.contact_route_id) {
        (None, None) => Some(None),
        (Some(creator_id), Some(contact_route_id)) => {
            Some(Some((creator_id.clone(), contact_route_id.clone())))
        }
        _ => None,
    }
}

fn owner_accepts_snapshot(existing: &SentThreadOwner, incoming: &SentThreadOwner) -> bool {
    match existing {
        None => true,
        Some(existing) => incoming.as_ref() == Some(existing),
    }
}

fn owners_can_merge(target: &SentThreadOwner, source: &SentThreadOwner) -> bool {
    match (target, source) {
        (None, None) | (Some(_), None) => true,
        (Some(target), Some(source)) => target == source,
        (None, Some(_)) => false,
    }
}

fn accumulate(report: &mut SentSyncReport, import: &SentPageImport) {
    report.details_fetched += import.details_fetched;
    report.skipped_complete += import.skipped_complete;
    report.imported += import.imported;
    report.reconciled_statuses += import.reconciled_statuses;
    report.merged_threads += import.merged_threads;
    for thread_id in &import.changed_thread_ids {
        if !report.changed_thread_ids.contains(thread_id) {
            report.changed_thread_ids.push(thread_id.clone());
        }
    }
}

fn sent_read_segment_error(
    stage: &str,
    error: crate::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) => {
            format!("sent sync {stage} repository read failed: {error}")
        }
    }
}

fn sent_write_segment_error(
    stage: &str,
    error: crate::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            format!(
                "sent sync {stage} did not start: {error}; this segment made no additional \
                 repository change; earlier pages may already be durable"
            )
        }
        crate::CreatorRepositoryCallError::Dispatch(error) => format!(
            "sent sync {stage} outcome is unknown: {error}; rerun the same mailbox for durable \
             reconciliation"
        ),
    }
}

fn crossed_watermark(
    summaries: &[PlatformSentEmailSummary],
    bounds: Option<&onecrew_email_contracts::PlatformPageBounds>,
    watermark_id: Option<&str>,
    watermark_occurred_at: Option<u64>,
) -> Result<bool, String> {
    let Some(watermark_id) = watermark_id else {
        return Ok(false);
    };
    // The watermark is a raw team-level item; the raw boundary proves the
    // crossing even when sender filtering empties the page.
    if let Some(bounds) = bounds {
        if bounds.newest_provider_message_id == watermark_id {
            return Ok(true);
        }
        if let Some(newest_occurred_at) = watermark_occurred_at
            && parse_provider_time(&bounds.oldest_created_at)? < newest_occurred_at
        {
            return Ok(true);
        }
    }
    for summary in summaries {
        if summary.provider_message_id == watermark_id {
            return Ok(true);
        }
        if let Some(newest_occurred_at) = watermark_occurred_at
            && parse_provider_time(&summary.created_at)? < newest_occurred_at
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn missing_failure(missing: &[(String, String)]) -> EmailSyncFailure {
    EmailSyncFailure {
        missing_provider_message_ids: missing.iter().map(|(id, _)| id.clone()).collect(),
        error_class: "detail_retrieval_failed".into(),
        message: missing
            .first()
            .map(|(_, error)| error.clone())
            .unwrap_or_default(),
    }
}

fn sent_progress(
    report: &SentSyncReport,
    estimated_percent: Option<u8>,
) -> crate::MailboxSyncProgress {
    crate::MailboxSyncProgress {
        direction: EmailDirection::Outbound,
        pages_listed: report.pages_listed,
        summaries_listed: report.summaries_listed,
        details_fetched: report.details_fetched,
        imported: report.imported,
        skipped_complete: report.skipped_complete,
        estimated_percent,
    }
}

fn page_budget_failure() -> EmailSyncFailure {
    EmailSyncFailure {
        missing_provider_message_ids: Vec::new(),
        error_class: "page_budget_exhausted".into(),
        message: format!(
            "stopped after {MAX_RUN_PAGES} provider pages in one run; the durable cursor advanced and the next run resumes from it"
        ),
    }
}

#[cfg(test)]
mod planner_tests {
    use super::*;

    #[test]
    fn chained_merges_canonicalize_non_adjacent_aliases_and_self_sources() {
        let target = crate::EmailThreadId::new("thread-a");
        let first_source = crate::EmailThreadId::new("thread-b");
        let second_source = crate::EmailThreadId::new("thread-c");
        let mut planner = SentPagePlanner::default();

        planner
            .record_merge(target.clone(), vec![first_source.clone()])
            .unwrap();
        planner
            .record_merge(
                target.clone(),
                vec![
                    first_source.clone(),
                    second_source.clone(),
                    target.clone(),
                    first_source.clone(),
                ],
            )
            .unwrap();

        assert_eq!(planner.thread_merges.len(), 2);
        assert_eq!(planner.thread_merges[0].sources, vec![first_source.clone()]);
        assert_eq!(
            planner.thread_merges[1].sources,
            vec![second_source.clone()]
        );
        assert_eq!(planner.canonical(&first_source).unwrap(), target);
        assert_eq!(planner.canonical(&second_source).unwrap(), target);
    }

    #[test]
    fn alias_cycles_fail_closed_instead_of_looping() {
        let first = crate::EmailThreadId::new("thread-a");
        let second = crate::EmailThreadId::new("thread-b");
        let mut planner = SentPagePlanner::default();
        planner.aliases.insert(first.clone(), second.clone());
        planner.aliases.insert(second, first.clone());

        let error = planner.canonical(&first).unwrap_err();
        assert!(error.contains("cycle"), "{error}");
    }
}
