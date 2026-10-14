//! The coalesced bidirectional sync operation both peer adapters project.
//!
//! Per-mailbox in-flight coalescing is process-local and runtime-owned: a
//! second caller — Desktop or MCP — joins the run already in flight instead
//! of starting a duplicate provider walk. The run itself executes in a
//! spawned task, so a dropped caller never abandons provider or commit work
//! mid-flight; registry state clears when the task finishes, on success and
//! failure alike, which is also what keeps `running` from ever reading true
//! after the operation ended.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Notify;

use crate::{
    CreatorChangeScope, EmailCapability, EmailDirection, EmailSyncRunMode, EmailSyncStateView,
    MailboxDirectionOutcome, MailboxRefId, MailboxSyncOutcome, MailboxSyncProgress,
    MailboxSyncStatus, ReceivedSyncReport, SentSyncReport, SyncMailboxRequest,
};

struct SyncSlot {
    notify: Arc<Notify>,
    outcome: Arc<Mutex<Option<Result<MailboxSyncOutcome, String>>>>,
    progress: Option<MailboxSyncProgress>,
}

const REDACTED_SYNC_TASK_FAILURE: &str = "mailbox sync task failed unexpectedly";

/// Process-local registry of in-flight mailbox sync runs.
#[derive(Default)]
pub(crate) struct EmailSyncRegistry {
    inner: Mutex<HashMap<String, SyncSlot>>,
    #[cfg(test)]
    before_readback: Mutex<
        std::collections::VecDeque<(
            tokio::sync::oneshot::Sender<()>,
            tokio::sync::oneshot::Receiver<()>,
        )>,
    >,
    #[cfg(test)]
    after_finish_remove: Mutex<
        std::collections::VecDeque<(
            tokio::sync::oneshot::Sender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}

impl EmailSyncRegistry {
    /// Take one coherent process-local status snapshot. Looking up the slot
    /// and copying its progress happen under the same registry lock, so an
    /// idle result can never carry progress from a finishing or replacement
    /// run.
    pub(crate) fn status_snapshot(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> (bool, Option<MailboxSyncProgress>) {
        let inner = self.inner.lock().expect("sync registry poisoned");
        match inner.get(mailbox_ref_id.as_str()) {
            Some(slot) => (true, slot.progress),
            None => (false, None),
        }
    }

    /// Records live counters for an in-flight run. Returns false — and keeps
    /// nothing — when no run is registered for the mailbox, so a directly
    /// invoked engine never leaves stale progress behind.
    pub(crate) fn record(
        &self,
        mailbox_ref_id: &MailboxRefId,
        progress: MailboxSyncProgress,
    ) -> bool {
        let mut inner = self.inner.lock().expect("sync registry poisoned");
        match inner.get_mut(mailbox_ref_id.as_str()) {
            Some(slot) => {
                slot.progress = Some(progress);
                true
            }
            None => false,
        }
    }

    /// Stores the terminal result and clears the matching run under the same
    /// short registry critical section. A new caller therefore cannot attach
    /// to a completed slot between those two ordered finalization steps.
    fn finish(
        &self,
        key: &str,
        slot: &Arc<Mutex<Option<Result<MailboxSyncOutcome, String>>>>,
        result: Result<MailboxSyncOutcome, String>,
    ) {
        let mut inner = self.inner.lock().expect("sync registry poisoned");
        *slot.lock().expect("sync slot poisoned") = Some(result);
        inner.remove(key);
        #[cfg(test)]
        if let Some((entered, release)) = self.after_finish_remove.lock().unwrap().pop_front() {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
    }

    #[cfg(test)]
    fn install_before_readback_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        self.before_readback
            .lock()
            .unwrap()
            .push_back((entered_tx, release_rx));
        (entered_rx, release_tx)
    }

    #[cfg(test)]
    async fn pause_before_readback(&self) {
        let barrier = self.before_readback.lock().unwrap().pop_front();
        if let Some((entered, release)) = barrier {
            // A test that dropped either handle wants the readback to continue.
            if entered.send(()).is_err() {
                log::debug!("readback barrier observer already gone");
            } else if let Err(error) = release.await {
                log::debug!("readback barrier released by drop: {error}");
            }
        }
    }

    #[cfg(test)]
    fn install_after_finish_remove_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        self.after_finish_remove
            .lock()
            .unwrap()
            .push_back((entered_tx, release_rx));
        (entered_rx, release_tx)
    }
}

impl crate::CreatorRuntime {
    /// Runs (or joins) the canonical pull-only sync for one mailbox:
    /// received first, then sent, both through their durable
    /// direction state. Concurrent callers for the same mailbox coalesce
    /// under one runtime operation.
    pub async fn sync_mailbox_full(
        &self,
        request: SyncMailboxRequest,
    ) -> Result<MailboxSyncOutcome, String> {
        let EmailSyncRunMode::Incremental = request.mode;
        let key = request.mailbox_ref_id.as_str().to_owned();

        let (notify, slot, spawned) = {
            let mut inner = self
                .email_sync_registry
                .inner
                .lock()
                .expect("sync registry poisoned");
            match inner.get(&key) {
                Some(slot) => (Arc::clone(&slot.notify), Arc::clone(&slot.outcome), false),
                None => {
                    let notify = Arc::new(Notify::new());
                    let slot = Arc::new(Mutex::new(None));
                    inner.insert(
                        key.clone(),
                        SyncSlot {
                            notify: Arc::clone(&notify),
                            outcome: Arc::clone(&slot),
                            progress: None,
                        },
                    );
                    (notify, slot, true)
                }
            }
        };

        if spawned {
            let runtime = self.clone();
            let mailbox_ref_id = request.mailbox_ref_id.clone();
            let task_notify = Arc::clone(&notify);
            let task_slot = Arc::clone(&slot);
            let task_key = key;
            tokio::spawn(async move {
                runtime.publish_changes([CreatorChangeScope::EmailSync]);
                let owner_runtime = runtime.clone();
                let owner = tokio::spawn(async move {
                    owner_runtime.run_full_mailbox_sync(&mailbox_ref_id).await
                });
                let result = match owner.await {
                    Ok(result) => result,
                    Err(_) => Err(REDACTED_SYNC_TASK_FAILURE.to_owned()),
                };
                runtime
                    .email_sync_registry
                    .finish(&task_key, &task_slot, result);
                runtime.publish_changes([CreatorChangeScope::EmailSync]);
                task_notify.notify_waiters();
            });
        }

        loop {
            let notified = notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = slot.lock().expect("sync slot poisoned").clone() {
                return result;
            }
            notified.await;
        }
    }

    /// Idle-time status: process-local running state plus the durable
    /// per-direction facts. Never triggers provider work.
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn mailbox_sync_status_async(
        &self,
        mailbox_ref_id: MailboxRefId,
    ) -> Result<MailboxSyncStatus, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.mailbox_sync_status(&mailbox_ref_id))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::mailbox_sync_status_async`] to own admission and dispatch.
    pub fn mailbox_sync_status(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxSyncStatus, String> {
        let mailbox = self.require_mailbox_ref(mailbox_ref_id)?;
        let inbound = self
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Inbound)?
            .state
            .as_ref()
            .map(EmailSyncStateView::from);
        let outbound = self
            .repository
            .email_sync_checkpoint(&mailbox.mailbox_ref_id, EmailDirection::Outbound)?
            .state
            .as_ref()
            .map(EmailSyncStateView::from);
        let (running, progress) = self
            .email_sync_registry
            .status_snapshot(&mailbox.mailbox_ref_id);
        Ok(MailboxSyncStatus {
            running,
            progress,
            mailbox,
            inbound,
            outbound,
        })
    }

    /// Engine page hook: publish the in-flight run's live counters so status
    /// readers see movement between the run's start and end. A direct engine
    /// call outside `sync_mailbox_full` records nothing and stays silent.
    pub(crate) fn record_sync_progress(
        &self,
        mailbox_ref_id: &MailboxRefId,
        progress: MailboxSyncProgress,
    ) {
        if self.email_sync_registry.record(mailbox_ref_id, progress) {
            self.publish_changes([CreatorChangeScope::EmailSync]);
        }
    }

    #[cfg(test)]
    pub(crate) fn install_next_full_sync_readback_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        self.email_sync_registry.install_before_readback_barrier()
    }

    #[cfg(test)]
    pub(crate) fn install_next_full_sync_finish_barrier(
        &self,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        self.email_sync_registry
            .install_after_finish_remove_barrier()
    }

    async fn run_full_mailbox_sync(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxSyncOutcome, String> {
        let context_runtime = self.clone();
        let context_mailbox_ref_id = mailbox_ref_id.clone();
        let (mailbox, can_receive, can_read_sent) = self
            .repository_read_segment(move || {
                let mailbox = context_runtime.require_mailbox_ref(&context_mailbox_ref_id)?;
                // Gmail reads only replies to locally confirmed sends; the received
                // runtime owns that scope even when invoked by old clients.
                let can_receive = mailbox.capabilities.contains(&EmailCapability::Inbox);
                let can_read_sent = mailbox
                    .capabilities
                    .contains(&EmailCapability::SentReadback)
                    // Gmail supports specific post-send readback, not a sent-history crawl.
                    && mailbox.provider != crate::MailboxProvider::Gmail;
                if !can_receive && !can_read_sent && mailbox.provider != crate::MailboxProvider::Gmail {
                    return Err("mailbox supports neither received nor sent reads".into());
                }
                Ok((mailbox, can_receive, can_read_sent))
            })
            .await
            .map_err(|error| full_read_segment_error("preflight", error))?;

        let received = if can_receive {
            match self.sync_mailbox_received(&mailbox.mailbox_ref_id).await {
                Ok(report) => Some(report),
                Err(error) => {
                    // A terminal list/readback failure must replace historical
                    // detail errors without advancing any durable cursor.
                    let repository = Arc::clone(&self.repository);
                    let id = mailbox.mailbox_ref_id.clone();
                    let failure = crate::EmailSyncFailure {
                        error_class: "sync_run_failed".into(),
                        message: safe_sync_failure(&error),
                        missing_provider_message_ids: Vec::new(),
                    };
                    self.repository_write_segment(move || repository.commit_email_sync_page(crate::EmailSyncPageCommit {
                        mailbox_ref_id: id,
                        direction: EmailDirection::Inbound,
                        snapshots: Vec::new(),
                        advance: None,
                        failure: Some(failure),
                        incremental_continuation: Default::default(),
                        sent_mutations: Default::default(),
                    })).await.map_err(|error| full_read_segment_error("failure checkpoint", error))?;
                    return Err(error);
                }
            }
        } else {
            None
        };
        let sent = if can_read_sent {
            Some(self.sync_mailbox_sent(&mailbox.mailbox_ref_id).await?)
        } else {
            None
        };

        let mut changed_thread_ids = Vec::new();
        for thread_id in received
            .iter()
            .flat_map(|report| report.changed_thread_ids.iter())
            .chain(
                sent.iter()
                    .flat_map(|report| report.changed_thread_ids.iter()),
            )
        {
            if !changed_thread_ids.contains(thread_id) {
                changed_thread_ids.push(thread_id.clone());
            }
        }

        #[cfg(test)]
        self.email_sync_registry.pause_before_readback().await;
        let repository = Arc::clone(&self.repository);
        let readback_mailbox_ref_id = mailbox.mailbox_ref_id.clone();
        let (inbound_state, outbound_state, pending_triage) = self
            .repository_read_segment(move || {
                repository.mailbox_sync_readback(
                    &readback_mailbox_ref_id,
                    crate::PENDING_REPLY_TRIAGE_LIMIT,
                )
            })
            .await
            .map_err(|error| full_read_segment_error("final readback", error))?;

        Ok(MailboxSyncOutcome {
            mailbox,
            inbound: received
                .map(|report| received_outcome(report, inbound_state.as_ref()))
                .unwrap_or_default(),
            outbound: sent
                .map(|report| sent_outcome(report, outbound_state.as_ref()))
                .unwrap_or_default(),
            changed_thread_ids,
            pending_triage,
        })
    }
}

fn safe_sync_failure(error: &str) -> String {
    // Only machine codes, never arbitrary provider bodies or database errors.
    let known = ["gmail_read_timeout", "gmail_read_transport_failed", "gmail_read_response_too_large",
        "gmail_read_response_invalid_json", "gmail_authorization_required", "gmail_mailbox_unavailable",
        "gmail_account_mismatch", "gmail_connection_missing"];
    if known.contains(&error) { return error.into(); }
    if let Some(rest) = error.strip_prefix("gmail_read_http_") {
        let (status, reason) = rest.split_once('_').unwrap_or((rest, ""));
        if status.len() == 3 && status.bytes().all(|b| b.is_ascii_digit())
            && ["", "retry_after", "rateLimitExceeded", "userRateLimitExceeded", "dailyLimitExceeded",
                "domainPolicy", "insufficientPermissions", "forbidden"].contains(&reason) {
            return error.into();
        }
    }
    "mailbox_sync_failed".into()
}

fn full_read_segment_error(
    stage: &str,
    error: crate::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        crate::CreatorRepositoryCallError::Owner(error) => error,
        crate::CreatorRepositoryCallError::Dispatch(error) => {
            format!("full mailbox sync {stage} repository read failed: {error}")
        }
    }
}

fn received_outcome(
    report: ReceivedSyncReport,
    state: Option<&crate::EmailSyncState>,
) -> MailboxDirectionOutcome {
    MailboxDirectionOutcome {
        pages_listed: report.pages_listed,
        summaries_listed: report.summaries_listed,
        details_fetched: report.details_fetched,
        skipped_complete: report.skipped_complete,
        imported: report.imported,
        reconciled_statuses: 0,
        merged_threads: 0,
        missing_provider_message_ids: report.missing_provider_message_ids,
        backfill_complete: report.backfill_complete,
        last_full_sync_at: state.and_then(|state| state.last_full_sync_at),
        failure: state.and_then(|state| state.failure.clone()),
    }
}

fn sent_outcome(
    report: SentSyncReport,
    state: Option<&crate::EmailSyncState>,
) -> MailboxDirectionOutcome {
    MailboxDirectionOutcome {
        pages_listed: report.pages_listed,
        summaries_listed: report.summaries_listed,
        details_fetched: report.details_fetched,
        skipped_complete: report.skipped_complete,
        imported: report.imported,
        reconciled_statuses: report.reconciled_statuses,
        merged_threads: report.merged_threads,
        missing_provider_message_ids: report.missing_provider_message_ids,
        backfill_complete: report.backfill_complete,
        last_full_sync_at: state.and_then(|state| state.last_full_sync_at),
        failure: state.and_then(|state| state.failure.clone()),
    }
}
