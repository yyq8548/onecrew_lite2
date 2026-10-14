use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use futures::{StreamExt, stream::FuturesUnordered};
use onecrew_local_runtime::{
    ExecutedResultInput, InitialResultState, LocalPlatform, OwnerActionPayload, PendingUserAction,
    ResultOrigin, ResultRef, ResultStoreError, ResultViewKind, TerminalResult,
};
use schemars::JsonSchema;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::operation_registry::{
    descriptor::{ApprovalClass, EntryId, RegistryEntry, SurfaceId},
    execution::{
        BindingOutput, BindingOutputClass, BoundedPublicValue, ExecuteEffectState, ExecuteFailure,
        ExecuteFailurePresentation, ExecutionContext, ExecutionDeclaration, PreparedInvocation,
        PublicFailureCode, dispatch_prepared_output, prepare_invocation, project_execute_failure,
    },
    registration::OperationRegistry,
};

use super::{
    contract::{
        AskUserQuestionInput, PresentationRequest, ToolExecuteInput, ToolExecuteResult,
        ToolExecutionCall, ToolExecutionOutcome, ToolInputKind, ToolPresentationFailure,
    },
    projection::{
        ProjectionCall, batch_is_presentable, confirm_action_label, confirmation_projection,
        executed_batch, presentation_failure_projection, readonly_prefix_projection,
        terminal_projection,
    },
    resume::{ResumeCall, encode_resume_envelope},
    state::mint_choice_result_v2,
};

const MAX_BATCH_REQUEST_ID_BYTES: usize = 256;
pub(super) const SKIPPED_AFTER_STOP: &str = "earlier_call_requires_attention";
/// A card's locale is frozen at presentation (`present_result_with_locale`),
/// which happens after this mint, so no card locale exists here yet. Copy
/// projected at mint time is written in the catalog's source locale; every
/// transition after presentation projects in the card's own frozen locale.
const MINT_TIME_LOCALE: onecrew_local_runtime::PresentationLocale =
    onecrew_local_runtime::PresentationLocale::English;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case", deny_unknown_fields)]
pub(in crate::operation_registry) enum ToolExecuteOrchestrationFailure {
    SearchPlanConfirmationRequired {
        call_id: EntryId,
        operation_id: EntryId,
        prepare_operation_id: String,
    },
    SearchPlanPayloadTooLarge,
    SearchPlanScopeInvalid {
        call_id: EntryId,
        operation_id: EntryId,
        reason: super::search_plan::SearchPlanScopeInvalidReason,
        /// JSON Pointer relative to this call's arguments; contains only public
        /// field names and array indices, never rejected values.
        parameter_pointer: String,
    },
    SearchPlanApprovalRejected {
        call_id: EntryId,
        operation_id: EntryId,
        reason: onecrew_local_runtime::SearchPlanApprovalError,
    },
    SearchPlanInvalid {
        call_id: EntryId,
        operation_id: EntryId,
        reason: onecrew_local_runtime::SearchPlanEditorValidationReason,
        /// JSON Pointer relative to this call's arguments; contains only public
        /// field names and array indices, never rejected values.
        parameter_pointer: String,
    },
    InvalidBatchRequestId,
    PresentationUnavailable,
    ChoiceBatchRequiresSingleCall,
    Prevalidation {
        call_id: EntryId,
        operation_id: EntryId,
        /// Boxed: the presentation carries the bounded owner message, which
        /// would otherwise make this the largest variant of every batch
        /// result.
        failure: Box<ExecuteFailurePresentation>,
    },
}

struct PreparedBatchCall<'entry> {
    call_id: EntryId,
    operation_id: EntryId,
    approval: ApprovalClass,
    output_class: BindingOutputClass,
    approval_prompt: Option<String>,
    projection: ProjectionCall,
    resume: ResumeCall,
    action: PreparedBatchAction<'entry>,
    confirmation_projection: Option<onecrew_local_runtime::PresentationProjection>,
    confirmation_payload: Option<OwnerActionPayload>,
    owner_peer: Option<onecrew_local_runtime::PeerResumePayload>,
    execution: ExecutionDeclaration,
}

enum PreparedBatchAction<'entry> {
    Invoke(PreparedInvocation<'entry>),
    Choice(AskUserQuestionInput),
}

impl OperationRegistry {
    pub(in crate::operation_registry) async fn tool_execute_v2(
        &self,
        platform: &LocalPlatform,
        origin: &ResultOrigin,
        surface: SurfaceId,
        batch_request_id: &str,
        input: &ToolExecuteInput,
    ) -> Result<ToolExecuteResult, ToolExecuteOrchestrationFailure> {
        self.tool_execute_v2_with_minter(
            platform,
            origin,
            surface,
            batch_request_id,
            input,
            &|input| platform.mint_execution_result(input),
        )
        .await
    }

    pub(super) async fn tool_execute_v2_with_minter(
        &self,
        platform: &LocalPlatform,
        origin: &ResultOrigin,
        surface: SurfaceId,
        batch_request_id: &str,
        input: &ToolExecuteInput,
        minter: &(dyn Fn(ExecutedResultInput) -> Result<ResultRef, ResultStoreError> + Sync),
    ) -> Result<ToolExecuteResult, ToolExecuteOrchestrationFailure> {
        let batch_id = derive_batch_id(batch_request_id)?;
        let batch_started = Instant::now();

        let mut prepared_calls = Vec::with_capacity(input.calls.len());

        for (index, call) in input.calls.iter().enumerate() {
            let search_policy = super::search_plan::call_search_policy(call);
            if super::search_plan::requires_plan(self, surface, call.operation_id.as_str())
                && (search_policy == onecrew_local_runtime::SearchPlanPolicy::RequireConfirmation
                    && !call
                        .arguments
                        .get("confirmed_plan_ref")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|reference| !reference.is_empty()))
            {
                return Err(
                    ToolExecuteOrchestrationFailure::SearchPlanConfirmationRequired {
                        call_id: call.call_id.clone(),
                        operation_id: call.operation_id.clone(),
                        prepare_operation_id: super::search_plan::OPERATION_ID.to_owned(),
                    },
                );
            }
            let operation = match self.entry(&call.operation_id, surface) {
                Some(RegistryEntry::Operation(operation)) => operation.as_ref(),
                Some(RegistryEntry::Playbook(_) | RegistryEntry::ToolkitGuide(_)) | None => {
                    return Err(prevalidation_failure(
                        call,
                        ExecuteFailure::unknown_operation(),
                    ));
                }
            };
            let request_id = derive_call_request_id(batch_request_id, index, &call.call_id);
            let arguments = serde_json::Value::Object(call.arguments.clone());
            let mut invocation = prepare_invocation(
                operation,
                ExecutionContext {
                    surface,
                    request_id: request_id.clone(),
                    connection: call.connection.as_ref(),
                    confirmed: false,
                },
                &call.schema_revision,
                arguments.clone(),
            )
            .map_err(|failure| prevalidation_failure(call, failure))?;
            if super::search_plan::requires_plan(self, surface, call.operation_id.as_str())
                && search_policy == onecrew_local_runtime::SearchPlanPolicy::RequireConfirmation
            {
                let authorization = super::search_plan::authorize_call(platform, call, &invocation)
                    .map_err(|reason| {
                        ToolExecuteOrchestrationFailure::SearchPlanApprovalRejected {
                            call_id: call.call_id.clone(),
                            operation_id: call.operation_id.clone(),
                            reason,
                        }
                    })?;
                invocation.authorize_search(authorization);
            }
            let resume = ResumeCall {
                request_id: request_id.clone(),
                call: call.clone(),
            };
            if operation.binding.output_class() == BindingOutputClass::OwnerReview {
                let digest = super::resume::owner_call_digest(surface, &resume, operation)
                    .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                invocation.bind_owner_review_context(origin, call.call_id.as_str(), digest);
            }
            let action = if operation.view_kind
                == crate::operation_registry::descriptor::OperationViewKind::Choice
            {
                let input = serde_json::from_value::<AskUserQuestionInput>(arguments)
                    .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                drop(invocation);
                PreparedBatchAction::Choice(input)
            } else {
                PreparedBatchAction::Invoke(invocation)
            };
            prepared_calls.push(PreparedBatchCall {
                call_id: call.call_id.clone(),
                operation_id: call.operation_id.clone(),
                approval: operation.approval,
                output_class: operation.binding.output_class(),
                approval_prompt: operation
                    .approval_request
                    .as_ref()
                    .map(|request| request.message.clone()),
                projection: ProjectionCall::new(
                    operation,
                    call.call_id.clone(),
                    call.arguments.clone(),
                ),
                resume,
                action,
                confirmation_projection: None,
                confirmation_payload: None,
                owner_peer: None,
                execution: operation.binding.execution(),
            });
        }

        if input
            .calls
            .iter()
            .any(|call| call.operation_id.as_str() == super::search_plan::OPERATION_ID)
        {
            if input.calls.len() != 1 {
                return Err(ToolExecuteOrchestrationFailure::ChoiceBatchRequiresSingleCall);
            }
            let call = &input.calls[0];
            let result_ref = super::search_plan::prepare_search_plan(self, surface, origin, call, minter)
                .map_err(|error| match error {
                    super::search_plan::PrepareSearchPlanError::ScopeInvalid { index, reason, pointer } => ToolExecuteOrchestrationFailure::SearchPlanScopeInvalid {
                        call_id: call.call_id.clone(), operation_id: call.operation_id.clone(), reason, parameter_pointer: format!("/scopes/{index}{pointer}"),
                    },
                    super::search_plan::PrepareSearchPlanError::DraftInvalid(error)
                        if error.reason
                            == onecrew_local_runtime::SearchPlanEditorValidationReason::PayloadTooLarge =>
                    {
                        ToolExecuteOrchestrationFailure::SearchPlanPayloadTooLarge
                    }
                    super::search_plan::PrepareSearchPlanError::DraftInvalid(error) => {
                        ToolExecuteOrchestrationFailure::SearchPlanInvalid {
                            call_id: call.call_id.clone(),
                            operation_id: call.operation_id.clone(),
                            reason: error.reason,
                            parameter_pointer: error.location.parameter_pointer(),
                        }
                    }
                    // Preserve the storage/presentation boundary, including InvalidInput.
                    super::search_plan::PrepareSearchPlanError::Store(_) => {
                        ToolExecuteOrchestrationFailure::PresentationUnavailable
                    }
                })?;
            return Ok(ToolExecuteResult {
                batch_id,
                outcomes: vec![ToolExecutionOutcome::InputRequired {
                    call_id: call.call_id.clone(),
                    operation_id: call.operation_id.clone(),
                    input_kind: ToolInputKind::Confirmation,
                    review_summary: None,
                }],
                result_ref: Some(result_ref),
                presentation_failure: None,
            });
        }

        let choice_count = prepared_calls
            .iter()
            .filter(|prepared| matches!(&prepared.action, PreparedBatchAction::Choice(_)))
            .count();
        if choice_count > 0 {
            if prepared_calls.len() != 1 {
                return Err(ToolExecuteOrchestrationFailure::ChoiceBatchRequiresSingleCall);
            }
            let prepared = prepared_calls
                .pop()
                .expect("one prevalidated choice call is present");
            let PreparedBatchAction::Choice(choice) = prepared.action else {
                unreachable!("choice count proves a choice action")
            };
            let outcome = ToolExecutionOutcome::InputRequired {
                call_id: prepared.call_id.clone(),
                operation_id: prepared.operation_id.clone(),
                input_kind: ToolInputKind::Choice,
                review_summary: None,
            };
            let minted = mint_choice_result_v2(
                platform,
                origin,
                prepared.call_id,
                prepared.operation_id,
                choice,
            )
            .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
            return Ok(ToolExecuteResult {
                batch_id,
                outcomes: vec![outcome],
                result_ref: Some(minted.result_ref),
                presentation_failure: None,
            });
        }

        let projection_calls = prepared_calls
            .iter()
            .map(|prepared| prepared.projection.clone())
            .collect::<Vec<_>>();
        let presentation_safe = batch_is_presentable(&projection_calls);
        let presentation_required = input.presentation == PresentationRequest::Requested
            || prepared_calls.iter().any(|prepared| {
                prepared.approval == ApprovalClass::AttendedPrompt
                    || prepared.output_class == BindingOutputClass::OwnerReview
            });
        if presentation_required && !presentation_safe {
            return Err(ToolExecuteOrchestrationFailure::PresentationUnavailable);
        }
        for index in 0..prepared_calls.len() {
            if prepared_calls[index].output_class == BindingOutputClass::OwnerReview {
                prepared_calls[index].owner_peer = Some(
                    super::resume::encode_owner_resume_envelope(
                        surface,
                        prepared_calls[index].resume.clone(),
                        prepared_calls[index + 1..]
                            .iter()
                            .map(|remaining| remaining.resume.clone())
                            .collect(),
                    )
                    .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?,
                );
            }
            if prepared_calls[index].approval == ApprovalClass::AttendedPrompt
                && prepared_calls[index].output_class == BindingOutputClass::Payload
            {
                // The prompt is the pending action's own copy rather than part of
                // the projection, and it stays required before any owner runs.
                if prepared_calls[index].approval_prompt.is_none() {
                    return Err(ToolExecuteOrchestrationFailure::PresentationUnavailable);
                }
                let projection = confirmation_projection(
                    &prepared_calls[index].projection,
                    &projection_calls,
                    MINT_TIME_LOCALE,
                )
                .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                let payload = encode_resume_envelope(
                    prepared_calls[index].resume.clone(),
                    prepared_calls[index + 1..]
                        .iter()
                        .map(|remaining| remaining.resume.clone())
                        .collect(),
                )
                .map_err(|_| ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                prepared_calls[index].confirmation_projection = Some(projection);
                prepared_calls[index].confirmation_payload = Some(payload);
            }
        }

        let mut pending_owner_review = None;
        let mut pending_owner_noop = None;
        let mut pending_action = None;
        let mut pending_confirmation_projection = None;
        let independent_reads = prepared_calls
            .iter()
            .all(|prepared| prepared.execution == ExecutionDeclaration::IndependentRead);
        let prevalidation_elapsed = batch_started.elapsed();
        let dispatch_started = Instant::now();
        let (outcomes, owner_may_have_run) = if independent_reads {
            execute_independent_reads(prepared_calls, &batch_id, dispatch_started).await
        } else {
            let mut outcomes = Vec::with_capacity(prepared_calls.len());
            let mut queue = VecDeque::from(prepared_calls);
            let mut owner_may_have_run = false;
            while let Some(prepared) = queue.pop_front() {
                if prepared.approval == ApprovalClass::AttendedPrompt
                    && prepared.output_class == BindingOutputClass::Payload
                {
                    if !presentation_safe {
                        return Err(ToolExecuteOrchestrationFailure::PresentationUnavailable);
                    }
                    let prompt = prepared
                        .approval_prompt
                        .clone()
                        .ok_or(ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                    let payload = prepared
                        .confirmation_payload
                        .ok_or(ToolExecuteOrchestrationFailure::PresentationUnavailable)?;
                    pending_confirmation_projection = prepared.confirmation_projection.clone();
                    pending_action = Some(PendingUserAction::Confirm {
                        prompt,
                        label: confirm_action_label(MINT_TIME_LOCALE),
                        payload,
                    });
                    outcomes.push(ToolExecutionOutcome::InputRequired {
                        call_id: prepared.call_id,
                        operation_id: prepared.operation_id,
                        input_kind: ToolInputKind::Confirmation,
                        review_summary: None,
                    });
                    outcomes.extend(queue.drain(..).map(|remaining| {
                        ToolExecutionOutcome::Skipped {
                            call_id: remaining.call_id,
                            operation_id: remaining.operation_id,
                            reason: SKIPPED_AFTER_STOP.to_owned(),
                        }
                    }));
                    break;
                }
                let PreparedBatchAction::Invoke(invocation) = prepared.action else {
                    unreachable!("choice calls return before owner dispatch")
                };
                owner_may_have_run = true;
                let operation_started = Instant::now();
                log::debug!(
                    "mcp execute operation started batch_id={} call_id={} operation_id={} mode=sequential queue_ms={}",
                    batch_id,
                    prepared.call_id,
                    prepared.operation_id,
                    dispatch_started.elapsed().as_millis(),
                );
                let dispatch_result = dispatch_prepared_output(invocation).await;
                let status = match &dispatch_result {
                    Ok(BindingOutput::Payload(_)) => "completed",
                    Ok(BindingOutput::OwnerNoop(noop)) => owner_noop_status(noop),
                    Ok(BindingOutput::OwnerReview(_)) => "input_required",
                    Err(_) => "failed",
                };
                log::debug!(
                    "mcp execute operation finished batch_id={} call_id={} operation_id={} mode=sequential status={} operation_ms={}",
                    batch_id,
                    prepared.call_id,
                    prepared.operation_id,
                    status,
                    operation_started.elapsed().as_millis(),
                );
                match dispatch_result {
                    Ok(BindingOutput::Payload(result)) => {
                        outcomes.push(ToolExecutionOutcome::Completed {
                            call_id: prepared.call_id,
                            operation_id: prepared.operation_id,
                            result,
                        })
                    }
                    Ok(BindingOutput::OwnerNoop(noop)) => {
                        let outcome =
                            owner_noop_outcome(&noop, &prepared.call_id, &prepared.operation_id);
                        if outcomes.is_empty() && queue.is_empty() {
                            pending_owner_noop = Some(noop);
                        }
                        let stops = matches!(
                            outcome,
                            ToolExecutionOutcome::Failed { .. }
                                | ToolExecutionOutcome::Unknown { .. }
                        );
                        outcomes.push(outcome);
                        if stops {
                            outcomes.extend(queue.drain(..).map(|remaining| {
                                ToolExecutionOutcome::Skipped {
                                    call_id: remaining.call_id,
                                    operation_id: remaining.operation_id,
                                    reason: SKIPPED_AFTER_STOP.to_owned(),
                                }
                            }));
                            break;
                        }
                    }
                    Ok(BindingOutput::OwnerReview(review)) => {
                        let review_summary = review.creator_save_review_summary();
                        let peer = prepared
                            .owner_peer
                            .expect("owner peer was bounded before any owner ran");
                        pending_owner_review = Some((review, peer));
                        outcomes.push(ToolExecutionOutcome::InputRequired {
                            call_id: prepared.call_id,
                            operation_id: prepared.operation_id,
                            input_kind: ToolInputKind::Confirmation,
                            review_summary,
                        });
                        outcomes.extend(queue.drain(..).map(|remaining| {
                            ToolExecutionOutcome::Skipped {
                                call_id: remaining.call_id,
                                operation_id: remaining.operation_id,
                                reason: SKIPPED_AFTER_STOP.into(),
                            }
                        }));
                        break;
                    }
                    Err(failure) => {
                        let failure = project_execute_failure(&failure);
                        // An effect that already applied is not a failed call. The
                        // owner proved the external write happened and only later
                        // work did not finish, so reporting it as failed told a
                        // user their email had not been sent when it had, and
                        // invited them to send it again.
                        let outcome = if matches!(
                            failure.effect,
                            ExecuteEffectState::Unknown | ExecuteEffectState::Applied
                        ) {
                            ToolExecutionOutcome::Unknown {
                                call_id: prepared.call_id,
                                operation_id: prepared.operation_id,
                                failure,
                                review_summary: None,
                            }
                        } else {
                            ToolExecutionOutcome::Failed {
                                call_id: prepared.call_id,
                                operation_id: prepared.operation_id,
                                failure,
                                review_summary: None,
                            }
                        };
                        outcomes.push(outcome);
                        outcomes.extend(queue.drain(..).map(|remaining| {
                            ToolExecutionOutcome::Skipped {
                                call_id: remaining.call_id,
                                operation_id: remaining.operation_id,
                                reason: SKIPPED_AFTER_STOP.to_owned(),
                            }
                        }));
                        break;
                    }
                }
            }
            (outcomes, owner_may_have_run)
        };
        log::debug!(
            "mcp execute batch settled batch_id={} calls={} mode={} prevalidation_ms={} dispatch_ms={} total_ms={}",
            batch_id,
            outcomes.len(),
            if independent_reads {
                "independent_read"
            } else {
                "sequential"
            },
            prevalidation_elapsed.as_millis(),
            dispatch_started.elapsed().as_millis(),
            batch_started.elapsed().as_millis(),
        );
        if pending_owner_review.is_some() || pending_action.is_some() {
            log::debug!(
                "mcp execute batch awaiting user review batch_id={} calls={}",
                batch_id,
                outcomes.len(),
            );
        }

        if let Some((review, peer)) = pending_owner_review {
            let batch = executed_batch(&outcomes, &projection_calls, MINT_TIME_LOCALE);
            let completed_sections = match readonly_prefix_projection(
                &outcomes,
                &projection_calls,
                MINT_TIME_LOCALE,
                &[],
                projection_calls.len(),
            ) {
                Ok(sections) => sections,
                Err(failure) => {
                    log::warn!(
                        "v2 execute: readonly completed prefix is not presentable ({failure:?})"
                    );
                    return Ok(execute_with_presentation_failure(
                        batch_id,
                        outcomes,
                        ToolPresentationFailure::ProjectionUnavailable,
                    ));
                }
            };
            return match platform.mint_owner_review(*review, batch, Some(peer), completed_sections)
            {
                Ok(result_ref) => Ok(ToolExecuteResult {
                    batch_id,
                    outcomes,
                    result_ref: Some(result_ref),
                    presentation_failure: None,
                }),
                Err(error) => Ok(execute_with_presentation_failure(
                    batch_id,
                    outcomes,
                    presentation_failure(error),
                )),
            };
        }
        let should_mint =
            pending_action.is_some() || input.presentation == PresentationRequest::Requested;
        let mut projection_failure = None;
        let result_ref = if should_mint {
            if !presentation_safe {
                return Err(ToolExecuteOrchestrationFailure::PresentationUnavailable);
            }
            let batch = pending_owner_noop
                .as_ref()
                .map(|noop| onecrew_local_runtime::ExecutedBatch {
                    outcomes: vec![noop.outcome().clone()],
                })
                .unwrap_or_else(|| executed_batch(&outcomes, &projection_calls, MINT_TIME_LOCALE));
            let (view_kind, presentation, initial_state) = if let Some(presentation) =
                pending_confirmation_projection
            {
                (
                    ResultViewKind::Confirmation,
                    presentation,
                    InitialResultState::Available,
                )
            } else if let Some(noop) = pending_owner_noop.as_ref() {
                (
                    ResultViewKind::ReadOnly,
                    noop.presentation().clone(),
                    initial_state_from_terminal(noop.terminal().clone()),
                )
            } else {
                let (presentation, initial_state) = match terminal_projection(
                    &outcomes,
                    &projection_calls,
                    MINT_TIME_LOCALE,
                ) {
                    Ok(projected) => projected,
                    // The owners already ran and their outcomes stand.
                    // A card that says the result cannot be shown
                    // completely is still a card; a null reference is
                    // the caller being told nothing at all.
                    Err(failure) => {
                        projection_failure = Some(ToolPresentationFailure::ProjectionUnavailable);
                        log::warn!(
                            "v2 execute: batch {batch_id} result is not presentable ({failure:?})"
                        );
                        presentation_failure_projection(&batch, &projection_calls, MINT_TIME_LOCALE)
                    }
                };
                (ResultViewKind::ReadOnly, presentation, initial_state)
            };
            match minter(ExecutedResultInput {
                origin: origin.clone(),
                view_kind,
                batch,
                presentation,
                pending_action,
                initial_state,
            }) {
                Ok(result_ref) => Some(result_ref),
                Err(error) if owner_may_have_run => {
                    return Ok(execute_with_presentation_failure(
                        batch_id,
                        outcomes,
                        presentation_failure(error),
                    ));
                }
                Err(_) => return Err(ToolExecuteOrchestrationFailure::PresentationUnavailable),
            }
        } else {
            None
        };

        Ok(ToolExecuteResult {
            batch_id,
            outcomes,
            result_ref,
            presentation_failure: projection_failure,
        })
    }
}

const MAX_INDEPENDENT_READS: usize = 4;

type IndependentDispatchFuture<'entry> =
    Pin<Box<dyn Future<Output = IndependentDispatchResult> + Send + 'entry>>;

struct IndependentDispatchResult {
    index: usize,
    started: bool,
    output: Option<Result<BindingOutput, ExecuteFailure>>,
}

struct IndependentInvocationTrace {
    batch_id: String,
    call_id: EntryId,
    operation_id: EntryId,
    queued_at: Instant,
}

/// Execute a batch whose sealed declarations all permit independent reads.
///
/// The queue is deliberately driven by completion rather than by spawning a
/// task per call. At most four owner futures are in the collection at once;
/// after a stopping outcome is observed, no new future is inserted, while the
/// collection is still fully drained so every already-started owner call is
/// settled by this lifecycle.
async fn execute_independent_reads<'entry>(
    prepared_calls: Vec<PreparedBatchCall<'entry>>,
    batch_id: &str,
    queued_at: Instant,
) -> (Vec<ToolExecutionOutcome>, bool) {
    let call_ids = prepared_calls
        .iter()
        .map(|prepared| prepared.call_id.clone())
        .collect::<Vec<_>>();
    let operation_ids = prepared_calls
        .iter()
        .map(|prepared| prepared.operation_id.clone())
        .collect::<Vec<_>>();
    let mut invocations = prepared_calls
        .into_iter()
        .map(|prepared| match prepared.action {
            PreparedBatchAction::Invoke(invocation) => Some(invocation),
            PreparedBatchAction::Choice(_) => {
                unreachable!("choice operations are handled before independent read scheduling")
            }
        })
        .collect::<Vec<_>>();
    let mut outcomes = (0..invocations.len())
        .map(|_| None)
        .collect::<Vec<Option<ToolExecutionOutcome>>>();
    let mut started = vec![false; invocations.len()];
    let mut next_index = 0;
    let mut stopped = false;
    let stop = Arc::new(AtomicBool::new(false));
    let mut in_flight = FuturesUnordered::<IndependentDispatchFuture<'entry>>::new();

    while next_index < invocations.len() && in_flight.len() < MAX_INDEPENDENT_READS {
        push_independent_invocation(
            &mut in_flight,
            &mut invocations,
            Arc::clone(&stop),
            IndependentInvocationTrace {
                batch_id: batch_id.to_owned(),
                call_id: call_ids[next_index].clone(),
                operation_id: operation_ids[next_index].clone(),
                queued_at,
            },
            next_index,
        );
        next_index += 1;
    }

    while let Some(completed) = in_flight.next().await {
        let index = completed.index;
        if !completed.started {
            continue;
        }
        started[index] = true;
        let outcome = match completed.output.expect("started dispatch has an output") {
            Ok(BindingOutput::Payload(result)) => ToolExecutionOutcome::Completed {
                call_id: call_ids[index].clone(),
                operation_id: operation_ids[index].clone(),
                result,
            },
            Ok(BindingOutput::OwnerNoop(noop)) => {
                let outcome = owner_noop_outcome(&noop, &call_ids[index], &operation_ids[index]);
                if matches!(
                    outcome,
                    ToolExecutionOutcome::Failed { .. } | ToolExecutionOutcome::Unknown { .. }
                ) {
                    stop.store(true, Ordering::Release);
                    stopped = true;
                }
                outcome
            }
            // Registration rejects this combination. Retain a closed stop
            // outcome if a future registry construction ever violates it,
            // rather than treating an owner review as ordinary read data.
            Ok(BindingOutput::OwnerReview(_)) => {
                stopped = true;
                stop.store(true, Ordering::Release);
                ToolExecutionOutcome::InputRequired {
                    call_id: call_ids[index].clone(),
                    operation_id: operation_ids[index].clone(),
                    input_kind: ToolInputKind::Confirmation,
                    review_summary: None,
                }
            }
            Err(failure) => {
                let failure = project_execute_failure(&failure);
                stopped = true;
                stop.store(true, Ordering::Release);
                if matches!(
                    failure.effect,
                    ExecuteEffectState::Unknown | ExecuteEffectState::Applied
                ) {
                    ToolExecutionOutcome::Unknown {
                        call_id: call_ids[index].clone(),
                        operation_id: operation_ids[index].clone(),
                        failure,
                        review_summary: None,
                    }
                } else {
                    ToolExecutionOutcome::Failed {
                        call_id: call_ids[index].clone(),
                        operation_id: operation_ids[index].clone(),
                        failure,
                        review_summary: None,
                    }
                }
            }
        };
        outcomes[index] = Some(outcome);

        if !stopped && next_index < invocations.len() {
            push_independent_invocation(
                &mut in_flight,
                &mut invocations,
                Arc::clone(&stop),
                IndependentInvocationTrace {
                    batch_id: batch_id.to_owned(),
                    call_id: call_ids[next_index].clone(),
                    operation_id: operation_ids[next_index].clone(),
                    queued_at,
                },
                next_index,
            );
            next_index += 1;
        }
    }

    let outcomes = outcomes
        .into_iter()
        .enumerate()
        .map(|(index, outcome)| {
            outcome.unwrap_or_else(|| ToolExecutionOutcome::Skipped {
                call_id: call_ids[index].clone(),
                operation_id: operation_ids[index].clone(),
                reason: SKIPPED_AFTER_STOP.to_owned(),
            })
        })
        .collect();
    (outcomes, started.into_iter().any(|value| value))
}

fn initial_state_from_terminal(
    terminal: TerminalResult,
) -> onecrew_local_runtime::InitialResultState {
    match terminal {
        TerminalResult::Resolved(value) => {
            onecrew_local_runtime::InitialResultState::Resolved(value)
        }
        TerminalResult::Failed(value) => onecrew_local_runtime::InitialResultState::Failed(value),
        TerminalResult::Unknown(value) => onecrew_local_runtime::InitialResultState::Unknown(value),
    }
}

fn owner_noop_status(noop: &onecrew_local_runtime::PreparedOwnerNoop) -> &'static str {
    match noop.terminal() {
        TerminalResult::Resolved(_) => "completed",
        TerminalResult::Failed(_) => "failed",
        TerminalResult::Unknown(_) => "unknown",
    }
}

pub(super) fn owner_noop_outcome(
    noop: &onecrew_local_runtime::PreparedOwnerNoop,
    call_id: &EntryId,
    operation_id: &EntryId,
) -> ToolExecutionOutcome {
    match noop.terminal() {
        TerminalResult::Resolved(_) => ToolExecutionOutcome::Completed {
            call_id: call_id.clone(),
            operation_id: operation_id.clone(),
            result: noop.output().clone(),
        },
        TerminalResult::Failed(failure) => ToolExecutionOutcome::Failed {
            call_id: call_id.clone(),
            operation_id: operation_id.clone(),
            failure: owner_noop_failure(
                &failure.code,
                failure.retryable,
                ExecuteEffectState::KnownNoEffect,
                &failure.summary,
            ),
            review_summary: noop.review_summary().cloned(),
        },
        TerminalResult::Unknown(unknown) => ToolExecutionOutcome::Unknown {
            call_id: call_id.clone(),
            operation_id: operation_id.clone(),
            failure: owner_noop_failure(
                &unknown.code,
                false,
                ExecuteEffectState::Unknown,
                &unknown.summary,
            ),
            review_summary: noop.review_summary().cloned(),
        },
    }
}

fn owner_noop_failure(
    code: &str,
    retry_safe: bool,
    effect: ExecuteEffectState,
    summary: &str,
) -> ExecuteFailurePresentation {
    let code = PublicFailureCode::parse(code.to_owned()).unwrap_or_else(|_| {
        PublicFailureCode::parse("output_contract_failed")
            .expect("static output contract failure code")
    });
    ExecuteFailurePresentation {
        code,
        phase: super::super::execution::ExecuteFailurePhase::AfterInvocation,
        retry_safe,
        effect,
        recovery: None,
        provider_status: None,
        provider_evidence: None,
        message: BoundedPublicValue::bounded(summary),
        dm_failure: None,
    }
}

fn push_independent_invocation<'entry>(
    in_flight: &mut FuturesUnordered<IndependentDispatchFuture<'entry>>,
    invocations: &mut [Option<PreparedInvocation<'entry>>],
    stop: Arc<AtomicBool>,
    trace: IndependentInvocationTrace,
    index: usize,
) {
    let invocation = invocations[index]
        .take()
        .expect("an independent invocation is scheduled at most once");
    let IndependentInvocationTrace {
        batch_id,
        call_id,
        operation_id,
        queued_at,
    } = trace;
    in_flight.push(Box::pin(async move {
        if stop.load(Ordering::Acquire) {
            log::debug!(
                "mcp execute operation skipped batch_id={} call_id={} operation_id={} reason=stop_before_poll queue_ms={}",
                batch_id,
                call_id,
                operation_id,
                queued_at.elapsed().as_millis(),
            );
            return IndependentDispatchResult {
                index,
                started: false,
                output: None,
            };
        }
        let operation_started = Instant::now();
        log::debug!(
            "mcp execute operation started batch_id={} call_id={} operation_id={} queue_ms={}",
            batch_id,
            call_id,
            operation_id,
            queued_at.elapsed().as_millis(),
        );
        let output = dispatch_prepared_output(invocation).await;
        let status = match &output {
            Ok(BindingOutput::Payload(_)) => "completed",
            Ok(BindingOutput::OwnerNoop(noop)) => owner_noop_status(noop),
            Ok(BindingOutput::OwnerReview(_)) => "input_required",
            Err(_) => "failed",
        };
        log::debug!(
            "mcp execute operation finished batch_id={} call_id={} operation_id={} status={} operation_ms={}",
            batch_id,
            call_id,
            operation_id,
            status,
            operation_started.elapsed().as_millis(),
        );
        IndependentDispatchResult {
            index,
            started: true,
            output: Some(output),
        }
    }));
}

fn prevalidation_failure(
    call: &ToolExecutionCall,
    failure: ExecuteFailure,
) -> ToolExecuteOrchestrationFailure {
    ToolExecuteOrchestrationFailure::Prevalidation {
        call_id: call.call_id.clone(),
        operation_id: call.operation_id.clone(),
        failure: Box::new(project_execute_failure(&failure)),
    }
}

pub(super) fn execute_with_presentation_failure(
    batch_id: String,
    outcomes: Vec<ToolExecutionOutcome>,
    failure: ToolPresentationFailure,
) -> ToolExecuteResult {
    ToolExecuteResult {
        batch_id,
        outcomes,
        result_ref: None,
        presentation_failure: Some(failure),
    }
}

pub(super) fn presentation_failure(error: ResultStoreError) -> ToolPresentationFailure {
    // The public codes stay closed, and capacity is already distinct from a
    // rejection. Only this log carries the rule the store actually applied, so
    // a reader does not have to guess between the two from the code alone.
    log::warn!("v2 execute: the result store did not retain this presentation ({error})");
    match error {
        ResultStoreError::Capacity(_) => ToolPresentationFailure::Capacity,
        ResultStoreError::Unavailable(_) => ToolPresentationFailure::StoreUnavailable,
        ResultStoreError::Absent
        | ResultStoreError::Expired
        | ResultStoreError::ViewMismatch
        | ResultStoreError::OriginMismatch
        | ResultStoreError::FingerprintMismatch
        | ResultStoreError::RevisionConflict { .. }
        | ResultStoreError::ContinuationRevisionConflict { .. }
        | ResultStoreError::ContinuationLeaseMismatch
        | ResultStoreError::InvalidInput(_)
        | ResultStoreError::InvalidState(_)
        | ResultStoreError::Replay(_) => ToolPresentationFailure::StoreRejected,
    }
}

fn derive_batch_id(batch_request_id: &str) -> Result<String, ToolExecuteOrchestrationFailure> {
    validate_batch_request_id(batch_request_id)?;
    let digest = format!("{:x}", Sha256::digest(batch_request_id.as_bytes()));
    Ok(format!("batch-{}", &digest[..32]))
}

fn derive_call_request_id(batch_request_id: &str, index: usize, call_id: &EntryId) -> String {
    let mut hasher = Sha256::new();
    hasher.update(batch_request_id.as_bytes());
    hasher.update([0]);
    hasher.update(index.to_be_bytes());
    hasher.update([0]);
    hasher.update(call_id.as_str().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("mcp-v2-{index:02}-{}", &digest[..24])
}

fn validate_batch_request_id(
    batch_request_id: &str,
) -> Result<(), ToolExecuteOrchestrationFailure> {
    (!batch_request_id.trim().is_empty()
        && batch_request_id.len() <= MAX_BATCH_REQUEST_ID_BYTES
        && !batch_request_id.chars().any(char::is_control))
    .then_some(())
    .ok_or(ToolExecuteOrchestrationFailure::InvalidBatchRequestId)
}
