//! Continue one exact MCP batch after consuming its current confirmation.
//! Business writes consume sealed owner grants; generic confirmations retain
//! their existing argument dispatch. Every later call starts unconfirmed.
use std::{collections::VecDeque, sync::Arc};

use onecrew_local_runtime::{
    ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput, InitialResultState, LocalPlatform,
    PendingActionView, PendingUserAction, ResultConfirmationSubmission, ResultSnapshot,
    ResultTerminalUpdate, ResultViewKind,
};

use crate::operation_registry::{
    descriptor::{ApprovalClass, RegistryEntry, SecretClass, SurfaceId},
    execution::{
        BindingOutput, BindingOutputClass, ClassifiedOwnerFailure, ExecuteEffectState,
        ExecutionContext, PreparedInvocation, PublicFailureCode, dispatch_prepared_output,
        prepare_invocation, project_classified_owner_failure, project_execute_failure,
    },
    registration::OperationRegistry,
};

use super::{
    contract::{ResultStateFailure, ResultStateOutput, ToolExecutionOutcome, ToolInputKind},
    execute::SKIPPED_AFTER_STOP,
    projection::{
        ProjectionCall, combine_executed_batch, confirm_action_label, confirmation_projection,
        executed_batch, execution_continuation, localize_adapter_presentation,
        presentation_failure_terminal_update, readonly_prefix_projection,
        resumed_terminal_projection,
    },
    resume::{
        ResumeCall, ResumeEnvelope, decode_owner_resume_envelope, decode_resume_envelope,
        encode_owner_resume_envelope, encode_resume_envelope, owner_call_digest,
    },
    state::{SubmittedTerminalGuard, map_store_failure, resume_matches_snapshot},
};

struct PreparedCall<'entry> {
    resume: ResumeCall,
    projection: ProjectionCall,
    invocation: PreparedInvocation<'entry>,
    approval: ApprovalClass,
    prompt: Option<String>,
    output_class: BindingOutputClass,
    owner_peer: Option<onecrew_local_runtime::PeerResumePayload>,
    generic_payload: Option<onecrew_local_runtime::OwnerActionPayload>,
}

pub(super) async fn resume(
    registry: &OperationRegistry,
    platform: &Arc<LocalPlatform>,
    surface: SurfaceId,
    current: ResultSnapshot,
    submission: ResultConfirmationSubmission,
) -> Result<ResultStateOutput, ResultStateFailure> {
    let mut dm_failure = None;
    let snapshot = resume_snapshot(
        registry,
        platform,
        surface,
        current,
        submission,
        &mut dm_failure,
    )
    .await?;
    Ok(ResultStateOutput::Snapshot {
        snapshot: localize_adapter_presentation(snapshot),
        dm_failure,
    })
}

async fn resume_snapshot(
    registry: &OperationRegistry,
    platform: &Arc<LocalPlatform>,
    surface: SurfaceId,
    current: ResultSnapshot,
    submission: ResultConfirmationSubmission,
    response_dm_failure: &mut Option<Box<onecrew_local_runtime::DmFailure>>,
) -> Result<ResultSnapshot, ResultStateFailure> {
    let (submitted_binding, calls, owner_grant) = match submission {
        ResultConfirmationSubmission::Generic(submitted) => {
            let (binding, _, bytes) = submitted.into_generic_parts().map_err(map_store_failure)?;
            let mut guard = SubmittedTerminalGuard::new(platform, binding.clone(), current.clone());
            guard.record_completed_sections(
                platform
                    .completed_result_sections(&binding)
                    .map_err(map_store_failure)?,
            );
            if matches!(
                current.pending_action,
                Some(PendingActionView::SearchPlan { .. })
            ) {
                return super::search_plan::confirm_search_plan(
                    registry, surface, platform, &current, bytes, guard,
                );
            }
            let ResumeEnvelope::V1 { pending, remaining } = match decode_resume_envelope(bytes) {
                Ok(value) => value,
                Err(_) => return guard.close_known("resume_envelope_invalid"),
            };
            guard.disarm();
            (
                binding,
                std::iter::once(pending)
                    .chain(remaining)
                    .collect::<Vec<_>>(),
                None,
            )
        }
        ResultConfirmationSubmission::Owner(grant) => {
            let binding = grant.binding().clone();
            let mut guard = SubmittedTerminalGuard::new(platform, binding.clone(), current.clone());
            guard.record_completed_sections(
                platform
                    .completed_result_sections(&binding)
                    .map_err(map_store_failure)?,
            );
            let envelope = match grant
                .peer_resume()
                .and_then(|peer| decode_owner_resume_envelope(peer.as_bytes()).ok())
            {
                Some(value) => value,
                None => return guard.close_known("owner_resume_envelope_invalid"),
            };
            let Some(RegistryEntry::Operation(operation)) =
                registry.entry(&envelope.pending.call.operation_id, surface)
            else {
                return guard.close_known("resume_operation_unavailable");
            };
            let digest = owner_call_digest(surface, &envelope.pending, operation)
                .map_err(|_| ResultStateFailure::Unavailable)?;
            if envelope.surface != surface
                || grant.context().origin() != &current.binding.origin
                || grant.context().call_id() != envelope.pending.call.call_id.as_str()
                || grant.context().operation_id() != envelope.pending.call.operation_id.as_str()
                || grant.context().call_digest() != Some(&digest)
                || operation.binding.output_class() != BindingOutputClass::OwnerReview
            {
                return guard.close_known("owner_resume_call_mismatch");
            }
            guard.disarm();
            (
                binding,
                std::iter::once(envelope.pending)
                    .chain(envelope.remaining)
                    .collect::<Vec<_>>(),
                Some(grant),
            )
        }
    };
    let mut guard =
        SubmittedTerminalGuard::new(platform, submitted_binding.clone(), current.clone());
    let mut completed_sections = platform
        .completed_result_sections(&submitted_binding)
        .map_err(map_store_failure)?;
    guard.record_completed_sections(completed_sections.clone());
    if !resume_matches_snapshot(&current.batch, &calls) {
        return guard.close_known("resume_snapshot_mismatch");
    }
    let mut prepared = Vec::with_capacity(calls.len());
    for (index, call) in calls.iter().enumerate() {
        let Some(RegistryEntry::Operation(operation)) =
            registry.entry(&call.call.operation_id, surface)
        else {
            return guard.close_known("resume_operation_unavailable");
        };
        if !operation.sensitive_input_paths.is_empty()
            || operation.secret != SecretClass::None
            || operation.view_kind
                == crate::operation_registry::descriptor::OperationViewKind::Choice
        {
            return guard.close_known("resume_contract_invalid");
        }
        if index == 0
            && owner_grant.is_none()
            && (operation.approval != ApprovalClass::AttendedPrompt
                || operation.binding.output_class() != BindingOutputClass::Payload)
        {
            return guard.close_known("resume_contract_invalid");
        }
        let mut invocation = match prepare_invocation(
            operation,
            ExecutionContext {
                surface,
                request_id: call.request_id.clone(),
                connection: call.call.connection.as_ref(),
                confirmed: index == 0 && owner_grant.is_none(),
            },
            &call.call.schema_revision,
            serde_json::Value::Object(call.call.arguments.clone()),
        ) {
            Ok(value) => value,
            Err(_) => return guard.close_known("resume_prevalidation_failed"),
        };
        if operation.binding.output_class() == BindingOutputClass::OwnerReview {
            let digest = owner_call_digest(surface, call, operation)
                .map_err(|_| ResultStateFailure::Unavailable)?;
            invocation.bind_owner_review_context(
                &current.binding.origin,
                call.call.call_id.as_str(),
                digest,
            );
        }
        if operation.binding.supports_search_plan()
            && super::search_plan::call_search_policy(&call.call)
                == onecrew_local_runtime::SearchPlanPolicy::RequireConfirmation
        {
            match super::search_plan::authorize_call(platform, &call.call, &invocation) {
                Ok(authorization) => invocation.authorize_search(authorization),
                Err(_) => return guard.close_known("resume_search_authorization_rejected"),
            }
        }
        let remaining = calls[index + 1..].to_vec();
        let owner_peer = if operation.binding.output_class() == BindingOutputClass::OwnerReview {
            match encode_owner_resume_envelope(surface, call.clone(), remaining.clone()) {
                Ok(value) => Some(value),
                Err(_) => return guard.close_known("owner_resume_envelope_invalid"),
            }
        } else {
            None
        };
        let generic_payload = if operation.approval == ApprovalClass::AttendedPrompt
            && operation.binding.output_class() == BindingOutputClass::Payload
        {
            match encode_resume_envelope(call.clone(), remaining) {
                Ok(value) => Some(value),
                Err(_) => return guard.close_known("resume_envelope_invalid"),
            }
        } else {
            None
        };
        prepared.push(PreparedCall {
            resume: call.clone(),
            projection: ProjectionCall::new(
                operation,
                call.call.call_id.clone(),
                call.call.arguments.clone(),
            ),
            invocation,
            approval: operation.approval,
            prompt: operation
                .approval_request
                .as_ref()
                .map(|request| request.message.clone()),
            output_class: operation.binding.output_class(),
            owner_peer,
            generic_payload,
        });
    }
    let projection_calls = prepared
        .iter()
        .map(|call| call.projection.clone())
        .collect::<Vec<_>>();
    let pending_id = calls[0].call.call_id.clone();
    let mut queue = VecDeque::from(prepared);
    let mut outcomes = Vec::new();
    let mut trusted_outcome = None;
    if let Some(grant) = owner_grant {
        let first = queue.pop_front().expect("submitted owner call is present");
        // The invocation above only revalidates the current contract. Never
        // dispatch it: the grant is the sole source of the confirmed intent.
        drop(first.invocation);
        guard.mark_dispatch_may_have_started();
        let completion = match platform.execute_submitted_owner(grant).await {
            Ok(value) => value,
            Err(error) => {
                let classified = classify_submitted_owner_failure(error);
                let failure = project_classified_owner_failure(classified);
                if failure.dm_failure.is_some() {
                    *response_dm_failure = failure.dm_failure.clone();
                }
                outcomes.push(failure_outcome(&first.resume, failure));
                finish_skipped(&mut outcomes, &queue);
                let batch = frame(
                    &current.batch,
                    pending_id.as_str(),
                    &outcomes,
                    &projection_calls,
                    None,
                    current.locale,
                );
                let (presentation, terminal) = match resumed_terminal_projection(
                    &outcomes,
                    &projection_calls,
                    current.presentation.kind.clone(),
                    current.locale,
                ) {
                    Ok(value) => value,
                    Err(_) => {
                        return guard.commit(presentation_failure_terminal_update(
                            batch,
                            current.presentation.kind,
                            current.locale,
                        ));
                    }
                };
                return guard.commit(ResultTerminalUpdate {
                    terminal,
                    presentation,
                    continuation: Some(execution_continuation(&batch, current.locale)),
                    batch,
                });
            }
        };
        if let onecrew_local_runtime::OwnerCallOutput::Outreach(outreach) = &completion.output {
            *response_dm_failure = outreach.failure.clone().map(Box::new);
        }
        trusted_outcome = Some(completion.outcome.clone());
        completed_sections.extend(completion.presentation.sections.clone());
        guard.record_completed_sections(completed_sections.clone());
        let mut completed_batch = current.batch.clone();
        if let Some(row) = completed_batch
            .outcomes
            .iter_mut()
            .find(|row| row.call_id == completion.outcome.call_id)
        {
            *row = completion.outcome.clone();
        }
        guard.record_progress(completed_batch);
        let output = match serde_json::to_value(&completion.output) {
            Ok(value) => value,
            Err(_) => {
                let mut batch = current.batch.clone();
                if let Some(row) = batch
                    .outcomes
                    .iter_mut()
                    .find(|row| row.call_id == completion.outcome.call_id)
                {
                    *row = completion.outcome.clone();
                }
                guard.record_progress(batch);
                return guard.close_known("owner_output_serialization_failed");
            }
        };
        outcomes.push(ToolExecutionOutcome::Completed {
            call_id: first.resume.call.call_id.clone(),
            operation_id: first.resume.call.operation_id.clone(),
            result: output.clone(),
        });
        let mut current_outcomes = outcomes.clone();
        finish_skipped(&mut current_outcomes, &queue);
        let batch = frame(
            &current.batch,
            pending_id.as_str(),
            &current_outcomes,
            &projection_calls,
            trusted_outcome.as_ref(),
            current.locale,
        );
        guard.record_progress(batch.clone());
        let Some(RegistryEntry::Operation(operation)) =
            registry.entry(&first.resume.call.operation_id, surface)
        else {
            return guard.close_known("resume_operation_unavailable");
        };
        if !operation.output.validate(&output) {
            return guard.close_known("owner_output_contract_failed");
        }
        if !completion.continue_batch || queue.is_empty() {
            return guard.commit(ResultTerminalUpdate {
                terminal: completion.terminal,
                presentation: onecrew_local_runtime::PresentationProjection {
                    sections: completed_sections.clone(),
                    ..completion.presentation
                },
                continuation: if batch.outcomes.len() == 1 {
                    completion.continuation
                } else {
                    Some(execution_continuation(&batch, current.locale))
                },
                batch,
            });
        }
    }

    let phase_prefix = completed_sections.clone();
    while let Some(call) = queue.pop_front() {
        // Only the original generic pending call has already been confirmed.
        let needs_generic_confirmation = call.output_class == BindingOutputClass::Payload
            && call.approval == ApprovalClass::AttendedPrompt
            && (!outcomes.is_empty() || trusted_outcome.is_some());
        if needs_generic_confirmation {
            let Some(prompt) = call.prompt.as_deref() else {
                return guard.close_known("resume_contract_invalid");
            };
            let presentation = match confirmation_projection(
                &call.projection,
                &projection_calls,
                current.locale,
            ) {
                Ok(value) => value,
                Err(_) => return guard.close_known("resume_next_review_unavailable"),
            };
            let payload = call
                .generic_payload
                .expect("generic continuation bounded before dispatch");
            outcomes.push(input_required(&call.resume, None));
            finish_skipped(&mut outcomes, &queue);
            let batch = frame(
                &current.batch,
                pending_id.as_str(),
                &outcomes,
                &projection_calls,
                trusted_outcome.as_ref(),
                current.locale,
            );
            guard.record_progress(batch.clone());
            let next = platform.advance_result_confirmation(
                &submitted_binding,
                ExecutedResultInput {
                    origin: current.binding.origin.clone(),
                    view_kind: ResultViewKind::Confirmation,
                    batch,
                    presentation,
                    pending_action: Some(PendingUserAction::Confirm {
                        prompt: prompt.into(),
                        label: confirm_action_label(current.locale),
                        payload,
                    }),
                    initial_state: InitialResultState::Available,
                },
                completed_sections.clone(),
            );
            return match next {
                Ok(snapshot) => {
                    guard.disarm();
                    Ok(snapshot)
                }
                Err(_) => guard.close_known("resume_next_review_unavailable"),
            };
        }
        guard.mark_dispatch_may_have_started();
        match dispatch_prepared_output(call.invocation).await {
            Ok(BindingOutput::Payload(result)) => {
                let outcome = ToolExecutionOutcome::Completed {
                    call_id: call.resume.call.call_id.clone(),
                    operation_id: call.resume.call.operation_id.clone(),
                    result,
                };
                outcomes.push(outcome);
                let mut pending = outcomes.clone();
                finish_skipped(&mut pending, &queue);
                let batch = frame(
                    &current.batch,
                    pending_id.as_str(),
                    &pending,
                    &projection_calls,
                    trusted_outcome.as_ref(),
                    current.locale,
                );
                guard.record_progress(batch.clone());
                completed_sections = match readonly_prefix_projection(
                    &outcomes,
                    &projection_calls,
                    current.locale,
                    &phase_prefix,
                    current.batch.outcomes.len(),
                ) {
                    Ok(sections) => sections,
                    Err(_) => {
                        return guard.commit(presentation_failure_terminal_update(
                            batch,
                            current.presentation.kind.clone(),
                            current.locale,
                        ));
                    }
                };
                guard.record_completed_sections(completed_sections.clone());
            }
            Ok(BindingOutput::OwnerNoop(noop)) => {
                let outcome = super::execute::owner_noop_outcome(
                    &noop,
                    &call.resume.call.call_id,
                    &call.resume.call.operation_id,
                );
                outcomes.push(outcome);
                completed_sections = match readonly_prefix_projection(
                    &outcomes,
                    &projection_calls,
                    current.locale,
                    &phase_prefix,
                    current.batch.outcomes.len(),
                ) {
                    Ok(sections) => sections,
                    Err(_) => return guard.close_known("resume_next_review_unavailable"),
                };
                guard.record_completed_sections(completed_sections.clone());
                if matches!(
                    outcomes.last(),
                    Some(
                        ToolExecutionOutcome::Failed { .. } | ToolExecutionOutcome::Unknown { .. }
                    )
                ) {
                    finish_skipped(&mut outcomes, &queue);
                    queue.clear();
                }
            }
            Ok(BindingOutput::OwnerReview(review)) => {
                let peer = call
                    .owner_peer
                    .expect("owner continuation bounded before dispatch");
                let review_summary = review.creator_save_review_summary();
                outcomes.push(input_required(&call.resume, review_summary));
                finish_skipped(&mut outcomes, &queue);
                let batch = frame(
                    &current.batch,
                    pending_id.as_str(),
                    &outcomes,
                    &projection_calls,
                    trusted_outcome.as_ref(),
                    current.locale,
                );
                guard.record_progress(batch.clone());
                return match platform.advance_owner_review(
                    &submitted_binding,
                    *review,
                    batch,
                    Some(peer),
                    completed_sections.clone(),
                ) {
                    Ok(snapshot) => {
                        guard.disarm();
                        Ok(snapshot)
                    }
                    Err(_) => guard.close_known("resume_next_review_unavailable"),
                };
            }
            Err(error) => {
                let failure = project_execute_failure(&error);
                if failure.dm_failure.is_some() {
                    *response_dm_failure = failure.dm_failure.clone();
                }
                outcomes.push(failure_outcome(&call.resume, failure));
                finish_skipped(&mut outcomes, &queue);
                queue.clear();
            }
        }
        let mut pending = outcomes.clone();
        finish_skipped(&mut pending, &queue);
        guard.record_progress(frame(
            &current.batch,
            pending_id.as_str(),
            &pending,
            &projection_calls,
            trusted_outcome.as_ref(),
            current.locale,
        ));
    }
    let batch = frame(
        &current.batch,
        pending_id.as_str(),
        &outcomes,
        &projection_calls,
        trusted_outcome.as_ref(),
        current.locale,
    );
    let stopped = outcomes
        .iter()
        .zip(&projection_calls)
        .filter(|(outcome, _)| !matches!(outcome, ToolExecutionOutcome::Completed { .. }))
        .map(|(outcome, projection)| (outcome.clone(), projection.clone()))
        .collect::<Vec<_>>();
    let (terminal_outcomes, terminal_calls): (Vec<_>, Vec<_>) = if stopped.is_empty() {
        (outcomes.clone(), projection_calls.clone())
    } else {
        stopped.into_iter().unzip()
    };
    let kind = if trusted_outcome.is_some() {
        onecrew_local_runtime::ResultPresentationKind::new("batch_result")
            .expect("static presentation kind")
    } else {
        current.presentation.kind.clone()
    };
    let (mut presentation, terminal) = match resumed_terminal_projection(
        &terminal_outcomes,
        &terminal_calls,
        kind,
        current.locale,
    ) {
        Ok(value) => value,
        Err(_) => {
            return guard.commit(presentation_failure_terminal_update(
                batch,
                current.presentation.kind,
                current.locale,
            ));
        }
    };
    // Every successful call's safe sections are already retained. The final
    // projection adds only failure/stop information, without duplicating receipts.
    if outcomes
        .iter()
        .all(|outcome| matches!(outcome, ToolExecutionOutcome::Completed { .. }))
    {
        presentation.sections = completed_sections;
    }
    guard.commit(ResultTerminalUpdate {
        terminal,
        presentation,
        continuation: Some(execution_continuation(&batch, current.locale)),
        batch,
    })
}

pub(super) fn classify_submitted_owner_failure(
    error: onecrew_local_runtime::OwnerExecutionFailure,
) -> ClassifiedOwnerFailure {
    let code = PublicFailureCode::parse(error.code).expect("owner execution exposes static codes");
    let mut classified = match error.effect {
        onecrew_local_runtime::OwnerExecutionEffect::KnownNoEffect => {
            ClassifiedOwnerFailure::known_no_effect(code, false, None)
        }
        onecrew_local_runtime::OwnerExecutionEffect::Applied => {
            ClassifiedOwnerFailure::applied(code, None)
        }
        onecrew_local_runtime::OwnerExecutionEffect::Unknown => {
            ClassifiedOwnerFailure::unknown(code, None)
        }
    }
    .with_message(error.message);
    if let Some(dm_failure) = error.dm_failure {
        classified = classified.with_dm_failure(*dm_failure);
    }
    classified
}

fn input_required(
    call: &ResumeCall,
    review_summary: Option<onecrew_local_runtime::CreatorSaveReviewSummary>,
) -> ToolExecutionOutcome {
    ToolExecutionOutcome::InputRequired {
        call_id: call.call.call_id.clone(),
        operation_id: call.call.operation_id.clone(),
        input_kind: ToolInputKind::Confirmation,
        review_summary,
    }
}

fn finish_skipped(outcomes: &mut Vec<ToolExecutionOutcome>, queue: &VecDeque<PreparedCall<'_>>) {
    outcomes.extend(queue.iter().map(|call| ToolExecutionOutcome::Skipped {
        call_id: call.resume.call.call_id.clone(),
        operation_id: call.resume.call.operation_id.clone(),
        reason: SKIPPED_AFTER_STOP.into(),
    }));
}

fn failure_outcome(
    call: &ResumeCall,
    failure: crate::operation_registry::execution::ExecuteFailurePresentation,
) -> ToolExecutionOutcome {
    // Resuming takes the same care as the first attempt: an effect the owner
    // proved applied is not a failed call, and saying it failed would invite a
    // second send of something already sent.
    if matches!(
        failure.effect,
        ExecuteEffectState::Unknown | ExecuteEffectState::Applied
    ) {
        ToolExecutionOutcome::Unknown {
            call_id: call.call.call_id.clone(),
            operation_id: call.call.operation_id.clone(),
            failure,
            review_summary: None,
        }
    } else {
        ToolExecutionOutcome::Failed {
            call_id: call.call.call_id.clone(),
            operation_id: call.call.operation_id.clone(),
            failure,
            review_summary: None,
        }
    }
}

fn frame(
    prior: &ExecutedBatch,
    pending_id: &str,
    outcomes: &[ToolExecutionOutcome],
    projections: &[ProjectionCall],
    trusted: Option<&ExecutedCallOutcome>,
    locale: onecrew_local_runtime::PresentationLocale,
) -> ExecutedBatch {
    let mut resumed = executed_batch(outcomes, projections, locale);
    if let Some(trusted) = trusted
        && let Some(row) = resumed
            .outcomes
            .iter_mut()
            .find(|row| row.call_id == trusted.call_id)
    {
        *row = trusted.clone();
    }
    combine_executed_batch(prior, pending_id, resumed)
        .expect("the validated resume preserves the original bounded call sequence")
}
