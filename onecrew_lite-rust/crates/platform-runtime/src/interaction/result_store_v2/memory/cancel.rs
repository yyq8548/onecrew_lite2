//! Derive mixed-batch cancellation facts from the locked result, never from a
//! caller-supplied Execution continuation. The wire still accepts only Cancelled.
use super::super::{
    CallDisposition, ExecutedBatch, ResultContinuationContext, ResultContinuationOutcomeSummary,
    ResultContinuationRequest, ResultResolution, ResultState, ResultStoreError,
};
use super::ResultRecord;

pub(super) struct CancellationProjection {
    pub(super) batch: ExecutedBatch,
    pub(super) state: ResultState,
    pub(super) continuation: Option<ResultContinuationRequest>,
}

pub(super) fn project(
    record: &ResultRecord,
    requested: Option<ResultContinuationRequest>,
) -> Result<CancellationProjection, ResultStoreError> {
    let has_pending = record
        .batch
        .outcomes
        .iter()
        .any(|call| matches!(call.disposition, CallDisposition::NotRun { .. }));
    let has_completed = record.completed_sections_len > 0
        || record
            .batch
            .outcomes
            .iter()
            .any(|call| matches!(call.disposition, CallDisposition::Succeeded));
    if !has_pending || !has_completed {
        return Ok(CancellationProjection {
            batch: record.batch.clone(),
            state: ResultState::Resolved(ResultResolution {
                code: "user_cancelled".into(),
                summary: "The user cancelled this action.".into(),
            }),
            continuation: requested,
        });
    }
    if record.batch.outcomes.len() > 8 {
        return Err(ResultStoreError::InvalidInput("cancel_execution_batch"));
    }
    let (summary, not_run, message) = match record.locale {
        crate::PresentationLocale::English => (
            "The pending action was cancelled. Completed calls and full result details are retained.",
            "Not run because the remaining actions were cancelled.",
            "The pending action and remaining calls were cancelled. Earlier completed calls must not be repeated. Full result details remain on this result; this continuation contains only a bounded selection of resource references.",
        ),
        crate::PresentationLocale::SimplifiedChinese => (
            "已取消待执行的操作。已完成的调用及完整结果详情已保留。",
            "后续操作已取消，此调用未执行。",
            "待执行的操作及后续调用已取消，先前已完成的调用不应重复。完整结果详情仍保留在此结果中；后续上下文仅包含有限数量的资源引用。",
        ),
    };
    let mut batch = record.batch.clone();
    for call in &mut batch.outcomes {
        if matches!(call.disposition, CallDisposition::NotRun { .. }) {
            call.disposition = CallDisposition::NotRun {
                reason: "user_cancelled".into(),
            };
            call.summary = not_run.into();
        }
    }
    let continuation = requested.map(|_| ResultContinuationRequest {
        message: message.into(),
        requires_model_context: true,
        context: ResultContinuationContext::Execution {
            outcomes: batch
                .outcomes
                .iter()
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
    });
    if let Some(request) = &continuation {
        super::super::validation::validate_continuation_request(request)?;
    }
    Ok(CancellationProjection {
        batch,
        state: ResultState::Resolved(ResultResolution {
            code: "user_cancelled".into(),
            summary: summary.into(),
        }),
        continuation,
    })
}
