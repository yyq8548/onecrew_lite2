mod cancel;
mod continuation;
mod edit;
mod expiry;
mod lifecycle;
mod owner_review;
mod search_plan;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use super::validation::{
    fingerprint_result, result_payload_bytes, stored_terminal_payload_bytes,
    terminal_update_payload_bytes, validate_cancel_result_input, validate_choice_answers,
    validate_continuation_for_action, validate_executed_result, validate_terminal_update,
};
use super::{
    AVAILABLE_TTL, CancelResultInput, DEFAULT_MAX_RESULTS, DEFAULT_MAX_TOTAL_PAYLOAD_BYTES,
    ExecutedBatch, ExecutedResultInput, InitialResultState, PendingActionView, PendingUserAction,
    PresentationProjection, ResultBinding, ResultChoiceAnswer, ResultCleanupMetrics,
    ResultFingerprint, ResultOrigin, ResultRef, ResultSnapshot, ResultState, ResultStoreError,
    ResultTerminalUpdate, ResultViewBinding, ResultViewIdentity, SubmittedActionView,
    SubmittedOwnerAction, TERMINAL_TTL, TerminalResult,
};
use continuation::{ContinuationRecord, recover_expired_continuation};
use lifecycle::{
    cleanup_locked, ensure_capacity, expire_target, require_user_action_available,
    validate_binding, validate_identity,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct StoreLimits {
    pub(super) max_results: usize,
    pub(super) max_total_payload_bytes: usize,
    pub(super) available_ttl: Duration,
    pub(super) terminal_ttl: Duration,
}

impl Default for StoreLimits {
    fn default() -> Self {
        Self {
            max_results: DEFAULT_MAX_RESULTS,
            max_total_payload_bytes: DEFAULT_MAX_TOTAL_PAYLOAD_BYTES,
            available_ttl: AVAILABLE_TTL,
            terminal_ttl: TERMINAL_TTL,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ResultMemoryStore {
    inner: Arc<Mutex<ResultMap>>,
    limits: StoreLimits,
}

#[derive(Default)]
struct ResultMap {
    records: HashMap<ResultRef, ResultRecord>,
    total_payload_bytes: usize,
}

struct ResultRecord {
    locale: crate::PresentationLocale,
    result_ref: ResultRef,
    view: ResultViewBinding,
    origin: ResultOrigin,
    fingerprint: ResultFingerprint,
    revision: u64,
    presented: bool,
    batch: ExecutedBatch,
    presentation: PresentationProjection,
    completed_sections_len: usize,
    pending_action: Option<PendingUserAction>,
    submitted_owner_context: Option<super::OwnerReviewContext>,
    state: ResultState,
    continuation: ContinuationRecord,
    last_accessed_at: Instant,
    state_changed_at: Instant,
    payload_bytes: usize,
}

impl ResultMemoryStore {
    /// Offline tests advance only the prepared authority's clock; production
    /// mint/submit deadline checks remain unchanged and still reject expiry.
    #[cfg(feature = "test-support")]
    pub(crate) fn expire_creator_save_fixture(
        &self,
        reference: &ResultRef,
    ) -> Result<(), ResultStoreError> {
        let mut map = self.lock()?;
        let record = map
            .records
            .get_mut(reference)
            .ok_or(ResultStoreError::Absent)?;
        record
            .pending_action
            .as_mut()
            .ok_or(ResultStoreError::InvalidState(
                "creator_save_review_required",
            ))?
            .payload_mut()
            .expire_creator_save_fixture()
    }
    #[cfg(test)]
    pub(in super::super) fn test_binding(&self, reference: &ResultRef) -> ResultBinding {
        self.lock().unwrap().records[reference].binding()
    }

    pub(crate) fn new() -> Self {
        Self::with_limits(StoreLimits::default())
    }

    pub(super) fn with_limits(limits: StoreLimits) -> Self {
        Self {
            inner: Arc::new(Mutex::new(ResultMap::default())),
            limits,
        }
    }

    /// The only minting entry point: callers must provide a completed execute result.
    pub(super) fn mint_from_execute(
        &self,
        input: ExecutedResultInput,
    ) -> Result<ResultRef, ResultStoreError> {
        self.mint_with_completed_prefix(input, 0)
    }

    fn mint_with_completed_prefix(
        &self,
        input: ExecutedResultInput,
        completed_sections_len: usize,
    ) -> Result<ResultRef, ResultStoreError> {
        if completed_sections_len == 0 {
            validate_executed_result(&input)?;
        } else {
            super::validation::validate_initial_completed_sections(
                &input.presentation,
                completed_sections_len,
            )?;
            super::validation::validate_execution_frame_with_prefix(
                &input,
                super::MAX_OWNER_FRAME_RESOURCE_REFS,
                completed_sections_len,
            )?;
        }
        let result_ref = ResultRef::mint();
        let view = ResultViewBinding {
            identity: ResultViewIdentity::mint(),
            kind: input.view_kind,
        };
        let fingerprint = fingerprint_result(&result_ref, &view, &input)?;
        let now = Instant::now();
        let state = initial_state(input.initial_state.clone());
        let payload_bytes = result_payload_bytes(&input)?;
        let record = ResultRecord {
            locale: crate::PresentationLocale::default(),
            result_ref: result_ref.clone(),
            view,
            origin: input.origin,
            fingerprint,
            revision: 1,
            presented: false,
            batch: input.batch,
            presentation: input.presentation,
            completed_sections_len,
            pending_action: input.pending_action,
            submitted_owner_context: None,
            state,
            continuation: ContinuationRecord::new(),
            last_accessed_at: now,
            state_changed_at: now,
            payload_bytes,
        };

        let mut map = self.lock()?;
        let now = Instant::now();
        if record.pending_action.as_ref().is_some_and(|action| {
            action
                .payload()
                .prepared_deadline()
                .is_some_and(|deadline| now >= deadline)
        }) {
            return Err(ResultStoreError::Expired);
        }
        cleanup_locked(&mut map, self.limits, now)?;
        ensure_capacity(&map, self.limits, payload_bytes)?;
        if map.records.contains_key(&result_ref) {
            return Err(ResultStoreError::Unavailable("result_ref_collision"));
        }
        map.total_payload_bytes = map
            .total_payload_bytes
            .checked_add(payload_bytes)
            .ok_or(ResultStoreError::Capacity("result_payload_bytes"))?;
        map.records.insert(result_ref.clone(), record);
        Ok(result_ref)
    }

    #[cfg(test)]
    pub(super) fn present(
        &self,
        result_ref: &ResultRef,
        origin: &ResultOrigin,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.present_with_locale(result_ref, origin, crate::PresentationLocale::default())
    }

    pub(super) fn present_with_locale(
        &self,
        result_ref: &ResultRef,
        origin: &ResultOrigin,
        locale: crate::PresentationLocale,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let now = Instant::now();
        let mut map = self.lock()?;
        expire_target(&mut map, result_ref, self.limits, now)?;
        let record = map
            .records
            .get(result_ref)
            .ok_or(ResultStoreError::Absent)?;
        if record.origin != *origin {
            return Err(ResultStoreError::OriginMismatch);
        }
        if record.presented {
            return Err(ResultStoreError::Replay("result_already_presented"));
        }
        // Translate only the current canonical Outreach review. Completed
        // sections already carry their frozen locale and are never copied or
        // translated again; the immutable authority itself is untouched.
        let localized = if let Some(PendingUserAction::Confirm {
            payload,
            prompt,
            label,
        }) = &record.pending_action
            && payload.owner_review_kind() == Some(super::OwnerReviewKind::Outreach)
        {
            let mut current = PresentationProjection {
                kind: record.presentation.kind.clone(),
                title: record.presentation.title.clone(),
                summary: record.presentation.summary.clone(),
                sections: record.presentation.sections[record.completed_sections_len..].to_vec(),
            };
            let old_bytes = serde_json::to_vec(&(&current, prompt, label))
                .map_err(|_| ResultStoreError::Unavailable("projection_encode"))?
                .len();
            let (mut prompt, mut label) = (prompt.clone(), label.clone());
            super::owner_review::localize_outreach_copy(
                &mut current,
                &mut prompt,
                &mut label,
                locale,
            )?;
            super::validation::validate_projection(&current)?;
            let new_bytes = serde_json::to_vec(&(&current, &prompt, &label))
                .map_err(|_| ResultStoreError::Unavailable("projection_encode"))?
                .len();
            let bytes = record
                .payload_bytes
                .checked_sub(old_bytes)
                .and_then(|bytes| bytes.checked_add(new_bytes))
                .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
            Some((current, prompt, label, bytes))
        } else {
            None
        };
        let payload_bytes = localized
            .as_ref()
            .map_or(record.payload_bytes, |(_, _, _, bytes)| *bytes);
        let total = map
            .total_payload_bytes
            .checked_sub(record.payload_bytes)
            .and_then(|total| total.checked_add(payload_bytes))
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if total > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }
        let record = map
            .records
            .get_mut(result_ref)
            .ok_or(ResultStoreError::Absent)?;
        record.bump_revision(now)?;
        if let Some((mut current, translated_prompt, translated_label, _)) = localized {
            record
                .presentation
                .sections
                .truncate(record.completed_sections_len);
            record.presentation.sections.append(&mut current.sections);
            record.presentation.title = current.title;
            record.presentation.summary = current.summary;
            if let Some(PendingUserAction::Confirm { prompt, label, .. }) =
                &mut record.pending_action
            {
                *prompt = translated_prompt;
                *label = translated_label;
            }
        }
        record.payload_bytes = payload_bytes;
        record.locale = locale;
        record.presented = true;
        let snapshot = record.snapshot();
        map.total_payload_bytes = total;
        Ok(snapshot)
    }

    pub(super) fn read(&self, binding: &ResultBinding) -> Result<ResultSnapshot, ResultStoreError> {
        self.inspect(binding, false)
    }

    pub(super) fn current(
        &self,
        binding: &ResultBinding,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.current_at(binding, Instant::now())
    }

    pub(super) fn current_at(
        &self,
        binding: &ResultBinding,
        now: Instant,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let mut map = self.lock()?;
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_identity(record, binding)?;
        if binding.revision > record.revision {
            return Err(ResultStoreError::RevisionConflict {
                current_revision: record.revision,
            });
        }
        if !record.presented {
            return Err(ResultStoreError::InvalidState("result_not_presented"));
        }
        recover_expired_continuation(record, now)?;
        record.last_accessed_at = now;
        Ok(record.snapshot())
    }

    pub(super) fn submit(
        &self,
        binding: &ResultBinding,
    ) -> Result<SubmittedOwnerAction, ResultStoreError> {
        self.transition(binding, |record, now| {
            require_user_action_available(record)?;
            if record
                .pending_action
                .as_ref()
                .is_some_and(|action| action.payload().owner_review_context().is_some())
            {
                return Err(ResultStoreError::InvalidState(
                    "typed_owner_review_required",
                ));
            }
            let pending = record
                .pending_action
                .take()
                .ok_or(ResultStoreError::InvalidState("confirmation_required"))?;
            let payload = match pending {
                PendingUserAction::Confirm { payload, .. }
                | PendingUserAction::SearchPlan { payload, .. } => payload,
                other => {
                    record.pending_action = Some(other);
                    return Err(ResultStoreError::InvalidState("confirmation_required"));
                }
            };
            let action = SubmittedActionView::Confirmed {};
            record.state = ResultState::Submitted {
                action: action.clone(),
            };
            record.bump_revision(now)?;
            Ok(SubmittedOwnerAction {
                binding: record.binding(),
                action,
                owner_payload: payload,
            })
        })
    }

    pub(super) fn select(
        &self,
        binding: &ResultBinding,
        answers: Vec<ResultChoiceAnswer>,
    ) -> Result<SubmittedOwnerAction, ResultStoreError> {
        self.transition(binding, |record, now| {
            require_user_action_available(record)?;
            let ordered_answers = match &record.pending_action {
                Some(PendingUserAction::Choice { questions, .. }) => {
                    validate_choice_answers(questions, &answers)?
                }
                _ => return Err(ResultStoreError::InvalidState("choice_required")),
            };
            let pending = record
                .pending_action
                .take()
                .ok_or(ResultStoreError::InvalidState("choice_required"))?;
            let payload = match pending {
                PendingUserAction::Choice { payload, .. } => payload,
                other => {
                    record.pending_action = Some(other);
                    return Err(ResultStoreError::InvalidState("choice_required"));
                }
            };
            let action = SubmittedActionView::Selected {
                answers: ordered_answers,
            };
            record.state = ResultState::Submitted {
                action: action.clone(),
            };
            record.bump_revision(now)?;
            Ok(SubmittedOwnerAction {
                binding: record.binding(),
                action,
                owner_payload: payload,
            })
        })
    }

    pub(super) fn cancel(
        &self,
        input: CancelResultInput,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        validate_cancel_result_input(&input)?;
        let mut map = self.lock()?;
        let now = Instant::now();
        expire_target(&mut map, &input.binding.result_ref, self.limits, now)?;
        let (projection, previous_payload_bytes, final_payload_bytes, next_revision) = {
            let record = map
                .records
                .get(&input.binding.result_ref)
                .ok_or(ResultStoreError::Absent)?;
            validate_binding(record, &input.binding)?;
            require_user_action_available(record)?;
            let projection = cancel::project(record, input.continuation)?;
            let final_payload_bytes = stored_terminal_payload_bytes(
                &projection.batch,
                &record.presentation,
                &projection.state,
                projection.continuation.as_ref(),
            )?;
            let next_revision = record
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("result_revision"))?;
            (
                projection,
                record.payload_bytes,
                final_payload_bytes,
                next_revision,
            )
        };
        let next_total_payload_bytes = map
            .total_payload_bytes
            .checked_sub(previous_payload_bytes)
            .and_then(|retained| retained.checked_add(final_payload_bytes))
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if next_total_payload_bytes > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }

        let snapshot = {
            let record = map
                .records
                .get_mut(&input.binding.result_ref)
                .ok_or(ResultStoreError::Unavailable("result_record"))?;
            record
                .continuation
                .apply_terminal_request(projection.continuation)?;
            record.pending_action = None;
            record.batch = projection.batch;
            record.state = projection.state;
            record.payload_bytes = final_payload_bytes;
            record.revision = next_revision;
            record.last_accessed_at = now;
            record.state_changed_at = now;
            record.snapshot()
        };
        map.total_payload_bytes = next_total_payload_bytes;
        Ok(snapshot)
    }

    pub(super) fn terminal(
        &self,
        binding: &ResultBinding,
        update: ResultTerminalUpdate,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        super::validation::validate_terminal_frame_with_prefix(
            &update,
            super::MAX_OWNER_FRAME_RESOURCE_REFS,
            true,
        )?;
        let final_payload_bytes = terminal_update_payload_bytes(&update)?;
        let now = Instant::now();
        let mut map = self.lock()?;
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let (previous_payload_bytes, next_revision) = {
            let record = map
                .records
                .get(&binding.result_ref)
                .ok_or(ResultStoreError::Absent)?;
            validate_binding(record, binding)?;
            let ResultState::Submitted { action } = &record.state else {
                return Err(ResultStoreError::InvalidState(
                    "terminal_requires_submitted",
                ));
            };
            if update
                .presentation
                .sections
                .get(..record.completed_sections_len)
                != Some(&record.presentation.sections[..record.completed_sections_len])
            {
                return Err(ResultStoreError::InvalidInput(
                    "completed_sections_mismatch",
                ));
            }
            let owner_frame = record.submitted_owner_context.is_some()
                || record
                    .batch
                    .outcomes
                    .iter()
                    .any(|call| call.resource_refs.len() > super::MAX_RESOURCE_REFS);
            if record.completed_sections_len > 0 || record.submitted_owner_context.is_some() {
                super::validation::validate_terminal_append(
                    &update.presentation,
                    record.completed_sections_len,
                    record.submitted_owner_context.is_some(),
                )?;
            } else if !owner_frame {
                validate_terminal_update(&update)?;
            }
            validate_continuation_for_action(action, update.continuation.as_ref())?;
            let next_revision = record
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("result_revision"))?;
            (record.payload_bytes, next_revision)
        };
        let retained_payload_bytes = map
            .total_payload_bytes
            .checked_sub(previous_payload_bytes)
            .ok_or(ResultStoreError::Unavailable("result_payload_counter"))?;
        let next_total_payload_bytes = retained_payload_bytes
            .checked_add(final_payload_bytes)
            .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
        if next_total_payload_bytes > self.limits.max_total_payload_bytes {
            return Err(ResultStoreError::Capacity("result_payload_limit"));
        }

        let snapshot = {
            let record = map
                .records
                .get_mut(&binding.result_ref)
                .ok_or(ResultStoreError::Unavailable("result_record"))?;
            record
                .continuation
                .apply_terminal_request(update.continuation)?;
            record.batch = update.batch;
            record.presentation = update.presentation;
            record.completed_sections_len = record.presentation.sections.len();
            record.pending_action = None;
            record.submitted_owner_context = None;
            record.state = match update.terminal {
                TerminalResult::Resolved(resolution) => ResultState::Resolved(resolution),
                TerminalResult::Failed(failure) => ResultState::Failed(failure),
                TerminalResult::Unknown(unknown) => ResultState::Unknown(unknown),
            };
            record.payload_bytes = final_payload_bytes;
            record.revision = next_revision;
            record.last_accessed_at = now;
            record.state_changed_at = now;
            record.snapshot()
        };
        map.total_payload_bytes = next_total_payload_bytes;
        Ok(snapshot)
    }

    pub(super) fn cleanup(&self) -> Result<ResultCleanupMetrics, ResultStoreError> {
        self.cleanup_at(Instant::now())
    }

    pub(super) fn cleanup_at(
        &self,
        now: Instant,
    ) -> Result<ResultCleanupMetrics, ResultStoreError> {
        let mut map = self.lock()?;
        let removed = cleanup_locked(&mut map, self.limits, now)?;
        Ok(ResultCleanupMetrics {
            removed,
            active: map.records.len(),
            payload_bytes: map.total_payload_bytes,
        })
    }

    fn inspect(
        &self,
        binding: &ResultBinding,
        require_presented: bool,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        let mut map = self.lock()?;
        let now = Instant::now();
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, binding)?;
        if require_presented && !record.presented {
            return Err(ResultStoreError::InvalidState("result_not_presented"));
        }
        recover_expired_continuation(record, now)?;
        record.last_accessed_at = now;
        Ok(record.snapshot())
    }

    fn transition<T>(
        &self,
        binding: &ResultBinding,
        transition: impl FnOnce(&mut ResultRecord, Instant) -> Result<T, ResultStoreError>,
    ) -> Result<T, ResultStoreError> {
        let mut map = self.lock()?;
        let now = Instant::now();
        expire_target(&mut map, &binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get_mut(&binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, binding)?;
        transition(record, now)
    }

    fn lock(&self) -> Result<MutexGuard<'_, ResultMap>, ResultStoreError> {
        self.inner
            .lock()
            .map_err(|_| ResultStoreError::Unavailable("result_lock"))
    }
}

impl ResultRecord {
    fn binding(&self) -> ResultBinding {
        ResultBinding {
            result_ref: self.result_ref.clone(),
            view: self.view.clone(),
            origin: self.origin.clone(),
            fingerprint: self.fingerprint.clone(),
            revision: self.revision,
        }
    }

    fn snapshot(&self) -> ResultSnapshot {
        debug_assert!(self.completed_sections_len <= self.presentation.sections.len());
        ResultSnapshot {
            locale: self.locale,
            binding: self.binding(),
            presented: self.presented,
            batch: self.batch.clone(),
            presentation: self.presentation.clone(),
            completed_sections_count: self.completed_sections_len,
            pending_action: if matches!(self.state, ResultState::Available) {
                self.pending_action.as_ref().map(PendingActionView::from)
            } else {
                None
            },
            state: self.state.clone(),
            continuation: self.continuation.snapshot(),
        }
    }

    fn bump_revision(&mut self, now: Instant) -> Result<(), ResultStoreError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ResultStoreError::Unavailable("result_revision"))?;
        self.last_accessed_at = now;
        self.state_changed_at = now;
        Ok(())
    }
}

impl From<&PendingUserAction> for PendingActionView {
    fn from(action: &PendingUserAction) -> Self {
        match action {
            PendingUserAction::CreatorSaveBatch { review, .. } => Self::CreatorSaveBatch {
                review: review.clone(),
            },
            PendingUserAction::SearchPlan { draft, payload } => Self::SearchPlan {
                draft: draft.clone(),
                draft_digest: payload.digest(),
            },
            PendingUserAction::Confirm { prompt, label, .. } => Self::Confirm {
                prompt: prompt.clone(),
                label: label.clone(),
            },
            PendingUserAction::Choice { questions, .. } => Self::Choice {
                questions: questions.clone(),
            },
        }
    }
}

fn initial_state(initial: InitialResultState) -> ResultState {
    match initial {
        InitialResultState::Available => ResultState::Available,
        InitialResultState::Resolved(resolution) => ResultState::Resolved(resolution),
        InitialResultState::Failed(failure) => ResultState::Failed(failure),
        InitialResultState::Unknown(unknown) => ResultState::Unknown(unknown),
    }
}
