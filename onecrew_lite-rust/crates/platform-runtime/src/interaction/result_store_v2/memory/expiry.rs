//! Revoke an expired review without erasing already-completed calls.
use std::time::Instant;

use super::super::{
    CallDisposition, ResultContinuationContext, ResultContinuationOutcomeSummary,
    ResultContinuationRequest, ResultFailure, ResultRef, ResultState, ResultStoreError,
};
use super::{ResultMap, StoreLimits};

pub(super) fn expire_review(
    map: &mut ResultMap,
    reference: &ResultRef,
    limits: StoreLimits,
    now: Instant,
) -> Result<(), ResultStoreError> {
    let record = map.records.get(reference).ok_or(ResultStoreError::Absent)?;
    let (summary, not_run, continuation_text) = match record.locale {
        crate::PresentationLocale::English => (
            "The current review expired. Completed calls and full result details are retained; pending calls were not run.",
            "Not run because the review expired.",
            "The current review expired. Earlier completed calls must not be repeated; pending calls were not run. Full result details remain on this result. The continuation includes only a bounded selection of resource references.",
        ),
        crate::PresentationLocale::SimplifiedChinese => (
            "当前确认已过期。已完成的调用和完整结果详情已保留，待执行的调用未运行。",
            "确认已过期，此调用未执行。",
            "当前确认已过期。先前已完成的调用不应重复，待执行的调用未运行。完整结果详情仍保留在此结果中；本次后续上下文仅包含有限数量的资源引用。",
        ),
    };
    let mut batch = record.batch.clone();
    for call in &mut batch.outcomes {
        if matches!(call.disposition, CallDisposition::NotRun { .. }) {
            call.disposition = CallDisposition::NotRun {
                reason: "review_expired".into(),
            };
            call.summary = not_run.into();
        }
    }
    let state = ResultState::Failed(ResultFailure {
        code: "review_expired".into(),
        summary: summary.into(),
        retryable: false,
    });
    // The shared Execution continuation contract carries at most eight calls
    // and 64 refs per call. All rows and refs remain in the result itself.
    let mut continuation = ResultContinuationRequest {
        message: continuation_text.into(),
        requires_model_context: true,
        context: ResultContinuationContext::Execution {
            outcomes: batch
                .outcomes
                .iter()
                .take(8)
                .map(|call| ResultContinuationOutcomeSummary {
                    call_id: call.call_id.clone(),
                    operation_id: call.operation_id.clone(),
                    disposition: call.disposition.clone(),
                    summary: call.summary.clone(),
                    resource_refs: call
                        .resource_refs
                        .iter()
                        .take(super::super::MAX_RESOURCE_REFS)
                        .map(|resource| super::super::ResultResourceRef {
                            kind: resource.kind,
                            id: resource.id.clone(),
                            label: None,
                        })
                        .collect(),
                })
                .collect(),
        },
    };
    super::super::validation::validate_continuation_request(&continuation)?;
    let mut bytes = super::super::validation::stored_terminal_payload_bytes(
        &batch,
        &record.presentation,
        &state,
        Some(&continuation),
    )?;
    let retained_bytes = map
        .total_payload_bytes
        .checked_sub(record.payload_bytes)
        .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
    if retained_bytes
        .checked_add(bytes)
        .is_none_or(|total| total > limits.max_total_payload_bytes)
    {
        // Capacity bounds the model notification, never the already-recorded
        // history. At a full store notify only the current expired call; the
        // message still explicitly retains and protects earlier completed work.
        let current = batch
            .outcomes
            .iter()
            .find(|call| matches!(call.disposition, CallDisposition::NotRun { .. }))
            .ok_or(ResultStoreError::Unavailable("expired_pending_call"))?;
        continuation.context = ResultContinuationContext::Execution {
            outcomes: vec![ResultContinuationOutcomeSummary {
                call_id: current.call_id.clone(),
                operation_id: current.operation_id.clone(),
                disposition: current.disposition.clone(),
                summary: not_run.into(),
                resource_refs: Vec::new(),
            }],
        };
        bytes = super::super::validation::stored_terminal_payload_bytes(
            &batch,
            &record.presentation,
            &state,
            Some(&continuation),
        )?;
    }
    let total = retained_bytes
        .checked_add(bytes)
        .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
    if total > limits.max_total_payload_bytes {
        // Authority is already expired and submit still rejects it. Preserve
        // the record unchanged if even its bounded notification cannot fit.
        return Err(ResultStoreError::Capacity("result_payload_limit"));
    }
    let revision = record
        .revision
        .checked_add(1)
        .ok_or(ResultStoreError::Unavailable("result_revision"))?;
    let record = map
        .records
        .get_mut(reference)
        .ok_or(ResultStoreError::Absent)?;
    record
        .continuation
        .apply_terminal_request(Some(continuation))?;
    record.pending_action = None;
    record.submitted_owner_context = None;
    record.state = state;
    record.batch = batch;
    record.revision = revision;
    record.payload_bytes = bytes;
    record.last_accessed_at = now;
    record.state_changed_at = now;
    // Keep presentation, its completed prefix length and frozen identity. The
    // expired current review is readable context, never marked as completed.
    map.total_payload_bytes = total;
    Ok(())
}
