//! Atomic review admission, consumption and sequential batch confirmation.

use super::super::{
    CallDisposition, ExecutedBatch, ExecutedResultInput, InitialResultState, OwnerReviewKind,
    OwnerReviewSelection, PeerResumePayload, PendingUserAction, PreparedOwnerReview, ResultBinding,
    ResultRef, ResultSnapshot, ResultState, ResultStoreError, SubmittedActionView,
    SubmittedOwnerGrant,
};
use super::{ResultMemoryStore, expire_target, require_user_action_available, validate_binding};

impl ResultMemoryStore {
    pub(in super::super) fn mint_owner_review(
        &self,
        review: PreparedOwnerReview,
        frame: ExecutedBatch,
        peer: Option<PeerResumePayload>,
        completed_sections: Vec<super::super::PresentationSection>,
    ) -> Result<ResultRef, ResultStoreError> {
        let mut input = review.into_input(frame, peer)?;
        let completed_sections_len = prepend_completed(&mut input.presentation, completed_sections);
        self.mint_with_completed_prefix(input, completed_sections_len)
    }

    pub(in super::super) fn completed_result_sections(
        &self,
        binding: &ResultBinding,
    ) -> Result<Vec<super::super::PresentationSection>, ResultStoreError> {
        let mut map = self.lock()?;
        let now = std::time::Instant::now();
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, binding)?;
        if !record.presented {
            return Err(ResultStoreError::InvalidState("result_not_presented"));
        }
        record.last_accessed_at = now;
        Ok(record.presentation.sections[..record.completed_sections_len].to_vec())
    }

    pub(in super::super) fn submit_owner_review(
        &self,
        binding: &ResultBinding,
        selection: OwnerReviewSelection,
    ) -> Result<SubmittedOwnerGrant, ResultStoreError> {
        match self.submit_review(binding, selection, true)? {
            super::super::ResultConfirmationSubmission::Owner(grant) => Ok(grant),
            super::super::ResultConfirmationSubmission::Generic(_) => {
                unreachable!("owner-only submission")
            }
        }
    }

    pub(in super::super) fn submit_result_review(
        &self,
        binding: &ResultBinding,
        selection: OwnerReviewSelection,
    ) -> Result<super::super::ResultConfirmationSubmission, ResultStoreError> {
        self.submit_review(binding, selection, false)
    }

    fn submit_review(
        &self,
        binding: &ResultBinding,
        selection: OwnerReviewSelection,
        require_owner: bool,
    ) -> Result<super::super::ResultConfirmationSubmission, ResultStoreError> {
        self.transition(binding, |record, _| {
            // Admission can wait for the mutex; check the monotonic deadline
            // after acquiring it, not at a timestamp sampled before waiting.
            let now = std::time::Instant::now();
            require_user_action_available(record)?;
            let pending = record
                .pending_action
                .as_ref()
                .ok_or(ResultStoreError::InvalidState("owner_review_required"))?;
            if pending.payload().owner_review_kind().is_none() {
                if require_owner {
                    return Err(ResultStoreError::InvalidState("owner_review_required"));
                }
                if !matches!(selection, OwnerReviewSelection::Confirm)
                    || !matches!(
                        pending,
                        PendingUserAction::Confirm { .. } | PendingUserAction::SearchPlan { .. }
                    )
                {
                    return Err(ResultStoreError::InvalidState("confirmation_required"));
                }
                let revision = record
                    .revision
                    .checked_add(1)
                    .ok_or(ResultStoreError::Unavailable("result_revision"))?;
                let payload = match record
                    .pending_action
                    .take()
                    .expect("validated generic pending")
                {
                    PendingUserAction::Confirm { payload, .. }
                    | PendingUserAction::SearchPlan { payload, .. } => payload,
                    _ => unreachable!("validated generic confirmation"),
                };
                let action = SubmittedActionView::Confirmed {};
                record.state = ResultState::Submitted {
                    action: action.clone(),
                };
                record.revision = revision;
                record.last_accessed_at = now;
                record.state_changed_at = now;
                return Ok(super::super::ResultConfirmationSubmission::Generic(
                    super::super::SubmittedOwnerAction {
                        binding: record.binding(),
                        action,
                        owner_payload: payload,
                    },
                ));
            }
            let context = pending
                .payload()
                .owner_review_context()
                .ok_or(ResultStoreError::InvalidState("owner_review_required"))?
                .clone();
            if pending
                .payload()
                .prepared_deadline()
                .is_none_or(|deadline| now >= deadline)
            {
                return Err(ResultStoreError::Expired);
            }
            // Compute every fallible value before changing the record. A bad
            // selection or exhausted revision must leave authority available.
            let selected = match (pending, selection) {
                (
                    PendingUserAction::CreatorSaveBatch { payload, .. },
                    OwnerReviewSelection::CreatorSave { selections },
                ) => Some(payload.select_creator_save(&selections)?),
                (PendingUserAction::Confirm { payload, .. }, OwnerReviewSelection::Confirm)
                    if matches!(
                        payload.owner_review_kind(),
                        Some(
                            OwnerReviewKind::Outreach
                                | OwnerReviewKind::Email
                                | OwnerReviewKind::CreatorBatch
                        )
                    ) =>
                {
                    None
                }
                _ => {
                    return Err(ResultStoreError::InvalidState(
                        "owner_review_selection_mismatch",
                    ));
                }
            };
            let revision = record
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("result_revision"))?;
            let pending = record
                .pending_action
                .take()
                .expect("validated pending review");
            let payload = match (pending, selected) {
                (PendingUserAction::CreatorSaveBatch { payload, .. }, Some(selected)) => {
                    payload.with_selected_creator_save(selected)
                }
                (PendingUserAction::Confirm { payload, .. }, None) => payload,
                _ => unreachable!("validated owner selection"),
            };
            record.state = ResultState::Submitted {
                action: SubmittedActionView::Confirmed {},
            };
            record.submitted_owner_context = Some(context);
            record.revision = revision;
            record.last_accessed_at = now;
            record.state_changed_at = now;
            Ok(super::super::ResultConfirmationSubmission::Owner(
                SubmittedOwnerGrant::new(record.binding(), payload, record.locale),
            ))
        })
    }

    pub(in super::super) fn advance_owner_review(
        &self,
        binding: &ResultBinding,
        review: PreparedOwnerReview,
        frame: ExecutedBatch,
        peer: Option<PeerResumePayload>,
        completed_sections: Vec<super::super::PresentationSection>,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let candidate = review.into_input(frame, peer)?;
        self.advance_confirmation(binding, candidate, completed_sections)
    }

    pub(in super::super) fn advance_result_confirmation(
        &self,
        binding: &ResultBinding,
        input: ExecutedResultInput,
        completed_sections: Vec<super::super::PresentationSection>,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        if !matches!(input.initial_state, InitialResultState::Available)
            || input.view_kind != super::super::ResultViewKind::Confirmation
            || !matches!(&input.pending_action, Some(PendingUserAction::Confirm { payload, .. })
                if payload.owner_review_kind().is_none())
        {
            return Err(ResultStoreError::InvalidInput(
                "generic_confirmation_required",
            ));
        }
        self.advance_confirmation(binding, input, completed_sections)
    }

    fn advance_confirmation(
        &self,
        binding: &ResultBinding,
        mut candidate: ExecutedResultInput,
        completed_sections: Vec<super::super::PresentationSection>,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        super::super::validation::validate_execution_frame(
            &candidate,
            super::super::MAX_OWNER_FRAME_RESOURCE_REFS,
        )?;
        let mut map = self.lock()?;
        let now = std::time::Instant::now();
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, binding)?;
        if !record.presented || !matches!(record.state, ResultState::Submitted { .. }) {
            return Err(ResultStoreError::InvalidState("advance_requires_submitted"));
        }
        if candidate.origin != record.origin || candidate.view_kind != record.view.kind {
            return Err(ResultStoreError::InvalidInput(
                "owner_review_context_mismatch",
            ));
        }
        if completed_sections.len() < record.completed_sections_len
            || completed_sections[..record.completed_sections_len]
                != record.presentation.sections[..record.completed_sections_len]
        {
            return Err(ResultStoreError::InvalidInput(
                "completed_sections_mismatch",
            ));
        }
        super::super::owner_review::localize_owner_review(
            &mut candidate.presentation,
            candidate.pending_action.as_mut(),
            record.locale,
        )?;
        let completed_sections_len =
            prepend_completed(&mut candidate.presentation, completed_sections);
        super::super::validation::validate_completed_append(
            &candidate.presentation,
            record.completed_sections_len,
            completed_sections_len,
            record.submitted_owner_context.is_some(),
        )?;
        super::super::validation::validate_execution_frame_with_prefix(
            &candidate,
            super::super::MAX_OWNER_FRAME_RESOURCE_REFS,
            completed_sections_len,
        )?;
        let bytes = super::super::validation::result_payload_bytes(&candidate)?;
        let owner_frame = record.submitted_owner_context.is_some()
            || candidate
                .pending_action
                .as_ref()
                .is_some_and(|action| action.payload().owner_review_context().is_some())
            || record
                .batch
                .outcomes
                .iter()
                .any(|call| call.resource_refs.len() > super::super::MAX_RESOURCE_REFS);
        if !owner_frame
            && candidate
                .batch
                .outcomes
                .iter()
                .any(|call| call.resource_refs.len() > super::super::MAX_RESOURCE_REFS)
        {
            return Err(ResultStoreError::InvalidInput("resource_refs"));
        }
        let payload = candidate
            .pending_action
            .as_ref()
            .expect("prepared review")
            .payload();
        validate_advanced_frame(
            &record.batch,
            &candidate.batch,
            record.submitted_owner_context.as_ref(),
            payload.owner_review_context(),
        )?;
        if payload
            .prepared_deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            return Err(ResultStoreError::Expired);
        }
        let total = map
            .total_payload_bytes
            .checked_sub(record.payload_bytes)
            .and_then(|bytes_without_prior| bytes_without_prior.checked_add(bytes))
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if total > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }
        let revision = record
            .revision
            .checked_add(1)
            .ok_or(ResultStoreError::Unavailable("result_revision"))?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        record.batch = candidate.batch;
        record.presentation = candidate.presentation;
        record.completed_sections_len = completed_sections_len;
        record.pending_action = candidate.pending_action;
        record.submitted_owner_context = None;
        record.state = ResultState::Available;
        record.revision = revision;
        record.payload_bytes = bytes;
        record.last_accessed_at = now;
        record.state_changed_at = now;
        // Mint identity, locale, presented flag and continuation stay unchanged.
        // No terminal continuation is published until the whole batch stops.
        let snapshot = record.snapshot();
        map.total_payload_bytes = total;
        Ok(snapshot)
    }
}

fn prepend_completed(
    presentation: &mut super::super::PresentationProjection,
    mut completed_sections: Vec<super::super::PresentationSection>,
) -> usize {
    let count = completed_sections.len();
    completed_sections.append(&mut presentation.sections);
    presentation.sections = completed_sections;
    count
}

/// An advance cannot discard an executed prefix or substitute a different call.
/// Pending call integrity beyond this protocol-neutral frame belongs to the
/// peer's exact call digest and opaque continuation checks before dispatch.
fn validate_advanced_frame(
    prior: &ExecutedBatch,
    next: &ExecutedBatch,
    submitted: Option<&super::super::OwnerReviewContext>,
    pending: Option<&super::super::OwnerReviewContext>,
) -> Result<(), ResultStoreError> {
    if prior.outcomes.len() != next.outcomes.len() {
        return Err(ResultStoreError::InvalidInput(
            "owner_review_frame_mismatch",
        ));
    }
    for (prior, next) in prior.outcomes.iter().zip(&next.outcomes) {
        if prior.call_id != next.call_id
            || prior.operation_id != next.operation_id
            || (!matches!(prior.disposition, CallDisposition::NotRun { .. }) && prior != next)
        {
            return Err(ResultStoreError::InvalidInput(
                "owner_review_frame_mismatch",
            ));
        }
    }
    let pending_index = next
        .outcomes
        .iter()
        .position(|call| {
            pending.map_or_else(
                || matches!(call.disposition, CallDisposition::NotRun { .. }),
                |pending| call.call_id == pending.call_id(),
            )
        })
        .ok_or(ResultStoreError::InvalidInput("owner_review_call_missing"))?;
    let submitted_index = prior
        .outcomes
        .iter()
        .position(|call| {
            submitted.map_or_else(
                || matches!(call.disposition, CallDisposition::NotRun { .. }),
                |submitted| call.call_id == submitted.call_id(),
            )
        })
        .ok_or(ResultStoreError::InvalidInput(
            "submitted_owner_call_missing",
        ))?;
    if pending_index <= submitted_index {
        return Err(ResultStoreError::InvalidInput(
            "owner_review_did_not_advance",
        ));
    }
    if next.outcomes[..pending_index]
        .iter()
        .any(|call| !matches!(call.disposition, CallDisposition::Succeeded))
        || next.outcomes[pending_index..]
            .iter()
            .any(|call| !matches!(call.disposition, CallDisposition::NotRun { .. }))
    {
        return Err(ResultStoreError::InvalidInput(
            "owner_review_frame_progress",
        ));
    }
    Ok(())
}
