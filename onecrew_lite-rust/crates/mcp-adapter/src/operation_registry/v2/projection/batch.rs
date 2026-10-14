//! Executed-batch outcome summaries and continuation composition.
use super::{
    call::{AdapterPresentationKind, ProjectionCall},
    exact::output_resource_refs,
};
use crate::operation_registry::{
    execution::ExecuteEffectState, v2::contract::ToolExecutionOutcome,
};
use onecrew_local_runtime::{
    CallDisposition, ExecutedBatch, ExecutedCallOutcome, PresentationLocale,
    ResultContinuationContext, ResultContinuationOutcomeSummary, ResultContinuationRequest,
};

pub(in crate::operation_registry::v2) fn batch_is_presentable(calls: &[ProjectionCall]) -> bool {
    calls.iter().all(|call| call.presentable)
}

/// The executed frame: what each call did, and the resources its output named.
///
/// Composing this cannot fail, because none of it is display. The resource
/// references used to be taken from the display projection, so an output with
/// more leaves than one card section can hold produced no references — and,
/// through that, no result reference at all. The owner had already run and the
/// caller was handed nothing to look at.
pub(in crate::operation_registry::v2) fn executed_batch(
    outcomes: &[ToolExecutionOutcome],
    calls: &[ProjectionCall],
    locale: PresentationLocale,
) -> ExecutedBatch {
    ExecutedBatch {
        outcomes: outcomes
            .iter()
            .zip(calls)
            .map(|(outcome, call)| ExecutedCallOutcome {
                call_id: call.call_id.as_str().to_owned(),
                operation_id: call.operation_id.as_str().to_owned(),
                disposition: disposition(outcome),
                summary: outcome_summary(outcome, call, locale),
                resource_refs: match outcome {
                    ToolExecutionOutcome::Completed { result, .. } => output_resource_refs(result),
                    ToolExecutionOutcome::InputRequired { .. }
                    | ToolExecutionOutcome::Failed { .. }
                    | ToolExecutionOutcome::Unknown { .. }
                    | ToolExecutionOutcome::Skipped { .. } => Vec::new(),
                },
            })
            .collect(),
    }
}

pub(in crate::operation_registry::v2) fn combine_executed_batch(
    prior: &ExecutedBatch,
    pending_call_id: &str,
    resumed: ExecutedBatch,
) -> Option<ExecutedBatch> {
    let pending_index = prior
        .outcomes
        .iter()
        .position(|outcome| outcome.call_id == pending_call_id)?;
    let mut outcomes = prior.outcomes[..pending_index].to_vec();
    outcomes.extend(resumed.outcomes);
    (outcomes.len() <= 8).then_some(ExecutedBatch { outcomes })
}

pub(in crate::operation_registry::v2) fn execution_continuation(
    batch: &ExecutedBatch,
    locale: onecrew_local_runtime::PresentationLocale,
) -> ResultContinuationRequest {
    ResultContinuationRequest {
        requires_model_context: true,
        message: super::continuation_copy::continuation_copy(locale)
            .execution
            .clone(),
        context: ResultContinuationContext::Execution {
            outcomes: batch
                .outcomes
                .iter()
                .map(|outcome| ResultContinuationOutcomeSummary {
                    call_id: outcome.call_id.clone(),
                    operation_id: outcome.operation_id.clone(),
                    disposition: outcome.disposition.clone(),
                    summary: continuation_summary(outcome, locale),
                    resource_refs: outcome.resource_refs.iter().take(64).cloned().collect(),
                })
                .collect(),
        },
    }
}

fn continuation_summary(
    outcome: &ExecutedCallOutcome,
    locale: onecrew_local_runtime::PresentationLocale,
) -> String {
    if outcome.resource_refs.len() <= 64 {
        return outcome.summary.clone();
    }
    let total = outcome.resource_refs.len();
    let suffix = match locale {
        onecrew_local_runtime::PresentationLocale::English => format!(
            " Showing 64 of {total} resource references; complete receipts remain in this result."
        ),
        onecrew_local_runtime::PresentationLocale::SimplifiedChinese => {
            format!(" 仅展示 {total} 个资源引用中的前 64 个；完整回执保留在此结果中。")
        }
    };
    let mut end = outcome.summary.len().min(8192 - suffix.len());
    while !outcome.summary.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &outcome.summary[..end], suffix)
}
pub(super) fn batch_presentation_kind(calls: &[ProjectionCall]) -> AdapterPresentationKind {
    let Some(first) = calls.first() else {
        return AdapterPresentationKind::BatchResult;
    };
    if calls.iter().all(|call| call.kind == first.kind) {
        first.kind
    } else {
        AdapterPresentationKind::BatchResult
    }
}
fn disposition(outcome: &ToolExecutionOutcome) -> CallDisposition {
    match outcome {
        ToolExecutionOutcome::Completed { .. } => CallDisposition::Succeeded,
        ToolExecutionOutcome::InputRequired { .. } => CallDisposition::NotRun {
            reason: "confirmation_required".to_owned(),
        },
        ToolExecutionOutcome::Failed { failure, .. } => CallDisposition::Failed {
            code: failure.code.as_str().to_owned(),
        },
        ToolExecutionOutcome::Unknown { failure, .. } => CallDisposition::Unknown {
            code: failure.code.as_str().to_owned(),
        },
        ToolExecutionOutcome::Skipped { reason, .. } => CallDisposition::NotRun {
            reason: reason.clone(),
        },
    }
}

/// One line per call, in the card's own language. The operation title inside it
/// is the catalog's and stays as the catalog wrote it.
pub(super) fn outcome_summary(
    outcome: &ToolExecutionOutcome,
    call: &ProjectionCall,
    locale: PresentationLocale,
) -> String {
    let copy = super::terminal::terminal_copy(locale);
    let template = match outcome {
        ToolExecutionOutcome::Completed { .. } => &copy.call_completed,
        ToolExecutionOutcome::InputRequired { .. } => &copy.call_requires_confirmation,
        ToolExecutionOutcome::Failed { .. } => &copy.call_did_not_complete,
        ToolExecutionOutcome::Unknown { failure, .. }
            if failure.effect == ExecuteEffectState::Applied =>
        {
            &copy.call_applied_unconfirmed
        }
        ToolExecutionOutcome::Unknown { .. } => &copy.call_effect_unverified,
        ToolExecutionOutcome::Skipped { .. } => &copy.call_was_not_run,
    };
    template.replace("{call}", &call.title)
}
