use onecrew_local_runtime::{
    CancelResultInput, ExecutedBatch, ExecutedResultInput, InitialResultState, LocalPlatform,
    PendingActionView, PendingUserAction, ResultBinding, ResultOrigin, ResultRef, ResultSnapshot,
    ResultState, ResultStoreError, ResultTerminalUpdate, ResultViewKind, SubmittedActionView,
};

use crate::operation_registry::{descriptor::SurfaceId, registration::OperationRegistry};

use super::{
    contract::{
        AskUserQuestionInput, ResultPresentInput, ResultPresentResult, ResultPresentStatus,
        ResultStateFailure, ResultStateInput, ResultStateOutput,
    },
    projection::{
        cancelled_continuation, choice_batch, choice_projection, localize_adapter_presentation,
        selected_choice_update, stranded_terminal_update,
    },
    resume::{
        ChoiceResumeEnvelope, ResumeCall, decode_choice_resume_envelope,
        encode_choice_resume_envelope,
    },
};

#[derive(Debug)]
pub(in crate::operation_registry) struct PresentedResultV2 {
    pub(in crate::operation_registry) model: ResultPresentResult,
    pub(in crate::operation_registry) snapshot: ResultSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::operation_registry) struct MintedChoiceResultV2 {
    pub(in crate::operation_registry) result_ref: ResultRef,
}

#[derive(Clone, Copy)]
enum SubmittedDispatchRisk {
    KnownNoEffect,
    DispatchMayHaveStarted,
}

pub(super) struct SubmittedTerminalGuard<'platform> {
    platform: &'platform LocalPlatform,
    binding: ResultBinding,
    prior: ResultSnapshot,
    completed_sections: Vec<onecrew_local_runtime::PresentationSection>,
    risk: SubmittedDispatchRisk,
    armed: bool,
}

impl<'platform> SubmittedTerminalGuard<'platform> {
    pub(super) fn new(
        platform: &'platform LocalPlatform,
        binding: ResultBinding,
        prior: ResultSnapshot,
    ) -> Self {
        Self {
            platform,
            binding,
            prior,
            completed_sections: Vec::new(),
            risk: SubmittedDispatchRisk::KnownNoEffect,
            armed: true,
        }
    }

    pub(super) fn mark_dispatch_may_have_started(&mut self) {
        self.risk = SubmittedDispatchRisk::DispatchMayHaveStarted;
    }

    pub(super) fn record_progress(&mut self, batch: ExecutedBatch) {
        self.prior.batch = batch;
        self.risk = SubmittedDispatchRisk::KnownNoEffect;
    }

    pub(super) fn record_completed_sections(
        &mut self,
        sections: Vec<onecrew_local_runtime::PresentationSection>,
    ) {
        self.completed_sections = sections;
    }

    fn retain_completed_sections(&self, update: &mut ResultTerminalUpdate) {
        if !update
            .presentation
            .sections
            .starts_with(&self.completed_sections)
        {
            let current = std::mem::take(&mut update.presentation.sections);
            update.presentation.sections = self.completed_sections.clone();
            update.presentation.sections.extend(current);
        }
    }

    pub(super) fn disarm(mut self) {
        self.armed = false;
    }

    pub(super) fn close_known(self, code: &str) -> Result<ResultSnapshot, ResultStateFailure> {
        let update = stranded_terminal_update(
            self.prior.batch.clone(),
            self.prior.presentation.kind.clone(),
            code,
            false,
            self.prior.locale,
        );
        self.commit(update)
    }

    pub(super) fn commit(
        mut self,
        mut update: ResultTerminalUpdate,
    ) -> Result<ResultSnapshot, ResultStateFailure> {
        self.retain_completed_sections(&mut update);
        match self
            .platform
            .transition_result_terminal(&self.binding, update)
        {
            Ok(snapshot) => {
                self.armed = false;
                Ok(snapshot)
            }
            Err(error) => Err(map_store_failure(error)),
        }
    }
}

impl Drop for SubmittedTerminalGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let (code, effect_unknown) = match self.risk {
            SubmittedDispatchRisk::KnownNoEffect => {
                ("confirmation_abandoned_before_dispatch", false)
            }
            SubmittedDispatchRisk::DispatchMayHaveStarted => {
                ("confirmation_abandoned_after_dispatch", true)
            }
        };
        // Drop cannot report; a result that could not be closed stays visible
        // to the next state read, so the failure is recorded here.
        let mut update = stranded_terminal_update(
            self.prior.batch.clone(),
            self.prior.presentation.kind.clone(),
            code,
            effect_unknown,
            self.prior.locale,
        );
        self.retain_completed_sections(&mut update);
        if let Err(error) = self
            .platform
            .transition_result_terminal(&self.binding, update)
        {
            log::warn!("abandoned confirmation could not be closed ({code}): {error}");
        }
    }
}

pub(in crate::operation_registry) fn result_present_v2(
    platform: &LocalPlatform,
    origin: &ResultOrigin,
    input: ResultPresentInput,
) -> Result<PresentedResultV2, ResultStateFailure> {
    let snapshot = platform
        .present_result_with_locale(&input.result_ref, origin, input.locale)
        .map(localize_adapter_presentation)
        .map_err(map_store_failure)?;
    Ok(PresentedResultV2 {
        model: ResultPresentResult {
            result_ref: input.result_ref,
            status: present_status(&snapshot.state),
            failure: match &snapshot.state {
                ResultState::Failed(failure) => Some(failure.clone()),
                ResultState::Unknown(unknown) if unknown.code == "presentation_unavailable" => {
                    Some(onecrew_local_runtime::ResultFailure {
                        code: unknown.code.clone(),
                        summary: unknown.summary.clone(),
                        retryable: false,
                    })
                }
                ResultState::Available
                | ResultState::Submitted { .. }
                | ResultState::Resolved(_)
                | ResultState::Unknown(_) => None,
            },
        },
        snapshot,
    })
}

pub(in crate::operation_registry) fn mint_choice_result_v2(
    platform: &LocalPlatform,
    origin: &ResultOrigin,
    call_id: crate::operation_registry::descriptor::EntryId,
    operation_id: crate::operation_registry::descriptor::EntryId,
    input: AskUserQuestionInput,
) -> Result<MintedChoiceResultV2, ResultStateFailure> {
    if !input.valid() || operation_id.as_str() != super::projection::ASK_USER_QUESTION_OPERATION_ID
    {
        return Err(ResultStateFailure::InvalidInput);
    }
    let payload = encode_choice_resume_envelope(
        call_id.clone(),
        operation_id.clone(),
        input.interaction_id.clone(),
        input.questions.clone(),
    )
    .map_err(|_| ResultStateFailure::InvalidInput)?;
    let result_ref = platform
        .mint_execution_result(ExecutedResultInput {
            origin: origin.clone(),
            view_kind: ResultViewKind::Choice,
            batch: choice_batch(call_id.as_str(), operation_id.as_str(), false),
            presentation: choice_projection(&input),
            pending_action: Some(PendingUserAction::Choice {
                questions: input.questions,
                payload,
            }),
            initial_state: InitialResultState::Available,
        })
        .map_err(map_store_failure)?;
    Ok(MintedChoiceResultV2 { result_ref })
}

impl OperationRegistry {
    pub(in crate::operation_registry) async fn result_state_v2(
        &self,
        platform: &std::sync::Arc<LocalPlatform>,
        surface: SurfaceId,
        input: ResultStateInput,
    ) -> Result<ResultStateOutput, ResultStateFailure> {
        if result_state_binding(&input).origin.as_str() != surface_origin_scope(surface) {
            return Err(ResultStateFailure::OriginMismatch);
        }
        if !matches!(&input, ResultStateInput::ReadCurrent { .. }) {
            let current = platform.read_current_result(result_state_binding(&input))
                .map_err(map_store_failure)?;
            if onecrew_local_runtime::is_retired_result(&current) {
                return Err(ResultStateFailure::InvalidState);
            }
        }
        match input {
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding,
                selections,
            } => {
                self.confirm_result(
                    platform,
                    surface,
                    binding,
                    onecrew_local_runtime::OwnerReviewSelection::CreatorSave { selections },
                )
                .await
            }
            ResultStateInput::ReviseSearchPlan { binding, patch } => platform
                .revise_search_plan_result(&binding, patch)
                .map(localize_snapshot_output)
                .map_err(map_store_failure),
            ResultStateInput::ReadCurrent { binding } => platform
                .read_current_result(&binding)
                .map(localize_snapshot_output)
                .map_err(map_store_failure),
            ResultStateInput::Confirm { binding } => {
                self.confirm_result(
                    platform,
                    surface,
                    binding,
                    onecrew_local_runtime::OwnerReviewSelection::Confirm,
                )
                .await
            }
            ResultStateInput::Select { binding, answers } => self
                .select_choice(platform, binding, answers)
                .map(localize_snapshot_output),
            ResultStateInput::Cancel { binding } => {
                // Presentation fixes the card's locale once, before any action.
                let locale = platform
                    .read_current_result(&binding)
                    .map_err(map_store_failure)?
                    .locale;
                platform
                    .cancel_result(CancelResultInput {
                        binding,
                        continuation: Some(cancelled_continuation(locale)),
                    })
                    .map(localize_snapshot_output)
                    .map_err(map_store_failure)
            }
            ResultStateInput::RequestEdit { input } => platform
                .request_result_edit(input)
                .map(localize_snapshot_output)
                .map_err(map_store_failure),
            ResultStateInput::ClaimContinuation { input } => platform
                .claim_result_continuation(input)
                .map(|claimed| ResultStateOutput::ClaimedContinuation { claimed })
                .map_err(map_store_failure),
            ResultStateInput::CompleteContinuation { input } => platform
                .complete_result_continuation(input)
                .map(|completed| ResultStateOutput::CompletedContinuation { completed })
                .map_err(map_store_failure),
        }
    }

    fn select_choice(
        &self,
        platform: &LocalPlatform,
        binding: ResultBinding,
        answers: Vec<onecrew_local_runtime::ResultChoiceAnswer>,
    ) -> Result<ResultSnapshot, ResultStateFailure> {
        let current = platform
            .read_current_result(&binding)
            .map_err(map_store_failure)?;
        if current.binding != binding {
            return Err(ResultStateFailure::RevisionConflict {
                current_revision: current.binding.revision,
            });
        }
        let (submitted_binding, action, owner_payload) = platform
            .select_result_options(&binding, answers)
            .map_err(map_store_failure)?
            .into_generic_parts()
            .map_err(map_store_failure)?;
        let SubmittedActionView::Selected { answers } = action else {
            return close_submitted_choice_failure(
                platform,
                &submitted_binding,
                &current,
                "choice_action_invalid",
            );
        };
        let envelope = match decode_choice_resume_envelope(owner_payload) {
            Ok(envelope) => envelope,
            Err(_) => {
                return close_submitted_choice_failure(
                    platform,
                    &submitted_binding,
                    &current,
                    "choice_envelope_invalid",
                );
            }
        };
        let ChoiceResumeEnvelope::V2 {
            call_id,
            operation_id,
            interaction_id: _,
            questions,
        } = envelope;
        if !choice_matches_snapshot(
            &current,
            call_id.as_str(),
            operation_id.as_str(),
            &questions,
            &answers,
        ) {
            return close_submitted_choice_failure(
                platform,
                &submitted_binding,
                &current,
                "choice_snapshot_mismatch",
            );
        }
        let update = selected_choice_update(
            call_id.as_str(),
            operation_id.as_str(),
            &current.presentation.title,
            &questions,
            &answers,
            current.locale,
        );
        match platform.transition_result_terminal(&submitted_binding, update) {
            Ok(snapshot) => Ok(snapshot),
            Err(_) => close_submitted_choice_failure(
                platform,
                &submitted_binding,
                &current,
                "choice_terminal_transition_failed",
            ),
        }
    }

    async fn confirm_result(
        &self,
        platform: &std::sync::Arc<LocalPlatform>,
        surface: SurfaceId,
        binding: ResultBinding,
        selection: onecrew_local_runtime::OwnerReviewSelection,
    ) -> Result<ResultStateOutput, ResultStateFailure> {
        let current = platform
            .read_current_result(&binding)
            .map_err(map_store_failure)?;
        if current.binding != binding {
            return Err(ResultStateFailure::RevisionConflict {
                current_revision: current.binding.revision,
            });
        }
        let submission = platform
            .submit_result_review(&binding, selection)
            .map_err(map_store_failure)?;
        super::batch_resume::resume(self, platform, surface, current, submission).await
    }
}

fn localize_snapshot_output(snapshot: ResultSnapshot) -> ResultStateOutput {
    ResultStateOutput::snapshot(localize_adapter_presentation(snapshot))
}

fn result_state_binding(input: &ResultStateInput) -> &ResultBinding {
    match input {
        ResultStateInput::ConfirmCreatorSaveBatch { binding, .. }
        | ResultStateInput::ReviseSearchPlan { binding, .. }
        | ResultStateInput::ReadCurrent { binding }
        | ResultStateInput::Confirm { binding }
        | ResultStateInput::Select { binding, .. }
        | ResultStateInput::Cancel { binding } => binding,
        ResultStateInput::RequestEdit { input } => &input.binding,
        ResultStateInput::ClaimContinuation { input } => &input.binding,
        ResultStateInput::CompleteContinuation { input } => &input.binding,
    }
}

pub(super) const fn surface_origin_scope(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Full => "mcp:full",
        SurfaceId::Compact => "mcp:compact",
        SurfaceId::Accounts => "mcp:accounts",
        SurfaceId::Creators => "mcp:creators",
        SurfaceId::Outreach => "mcp:outreach",
        SurfaceId::Mail => "mcp:mail",
        SurfaceId::Skills => "mcp:skills",
    }
}

pub(super) fn resume_matches_snapshot(batch: &ExecutedBatch, calls: &[ResumeCall]) -> bool {
    let Some(first) = calls.first() else {
        return false;
    };
    let Some(index) = batch
        .outcomes
        .iter()
        .position(|outcome| outcome.call_id == first.call.call_id.as_str())
    else {
        return false;
    };
    batch.outcomes[index..]
        .iter()
        .map(|outcome| outcome.call_id.as_str())
        .eq(calls.iter().map(|call| call.call.call_id.as_str()))
}

fn choice_matches_snapshot(
    snapshot: &ResultSnapshot,
    call_id: &str,
    operation_id: &str,
    questions: &[onecrew_local_runtime::ResultChoiceQuestion],
    answers: &[onecrew_local_runtime::ResultChoiceAnswer],
) -> bool {
    let Some(PendingActionView::Choice {
        questions: current_questions,
    }) = snapshot.pending_action.as_ref()
    else {
        return false;
    };
    let batch_matches = matches!(snapshot.batch.outcomes.as_slice(), [outcome]
        if outcome.call_id == call_id
            && outcome.operation_id == operation_id
            && operation_id == super::projection::ASK_USER_QUESTION_OPERATION_ID);
    batch_matches
        && current_questions == questions
        && questions.len() == answers.len()
        && questions
            .iter()
            .zip(answers)
            .all(|(question, answer)| answer.matches_question(question))
}

fn close_submitted_choice_failure(
    platform: &LocalPlatform,
    binding: &ResultBinding,
    snapshot: &ResultSnapshot,
    code: &str,
) -> Result<ResultSnapshot, ResultStateFailure> {
    let mut update = stranded_terminal_update(
        snapshot.batch.clone(),
        snapshot.presentation.kind.clone(),
        code,
        false,
        snapshot.locale,
    );
    update.continuation = None;
    platform
        .transition_result_terminal(binding, update)
        .map_err(map_store_failure)
}

fn present_status(state: &ResultState) -> ResultPresentStatus {
    match state {
        ResultState::Available => ResultPresentStatus::Available,
        ResultState::Submitted { .. } => ResultPresentStatus::Submitted,
        ResultState::Resolved(_) => ResultPresentStatus::Resolved,
        ResultState::Failed(_) => ResultPresentStatus::Failed,
        ResultState::Unknown(_) => ResultPresentStatus::Unknown,
    }
}

pub(super) fn map_store_failure(error: ResultStoreError) -> ResultStateFailure {
    match error {
        ResultStoreError::Absent => ResultStateFailure::Absent,
        ResultStoreError::Expired => ResultStateFailure::Expired,
        ResultStoreError::ViewMismatch => ResultStateFailure::ViewMismatch,
        ResultStoreError::OriginMismatch => ResultStateFailure::OriginMismatch,
        ResultStoreError::FingerprintMismatch => ResultStateFailure::FingerprintMismatch,
        ResultStoreError::RevisionConflict { current_revision } => {
            ResultStateFailure::RevisionConflict { current_revision }
        }
        ResultStoreError::ContinuationRevisionConflict { current_revision } => {
            ResultStateFailure::ContinuationRevisionConflict { current_revision }
        }
        ResultStoreError::ContinuationLeaseMismatch => {
            ResultStateFailure::ContinuationLeaseMismatch
        }
        ResultStoreError::InvalidInput("search_plan_payload_too_large") => {
            ResultStateFailure::SearchPlanPayloadTooLarge
        }
        ResultStoreError::InvalidInput(_) => ResultStateFailure::InvalidInput,
        ResultStoreError::InvalidState(_) => ResultStateFailure::InvalidState,
        ResultStoreError::Replay(_) => ResultStateFailure::Replay,
        ResultStoreError::Capacity(_) => ResultStateFailure::Capacity,
        ResultStoreError::Unavailable(_) => ResultStateFailure::Unavailable,
    }
}
