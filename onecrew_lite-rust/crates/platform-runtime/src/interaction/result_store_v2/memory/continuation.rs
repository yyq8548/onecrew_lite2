use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::validation::stored_terminal_payload_bytes;
use super::super::{
    ClaimResultContinuationInput, ClaimedResultContinuation, CompleteResultContinuationInput,
    CompletedResultContinuation, RESULT_CONTINUATION_DISPATCH_LEASE_MS,
    ResultContinuationClaimMode, ResultContinuationCompletionOutcome, ResultContinuationLeaseId,
    ResultContinuationRequest, ResultContinuationSnapshot, ResultContinuationState, ResultState,
    ResultStoreError,
};
use super::{ResultMemoryStore, ResultRecord, expire_target, validate_binding};

pub(super) struct ContinuationRecord {
    pub(super) state: ResultContinuationState,
    pub(super) revision: u64,
    pub(super) request: Option<ResultContinuationRequest>,
    lease_id: Option<ResultContinuationLeaseId>,
    dispatch_started_at_ms: Option<u64>,
    dispatch_not_after_ms: Option<u64>,
    dispatch_not_after: Option<Instant>,
}

impl ContinuationRecord {
    pub(super) fn new() -> Self {
        Self {
            state: ResultContinuationState::NotRequested,
            revision: 1,
            request: None,
            lease_id: None,
            dispatch_started_at_ms: None,
            dispatch_not_after_ms: None,
            dispatch_not_after: None,
        }
    }

    pub(super) fn snapshot(&self) -> ResultContinuationSnapshot {
        ResultContinuationSnapshot {
            state: self.state,
            continuation_revision: self.revision,
            dispatch_started_at_ms: self.dispatch_started_at_ms,
            dispatch_not_after_ms: self.dispatch_not_after_ms,
        }
    }

    pub(super) fn apply_terminal_request(
        &mut self,
        request: Option<ResultContinuationRequest>,
    ) -> Result<(), ResultStoreError> {
        if request.is_some() {
            self.revision = self
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("continuation_revision"))?;
            self.state = ResultContinuationState::Pending;
        } else {
            self.state = ResultContinuationState::NotRequested;
        }
        self.request = request;
        self.lease_id = None;
        self.dispatch_started_at_ms = None;
        self.dispatch_not_after_ms = None;
        self.dispatch_not_after = None;
        Ok(())
    }
}

impl ResultMemoryStore {
    pub(in crate::interaction::result_store_v2) fn claim_continuation(
        &self,
        input: ClaimResultContinuationInput,
    ) -> Result<ClaimedResultContinuation, ResultStoreError> {
        if input.expected_continuation_revision == 0 {
            return Err(ResultStoreError::InvalidInput("continuation_revision"));
        }
        let now = Instant::now();
        let started_at_ms = epoch_ms()?;
        let dispatch_not_after = now
            .checked_add(Duration::from_millis(RESULT_CONTINUATION_DISPATCH_LEASE_MS))
            .ok_or(ResultStoreError::Unavailable("continuation_deadline"))?;
        let dispatch_not_after_ms = started_at_ms
            .checked_add(RESULT_CONTINUATION_DISPATCH_LEASE_MS)
            .ok_or(ResultStoreError::Unavailable("continuation_deadline"))?;
        let mut map = self.lock()?;
        expire_target(&mut map, &input.binding.result_ref, self.limits, now)?;
        let record = map
            .records
            .get_mut(&input.binding.result_ref)
            .ok_or(ResultStoreError::Absent)?;
        validate_binding(record, &input.binding)?;
        require_terminal(record)?;
        recover_expired_continuation(record, now)?;
        validate_continuation_revision(record, input.expected_continuation_revision)?;
        match (record.continuation.state, input.mode) {
            (ResultContinuationState::Pending, ResultContinuationClaimMode::Pending)
            | (ResultContinuationState::Paused, ResultContinuationClaimMode::RetryPaused) => {}
            (ResultContinuationState::Paused, _) => {
                return Err(ResultStoreError::InvalidState(
                    "continuation_retry_requires_explicit_mode",
                ));
            }
            _ => {
                return Err(ResultStoreError::InvalidState("continuation_not_claimable"));
            }
        }
        let request = record
            .continuation
            .request
            .clone()
            .ok_or(ResultStoreError::Unavailable("continuation_request"))?;
        let next_revision = record
            .continuation
            .revision
            .checked_add(1)
            .ok_or(ResultStoreError::Unavailable("continuation_revision"))?;
        let lease_id = ResultContinuationLeaseId::mint();
        record.continuation.state = ResultContinuationState::Dispatching;
        record.continuation.revision = next_revision;
        record.continuation.lease_id = Some(lease_id.clone());
        record.continuation.dispatch_started_at_ms = Some(started_at_ms);
        record.continuation.dispatch_not_after_ms = Some(dispatch_not_after_ms);
        record.continuation.dispatch_not_after = Some(dispatch_not_after);
        Ok(ClaimedResultContinuation {
            binding: record.binding(),
            continuation_revision: next_revision,
            lease_id,
            request,
            dispatch_started_at_ms: started_at_ms,
            dispatch_not_after_ms,
        })
    }

    pub(in crate::interaction::result_store_v2) fn complete_continuation(
        &self,
        input: CompleteResultContinuationInput,
    ) -> Result<CompletedResultContinuation, ResultStoreError> {
        if input.expected_continuation_revision == 0 {
            return Err(ResultStoreError::InvalidInput("continuation_revision"));
        }
        let now = Instant::now();
        let mut map = self.lock()?;
        expire_target(&mut map, &input.binding.result_ref, self.limits, now)?;
        let (previous_payload_bytes, delivered_payload_bytes) = {
            let record = map
                .records
                .get_mut(&input.binding.result_ref)
                .ok_or(ResultStoreError::Absent)?;
            validate_binding(record, &input.binding)?;
            require_terminal(record)?;
            recover_expired_continuation(record, now)?;
            validate_continuation_revision(record, input.expected_continuation_revision)?;
            if record.continuation.state != ResultContinuationState::Dispatching {
                return Err(ResultStoreError::InvalidState(
                    "continuation_not_dispatching",
                ));
            }
            if record.continuation.lease_id.as_ref() != Some(&input.lease_id) {
                return Err(ResultStoreError::ContinuationLeaseMismatch);
            }
            let delivered_payload_bytes = if matches!(
                input.outcome,
                ResultContinuationCompletionOutcome::Delivered
            ) {
                Some(stored_terminal_payload_bytes(
                    &record.batch,
                    &record.presentation,
                    &record.state,
                    None,
                )?)
            } else {
                None
            };
            (record.payload_bytes, delivered_payload_bytes)
        };

        let next_total_payload_bytes =
            if let Some(delivered_payload_bytes) = delivered_payload_bytes {
                map.total_payload_bytes
                    .checked_sub(previous_payload_bytes)
                    .and_then(|retained| retained.checked_add(delivered_payload_bytes))
                    .ok_or(ResultStoreError::Unavailable("result_payload_counter"))?
            } else {
                map.total_payload_bytes
            };

        let continuation = {
            let record = map
                .records
                .get_mut(&input.binding.result_ref)
                .ok_or(ResultStoreError::Unavailable("result_record"))?;
            record.continuation.revision = record
                .continuation
                .revision
                .checked_add(1)
                .ok_or(ResultStoreError::Unavailable("continuation_revision"))?;
            record.continuation.state = match input.outcome {
                ResultContinuationCompletionOutcome::Delivered => {
                    record.continuation.request = None;
                    if let Some(delivered_payload_bytes) = delivered_payload_bytes {
                        record.payload_bytes = delivered_payload_bytes;
                    }
                    ResultContinuationState::Delivered
                }
                ResultContinuationCompletionOutcome::Paused => ResultContinuationState::Paused,
                ResultContinuationCompletionOutcome::DeliveryUnknown => {
                    ResultContinuationState::DeliveryUnknown
                }
            };
            record.continuation.lease_id = None;
            record.continuation.dispatch_not_after = None;
            record.continuation.snapshot()
        };
        map.total_payload_bytes = next_total_payload_bytes;
        Ok(CompletedResultContinuation {
            binding: input.binding,
            continuation,
        })
    }
}

pub(super) fn recover_expired_continuation(
    record: &mut ResultRecord,
    now: Instant,
) -> Result<(), ResultStoreError> {
    if record.continuation.state == ResultContinuationState::Dispatching
        && record
            .continuation
            .dispatch_not_after
            .is_some_and(|deadline| now >= deadline)
    {
        record.continuation.revision = record
            .continuation
            .revision
            .checked_add(1)
            .ok_or(ResultStoreError::Unavailable("continuation_revision"))?;
        record.continuation.state = ResultContinuationState::DeliveryUnknown;
        record.continuation.lease_id = None;
        record.continuation.dispatch_not_after = None;
    }
    Ok(())
}

fn validate_continuation_revision(
    record: &ResultRecord,
    expected_revision: u64,
) -> Result<(), ResultStoreError> {
    if record.continuation.revision != expected_revision {
        return Err(ResultStoreError::ContinuationRevisionConflict {
            current_revision: record.continuation.revision,
        });
    }
    Ok(())
}

fn require_terminal(record: &ResultRecord) -> Result<(), ResultStoreError> {
    if !matches!(
        record.state,
        ResultState::Resolved(_) | ResultState::Failed(_) | ResultState::Unknown(_)
    ) {
        return Err(ResultStoreError::InvalidState(
            "continuation_requires_terminal_result",
        ));
    }
    Ok(())
}

fn epoch_ms() -> Result<u64, ResultStoreError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ResultStoreError::Unavailable("continuation_clock"))?
        .as_millis();
    u64::try_from(millis).map_err(|_| ResultStoreError::Unavailable("continuation_clock"))
}
