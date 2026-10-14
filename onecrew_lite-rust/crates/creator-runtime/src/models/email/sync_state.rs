use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{EmailDirection, MailboxRefId};

/// How an email fact entered the local Creator store. Provider presence proves the
/// email; only `ConfirmedSend` proves this computer performed the guarded
/// send workflow (confirmation, idempotency, readback, audit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailOrigin {
    ConfirmedSend,
    ProviderImport,
}

impl EmailOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConfirmedSend => "confirmed_send",
            Self::ProviderImport => "provider_import",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "confirmed_send" => Ok(Self::ConfirmedSend),
            "provider_import" => Ok(Self::ProviderImport),
            other => Err(format!("unknown email origin {other}")),
        }
    }
}

/// Durable attachment metadata carried by a message snapshot. Never a
/// download URL: provider access URLs expire and are fetched on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailAttachmentInput {
    pub provider_attachment_id: String,
    pub filename: String,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size_bytes: Option<u64>,
    pub available: bool,
}

/// Sanitized, resumable failure state for one mailbox direction. Persisted
/// as JSON in `mailbox_sync_state.failure_json`; carries provider ids and an
/// error class only — never secrets, URLs, or raw provider payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmailSyncFailure {
    pub missing_provider_message_ids: Vec<String>,
    pub error_class: String,
    pub message: String,
}

/// One mailbox direction's durable sync progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailSyncState {
    pub mailbox_ref_id: MailboxRefId,
    pub direction: EmailDirection,
    pub newest_provider_message_id: Option<String>,
    pub newest_occurred_at: Option<u64>,
    pub oldest_backfill_cursor: Option<String>,
    pub backfill_complete: bool,
    pub last_page_at: Option<u64>,
    pub last_full_sync_at: Option<u64>,
    pub failure: Option<EmailSyncFailure>,
    pub updated_at: u64,
}

/// Private engine state for an unfinished newest-first incremental walk.
/// This is deliberately separate from both the public sync status and the
/// historical `oldest_backfill_cursor`: Phase A must finish before Phase B
/// may resume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EmailSyncIncrementalContinuation {
    pub mailbox_ref_id: MailboxRefId,
    pub direction: EmailDirection,
    pub next_cursor: String,
    pub newest_provider_message_id: String,
    pub newest_occurred_at: u64,
    pub updated_at: u64,
}

/// Repository-internal checkpoint read returned through the existing sync
/// state port slot. Public only because it appears in that trait signature;
/// its fields and private continuation remain crate-owned engine state.
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct EmailSyncCheckpoint {
    pub(crate) state: Option<EmailSyncState>,
    pub(crate) incremental_continuation: Option<EmailSyncIncrementalContinuation>,
}

/// The Phase A continuation mutation earned by one successful page. Failed
/// pages preserve the prior row so a retry starts at the same page boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EmailSyncIncrementalContinuationMutation {
    #[default]
    Preserve,
    Set {
        next_cursor: String,
        newest_provider_message_id: String,
        newest_occurred_at: u64,
    },
    Clear,
}

/// The durable boundary movement a completed page has earned. Absent when a
/// page had failures: successful snapshots still commit once, but the
/// watermark and backfill cursor stay put so the next run retries only the
/// missing work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailSyncAdvance {
    pub newest_provider_message_id: Option<String>,
    pub newest_occurred_at: Option<u64>,
    pub oldest_backfill_cursor: Option<String>,
    pub backfill_complete: bool,
    /// True when this page completes a full reconciliation run, stamping
    /// `last_full_sync_at`.
    pub mark_full_sync: bool,
}

/// One atomic page commit: message snapshots (each carrying attachments and
/// origin), the direction's eligible progress, and the sanitized failure
/// state to persist for resume. Provider network work never happens inside
/// this commit.
#[derive(Debug, Clone)]
pub struct EmailSyncPageCommit {
    pub mailbox_ref_id: MailboxRefId,
    pub direction: EmailDirection,
    pub snapshots: Vec<super::recovery::EmailSnapshotInput>,
    pub advance: Option<EmailSyncAdvance>,
    pub failure: Option<EmailSyncFailure>,
    pub incremental_continuation: EmailSyncIncrementalContinuationMutation,
    pub sent_mutations: EmailSyncSentMutations,
}

#[derive(Debug, Clone, Default)]
pub struct EmailSyncSentMutations {
    pub status_updates: Vec<(String, String)>,
    pub thread_merges: Vec<EmailSyncThreadMerge>,
}

#[derive(Debug, Clone)]
pub struct EmailSyncThreadMerge {
    pub target: crate::EmailThreadId,
    pub sources: Vec<crate::EmailThreadId>,
}

/// What one page commit changed, for page-level publication decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailSyncPageOutcome {
    pub changed_thread_ids: Vec<crate::models::EmailThreadId>,
    /// True only when the durable checkpoint row was inserted or updated.
    /// An idempotent overlap page may carry an `advance` candidate without
    /// rewriting identical sync state.
    pub checkpoint_changed: bool,
    /// True only when a changed checkpoint committed an `advance`.
    pub advanced: bool,
    /// True only when the private Phase A continuation row changed.
    pub continuation_changed: bool,
    pub reconciled_statuses: u32,
    pub merged_threads: u32,
}

/// Outcome of one received-sync run. Internal until the Phase 7 surface
/// swap projects a secret-free status model over it.
#[derive(Debug, Clone, Default)]
pub struct ReceivedSyncReport {
    pub pages_listed: u32,
    pub summaries_listed: u32,
    pub details_fetched: u32,
    pub skipped_complete: u32,
    pub imported: u32,
    pub matched: u32,
    pub unmatched: u32,
    pub changed_thread_ids: Vec<crate::models::EmailThreadId>,
    pub missing_provider_message_ids: Vec<String>,
    pub backfill_complete: bool,
    pub full_sync_recorded: bool,
    pub pending_triage: Option<crate::PendingReplyTriageDebt>,
}

/// The one canonical public sync intent: a mailbox reference and an
/// explicit bounded mode. Provider cursors never cross a public boundary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SyncMailboxRequest {
    pub mailbox_ref_id: MailboxRefId,
    pub mode: EmailSyncRunMode,
}

/// Bounded run modes. `Incremental` is the canonical pull-only run: bounded
/// newest-first reconciliation plus automatic resumption of an unfinished
/// historical backfill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailSyncRunMode {
    Incremental,
}

/// Public projection of one direction's durable sync state. Deliberately
/// omits provider pagination positions (`newest_provider_message_id`,
/// `oldest_backfill_cursor`): those are private engine state, not status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmailSyncStateView {
    pub backfill_complete: bool,
    pub last_page_at: Option<u64>,
    pub last_full_sync_at: Option<u64>,
    pub failure: Option<EmailSyncFailure>,
}

/// One direction's share of a completed sync run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MailboxDirectionOutcome {
    pub pages_listed: u32,
    pub summaries_listed: u32,
    pub details_fetched: u32,
    pub skipped_complete: u32,
    pub imported: u32,
    /// Delivery statuses reconciled in place from sent list metadata;
    /// always zero for the inbound direction.
    pub reconciled_statuses: u32,
    /// RFC-proven thread merges performed by imported outbound roots;
    /// always zero for the inbound direction.
    pub merged_threads: u32,
    pub missing_provider_message_ids: Vec<String>,
    pub backfill_complete: bool,
    pub last_full_sync_at: Option<u64>,
    pub failure: Option<EmailSyncFailure>,
}

/// The result of one coalesced bidirectional sync run.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MailboxSyncOutcome {
    pub mailbox: super::mailboxes::MailboxDetail,
    pub inbound: MailboxDirectionOutcome,
    pub outbound: MailboxDirectionOutcome,
    pub changed_thread_ids: Vec<crate::models::EmailThreadId>,
    pub pending_triage: crate::PendingReplyTriageDebt,
}

/// Live counters of an in-flight run: the direction currently walking and
/// what this run has listed, fetched, and imported so far. Process-local by
/// construction — never durable state, never provider cursors — and reset
/// per direction, so the UI reads "which phase, how far".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MailboxSyncProgress {
    pub direction: EmailDirection,
    pub pages_listed: u32,
    pub summaries_listed: u32,
    pub details_fetched: u32,
    pub imported: u32,
    pub skipped_complete: u32,
    /// Estimated share (0–100) of the backfill window already walked. The
    /// provider exposes no total count, but the walk moves newest→oldest
    /// through a bounded retention window, so elapsed time-span is an
    /// honest determinate estimate. Absent when no page boundary is known.
    pub estimated_percent: Option<u8>,
}

/// Idle-time status: whether a run is in flight in this process plus the
/// durable per-direction facts. Process-only running state stays separate
/// from durable watermarks by construction.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MailboxSyncStatus {
    pub mailbox: super::mailboxes::MailboxDetail,
    pub running: bool,
    /// Present only while `running`: the in-flight run's live counters.
    pub progress: Option<MailboxSyncProgress>,
    pub inbound: Option<EmailSyncStateView>,
    pub outbound: Option<EmailSyncStateView>,
}

impl From<&EmailSyncState> for EmailSyncStateView {
    fn from(state: &EmailSyncState) -> Self {
        Self {
            backfill_complete: state.backfill_complete,
            last_page_at: state.last_page_at,
            last_full_sync_at: state.last_full_sync_at,
            failure: state.failure.clone(),
        }
    }
}

/// Outcome of one sent-history run. Internal until the Phase 7 surface swap
/// projects a secret-free status model over it.
#[derive(Debug, Clone, Default)]
pub struct SentSyncReport {
    pub pages_listed: u32,
    pub summaries_listed: u32,
    pub details_fetched: u32,
    pub skipped_complete: u32,
    pub imported: u32,
    pub reconciled_statuses: u32,
    pub merged_threads: u32,
    pub changed_thread_ids: Vec<crate::models::EmailThreadId>,
    pub missing_provider_message_ids: Vec<String>,
    pub backfill_complete: bool,
    pub full_sync_recorded: bool,
}
