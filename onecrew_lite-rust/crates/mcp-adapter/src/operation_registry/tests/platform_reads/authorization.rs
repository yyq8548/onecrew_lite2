//! The registry is the only path from a confirmed plan to an owner call: it
//! compiles the exact operation scope, asks the OneCrew authority for an
//! `OperationSearchAuthorization`, and hands the owner a binding to the typed
//! request. No argument object can carry an authorization.
use super::*;
use crate::operation_registry::v2::{
    PresentationRequest, ResultStateInput, ResultStateOutput, ToolExecuteInput,
    ToolExecuteOrchestrationFailure, ToolExecutionAuthority, ToolExecutionCall,
    ToolExecutionOutcome,
};
use onecrew_local_runtime::{
    CallDisposition, ClaimResultContinuationInput, PendingActionView, ResultContinuationClaimMode,
    ResultContinuationContext, ResultOrigin, ResultState, SearchPlanScope,
};

pub(super) fn execution_call(
    registry: &crate::operation_registry::OperationRegistry,
    call_id: &str,
    operation_id: &str,
    connection: Option<ConnectionRef>,
    arguments: Value,
) -> ToolExecutionCall {
    ToolExecutionCall {
        call_id: EntryId::parse(call_id).unwrap(),
        operation_id: EntryId::parse(operation_id).unwrap(),
        schema_revision: registry_operation(registry, operation_id)
            .input
            .revision()
            .to_owned(),
        authority: ToolExecutionAuthority::Direct {},
        connection,
        arguments: arguments.as_object().cloned().unwrap(),
    }
}

/// Prepare and confirm a plan holding exactly `scopes`; return the reference.
pub(super) async fn confirm_plan(
    registry: &crate::operation_registry::OperationRegistry,
    platform: &Arc<onecrew_local_runtime::LocalPlatform>,
    origin: &ResultOrigin,
    scopes: Vec<Value>,
) -> (String, Vec<SearchPlanScope>) {
    let prepared = registry
        .tool_execute_v2(
            platform,
            origin,
            SurfaceId::Full,
            "prepare-plan",
            &ToolExecuteInput {
                calls: vec![execution_call(
                    registry,
                    "plan",
                    "onecrew.search.plan.prepare",
                    None,
                    json!({
                        "description": "Technical original tutorials",
                        "keywords": ["rust"],
                        "scopes": scopes
                    }),
                )],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    confirm_prepared_plan(registry, platform, origin, &prepared.result_ref.unwrap()).await
}

/// Confirm the prepared plan behind `result_ref`; return the reference and
/// the scopes the model prepared (the card cannot change them).
pub(super) async fn confirm_prepared_plan(
    registry: &crate::operation_registry::OperationRegistry,
    platform: &Arc<onecrew_local_runtime::LocalPlatform>,
    origin: &ResultOrigin,
    result_ref: &onecrew_local_runtime::ResultRef,
) -> (String, Vec<SearchPlanScope>) {
    let snapshot = platform.present_result(result_ref, origin).unwrap();
    let Some(PendingActionView::SearchPlan { draft, .. }) = snapshot.pending_action.clone() else {
        panic!("search plan pending action");
    };
    let ResultStateOutput::Snapshot { snapshot, .. } = registry
        .result_state_v2(
            platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: snapshot.binding,
            },
        )
        .await
        .unwrap()
    else {
        panic!("confirmed plan snapshot");
    };
    let continuation = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: snapshot.binding,
            expected_continuation_revision: snapshot.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::SearchPlanConfirmed {
        confirmed_plan_ref,
        description,
        keywords,
    } = continuation.request.context
    else {
        panic!("confirmed plan context");
    };
    assert_eq!((description, keywords), (draft.description, draft.keywords));
    (confirmed_plan_ref, draft.scopes)
}

pub(super) fn youtube_scope(registry: &crate::operation_registry::OperationRegistry) -> Value {
    json!({
        "operation_id": "youtube.channels.search",
        "schema_revision": registry_operation(registry, "youtube.channels.search").input.revision(),
        "connection": {"kind":"service_credential","toolkit":"youtube","credential_id": UNKNOWN_CONNECTION},
        "arguments": {"query":"rust","count":5}
    })
}

pub(super) fn scope_call(
    registry: &crate::operation_registry::OperationRegistry,
    scope: &SearchPlanScope,
    confirmed_plan_ref: &str,
) -> ToolExecutionCall {
    let mut arguments = Value::Object(scope.arguments.clone());
    arguments["confirmed_plan_ref"] = json!(confirmed_plan_ref);
    execution_call(
        registry,
        "search",
        &scope.operation_id,
        Some(serde_json::from_value(serde_json::to_value(&scope.connection).unwrap()).unwrap()),
        arguments,
    )
}

#[tokio::test]
async fn confirmed_scope_and_provider_cursor_pass_authorization_before_connection_admission() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let (confirmed_plan_ref, scopes) = confirm_plan(
        &registry,
        &platform,
        &origin,
        vec![youtube_scope(&registry)],
    )
    .await;
    let approved = scope_call(&registry, &scopes[0], &confirmed_plan_ref);

    for cursor in [None, Some("provider+/page==")] {
        let mut call = approved.clone();
        if let Some(cursor) = cursor {
            call.arguments.insert("cursor".into(), json!(cursor));
        }
        let output = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                "confirmed-search",
                &ToolExecuteInput {
                    calls: vec![call],
                    presentation: PresentationRequest::Never,
                },
            )
            .await
            .unwrap();
        // A real toolkit rejects the absent connection after its bound
        // authorization passes; no provider or candidate fixture is involved.
        assert_connection_admission_failure(&output.outcomes);
    }
}

/// One attended local write followed by a confirmed search: the batch stops
/// at the confirmation, and the search runs only on the resumed dispatch.
pub(super) fn attended_then_search_batch(
    registry: &crate::operation_registry::OperationRegistry,
    scope: &SearchPlanScope,
    confirmed_plan_ref: &str,
) -> ToolExecuteInput {
    let attended = execution_call(
        registry,
        "batch",
        "onecrew.creator.batch.create",
        None,
        json!({
            "idempotency_key":"resume-authorization",
            "validation":"strict",
            "items":[{
                "display_name":"Creator One",
                "notes":"Queued before a confirmed search",
                "contact_routes":[]
            }]
        }),
    );
    ToolExecuteInput {
        calls: vec![attended, scope_call(registry, scope, confirmed_plan_ref)],
        presentation: PresentationRequest::Never,
    }
}

pub(super) async fn stop_at_attended_confirmation(
    registry: &crate::operation_registry::OperationRegistry,
    platform: &onecrew_local_runtime::LocalPlatform,
    origin: &ResultOrigin,
    input: &ToolExecuteInput,
) -> onecrew_local_runtime::ResultBinding {
    let output = registry
        .tool_execute_v2(
            platform,
            origin,
            SurfaceId::Full,
            "attended-then-search",
            input,
        )
        .await
        .unwrap();
    assert!(
        matches!(
            output.outcomes.as_slice(),
            [
                ToolExecutionOutcome::InputRequired { .. },
                ToolExecutionOutcome::Skipped { .. }
            ]
        ),
        "{:?}",
        output.outcomes
    );
    platform
        .present_result(&output.result_ref.unwrap(), origin)
        .unwrap()
        .binding
}

#[tokio::test]
async fn confirmed_scope_queued_behind_an_attended_call_is_authorized_on_resume() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let (confirmed_plan_ref, scopes) = confirm_plan(
        &registry,
        &platform,
        &origin,
        vec![youtube_scope(&registry)],
    )
    .await;
    let input = attended_then_search_batch(&registry, &scopes[0], &confirmed_plan_ref);
    let binding = stop_at_attended_confirmation(&registry, &platform, &origin, &input).await;
    let ResultStateOutput::Snapshot { snapshot, .. } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm { binding },
        )
        .await
        .unwrap()
    else {
        panic!("resumed snapshot");
    };
    let [attended, search] = snapshot.batch.outcomes.as_slice() else {
        panic!("two resumed outcomes: {:?}", snapshot.batch.outcomes);
    };
    assert_eq!(attended.disposition, CallDisposition::Succeeded);
    assert_eq!(search.operation_id, "youtube.channels.search");
    assert_eq!(
        search.disposition,
        CallDisposition::Failed {
            code: "connection_not_found_or_mismatch".into(),
        }
    );
}

#[tokio::test]
async fn plan_revoked_while_the_batch_waited_fails_the_resume_before_any_owner_runs() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let (confirmed_plan_ref, scopes) = confirm_plan(
        &registry,
        &platform,
        &origin,
        vec![youtube_scope(&registry)],
    )
    .await;
    let input = attended_then_search_batch(&registry, &scopes[0], &confirmed_plan_ref);
    let binding = stop_at_attended_confirmation(&registry, &platform, &origin, &input).await;
    platform.cancel_search_plan(&confirmed_plan_ref).unwrap();

    let ResultStateOutput::Snapshot { snapshot, .. } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm { binding },
        )
        .await
        .unwrap()
    else {
        panic!("closed snapshot");
    };
    assert!(
        matches!(&snapshot.state, ResultState::Failed(failure) if failure.code == "resume_search_authorization_rejected"),
        "{:?}",
        snapshot.state
    );
}

#[tokio::test]
async fn changed_arguments_or_connection_under_the_same_reference_never_reach_the_owner() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let (confirmed_plan_ref, scopes) = confirm_plan(
        &registry,
        &platform,
        &origin,
        vec![youtube_scope(&registry)],
    )
    .await;
    let approved = scope_call(&registry, &scopes[0], &confirmed_plan_ref);

    let mut changed_query = approved.clone();
    changed_query
        .arguments
        .insert("query".into(), json!("different"));
    let mut changed_count = approved.clone();
    changed_count.arguments.insert("count".into(), json!(6));
    let mut changed_connection = approved.clone();
    changed_connection.connection = Some(ConnectionRef::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
        credential_id: "another-credential".into(),
    });
    let mut changed_operation = approved.clone();
    changed_operation.operation_id = EntryId::parse("youtube.videos.search").unwrap();
    changed_operation.schema_revision = registry_operation(&registry, "youtube.videos.search")
        .input
        .revision()
        .to_owned();
    let mut unknown_reference = approved;
    unknown_reference
        .arguments
        .insert("confirmed_plan_ref".into(), json!("not-a-confirmed-plan"));

    use onecrew_local_runtime::SearchPlanApprovalError as Reason;
    for (label, call, expected) in [
        ("query", changed_query, Reason::ScopeMismatch),
        ("count", changed_count, Reason::ScopeMismatch),
        ("connection", changed_connection, Reason::ScopeMismatch),
        ("operation", changed_operation, Reason::ScopeMismatch),
        ("reference", unknown_reference, Reason::InvalidOrExpired),
    ] {
        let failure = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &format!("changed-{label}"),
                &ToolExecuteInput {
                    calls: vec![call],
                    presentation: PresentationRequest::Never,
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(
                &failure,
                ToolExecuteOrchestrationFailure::SearchPlanApprovalRejected { reason, .. }
                    if *reason == expected
            ),
            "{label}: {failure:?}"
        );
    }
}

/// Direct-policy arguments with a plan reference for each plan-governed
/// operation, with its matching connection.
fn direct_with_reference_calls() -> Vec<(&'static str, ConnectionRef, Value)> {
    let mut calls = search_calls();
    for (_, _, arguments) in &mut calls {
        arguments["search_plan_policy"] = json!("user_requested_direct");
        arguments["confirmed_plan_ref"] = json!("plan-1");
    }
    calls
}

#[tokio::test]
async fn direct_policy_with_a_plan_reference_fails_prevalidation_on_every_plan_governed_operation()
{
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let calls = direct_with_reference_calls();
    // The expected set comes from the production registry, so an operation
    // that starts accepting a plan scope fails here until it has a case.
    let production = crate::operation_registry::build_registry(
        production_owners(Arc::clone(&platform)),
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    let plan_governed = plan_governed_operations(&production)
        .into_iter()
        .map(|(id, _)| id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let covered = calls
        .iter()
        .map(|(operation, _, _)| (*operation).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(covered.len(), calls.len(), "each operation has one case");
    assert_eq!(covered, plan_governed);
    for (index, (operation, connection, arguments)) in calls.into_iter().enumerate() {
        let failure = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &format!("direct-with-reference-{index}"),
                &ToolExecuteInput {
                    calls: vec![execution_call(
                        &registry,
                        "search",
                        operation,
                        Some(connection.clone()),
                        arguments.clone(),
                    )],
                    presentation: PresentationRequest::Never,
                },
            )
            .await
            .unwrap_err();
        let ToolExecuteOrchestrationFailure::Prevalidation {
            operation_id,
            failure,
            ..
        } = failure
        else {
            panic!("{operation}: expected a prevalidation failure, got {failure:?}");
        };
        assert_eq!(operation_id.as_str(), operation);
        assert_eq!(
            failure.code.as_str(),
            "typed_decode_rejected",
            "{operation}"
        );
        assert_eq!(failure.phase, ExecuteFailurePhase::BeforeInvocation);
        assert_eq!(failure.effect, ExecuteEffectState::NotInvoked);

        // Without the reference the same direct call prepares, so the pair
        // alone is what the validator refuses.
        let mut direct = arguments;
        direct.as_object_mut().unwrap().remove("confirmed_plan_ref");
        let entry = registry_operation(&registry, operation);
        assert!(
            prepare_invocation(
                entry,
                ExecutionContext {
                    surface: SurfaceId::Full,
                    request_id: "direct-without-reference".into(),
                    connection: Some(&connection),
                    confirmed: false,
                },
                entry.input.revision(),
                direct,
            )
            .is_ok(),
            "{operation}: a bare direct call must prepare"
        );
    }
}

fn x_scope(registry: &crate::operation_registry::OperationRegistry) -> Value {
    json!({
        "operation_id": "x.posts.search.advanced",
        "schema_revision": registry_operation(registry, "x.posts.search.advanced").input.revision(),
        "connection": {"kind":"operator_account","toolkit":"x","operator_account_id": UNKNOWN_CONNECTION},
        "arguments": {"all_words":["rust"],"count":5}
    })
}

/// A confirmed plan fixes criteria, not a page budget. Repeated native
/// cursors pass scope authorization; a changed criterion is rejected before
/// even the empty connection store can be consulted.
#[tokio::test]
async fn repeated_pages_of_a_confirmed_scope_are_admitted_and_a_changed_criterion_is_refused() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let (confirmed_plan_ref, scopes) =
        confirm_plan(&registry, &platform, &origin, vec![x_scope(&registry)]).await;
    let approved = scope_call(&registry, &scopes[0], &confirmed_plan_ref);
    for page in 0..48 {
        let mut call = approved.clone();
        if page > 0 {
            call.arguments
                .insert("cursor".into(), json!(format!("page-{page}")));
        }
        let output = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &format!("page-{page}"),
                &ToolExecuteInput {
                    calls: vec![call],
                    presentation: PresentationRequest::Never,
                },
            )
            .await
            .unwrap();
        assert_connection_admission_failure(&output.outcomes);
    }
    let mut changed_criterion = approved;
    changed_criterion
        .arguments
        .insert("all_words".into(), json!(["python"]));
    changed_criterion
        .arguments
        .insert("cursor".into(), json!("page-1"));
    let failure = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "changed-criterion",
            &ToolExecuteInput {
                calls: vec![changed_criterion],
                presentation: PresentationRequest::Never,
            },
        )
        .await
        .unwrap_err();
    let ToolExecuteOrchestrationFailure::SearchPlanApprovalRejected { reason, .. } = failure else {
        panic!("expected a plan refusal, got {failure:?}");
    };
    assert_eq!(reason.code(), "search_plan_scope_mismatch");
}
