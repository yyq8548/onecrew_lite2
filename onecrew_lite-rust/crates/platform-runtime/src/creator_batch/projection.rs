use onecrew_creator_runtime::{ManualCreatorBatchCreateResult, ManualCreatorBatchItemOutcome};

use crate::{
    CallDisposition, ExecutedCallOutcome, OwnerCallCompletion, OwnerCallOutput, OwnerReviewContext,
    PresentationField, PresentationProjection, PresentationSection, ResultContinuationContext,
    ResultContinuationOutcomeSummary, ResultContinuationRequest, ResultPresentationKind,
    ResultResolution, ResultResourceKind, ResultResourceRef, ResultStoreError, TerminalResult,
};

/// A saved Creator's label on the result frame. The store bounds a label at
/// `MAX_PRESENTATION_TEXT_BYTES`; this is a receipt row rather than a review
/// row, so it takes the store's own per-string cap and not the review card's
/// tighter shared-projection budget.
const MAX_RESOURCE_LABEL_BYTES: usize = crate::interaction::MAX_PRESENTATION_TEXT_BYTES;

/// Project one completed Creator batch for both peer adapters.
///
/// A rejected item is a per-item business outcome the batch reports, not a
/// failure of the call: the batch ran and every row has a definite answer. That
/// keeps a partially rejected batch from reading as an uncertain write and
/// inviting a second submission of Creators that already exist.
///
/// # Errors
/// Returns a result-store error if the owner presentation kind is invalid.
pub(crate) fn into_owner_completion(
    result: ManualCreatorBatchCreateResult,
    context: &OwnerReviewContext,
) -> Result<OwnerCallCompletion, ResultStoreError> {
    let accepted = result
        .outcomes
        .iter()
        .filter(|outcome| matches!(outcome, ManualCreatorBatchItemOutcome::Accepted { .. }))
        .count();
    let rejected = result.outcomes.len() - accepted;
    let code = "creator_batch_completed";
    let mut summary =
        format!("The Creator batch completed: {accepted} created, {rejected} rejected.");
    if result.replayed {
        summary.push_str(" This result replayed the existing record; no new Creator was created.");
    }
    // One reference per created Creator. A batch creates at most
    // MAX_MANUAL_CREATOR_BATCH_ITEMS of them, which is inside the frame's
    // generic MAX_RESOURCE_REFS budget on its own; this deliberately does not
    // follow MAX_OWNER_FRAME_RESOURCE_REFS, which tracks the Creator *save*
    // batch and only coincides with the generic budget today.
    let resource_refs = result
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ManualCreatorBatchItemOutcome::Accepted { creator, .. } => Some(ResultResourceRef {
                kind: ResultResourceKind::Creator,
                id: creator.creator_id.as_str().to_owned(),
                label: bounded(creator.display_name.trim()),
            }),
            ManualCreatorBatchItemOutcome::Rejected { .. } => None,
        })
        .collect::<Vec<_>>();
    let disposition = CallDisposition::Succeeded;
    let outcome = ExecutedCallOutcome {
        call_id: context.call_id().into(),
        operation_id: context.operation_id().into(),
        disposition: disposition.clone(),
        summary: summary.clone(),
        resource_refs: resource_refs.clone(),
    };
    let continuation = ResultContinuationRequest {
        message: summary.clone(),
        requires_model_context: true,
        context: ResultContinuationContext::Execution {
            outcomes: vec![ResultContinuationOutcomeSummary {
                call_id: outcome.call_id.clone(),
                operation_id: outcome.operation_id.clone(),
                disposition,
                summary: outcome.summary.clone(),
                resource_refs,
            }],
        },
    };
    Ok(OwnerCallCompletion {
        outcome,
        presentation: PresentationProjection {
            kind: ResultPresentationKind::new("creator_batch_outcome")?,
            title: "Creator batch result".to_owned(),
            summary: Some(summary.clone()),
            sections: vec![PresentationSection::Fields {
                heading: None,
                fields: vec![
                    PresentationField {
                        label: "Status".to_owned(),
                        value: code.to_owned(),
                    },
                    PresentationField {
                        label: "Created".to_owned(),
                        value: accepted.to_string(),
                    },
                    PresentationField {
                        label: "Rejected".to_owned(),
                        value: rejected.to_string(),
                    },
                    PresentationField {
                        label: "Replayed".to_owned(),
                        value: result.replayed.to_string(),
                    },
                ],
            }],
        },
        terminal: TerminalResult::Resolved(ResultResolution {
            code: code.into(),
            summary,
        }),
        continuation: Some(continuation),
        output: OwnerCallOutput::CreatorBatch(Box::new(result)),
        // Every row has a definite outcome, so a mixed peer batch may continue.
        continue_batch: true,
    })
}

/// A resource label the store will accept, or none when the created Creator has
/// no name to show. A blank label is rejected and would cost the whole frame.
fn bounded(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.len() <= MAX_RESOURCE_LABEL_BYTES {
        return Some(value.to_owned());
    }
    let mut split = MAX_RESOURCE_LABEL_BYTES;
    while !value.is_char_boundary(split) {
        split -= 1;
    }
    Some(value[..split].to_owned())
}
