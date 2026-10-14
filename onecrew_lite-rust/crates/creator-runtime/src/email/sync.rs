//! The received-mail synchronization engine.
//!
//! Pull-only: every run lists team-scoped Receiving pages newest-first,
//! filters them to the mailbox, skips already-complete messages through the
//! durable identity map, fetches only missing details through the bounded
//! provider queue, and commits once per page together with the direction's
//! eligible watermark. Resend documents no durable delta token, so
//! incremental runs restart at the newest page and stop after crossing the
//! locally owned watermark; unfinished backfill resumes from its durable
//! cursor toward the provider retention floor.

use std::collections::HashMap;

use crate::{
    ContactChannel, ContactLocatorInput, CreatorChangeScope, CreatorContactRouteRecord,
    EmailCapability, EmailDirection, EmailGateway, EmailSnapshotInput, EmailSyncAdvance,
    EmailSyncFailure, EmailSyncIncrementalContinuation, EmailSyncIncrementalContinuationMutation,
    EmailSyncPageCommit, EmailThreadId, LocatorKind, MailboxDetail, ReceivedSyncReport,
};
use onecrew_email_contracts::PlatformEmailSummary;

use super::message::parse_provider_time;
use super::{
    ensure_mailbox, inbound_thread_key, isolated_inbound_thread_key, message_reference_ids,
    normalize_email_address, provider_message_input, safe_inbound_thread, validate_inbound,
};

pub(crate) const RECEIVED_PAGE_LIMIT: u16 = 100;
/// Safety bound on provider pages per run; far above any 30-day retention
/// window at 100 messages per page. Hitting it records resumable failure
/// state instead of looping forever on a provider fault.
const MAX_RUN_PAGES: u32 = 200;

struct PageImport {
    changed_thread_ids: Vec<EmailThreadId>,
    matched: u32,
    unmatched: u32,
    imported: u32,
    skipped_complete: u32,
    details_fetched: u32,
    missing: Vec<(String, String)>,
    committed_advance: Option<EmailSyncAdvance>,
    checkpoint_committed: bool,
    effective_failure: Option<EmailSyncFailure>,
    effective_continuation: EmailSyncIncrementalContinuationMutation,
    terminal_failure: Option<EmailSyncFailure>,
    full_sync_recorded: bool,
}

#[derive(Clone)]
pub(super) struct DurableSyncCheckpoint {
    pub(super) newest_provider_message_id: Option<String>,
    pub(super) newest_occurred_at: Option<u64>,
    pub(super) oldest_backfill_cursor: Option<String>,
    pub(super) backfill_complete: bool,
    pub(super) failure: Option<EmailSyncFailure>,
    pub(super) has_full_sync: bool,
    pub(super) incremental_continuation: Option<IncrementalContinuationCheckpoint>,
}

#[derive(Clone)]
pub(super) struct IncrementalContinuationCheckpoint {
    pub(super) next_cursor: String,
    pub(super) newest_provider_message_id: String,
    pub(super) newest_occurred_at: u64,
}

#[derive(Clone)]
pub(super) struct PageCheckpointPlan {
    pub(super) candidate_advance: Option<EmailSyncAdvance>,
    pub(super) success_failure: Option<EmailSyncFailure>,
    pub(super) candidate_continuation: EmailSyncIncrementalContinuationMutation,
    pub(super) stop_after_success: bool,
}

impl DurableSyncCheckpoint {
    pub(super) fn from_state(
        state: Option<&crate::EmailSyncState>,
        incremental_continuation: Option<EmailSyncIncrementalContinuation>,
    ) -> Self {
        Self {
            newest_provider_message_id: state
                .and_then(|state| state.newest_provider_message_id.clone()),
            newest_occurred_at: state.and_then(|state| state.newest_occurred_at),
            oldest_backfill_cursor: state.and_then(|state| state.oldest_backfill_cursor.clone()),
            backfill_complete: state.is_some_and(|state| state.backfill_complete),
            failure: state.and_then(|state| state.failure.clone()),
            has_full_sync: state.and_then(|state| state.last_full_sync_at).is_some(),
            incremental_continuation: incremental_continuation.map(|continuation| {
                IncrementalContinuationCheckpoint {
                    next_cursor: continuation.next_cursor,
                    newest_provider_message_id: continuation.newest_provider_message_id,
                    newest_occurred_at: continuation.newest_occurred_at,
                }
            }),
        }
    }

    pub(super) fn differs(
        &self,
        advance: Option<&EmailSyncAdvance>,
        failure: Option<&EmailSyncFailure>,
        incremental_continuation: &EmailSyncIncrementalContinuationMutation,
    ) -> bool {
        self.failure.as_ref() != failure
            || advance.is_some_and(|advance| {
                self.newest_provider_message_id != advance.newest_provider_message_id
                    || self.newest_occurred_at != advance.newest_occurred_at
                    || self.oldest_backfill_cursor != advance.oldest_backfill_cursor
                    || self.backfill_complete != advance.backfill_complete
                    || (advance.mark_full_sync && !self.has_full_sync)
            })
            || match incremental_continuation {
                EmailSyncIncrementalContinuationMutation::Preserve => false,
                EmailSyncIncrementalContinuationMutation::Set {
                    next_cursor,
                    newest_provider_message_id,
                    newest_occurred_at,
                } => self
                    .incremental_continuation
                    .as_ref()
                    .is_none_or(|current| {
                        current.next_cursor != *next_cursor
                            || current.newest_provider_message_id != *newest_provider_message_id
                            || current.newest_occurred_at != *newest_occurred_at
                    }),
                EmailSyncIncrementalContinuationMutation::Clear => {
                    self.incremental_continuation.is_some()
                }
            }
    }

    pub(super) fn apply(&mut self, advance: &EmailSyncAdvance, failure: Option<EmailSyncFailure>) {
        self.newest_provider_message_id = advance.newest_provider_message_id.clone();
        self.newest_occurred_at = advance.newest_occurred_at;
        self.oldest_backfill_cursor = advance.oldest_backfill_cursor.clone();
        self.backfill_complete = advance.backfill_complete;
        self.has_full_sync |= advance.mark_full_sync;
        self.failure = failure;
    }

    pub(super) fn apply_incremental_continuation(
        &mut self,
        mutation: &EmailSyncIncrementalContinuationMutation,
    ) {
        match mutation {
            EmailSyncIncrementalContinuationMutation::Preserve => {}
            EmailSyncIncrementalContinuationMutation::Set {
                next_cursor,
                newest_provider_message_id,
                newest_occurred_at,
            } => {
                self.incremental_continuation = Some(IncrementalContinuationCheckpoint {
                    next_cursor: next_cursor.clone(),
                    newest_provider_message_id: newest_provider_message_id.clone(),
                    newest_occurred_at: *newest_occurred_at,
                });
            }
            EmailSyncIncrementalContinuationMutation::Clear => {
                self.incremental_continuation = None;
            }
        }
    }
}

type PageThreadOwner = Option<(crate::CreatorId, crate::CreatorContactRouteId)>;

/// Page-local ledger of the thread keys chosen while preparing one page's
/// snapshots. The whole page commits in one transaction, and persist-time
/// ownership validation rejects an incompatible reuse — which would abort
/// the entire page. The ledger applies the same rules while keys are being
/// chosen, so a conflicting snapshot is isolated instead of aborting.
#[derive(Default)]
pub(super) struct PageThreadLedger {
    keys: HashMap<String, String>,
    owners: HashMap<String, PageThreadOwner>,
}

impl PageThreadLedger {
    fn key_for_references(&self, reference_ids: &[String]) -> Option<String> {
        reference_ids
            .iter()
            .find_map(|reference| self.keys.get(reference).cloned())
    }

    /// Mirrors `validate_existing_thread_owner`: equal owners and an owned
    /// snapshot adopting an unowned thread pass; an unowned snapshot joining
    /// an owned thread, or two different owners, conflict.
    pub(super) fn compatible(&self, key: &str, owner: &PageThreadOwner) -> bool {
        match self.owners.get(key) {
            None | Some(None) => true,
            Some(Some(existing)) => owner.as_ref() == Some(existing),
        }
    }

    pub(super) fn record(
        &mut self,
        internet_message_id: Option<String>,
        key: &str,
        owner: PageThreadOwner,
    ) {
        if let Some(internet_message_id) = internet_message_id {
            self.keys.insert(internet_message_id, key.to_owned());
        }
        let entry = self
            .owners
            .entry(key.to_owned())
            .or_insert_with(|| owner.clone());
        if entry.is_none() && owner.is_some() {
            // An owned snapshot adopted a thread this page created unowned.
            *entry = owner;
        }
    }
}

impl crate::CreatorRuntime {
    /// The canonical pull-only received engine: newest-first reconciliation
    /// that stops after crossing the durable inbound watermark, plus resumed
    /// backfill toward the provider retention floor when the initial walk
    /// has not completed. Peer adapters project this in Phase 7.
    pub async fn sync_mailbox_received(
        &self,
        mailbox_ref_id: &crate::MailboxRefId,
    ) -> Result<ReceivedSyncReport, String> {
        let gateway = self.email_gateway()?.clone();
        let context_runtime = self.clone();
        let context_mailbox_ref_id = mailbox_ref_id.clone();
        let (mailbox, state, incremental_continuation) = self
            .repository_read_segment(move || {
                let mailbox = context_runtime.require_mailbox_ref(&context_mailbox_ref_id)?;
                ensure_mailbox(&mailbox, EmailCapability::Inbox)?;
                let checkpoint = context_runtime
                    .repository
                    .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)?;
                Ok((
                    mailbox,
                    checkpoint.state,
                    checkpoint.incremental_continuation,
                ))
            })
            .await
            .map_err(|error| received_read_segment_error("preflight", error))?;
        if mailbox.provider == crate::MailboxProvider::Gmail {
            return self.sync_outreach_replies(mailbox).await;
        }
        // A fresh-mailbox walk already advances the historical cursor on
        // every page. If it also has a Phase A row (for caller-drop
        // durability), recognize that lineage and do not mistake its newly
        // established anchor for an old incremental watermark.
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

        let mut report = ReceivedSyncReport::default();
        let mut failure: Option<EmailSyncFailure> = None;

        // Phase A: newest-first reconciliation over the bounded overlap.
        loop {
            let page = gateway
                .received_emails(
                    &mailbox.mailbox_ref_id,
                    cursor.as_deref(),
                    RECEIVED_PAGE_LIMIT,
                )
                .await?;
            report.pages_listed += 1;
            report.summaries_listed += page.messages.len() as u32;

            // Gmail does not promise globally ordered list pages. Traverse the
            // inbox snapshot and rely on durable message-ID deduplication.
            let crossed = mailbox.provider != crate::MailboxProvider::Gmail && crossed_watermark(
                &page.messages,
                page.bounds.as_ref(),
                watermark_id.as_deref(),
                watermark_occurred_at,
            )?;
            // A Gmail continuation needs a durable anchor, but that anchor is
            // never interpreted as an ordered watermark or retention estimate.
            if newest_candidate.is_none() && mailbox.provider == crate::MailboxProvider::Gmail
                && let Some(message) = page.messages.first()
            {
                newest_candidate = Some((message.provider_message_id.clone(), parse_provider_time(&message.created_at)?));
            }
            // The watermark anchors on the RAW team-level newest item, so a
            // mailbox that matched nothing still stops at page one next run.
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
                Some(pagination_anchor_failure())
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
                .import_received_page(
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
            self.record_sync_progress(&mailbox.mailbox_ref_id, received_progress(&report, percent));
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
                    .received_emails(
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
                    .import_received_page(
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
                self.record_sync_progress(
                    &mailbox.mailbox_ref_id,
                    received_progress(&report, percent),
                );
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
        let repository = std::sync::Arc::clone(&self.repository);
        let triage_mailbox_ref_id = mailbox.mailbox_ref_id.clone();
        report.pending_triage = Some(
            self.repository_read_segment(move || {
                repository
                    .pending_email_triage(&triage_mailbox_ref_id, crate::PENDING_REPLY_TRIAGE_LIMIT)
            })
            .await
            .map_err(|error| received_read_segment_error("triage readback", error))?,
        );
        Ok(report)
    }

    async fn sync_outreach_replies(&self, mailbox: MailboxDetail) -> Result<ReceivedSyncReport, String> {
        ensure_mailbox(&mailbox, EmailCapability::Inbox)?;
        let gateway = self.email_gateway()?.clone();
        let repo = self.repository.clone();
        let id = mailbox.mailbox_ref_id.clone();
        let seeds = self.repository_read_segment(move || repo.outreach_reply_seeds(&id))
            .await.map_err(|e| received_read_segment_error("reply seeds", e))?;
        let mut report = ReceivedSyncReport::default();
        for (thread_id, seed) in seeds {
            let ids = gateway.outreach_reply_ids(&mailbox.mailbox_ref_id, &seed).await?;
            report.pages_listed += 1;
            report.summaries_listed += ids.len() as u32;
            for ids in ids.chunks(100) {
                let repo = self.repository.clone();
                let mailbox_id = mailbox.mailbox_ref_id.clone();
                let ids = ids.to_vec();
                let known_ids = ids.clone();
                let complete = self.repository_read_segment(move || repo.complete_email_provider_ids(&mailbox_id, &known_ids))
                    .await.map_err(|e| received_read_segment_error("reply dedupe", e))?;
                report.skipped_complete += complete.len() as u32;
                let missing: Vec<_> = ids.into_iter().filter(|id| !complete.contains(id)).collect();
                if missing.is_empty() { continue; }
                let outcomes = gateway.received_email_details(&mailbox.mailbox_ref_id, &missing).await?;
                report.details_fetched += outcomes.len() as u32;
                let mut details = Vec::new();
                for id in &missing {
                    match outcomes.iter().find(|(returned, _)| returned == id) {
                        Some((_, Ok(detail))) if detail.provider_message_id == *id => details.push(detail.clone()),
                        _ => report.missing_provider_message_ids.push(id.clone()),
                    }
                }
                let owner = self.clone();
                let mailbox = mailbox.clone();
                let thread_id = thread_id.clone();
                let imported = self.repository_write_segment(move || {
                    let thread = owner.repository.get_email_thread_record(&thread_id)?;
                    if thread.mailbox_ref_id != mailbox.mailbox_ref_id { return Err("reply_mailbox_mismatch".into()); }
                    let mut snapshots = Vec::new();
                    for detail in details {
                        validate_inbound(&mailbox, &detail)?;
                        snapshots.push(EmailSnapshotInput {
                            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
                            creator_id: thread.creator_id.clone(), contact_route_id: thread.contact_route_id.clone(),
                            provider_thread_key: thread.provider_thread_key.clone(), subject: thread.subject.clone(),
                            messages: vec![provider_message_input(detail, EmailDirection::Inbound,
                                crate::models::EmailOrigin::ProviderImport, None, Vec::new(), "unread")?],
                        });
                    }
                    let count = snapshots.len() as u32;
                    let result = owner.repository.commit_email_sync_page(EmailSyncPageCommit {
                        mailbox_ref_id: mailbox.mailbox_ref_id, direction: EmailDirection::Inbound,
                        snapshots, advance: None, failure: None,
                        incremental_continuation: EmailSyncIncrementalContinuationMutation::Preserve,
                        sent_mutations: Default::default(),
                    })?;
                    if !result.changed_thread_ids.is_empty() {
                        owner.publish_changes([CreatorChangeScope::EmailFolders, CreatorChangeScope::EmailThreads]);
                    }
                    Ok::<_, String>((count, result.changed_thread_ids))
                }).await.map_err(|e| received_write_segment_error("reply commit", e))?;
                report.imported += imported.0;
                for id in imported.1 {
                    if !report.changed_thread_ids.contains(&id) { report.changed_thread_ids.push(id); }
                }
            }
        }
        if !report.missing_provider_message_ids.is_empty() { return Err("gmail_reply_details_incomplete".into()); }
        // This checkpoint now describes only the locally authored conversation scope.
        // Never reuse an old inbox continuation, including after a service restart.
        let repo = self.repository.clone();
        let id = mailbox.mailbox_ref_id.clone();
        self.repository_write_segment(move || repo.commit_email_sync_page(EmailSyncPageCommit {
            mailbox_ref_id: id, direction: EmailDirection::Inbound, snapshots: Vec::new(),
            advance: Some(EmailSyncAdvance {
                newest_provider_message_id: None, newest_occurred_at: None,
                oldest_backfill_cursor: None, backfill_complete: true, mark_full_sync: true,
            }),
            failure: None, incremental_continuation: EmailSyncIncrementalContinuationMutation::Clear,
            sent_mutations: Default::default(),
        })).await.map_err(|e| received_write_segment_error("reply checkpoint", e))?;
        report.backfill_complete = true;
        report.full_sync_recorded = true;
        Ok(report)
    }

    fn email_gateway(&self) -> Result<&std::sync::Arc<dyn EmailGateway>, String> {
        self.gateways
            .email
            .as_ref()
            .ok_or_else(|| "email gateway is not configured".to_string())
    }

    /// Imports one mailbox-filtered summary page: dedupe against the identity
    /// map, fetch only missing details through the bounded queue, order the
    /// results so RFC parents precede their children, and commit everything
    /// in one transaction. Publishes at most one runtime change per page.
    async fn import_received_page(
        &self,
        mailbox: &MailboxDetail,
        gateway: &dyn EmailGateway,
        summaries: &[PlatformEmailSummary],
        checkpoint: PageCheckpointPlan,
        durable: DurableSyncCheckpoint,
    ) -> Result<PageImport, String> {
        let mut import = PageImport {
            changed_thread_ids: Vec::new(),
            matched: 0,
            unmatched: 0,
            imported: 0,
            skipped_complete: 0,
            details_fetched: 0,
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
            .map_err(|error| received_read_segment_error("page dedupe", error))?;
        import.skipped_complete = complete.len() as u32;
        let to_fetch: Vec<String> = ids
            .into_iter()
            .filter(|id| !complete.contains(id))
            .collect();
        let mut details = Vec::new();
        if !to_fetch.is_empty() {
            let outcomes = gateway
                .received_email_details(&mailbox.mailbox_ref_id, &to_fetch)
                .await?;
            import.details_fetched = outcomes.len() as u32;
            for (id, outcome) in outcomes {
                match outcome {
                    Ok(detail) if detail.provider_message_id == id => details.push(detail),
                    Ok(_) => {
                        import
                            .missing
                            .push((id, "detail did not match its list item".into()));
                    }
                    Err(error) => import.missing.push((id, error)),
                }
            }
        }

        // Roots before replies: older messages first so a reply on the same
        // page can attach to the thread its parent is about to create.
        details.sort_by(|left, right| {
            (left.created_at.as_str(), left.provider_message_id.as_str()).cmp(&(
                right.created_at.as_str(),
                right.provider_message_id.as_str(),
            ))
        });

        let page_runtime = self.clone();
        let mailbox = mailbox.clone();
        self.repository_write_segment(move || {
            let mut snapshots = Vec::new();
            let mut ledger = PageThreadLedger::default();
            for detail in details {
                let provider_message_id = detail.provider_message_id.clone();
                match page_runtime.prepare_inbound_snapshot(&mailbox, detail, &mut ledger) {
                    Ok((snapshot, owner_matched)) => {
                        if owner_matched {
                            import.matched += 1;
                        } else {
                            import.unmatched += 1;
                        }
                        snapshots.push(snapshot);
                    }
                    Err(error) => import.missing.push((provider_message_id, error)),
                }
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
                && !durable.differs(
                    advance.as_ref(),
                    failure.as_ref(),
                    &incremental_continuation,
                )
            {
                return Ok(import);
            }
            import.full_sync_recorded = advance
                .as_ref()
                .is_some_and(|advance| advance.mark_full_sync);
            let outcome = page_runtime
                .repository
                .commit_email_sync_page(EmailSyncPageCommit {
                    mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
                    direction: EmailDirection::Inbound,
                    snapshots,
                    advance: advance.clone(),
                    failure,
                    incremental_continuation,
                    sent_mutations: Default::default(),
                })?;
            import.checkpoint_committed = outcome.checkpoint_changed;
            import.committed_advance = outcome.advanced.then_some(advance).flatten();
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
        .map_err(|error| received_write_segment_error("page commit", error))
    }

    fn prepare_inbound_snapshot(
        &self,
        mailbox: &MailboxDetail,
        detail: onecrew_email_contracts::PlatformEmailMessage,
        ledger: &mut PageThreadLedger,
    ) -> Result<(EmailSnapshotInput, bool), String> {
        validate_inbound(mailbox, &detail)?;
        let owner = self.email_owner(&detail.from)?;
        let reference_ids = message_reference_ids(&detail);
        let referenced_thread = self
            .repository
            .find_email_thread_by_internet_message_ids(&mailbox.mailbox_ref_id, &reference_ids)?;
        let pending_key = ledger.key_for_references(&reference_ids);
        let had_referenced_thread = referenced_thread.is_some() || pending_key.is_some();
        let (thread, owner) = safe_inbound_thread(referenced_thread, owner);
        let mut owner_pair: PageThreadOwner =
            owner.map(|record| (record.route.creator_id, record.route.id));
        let mut provider_thread_key = match thread.as_ref() {
            Some(thread) => thread.provider_thread_key.clone(),
            None => match pending_key {
                Some(key) => key,
                None if had_referenced_thread => isolated_inbound_thread_key(&detail),
                None => inbound_thread_key(&detail),
            },
        };
        // Same-page ownership guard: a snapshot that cannot join the thread
        // an earlier same-page snapshot shaped is isolated unowned — exactly
        // what the cross-page path does — instead of failing the page commit.
        if !ledger.compatible(&provider_thread_key, &owner_pair) {
            provider_thread_key = isolated_inbound_thread_key(&detail);
            owner_pair = None;
        }
        let owner_matched = owner_pair.is_some();
        ledger.record(
            detail.internet_message_id.clone(),
            &provider_thread_key,
            owner_pair.clone(),
        );
        let (creator_id, contact_route_id) = match owner_pair {
            Some((creator_id, contact_route_id)) => (Some(creator_id), Some(contact_route_id)),
            None => (None, None),
        };
        let snapshot = EmailSnapshotInput {
            mailbox_ref_id: mailbox.mailbox_ref_id.clone(),
            creator_id,
            contact_route_id,
            provider_thread_key,
            subject: detail.subject.clone(),
            messages: vec![provider_message_input(
                detail,
                EmailDirection::Inbound,
                crate::models::EmailOrigin::ProviderImport,
                None,
                Vec::new(),
                "unread",
            )?],
        };
        Ok((snapshot, owner_matched))
    }

    pub(super) fn email_owner(
        &self,
        address: &str,
    ) -> Result<Option<CreatorContactRouteRecord>, String> {
        let normalized = normalize_email_address(address)?;
        let mut owners = self
            .repository
            .find_contact_route_owners(&ContactLocatorInput {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                normalized_value: normalized.clone(),
                display_value: normalized,
            })?
            .into_iter()
            .collect::<Vec<_>>();
        if owners.len() == 1 {
            Ok(owners.pop())
        } else {
            Ok(None)
        }
    }
}

fn accumulate(report: &mut ReceivedSyncReport, import: &PageImport) -> bool {
    report.details_fetched += import.details_fetched;
    report.skipped_complete += import.skipped_complete;
    report.imported += import.imported;
    report.matched += import.matched;
    report.unmatched += import.unmatched;
    let changed = !import.changed_thread_ids.is_empty();
    for thread_id in &import.changed_thread_ids {
        if !report.changed_thread_ids.contains(thread_id) {
            report.changed_thread_ids.push(thread_id.clone());
        }
    }
    changed
}

fn received_read_segment_error(
    stage: &str,
    error: crate::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) => {
            format!("received sync {stage} repository read failed: {error}")
        }
    }
}

fn received_write_segment_error(
    stage: &str,
    error: crate::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            format!(
                "received sync {stage} did not start: {error}; this segment made no additional \
                 repository change; earlier pages may already be durable"
            )
        }
        crate::CreatorRepositoryCallError::Dispatch(error) => format!(
            "received sync {stage} outcome is unknown: {error}; rerun the same mailbox for \
             durable reconciliation"
        ),
    }
}

fn crossed_watermark(
    summaries: &[PlatformEmailSummary],
    bounds: Option<&onecrew_email_contracts::PlatformPageBounds>,
    watermark_id: Option<&str>,
    watermark_occurred_at: Option<u64>,
) -> Result<bool, String> {
    let Some(watermark_id) = watermark_id else {
        return Ok(false);
    };
    // The watermark is a raw team-level item, so mailbox filtering may hide
    // it from `summaries`; the raw page boundary still proves the crossing.
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
            // Strictly older than the watermark: the boundary id itself is
            // gone (provider retention), but the walk has passed it.
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

fn received_progress(
    report: &ReceivedSyncReport,
    estimated_percent: Option<u8>,
) -> crate::MailboxSyncProgress {
    crate::MailboxSyncProgress {
        direction: EmailDirection::Inbound,
        pages_listed: report.pages_listed,
        summaries_listed: report.summaries_listed,
        details_fetched: report.details_fetched,
        imported: report.imported,
        skipped_complete: report.skipped_complete,
        estimated_percent,
    }
}

/// Estimated share of the backfill window already walked. Resend exposes no
/// total count, but the walk moves newest→oldest through the ~30-day
/// retention window, so the time-span between the run's newest anchor and
/// the current page's oldest item is an honest percentage. Mail volume is
/// not uniform in time, so the bar moves unevenly — but it only moves
/// forward and reaching the floor completes it.
pub(super) fn backfill_progress_percent(
    anchor_occurred_at: Option<u64>,
    bounds: Option<&onecrew_email_contracts::PlatformPageBounds>,
) -> Option<u8> {
    const RETENTION_WINDOW_SECONDS: u64 = 30 * 24 * 3600;
    let anchor = anchor_occurred_at?;
    let oldest = parse_provider_time(&bounds?.oldest_created_at).ok()?;
    let covered = anchor.saturating_sub(oldest).min(RETENTION_WINDOW_SECONDS);
    Some(((covered * 100) / RETENTION_WINDOW_SECONDS) as u8)
}

pub(super) fn pagination_cursor_failure() -> EmailSyncFailure {
    EmailSyncFailure {
        missing_provider_message_ids: Vec::new(),
        error_class: "pagination_cursor_missing".into(),
        message: "provider page reported more results without a continuation cursor".into(),
    }
}

pub(super) fn pagination_anchor_failure() -> EmailSyncFailure {
    EmailSyncFailure {
        missing_provider_message_ids: Vec::new(),
        error_class: "pagination_anchor_missing".into(),
        message: "provider page reported more results without a newest-page boundary".into(),
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
