//! Sealed business reviews and their single-use submitted authority.
//!
//! A peer may retain its bounded continuation alongside the business intent;
//! only that peer interprets its protocol. Neither presentation nor opaque
//! bytes can construct authority to execute a Creator save or outreach write.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    CreatorSaveBatchSelection, ExecutedBatch, ExecutedResultInput, InitialResultState,
    OwnerActionPayload, PendingUserAction, PresentationProjection, ResultBinding, ResultOrigin,
    ResultStoreError, ResultViewKind,
};

/// Digest of the exact peer call, including its contract and arguments.
/// Constructed by the trusted adapter, never deserialized from operation input.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnerCallDigest(String);

impl OwnerCallDigest {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(bytes)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Immutable server binding context. Protocol adapters construct this only
/// after validating and decoding the exact registered invocation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OwnerReviewContext {
    origin: ResultOrigin,
    call_id: String,
    operation_id: String,
    call_digest: Option<OwnerCallDigest>,
}

impl OwnerReviewContext {
    pub fn new(
        origin: ResultOrigin,
        call_id: impl Into<String>,
        operation_id: impl Into<String>,
        call_digest: Option<OwnerCallDigest>,
    ) -> Result<Self, ResultStoreError> {
        let call_id = call_id.into();
        let operation_id = operation_id.into();
        super::validation::validate_identifier(&call_id, 128, "call_id")?;
        super::validation::validate_identifier(&operation_id, 128, "operation_id")?;
        Ok(Self {
            origin,
            call_id,
            operation_id,
            call_digest,
        })
    }

    pub fn origin(&self) -> &ResultOrigin {
        &self.origin
    }
    pub fn call_id(&self) -> &str {
        &self.call_id
    }
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub fn call_digest(&self) -> Option<&OwnerCallDigest> {
        self.call_digest.as_ref()
    }

    pub(super) fn validate_frame(&self, frame: &ExecutedBatch) -> Result<(), ResultStoreError> {
        let Some(call) = frame
            .outcomes
            .iter()
            .find(|call| call.call_id == self.call_id)
        else {
            return Err(ResultStoreError::InvalidInput("owner_review_call_missing"));
        };
        if call.operation_id != self.operation_id
            || !matches!(call.disposition, super::CallDisposition::NotRun { .. })
        {
            return Err(ResultStoreError::InvalidInput("owner_review_call_mismatch"));
        }
        Ok(())
    }
}

/// Opaque peer continuation with a separate bounded budget. Debug never prints
/// arguments or message bodies carried by that continuation.
#[derive(PartialEq, Eq)]
pub struct PeerResumePayload(Vec<u8>);

impl PeerResumePayload {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, ResultStoreError> {
        let bytes = bytes.into();
        if bytes.is_empty() || bytes.len() > super::MAX_PEER_RESUME_BYTES {
            return Err(ResultStoreError::InvalidInput("peer_resume_payload"));
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl std::fmt::Debug for PeerResumePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerResumePayload")
            .field("bytes", &self.0.len())
            .finish()
    }
}

/// An owner-created review, before it occupies a result-store row.
/// Its private fields prevent adapters from replacing the reviewed intent or
/// presentation while attaching their own execution frame.
pub struct PreparedOwnerReview {
    context: OwnerReviewContext,
    presentation: PresentationProjection,
    action: PreparedOwnerReviewAction,
}

enum PreparedOwnerReviewAction {
    Review(Box<PendingUserAction>),
    Noop(Box<PreparedOwnerNoop>),
}

impl PreparedOwnerReviewAction {
    fn review(&self) -> Option<&PendingUserAction> {
        match self {
            Self::Review(action) => Some(action),
            Self::Noop(_) => None,
        }
    }
}

/// A typed owner result that completed without a user action.  It is used for
/// deterministic no-op outcomes such as an add-new request whose every row
/// already exists; no confirmation authority is created for that result.
#[derive(Debug)]
pub struct PreparedOwnerNoop {
    output: Value,
    presentation: PresentationProjection,
    terminal: super::TerminalResult,
    outcome: super::ExecutedCallOutcome,
    continuation: Option<super::ResultContinuationRequest>,
    review_summary: Option<super::CreatorSaveReviewSummary>,
}

impl PreparedOwnerNoop {
    pub(crate) fn new(
        output: Value,
        presentation: PresentationProjection,
        terminal: super::TerminalResult,
        outcome: super::ExecutedCallOutcome,
        continuation: Option<super::ResultContinuationRequest>,
        review_summary: Option<super::CreatorSaveReviewSummary>,
    ) -> Self {
        Self {
            output,
            presentation,
            terminal,
            outcome,
            continuation,
            review_summary,
        }
    }

    pub fn output(&self) -> &Value {
        &self.output
    }

    pub fn presentation(&self) -> &PresentationProjection {
        &self.presentation
    }

    pub fn terminal(&self) -> &super::TerminalResult {
        &self.terminal
    }

    pub fn outcome(&self) -> &super::ExecutedCallOutcome {
        &self.outcome
    }

    pub fn continuation(&self) -> Option<&super::ResultContinuationRequest> {
        self.continuation.as_ref()
    }

    pub fn review_summary(&self) -> Option<&super::CreatorSaveReviewSummary> {
        self.review_summary.as_ref()
    }
}

impl std::fmt::Debug for PreparedOwnerReview {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedOwnerReview")
            .field("context", &self.context)
            .field(
                "kind",
                &self
                    .action
                    .review()
                    .map(|pending| pending.payload().owner_review_kind()),
            )
            .finish_non_exhaustive()
    }
}

impl PreparedOwnerReview {
    /// Whether this owner completed deterministically without a user action.
    pub fn is_noop(&self) -> bool {
        matches!(&self.action, PreparedOwnerReviewAction::Noop(_))
    }

    pub(crate) fn creator_save(
        action: crate::creator_save::PreparedSaveAction,
        presentation: PresentationProjection,
        context: OwnerReviewContext,
    ) -> Result<Self, ResultStoreError> {
        if action.context.origin != context.origin
            || action.context.call_id != context.call_id
            || action.context.operation_id != context.operation_id
        {
            return Err(ResultStoreError::InvalidInput(
                "creator_save_context_mismatch",
            ));
        }
        let review = Box::new(action.review.clone());
        Ok(Self {
            context,
            presentation,
            action: PreparedOwnerReviewAction::Review(Box::new(
                PendingUserAction::CreatorSaveBatch {
                    review,
                    payload: OwnerActionPayload::from_prepared_creator_save(action)?,
                },
            )),
        })
    }

    /// Seal a typed terminal result without creating confirmation authority.
    pub(crate) fn noop(
        context: OwnerReviewContext,
        result: PreparedOwnerNoop,
    ) -> Result<Self, ResultStoreError> {
        if result.outcome.call_id != context.call_id
            || result.outcome.operation_id != context.operation_id
        {
            return Err(ResultStoreError::InvalidInput(
                "owner_noop_context_mismatch",
            ));
        }
        Ok(Self {
            context,
            presentation: result.presentation.clone(),
            action: PreparedOwnerReviewAction::Noop(Box::new(result)),
        })
    }

    /// Consume the no-op result for the adapter's terminal path. A normal
    /// review cannot be converted to a terminal result.
    pub fn into_noop(self) -> Result<PreparedOwnerNoop, ResultStoreError> {
        match self.action {
            PreparedOwnerReviewAction::Noop(result) => Ok(*result),
            PreparedOwnerReviewAction::Review(_) => {
                Err(ResultStoreError::InvalidState("owner_review_noop_required"))
            }
        }
    }

    pub(crate) fn outreach(
        action: crate::outreach::PreparedOutreachAction,
        context: OwnerReviewContext,
        presentation: PresentationProjection,
        prompt: String,
        label: String,
    ) -> Result<Self, ResultStoreError> {
        Ok(Self {
            context,
            presentation,
            action: PreparedOwnerReviewAction::Review(Box::new(PendingUserAction::Confirm {
                prompt,
                label,
                payload: OwnerActionPayload::from_prepared_outreach(action)?,
            })),
        })
    }

    /// Seal one email send behind a plain confirmation. The reviewed intent is
    /// the payload itself, so resuming the call cannot substitute arguments.
    pub(crate) fn email(
        action: crate::email::PreparedEmailAction,
        context: OwnerReviewContext,
        presentation: PresentationProjection,
        prompt: String,
        label: String,
    ) -> Result<Self, ResultStoreError> {
        Ok(Self {
            context,
            presentation,
            action: PreparedOwnerReviewAction::Review(Box::new(PendingUserAction::Confirm {
                prompt,
                label,
                payload: OwnerActionPayload::from_prepared_email(action)?,
            })),
        })
    }

    /// Seal one manual Creator batch behind a plain confirmation. The reviewed
    /// intent is the payload itself, so resuming the call cannot substitute the
    /// items, the validation mode, or the idempotency key.
    pub(crate) fn creator_batch(
        action: crate::creator_batch::PreparedCreatorBatchAction,
        context: OwnerReviewContext,
        presentation: PresentationProjection,
        prompt: String,
        label: String,
    ) -> Result<Self, ResultStoreError> {
        Ok(Self {
            context,
            presentation,
            action: PreparedOwnerReviewAction::Review(Box::new(PendingUserAction::Confirm {
                prompt,
                label,
                payload: OwnerActionPayload::from_prepared_creator_batch(action)?,
            })),
        })
    }

    pub fn context(&self) -> &OwnerReviewContext {
        &self.context
    }

    /// Project only the bounded status facts an Agent needs before confirmation.
    /// Contact values and internal identity evidence stay in the held review.
    pub fn creator_save_review_summary(&self) -> Option<super::CreatorSaveReviewSummary> {
        let Some(PendingUserAction::CreatorSaveBatch { review, .. }) = self.action.review() else {
            return None;
        };
        Some(super::CreatorSaveReviewSummary {
            summary: review.summary,
            rows: review
                .rows
                .iter()
                .map(|row| super::CreatorSaveReviewRowSummary {
                    row_id: row.row_id.clone(),
                    status: row.preview.status,
                    reason: row.preview.reason,
                })
                .collect(),
            failures: review
                .failures
                .iter()
                .map(|failure| super::CreatorSaveReviewFailureSummary {
                    row_id: failure.row_id.clone(),
                    reason: failure.reason,
                })
                .collect(),
        })
    }

    pub(super) fn into_input(
        self,
        frame: ExecutedBatch,
        peer: Option<PeerResumePayload>,
    ) -> Result<ExecutedResultInput, ResultStoreError> {
        self.context.validate_frame(&frame)?;
        if self.context.call_digest.is_some() != peer.is_some() {
            return Err(ResultStoreError::InvalidInput("owner_review_peer_context"));
        }
        if peer.is_none() && frame.outcomes.len() != 1 {
            return Err(ResultStoreError::InvalidInput("owner_review_peer_required"));
        }
        let Self {
            context,
            presentation,
            action,
        } = self;
        let PreparedOwnerReviewAction::Review(mut pending) = action else {
            return Err(ResultStoreError::InvalidState(
                "owner_review_action_required",
            ));
        };
        let origin = context.origin.clone();
        pending.payload_mut().bind_owner_review(context, peer)?;
        Ok(ExecutedResultInput {
            origin,
            view_kind: ResultViewKind::Confirmation,
            batch: frame,
            presentation,
            pending_action: Some(*pending),
            initial_state: InitialResultState::Available,
        })
    }
}

/// Only choices explicitly supported by a prepared review are accepted.
#[derive(Debug)]
pub enum OwnerReviewSelection {
    Confirm,
    CreatorSave {
        selections: Vec<CreatorSaveBatchSelection>,
    },
}

/// Closed business authority category, independent of protocol operation names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerReviewKind {
    CreatorSave,
    Outreach,
    Email,
    CreatorBatch,
}

/// One atomic confirmation consumes either generic continuation or sealed
/// business authority. Peers route on this type, never an error string.
#[derive(Debug)]
pub enum ResultConfirmationSubmission {
    Owner(SubmittedOwnerGrant),
    Generic(super::SubmittedOwnerAction),
}

/// Safe per-call output from a runtime owner. It is not deserializable and no
/// write API accepts it as a claim that a provider effect has happened.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
pub struct OwnerCallCompletion {
    pub output: OwnerCallOutput,
    pub continue_batch: bool,
    pub outcome: super::ExecutedCallOutcome,
    pub presentation: PresentationProjection,
    pub terminal: super::TerminalResult,
    pub continuation: Option<super::ResultContinuationRequest>,
}

/// The save operation's final safe result. Complete Creator records remain in
/// Creator-owned reads; this output reports only the reviewed rows and receipts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveCallResult {
    #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub rows: Vec<super::CreatorSaveBatchOutcome>,
    pub summary: super::CreatorSaveResultSummary,
}

/// Closed output variants allow exact registered final-schema validation
/// without accepting caller-authored receipts or erasing business status.
#[derive(Clone, Debug, Serialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum OwnerCallOutput {
    CreatorSave(CreatorSaveCallResult),
    Outreach(Box<crate::OutreachCompletion>),
    Email(Box<crate::EmailCallOutput>),
    CreatorBatch(Box<onecrew_creator_runtime::ManualCreatorBatchCreateResult>),
}

impl OwnerCallCompletion {
    /// A direct typed peer completes a singleton result; a batch peer merges
    /// the same outcome into its retained frame before its own final transition.
    pub fn into_singleton_terminal(self) -> super::ResultTerminalUpdate {
        super::ResultTerminalUpdate {
            batch: ExecutedBatch {
                outcomes: vec![self.outcome],
            },
            presentation: self.presentation,
            terminal: self.terminal,
            continuation: self.continuation,
        }
    }
}

/// Result-store-minted authority. Deliberately neither Clone nor Deserialize;
/// consuming it executes only the exact stored intent, never new wire inputs.
#[derive(Debug)]
pub struct SubmittedOwnerGrant {
    binding: ResultBinding,
    payload: OwnerActionPayload,
    locale: crate::PresentationLocale,
}

impl SubmittedOwnerGrant {
    pub(super) fn new(
        binding: ResultBinding,
        payload: OwnerActionPayload,
        locale: crate::PresentationLocale,
    ) -> Self {
        Self {
            binding,
            payload,
            locale,
        }
    }

    pub fn binding(&self) -> &ResultBinding {
        &self.binding
    }
    /// The presentation locale frozen before submission, without re-reading or
    /// cloning the potentially large retained history to recover this scalar.
    pub fn locale(&self) -> crate::PresentationLocale {
        self.locale
    }
    pub fn kind(&self) -> OwnerReviewKind {
        self.payload
            .owner_review_kind()
            .expect("store validated owner review authority")
    }
    pub fn context(&self) -> &OwnerReviewContext {
        self.payload
            .owner_review_context()
            .expect("store validated owner review context")
    }
    pub fn peer_resume(&self) -> Option<&PeerResumePayload> {
        self.payload.peer_resume()
    }

    pub(crate) fn into_outreach(
        self,
    ) -> Result<crate::outreach::PreparedOutreachAction, ResultStoreError> {
        self.payload.into_prepared_outreach()
    }

    pub(crate) fn into_email(self) -> Result<crate::email::PreparedEmailAction, ResultStoreError> {
        self.payload.into_prepared_email()
    }

    pub(crate) fn into_creator_save(
        self,
    ) -> Result<crate::creator_save::SelectedSaveAction, ResultStoreError> {
        self.payload.into_selected_creator_save()
    }

    pub(crate) fn into_creator_batch(
        self,
    ) -> Result<crate::creator_batch::PreparedCreatorBatchAction, ResultStoreError> {
        self.payload.into_prepared_creator_batch()
    }
}

impl PendingUserAction {
    pub(super) fn payload(&self) -> &OwnerActionPayload {
        match self {
            Self::CreatorSaveBatch { payload, .. }
            | Self::SearchPlan { payload, .. }
            | Self::Confirm { payload, .. }
            | Self::Choice { payload, .. } => payload,
        }
    }

    pub(super) fn payload_mut(&mut self) -> &mut OwnerActionPayload {
        match self {
            Self::CreatorSaveBatch { payload, .. }
            | Self::SearchPlan { payload, .. }
            | Self::Confirm { payload, .. }
            | Self::Choice { payload, .. } => payload,
        }
    }
}

pub(super) fn localize_owner_review(
    presentation: &mut PresentationProjection,
    pending: Option<&mut PendingUserAction>,
    locale: crate::PresentationLocale,
) -> Result<(), ResultStoreError> {
    if let Some(PendingUserAction::Confirm {
        payload,
        prompt,
        label,
    }) = pending
        && payload.owner_review_kind() == Some(OwnerReviewKind::Outreach)
    {
        localize_outreach_copy(presentation, prompt, label, locale)?;
    }
    Ok(())
}

pub(super) fn localize_outreach_copy(
    presentation: &mut PresentationProjection,
    prompt: &mut String,
    label: &mut String,
    locale: crate::PresentationLocale,
) -> Result<(), ResultStoreError> {
    crate::outreach::localize_review(presentation, locale)?;
    let (localized_prompt, localized_label) = crate::outreach::review_confirmation_text(locale);
    *prompt = localized_prompt.into();
    *label = localized_label.into();
    Ok(())
}

#[cfg(test)]
mod tests;
