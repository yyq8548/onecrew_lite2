//! Process-local owner for governed execution results and their user actions.
//!
//! The store owns only process-local execution results and their single pending
//! user action. Restart and eviction expire that authority; callers must mint a
//! new result rather than reconstructing or migrating an old interaction.

use std::time::Duration;

mod api;
mod continuation;
mod identity;
mod memory;
mod models;
mod owner_review;
#[cfg(test)]
mod tests;
mod validation;

pub use continuation::{
    CancelResultInput, ClaimResultContinuationInput, ClaimedResultContinuation,
    CompleteResultContinuationInput, CompletedResultContinuation,
    RESULT_CONTINUATION_DISPATCH_LEASE_MS, RequestResultEditInput, ResultContinuationClaimMode,
    ResultContinuationCompletionOutcome, ResultContinuationContext,
    ResultContinuationCreatorSaveOutcome, ResultContinuationOutcomeSummary,
    ResultContinuationRequest, ResultContinuationSnapshot, ResultContinuationState,
};
pub use identity::{
    ResultBinding, ResultContinuationLeaseId, ResultFingerprint, ResultOrigin,
    ResultPresentationKind, ResultRef, ResultViewBinding, ResultViewIdentity, ResultViewKind,
};
pub(crate) use memory::ResultMemoryStore;
pub use models::{
    CallDisposition, CreatorSaveBatchOperation, CreatorSaveBatchOutcome, CreatorSaveBatchReason,
    CreatorSaveBatchReview, CreatorSaveBatchRow, CreatorSaveBatchRowStatus,
    CreatorSaveBatchSelection, CreatorSaveCollision, CreatorSaveCollisionStatus,
    CreatorSavePreparedFailure, CreatorSavePreview, CreatorSaveResultSummary,
    CreatorSaveReviewFailureSummary, CreatorSaveReviewRowSummary, CreatorSaveReviewSummary,
    CreatorSaveRoute, ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput, InitialResultState,
    NoticeTone, OwnerActionPayload, PendingActionView, PendingUserAction, PresentationField,
    PresentationItem, PresentationProjection, PresentationSection, ResultChoiceAnswer,
    ResultChoiceOption, ResultChoiceQuestion, ResultChoiceSelectionMode, ResultCleanupMetrics,
    ResultFailure, ResultResolution, ResultResourceKind, ResultResourceRef, ResultSnapshot,
    ResultState, ResultStoreError, ResultTerminalUpdate, ResultUnknown, SubmittedActionView,
    SubmittedOwnerAction, TerminalResult,
};
pub use owner_review::{
    CreatorSaveCallResult, OwnerCallCompletion, OwnerCallDigest, OwnerCallOutput,
    OwnerReviewContext, OwnerReviewKind, OwnerReviewSelection, PeerResumePayload,
    PreparedOwnerNoop, PreparedOwnerReview, ResultConfirmationSubmission, SubmittedOwnerGrant,
};

// Generic execution-result limits are separate from Creator save's shared
// selection batch policy, which validates each prepared review and outcome.
const MAX_BATCH_OUTCOMES: usize = 1000;
const MAX_PROJECTION_BYTES: usize = 64 * 1024;
pub(super) const MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES: usize = 4 * 1024 * 1024;
const MAX_PROJECTION_SECTIONS: usize = 32;
const MAX_CREATOR_SAVE_BATCH_SECTIONS: usize =
    onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS + MAX_PROJECTION_SECTIONS;
const MAX_OWNER_BATCH_PROJECTION_BYTES: usize = 8 * MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES;
const MAX_OWNER_BATCH_SECTIONS: usize = 8 * MAX_CREATOR_SAVE_BATCH_SECTIONS;
const MAX_SECTION_ITEMS: usize = 128;
const MAX_RESOURCE_REFS: usize = 64;
// An owner frame's refs follow the save batch constant, because N reviewed rows
// yield at most N saved Creators. They never drop below the generic budget: the
// same frame also carries ordinary calls, and those keep their own 64-ref limit
// whether or not a Creator save shares the batch.
pub(super) const MAX_OWNER_FRAME_RESOURCE_REFS: usize =
    if onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS > MAX_RESOURCE_REFS {
        onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS
    } else {
        MAX_RESOURCE_REFS
    };
const MAX_STRING_BYTES: usize = 8 * 1024;
// A projection owner bounds its own text and previews against the exact limits
// validation enforces here. Emitting a value the store will reject costs the
// whole card and every provider read behind it, not the offending field.
pub(crate) const MAX_PRESENTATION_TEXT_BYTES: usize = MAX_STRING_BYTES;
pub(crate) const MAX_PRESENTATION_PREVIEW_BYTES: usize = MAX_PROJECTION_BYTES;
pub(crate) const MAX_PRESENTATION_PREVIEW_ROUTES: usize = 128;
const MAX_ACTION_QUESTIONS: usize = 32;
const MAX_ACTION_OPTIONS: usize = 32;
const MAX_OWNER_ACTION_BYTES: usize = 16 * 1024;
const MAX_OUTREACH_OWNER_ACTION_BYTES: usize = 64 * 1024;
// An email review seals the whole request it will send, and a batch request
// carries every rendered item. Its budget must hold that, not one message.
const MAX_EMAIL_OWNER_ACTION_BYTES: usize = 1024 * 1024;
// A manual Creator batch review seals the whole request it will write. This
// follows the Creator owner's own payload bound for those items, plus room for
// the key and validation mode sealed beside them. It is not the email budget it
// happens to resemble in magnitude: that one follows a rendered mail batch, and
// the two move for unrelated reasons.
const MAX_CREATOR_BATCH_OWNER_ACTION_BYTES: usize =
    onecrew_creator_runtime::MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES + 64 * 1024;
// The held authority contains canonical input plus its review and exact route
// observations. Its budget must include those representations for a full batch.
pub(super) const MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES: usize =
    onecrew_creator_runtime::MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES
        + 2 * MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES;
const MAX_ORIGIN_SCOPE_BYTES: usize = 128;
const MAX_PRESENTATION_KIND_BYTES: usize = 128;
const DEFAULT_MAX_RESULTS: usize = 10_000;
const DEFAULT_MAX_TOTAL_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
// An owner action that reaches its own legal ceiling must still be admissible.
// Without this, a batch could seal authority the store would then refuse to
// hold, failing only for the largest real batches.
const _: () = assert!(MAX_CREATOR_SAVE_BATCH_OWNER_ACTION_BYTES < DEFAULT_MAX_TOTAL_PAYLOAD_BYTES);
// A complete peer envelope may hold eight calls, each selecting 50 Creators.
// Candidate notes/provenance already make their theoretical bound larger than
// the store. Use that real shared budget, rather than a second smaller batch
// restriction; review, history and peer bytes still compete for the same total.
const MAX_PEER_RESUME_BYTES: usize = DEFAULT_MAX_TOTAL_PAYLOAD_BYTES;
const AVAILABLE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const TERMINAL_TTL: Duration = Duration::from_secs(72 * 60 * 60);

/// Secret-free projection that terminal cards render after the pending action
/// is gone. Its field values are business data only (the acceptance criteria);
/// the editable draft, keywords included, is carried as a typed pending action.
pub fn search_plan_projection(
    draft: &crate::SearchPlanEditorDraft,
) -> Result<PresentationProjection, ResultStoreError> {
    Ok(PresentationProjection {
        kind: ResultPresentationKind::new("search_plan_editor")?,
        title: "Review search plan".to_owned(),
        summary: Some(
            "Edit acceptance criteria and keywords, then confirm before searching.".to_owned(),
        ),
        sections: vec![PresentationSection::Fields {
            heading: Some("Search plan".to_owned()),
            fields: vec![PresentationField {
                label: "Acceptance criteria".to_owned(),
                value: draft.description.clone(),
            }],
        }],
    })
}
