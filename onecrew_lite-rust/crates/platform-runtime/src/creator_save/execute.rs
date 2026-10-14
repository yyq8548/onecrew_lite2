use std::{collections::BTreeMap, sync::Arc};

use onecrew_creator_runtime::CreatorSaveRowOutcome;

use super::{CreatorSaveError, SelectedSaveAction};
use crate::{
    CallDisposition, CreatorSaveBatchOperation, CreatorSaveBatchOutcome, CreatorSaveBatchReason,
    CreatorSaveBatchRowStatus, CreatorSaveBatchSelection, CreatorSaveCallResult,
    CreatorSaveResultSummary, ExecutedCallOutcome, LocalPlatform, OwnerCallCompletion,
    OwnerCallOutput, OwnerExecutionEffect, OwnerExecutionFailure, OwnerReviewSelection,
    PresentationProjection, PresentationSection, ResultBinding, ResultPresentationKind,
    ResultResolution, ResultSnapshot, ResultUnknown, SubmittedOwnerGrant, TerminalResult,
};

impl LocalPlatform {
    /// Consume the exact reviewed rows and routes. The owned task carries the
    /// operation through Creator admission, commit and terminal status
    /// even when an HTTP/MCP caller disconnects after submission.
    pub async fn confirm_creator_save(
        self: &Arc<Self>,
        binding: ResultBinding,
        selections: Vec<CreatorSaveBatchSelection>,
    ) -> Result<ResultSnapshot, CreatorSaveError> {
        let submitted =
            self.submit_owner_review(&binding, OwnerReviewSelection::CreatorSave { selections })?;
        let submitted_binding = submitted.binding().clone();
        let platform = Arc::clone(self);
        let worker = tokio::spawn(async move {
            let completion = platform.execute_creator_save_submitted(submitted).await?;
            match platform.transition_result_terminal(
                &submitted_binding,
                completion.into_singleton_terminal(),
            ) {
                Ok(snapshot) => Ok(snapshot),
                Err(error) => {
                    let failure = OwnerExecutionFailure {
                        code: "creator_save_terminal_transition_failed",
                        message: "The Creator save outcome could not be finalized. Inspect the submitted result before retrying.",
                        effect: OwnerExecutionEffect::Unknown,
                        dm_failure: None,
                    };
                    if platform
                        .fail_singleton_owner_execution(&submitted_binding, &failure)
                        .is_err()
                    {
                        log::warn!(
                            "Creator save terminal recovery could not close submitted result"
                        );
                    }
                    Err(CreatorSaveError::from(error))
                }
            }
        });
        worker
            .await
            .map_err(|_| CreatorSaveError::WorkerUnavailable)?
    }

    /// Execute one immutable submitted save under the caller's existing work
    /// owner. The singleton API above retains its stronger owned cancellation
    /// guarantee through the final result transition.
    pub async fn execute_creator_save_submitted(
        &self,
        grant: SubmittedOwnerGrant,
    ) -> Result<OwnerCallCompletion, CreatorSaveError> {
        let binding = grant.binding().clone();
        let locale = grant.locale();
        let action = grant.into_creator_save()?;
        execute(self, binding, action, locale).await
    }
}

async fn execute(
    platform: &LocalPlatform,
    binding: ResultBinding,
    action: SelectedSaveAction,
    locale: crate::PresentationLocale,
) -> Result<OwnerCallCompletion, CreatorSaveError> {
    let SelectedSaveAction {
        selected,
        rows,
        failures,
        selections,
        context,
    } = action;
    // A presented batch can contain more than one save. Its stable call ID
    // distinguishes those intents without changing on confirmation revisions.
    let execution_key = format!(
        "creator-save:{}:{}",
        binding.result_ref.as_str(),
        context.call_id
    );
    let result = platform
        .creators
        .execute_creator_save_batch_async(selected, execution_key)
        .await;
    let mut outcomes = Vec::with_capacity(rows.len() + failures.len());
    let mut resources = BTreeMap::new();
    let mut result_summary = CreatorSaveResultSummary {
        total: rows.len() + failures.len(),
        ..CreatorSaveResultSummary::default()
    };
    for row in rows {
        let chosen = selections
            .iter()
            .find(|selection| selection.row_id == row.row_id);
        let (status, operation, reason, creator_id, failure_code) = if chosen.is_none() {
            let (operation, reason) = unselected_row_result(row.status, row.reason);
            summary_for_operation(&mut result_summary, operation);
            (
                if operation == CreatorSaveBatchOperation::NotSelected {
                    CreatorSaveBatchRowStatus::NotSelected
                } else {
                    CreatorSaveBatchRowStatus::NotRun
                },
                operation,
                reason,
                None,
                None,
            )
        } else {
            match &result {
                Ok(result) => match result.outcomes.iter().find(|outcome| match outcome {
                    CreatorSaveRowOutcome::Saved { row_id, .. }
                    | CreatorSaveRowOutcome::Failed { row_id, .. }
                    | CreatorSaveRowOutcome::Skipped { row_id, .. } => row_id == &row.row_id,
                }) {
                    Some(CreatorSaveRowOutcome::Saved {
                        creator, created, ..
                    }) => {
                        let id = creator.creator.id.as_str().to_owned();
                        resources.insert(
                            id.clone(),
                            crate::ResultResourceRef {
                                kind: crate::ResultResourceKind::Creator,
                                id: id.clone(),
                                label: Some(creator.creator.display_name.clone()),
                            },
                        );
                        let operation = if *created {
                            CreatorSaveBatchOperation::Added
                        } else {
                            CreatorSaveBatchOperation::Updated
                        };
                        summary_for_operation(&mut result_summary, operation);
                        (
                            CreatorSaveBatchRowStatus::Saved,
                            operation,
                            None,
                            Some(id),
                            None,
                        )
                    }
                    Some(CreatorSaveRowOutcome::Failed { .. }) => {
                        summary_for_operation(
                            &mut result_summary,
                            CreatorSaveBatchOperation::Failed,
                        );
                        (
                            CreatorSaveBatchRowStatus::Failed,
                            CreatorSaveBatchOperation::Failed,
                            Some(CreatorSaveBatchReason::RowFailed),
                            None,
                            Some("creator_save_row_failed".into()),
                        )
                    }
                    Some(CreatorSaveRowOutcome::Skipped { reason, .. }) => {
                        let (operation, reason) = match reason {
                            onecrew_creator_runtime::CreatorSaveSkipReason::ExistingCreator => (
                                CreatorSaveBatchOperation::ExistingSkipped,
                                CreatorSaveBatchReason::ExistingCreator,
                            ),
                            onecrew_creator_runtime::CreatorSaveSkipReason::DuplicateSelection => (
                                CreatorSaveBatchOperation::DuplicateSkipped,
                                CreatorSaveBatchReason::DuplicateSelection,
                            ),
                        };
                        summary_for_operation(&mut result_summary, operation);
                        (
                            CreatorSaveBatchRowStatus::NotRun,
                            operation,
                            Some(reason),
                            None,
                            None,
                        )
                    }
                    None => {
                        summary_for_operation(
                            &mut result_summary,
                            CreatorSaveBatchOperation::Unknown,
                        );
                        (
                            CreatorSaveBatchRowStatus::Unknown,
                            CreatorSaveBatchOperation::Unknown,
                            Some(CreatorSaveBatchReason::CommitUnknown),
                            None,
                            Some("creator_save_readback_unavailable".into()),
                        )
                    }
                },
                Err(_) => {
                    summary_for_operation(&mut result_summary, CreatorSaveBatchOperation::Unknown);
                    (
                        CreatorSaveBatchRowStatus::Unknown,
                        CreatorSaveBatchOperation::Unknown,
                        Some(CreatorSaveBatchReason::CommitUnknown),
                        None,
                        Some("creator_save_commit_unknown".into()),
                    )
                }
            }
        };
        outcomes.push(CreatorSaveBatchOutcome {
            row_id: row.row_id.clone(),
            display_name: Some(row.account.display_name),
            platform: row.account.platform,
            handle: row.account.handle,
            profile_url: row.account.profile_url,
            avatar_url: None,
            status,
            operation,
            reason,
            creator_id,
            failure_code,
            confirmed_route_ids: chosen
                .map_or_else(Vec::new, |selection| selection.confirmed_route_ids.clone()),
        });
    }
    for failure in failures {
        summary_for_operation(&mut result_summary, CreatorSaveBatchOperation::Failed);
        outcomes.push(CreatorSaveBatchOutcome {
            row_id: failure.row_id,
            display_name: failure.display_name,
            platform: failure.platform,
            handle: failure.handle,
            profile_url: failure.profile_url,
            avatar_url: None,
            status: CreatorSaveBatchRowStatus::NotRun,
            operation: CreatorSaveBatchOperation::Failed,
            reason: Some(CreatorSaveBatchReason::PreparationFailed),
            creator_id: None,
            failure_code: Some("creator_save_preparation_failed".into()),
            confirmed_route_ids: Vec::new(),
        });
    }
    let unknown = outcomes
        .iter()
        .any(|row| row.status == CreatorSaveBatchRowStatus::Unknown);
    let failed = outcomes.iter().any(|row| {
        row.status == CreatorSaveBatchRowStatus::Failed
            || row.operation == CreatorSaveBatchOperation::Failed
    });
    let (title, message) = super::projection::outcome_text(locale, &outcomes);
    let terminal = if unknown {
        TerminalResult::Unknown(ResultUnknown {
            code: "creator_save_commit_unknown".into(),
            summary: message.clone(),
        })
    } else {
        TerminalResult::Resolved(ResultResolution {
            code: "creator_save_completed".into(),
            summary: message.clone(),
        })
    };
    let disposition = if unknown {
        CallDisposition::Unknown {
            code: "creator_save_commit_unknown".into(),
        }
    } else if failed {
        CallDisposition::Failed {
            code: "creator_save_partial_failure".into(),
        }
    } else {
        CallDisposition::Succeeded
    };
    let continuation = crate::ResultContinuationRequest {
        message: message.clone(),
        context: crate::ResultContinuationContext::CreatorSaveBatch {
            summary: result_summary,
            outcomes: outcomes
                .iter()
                .map(crate::ResultContinuationCreatorSaveOutcome::from)
                .collect(),
        },
        requires_model_context: true,
    };
    Ok(OwnerCallCompletion {
        output: OwnerCallOutput::CreatorSave(CreatorSaveCallResult {
            rows: outcomes.clone(),
            summary: result_summary,
        }),
        continue_batch: !unknown && !failed,
        terminal,
        outcome: ExecutedCallOutcome {
            call_id: context.call_id,
            operation_id: context.operation_id,
            disposition,
            summary: message.clone(),
            resource_refs: resources.into_values().collect(),
        },
        presentation: PresentationProjection {
            kind: ResultPresentationKind::new("creator_save_batch_outcome")?,
            title,
            summary: Some(message),
            sections: vec![PresentationSection::CreatorSaveRows { rows: outcomes }],
        },
        continuation: Some(continuation),
    })
}

fn unselected_row_result(
    status: onecrew_creator_runtime::CreatorSavePreparedStatus,
    reason: Option<onecrew_creator_runtime::CreatorSavePreparedReason>,
) -> (CreatorSaveBatchOperation, Option<CreatorSaveBatchReason>) {
    match status {
        onecrew_creator_runtime::CreatorSavePreparedStatus::ExistingSkipped => (
            CreatorSaveBatchOperation::ExistingSkipped,
            Some(CreatorSaveBatchReason::ExistingCreator),
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::DuplicateSkipped => (
            CreatorSaveBatchOperation::DuplicateSkipped,
            Some(CreatorSaveBatchReason::DuplicateSelection),
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::IdentityPending => (
            CreatorSaveBatchOperation::Failed,
            Some(match reason {
                Some(onecrew_creator_runtime::CreatorSavePreparedReason::ConflictingUpdate) => {
                    CreatorSaveBatchReason::ConflictingUpdate
                }
                _ => CreatorSaveBatchReason::IdentityAmbiguous,
            }),
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::ContactConflict => (
            CreatorSaveBatchOperation::Failed,
            Some(CreatorSaveBatchReason::RequiredContactRouteConflict),
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::Failed => (
            CreatorSaveBatchOperation::Failed,
            Some(CreatorSaveBatchReason::PreparationFailed),
        ),
        onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleNew
        | onecrew_creator_runtime::CreatorSavePreparedStatus::EligibleUpdate => (
            CreatorSaveBatchOperation::NotSelected,
            Some(CreatorSaveBatchReason::UserNotSelected),
        ),
    }
}

fn summary_for_operation(
    summary: &mut CreatorSaveResultSummary,
    operation: CreatorSaveBatchOperation,
) {
    match operation {
        CreatorSaveBatchOperation::Added => summary.added += 1,
        CreatorSaveBatchOperation::Updated => summary.updated += 1,
        CreatorSaveBatchOperation::ExistingSkipped => summary.existing_skipped += 1,
        CreatorSaveBatchOperation::DuplicateSkipped => summary.duplicate_skipped += 1,
        CreatorSaveBatchOperation::NotSelected => summary.not_selected += 1,
        CreatorSaveBatchOperation::Failed => summary.failed += 1,
        CreatorSaveBatchOperation::Unknown => summary.unknown += 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_creator_runtime::{CreatorSavePreparedReason, CreatorSavePreparedStatus};

    #[test]
    fn unselected_rows_keep_their_distinct_owner_reasons() {
        for status in [
            CreatorSavePreparedStatus::EligibleNew,
            CreatorSavePreparedStatus::EligibleUpdate,
        ] {
            assert_eq!(
                unselected_row_result(status, None),
                (
                    CreatorSaveBatchOperation::NotSelected,
                    Some(CreatorSaveBatchReason::UserNotSelected)
                )
            );
        }
        assert_eq!(
            unselected_row_result(
                CreatorSavePreparedStatus::IdentityPending,
                Some(CreatorSavePreparedReason::ConflictingUpdate),
            ),
            (
                CreatorSaveBatchOperation::Failed,
                Some(CreatorSaveBatchReason::ConflictingUpdate)
            )
        );
    }
}
