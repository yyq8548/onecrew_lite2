use super::super::contract::{
    PresentationRequest, ResultStateInput, ResultStateOutput, ToolExecuteInput, ToolExecutionCall,
};
use super::super::{
    execute::ToolExecuteOrchestrationFailure, search_plan::SearchPlanScopeInvalidReason,
};
use super::fixtures::{call, production_registry, result_owner, revision};
use crate::operation_registry::{
    descriptor::{ConnectionRef, EntryId, RegistryEntry, SurfaceId},
    execution::{ExecutionContext, prepare_invocation},
    registration::OperationRegistry,
};
use onecrew_local_runtime::{PendingActionView, ResultState};
use serde_json::{Value, json};

/// Top-level properties of the retired plan union.
pub(in crate::operation_registry) const RETIRED_PLAN_PROPERTIES: [&str; 2] =
    ["searches", "tiktok_searches"];
/// Type and field names of the retired plan wire. A scope's operation-owned
/// `arguments` may still carry a live provider field of the same name.
pub(in crate::operation_registry) const RETIRED_PLAN_WIRE_NAMES: [&str; 4] = [
    "SearchPlanSearch",
    "advanced_x",
    "min_followers",
    "criteria.any_words",
];
/// Search operations retired with the direct-X and Creator search flows.
pub(in crate::operation_registry) const RETIRED_SEARCH_OPERATIONS: [&str; 2] =
    ["x.users.search", "onecrew.creator.search"];

fn scope(registry: &OperationRegistry, operation: &str) -> Value {
    let (connection, arguments) = match operation {
        "youtube.channels.search" | "youtube.videos.search" => (
            json!({"kind":"service_credential","toolkit":"youtube","credential_id":"selected-credential"}),
            json!({"query":"rust","count":5}),
        ),
        "x.posts.search.advanced" => (
            json!({"kind":"operator_account","toolkit":"x","operator_account_id":"selected-operator"}),
            json!({"any_words":["rust"],"count":5}),
        ),
        "instagram.users.search" | "telegram.communities.search" => (
            json!({"kind":"operator_account","toolkit":operation.split('.').next().unwrap(),"operator_account_id":"selected-operator"}),
            json!({"query":"rust","count":5}),
        ),
        _ => unreachable!(),
    };
    json!({"operation_id":operation,"schema_revision":revision(registry, operation),"connection":connection,"arguments":arguments})
}

fn plan(scopes: Vec<Value>) -> Value {
    json!({"description":"Technical original tutorials","keywords":["rust"],"scopes":scopes})
}

fn prepare_call(registry: &OperationRegistry, draft: Value) -> ToolExecutionCall {
    call(
        "plan",
        "onecrew.search.plan.prepare",
        revision(registry, "onecrew.search.plan.prepare"),
        draft,
    )
}

#[tokio::test]
async fn youtube_channel_video_and_cross_platform_scopes_prepare_without_dispatch() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let operations = [
        "youtube.channels.search",
        "youtube.videos.search",
        "x.posts.search.advanced",
        "instagram.users.search",
        "telegram.communities.search",
    ];
    let draft = plan(operations.iter().map(|id| scope(&registry, id)).collect());
    let output = registry
        .tool_execute_v2_with_minter(
            &platform,
            &origin,
            SurfaceId::Full,
            "cross-platform-plan",
            &ToolExecuteInput {
                calls: vec![prepare_call(&registry, draft)],
                presentation: PresentationRequest::Requested,
            },
            &|input| {
                assert!(matches!(
                    &input.batch.outcomes[0].disposition,
                    onecrew_local_runtime::CallDisposition::NotRun { .. }
                ));
                platform.mint_execution_result(input)
            },
        )
        .await
        .unwrap();
    let snapshot = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
    let Some(PendingActionView::SearchPlan { draft, .. }) = snapshot.pending_action else {
        panic!("plan");
    };
    assert_eq!(draft.scopes.len(), operations.len());
    for (scope, operation) in draft.scopes.iter().zip(operations) {
        assert_eq!(scope.operation_id, operation);
        assert_eq!(scope.schema_revision, revision(&registry, operation));
        assert!(!scope.arguments.contains_key("kind"));
        assert!(!scope.arguments.contains_key("min_followers"));
    }
}

/// Prepare a plan whose scopes must be rejected; the minter panics, so a
/// returned failure proves no result_ref was minted.
async fn scope_rejection(
    registry: &OperationRegistry,
    scopes: Vec<Value>,
    presentation: PresentationRequest,
) -> (SearchPlanScopeInvalidReason, String) {
    let (_directory, platform, origin) = result_owner();
    let failure = registry
        .tool_execute_v2_with_minter(
            &platform,
            &origin,
            SurfaceId::Full,
            "invalid-plan",
            &ToolExecuteInput {
                calls: vec![prepare_call(registry, plan(scopes))],
                presentation,
            },
            &|_| panic!("invalid scopes must never mint"),
        )
        .await
        .unwrap_err();
    let ToolExecuteOrchestrationFailure::SearchPlanScopeInvalid {
        reason,
        parameter_pointer,
        ..
    } = failure
    else {
        panic!("expected a scope rejection, got {failure:?}");
    };
    (reason, parameter_pointer)
}

#[tokio::test]
async fn each_scope_check_fails_with_its_reason_and_field_before_result_mint() {
    use SearchPlanScopeInvalidReason as Reason;
    let registry = production_registry();
    let base = scope(&registry, "youtube.channels.search");
    let changed = |edit: &dyn Fn(&mut Value)| {
        let mut scope = base.clone();
        edit(&mut scope);
        scope
    };
    let mut cases = vec![
        (
            changed(&|scope| scope["operation_id"] = json!("x.users.search")),
            Reason::UnknownOperation,
            "/scopes/0/operation_id",
        ),
        (
            changed(&|scope| scope["operation_id"] = json!("Not An Operation")),
            Reason::UnknownOperation,
            "/scopes/0/operation_id",
        ),
        (
            changed(&|scope| {
                scope["operation_id"] = json!("x.profile.get");
                scope["schema_revision"] = json!(revision(&registry, "x.profile.get"));
            }),
            Reason::OperationNotPlanGoverned,
            "/scopes/0/operation_id",
        ),
        // Not a tagged connection reference at all.
        (
            changed(&|scope| scope["connection"]["toolkit"] = json!("x")),
            Reason::ConnectionInvalid,
            "/scopes/0/connection",
        ),
        // A valid reference the operation does not accept.
        (
            changed(&|scope| {
                scope["connection"] = json!({"kind":"operator_account","toolkit":"x","operator_account_id":"selected-operator"});
            }),
            Reason::ConnectionInvalid,
            "/scopes/0/connection",
        ),
        (
            changed(&|scope| scope["schema_revision"] = json!("stale")),
            Reason::SchemaRevisionMismatch,
            "/scopes/0/schema_revision",
        ),
        (
            changed(&|scope| scope["arguments"]["confirmed_plan_ref"] = json!("forged")),
            Reason::PlanReferenceInScope,
            "/scopes/0/arguments/confirmed_plan_ref",
        ),
        (
            changed(&|scope| {
                scope["arguments"]["search_plan_policy"] = json!("user_requested_direct");
            }),
            Reason::DirectPolicyInScope,
            "/scopes/0/arguments/search_plan_policy",
        ),
        (
            changed(&|scope| scope["arguments"]["search_plan_policy"] = json!("sometimes")),
            Reason::ArgumentsInvalid,
            "/scopes/0/arguments/search_plan_policy",
        ),
        (
            changed(&|scope| scope["arguments"]["search_plan_policy"] = Value::Null),
            Reason::ArgumentsInvalid,
            "/scopes/0/arguments/search_plan_policy",
        ),
        (
            changed(&|scope| scope["arguments"]["cursor"] = json!("next-page")),
            Reason::ContinuationInInitialPlan,
            "/scopes/0/arguments",
        ),
    ];
    for field in ["kind", "search", "platform"] {
        cases.push((
            changed(&|scope| scope["arguments"][field] = json!("standard")),
            Reason::ArgumentsInvalid,
            "/scopes/0/arguments",
        ));
    }
    for presentation in [
        PresentationRequest::Never,
        PresentationRequest::IfRequired,
        PresentationRequest::Requested,
    ] {
        for (scope, reason, pointer) in &cases {
            assert_eq!(
                scope_rejection(&registry, vec![scope.clone()], presentation).await,
                (*reason, (*pointer).to_owned()),
                "{scope}"
            );
        }
    }
}

#[tokio::test]
async fn neutral_plan_fields_prepare_and_compile_the_same_scope_as_their_absence() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    for operation in [
        "youtube.channels.search",
        "youtube.videos.search",
        "x.posts.search.advanced",
        "instagram.users.search",
        "telegram.communities.search",
    ] {
        let bare = scope(&registry, operation);
        let mut neutral = bare.clone();
        neutral["arguments"]["confirmed_plan_ref"] = Value::Null;
        neutral["arguments"]["search_plan_policy"] = json!("require_confirmation");
        let output = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                "neutral-plan-fields",
                &ToolExecuteInput {
                    calls: vec![prepare_call(&registry, plan(vec![neutral.clone()]))],
                    presentation: PresentationRequest::Requested,
                },
            )
            .await
            .unwrap_or_else(|failure| panic!("{operation}: {failure:?}"));
        assert!(output.result_ref.is_some(), "{operation}");
        // Both scopes compile to one token, so the second names no new search.
        assert_eq!(
            scope_rejection(
                &registry,
                vec![bare, neutral],
                PresentationRequest::Requested
            )
            .await,
            (
                SearchPlanScopeInvalidReason::DuplicateScope,
                "/scopes/1".to_owned()
            ),
            "{operation}"
        );
    }
}

#[tokio::test]
async fn duplicate_scope_fails_at_its_second_occurrence_before_result_mint() {
    let registry = production_registry();
    let exact = scope(&registry, "telegram.communities.search");
    // Omitted and explicit default counts compile to the same scope token.
    let mut omitted_count = scope(&registry, "instagram.users.search");
    omitted_count["arguments"] = json!({"query":"rust"});
    let mut default_count = omitted_count.clone();
    default_count["arguments"]["count"] = json!(20);
    for scopes in [
        vec![exact.clone(), exact],
        vec![omitted_count, default_count],
    ] {
        assert_eq!(
            scope_rejection(&registry, scopes, PresentationRequest::Requested).await,
            (
                SearchPlanScopeInvalidReason::DuplicateScope,
                "/scopes/1".to_owned()
            )
        );
    }
}

#[tokio::test]
async fn prepare_edit_confirm_binds_latest_snapshot_and_exact_operation_scope() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let draft = plan(vec![
        scope(&registry, "youtube.channels.search"),
        scope(&registry, "youtube.videos.search"),
        scope(&registry, "telegram.communities.search"),
    ]);
    let output = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "plan-confirm",
            &ToolExecuteInput {
                calls: vec![prepare_call(&registry, draft)],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let snapshot = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
    let ResultStateOutput::Snapshot {
        snapshot: revised, ..
    } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ReviseSearchPlan {
                binding: snapshot.binding.clone(),
                patch: onecrew_local_runtime::SearchPlanPatch {
                    description: Some("Final criteria".into()),
                    keywords: Some(vec!["tutorial".into()]),
                },
            },
        )
        .await
        .unwrap()
    else {
        panic!("revised");
    };
    assert!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm {
                    binding: snapshot.binding
                }
            )
            .await
            .is_err()
    );
    let Some(PendingActionView::SearchPlan { draft: visible, .. }) = &revised.pending_action else {
        panic!("plan");
    };
    assert_eq!(visible.scopes[0].arguments["query"], "rust");
    let ResultStateOutput::Snapshot {
        snapshot: confirmed,
        ..
    } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: revised.binding,
            },
        )
        .await
        .unwrap()
    else {
        panic!("confirmed");
    };
    assert!(
        matches!(&confirmed.state, ResultState::Resolved(resolution) if resolution.code == "search_plan_confirmed")
    );
    let continuation = platform
        .claim_result_continuation(onecrew_local_runtime::ClaimResultContinuationInput {
            binding: confirmed.binding.clone(),
            expected_continuation_revision: confirmed.continuation.continuation_revision,
            mode: onecrew_local_runtime::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    // The context carries the reference and the edited criteria only; the
    // model runs the scopes it prepared, which the card cannot change.
    let onecrew_local_runtime::ResultContinuationContext::SearchPlanConfirmed {
        confirmed_plan_ref,
        description,
        keywords,
    } = continuation.request.context
    else {
        panic!("confirmed plan context");
    };
    assert_eq!(description, "Final criteria");
    assert_eq!(keywords, ["tutorial"]);
    assert_eq!(
        continuation.request.message,
        "Search plan confirmed. Continue with the approved searches."
    );
    for scope in &visible.scopes {
        let RegistryEntry::Operation(operation) = registry
            .entry(
                &EntryId::parse(&scope.operation_id).unwrap(),
                SurfaceId::Full,
            )
            .unwrap()
        else {
            panic!("scope operation");
        };
        let connection: ConnectionRef =
            serde_json::from_value(serde_json::to_value(&scope.connection).unwrap()).unwrap();
        let mut execution = call(
            "search",
            &scope.operation_id,
            scope.schema_revision.clone(),
            Value::Object(scope.arguments.clone()),
        );
        execution.connection = Some(connection.clone());
        execution
            .arguments
            .insert("confirmed_plan_ref".into(), json!(confirmed_plan_ref));
        // Authorization is only reachable through the prepared typed
        // invocation: a call that fails typed preparation has no scope.
        let authorized = |execution: &ToolExecutionCall| {
            prepare_invocation(
                operation,
                ExecutionContext {
                    surface: SurfaceId::Full,
                    request_id: "authorize".into(),
                    connection: Some(&connection),
                    confirmed: false,
                },
                &scope.schema_revision,
                Value::Object(execution.arguments.clone()),
            )
            .ok()
            .is_some_and(|invocation| {
                super::super::search_plan::authorize_call(&platform, execution, &invocation).is_ok()
            })
        };
        assert!(authorized(&execution), "{}", scope.operation_id);
        // Change a field inside the compiled scope so the call still prepares
        // and the negative branch is a scope mismatch, not a decode rejection.
        execution
            .arguments
            .insert("query".into(), json!("different"));
        assert!(!authorized(&execution), "{}", scope.operation_id);
    }
    assert!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm {
                    binding: confirmed.binding
                }
            )
            .await
            .is_err()
    );
}

#[test]
fn compact_plan_contract_has_no_retired_union_or_provider_filters() {
    let registry = production_registry();
    let RegistryEntry::Operation(operation) = registry
        .entry(
            &EntryId::parse("onecrew.search.plan.prepare").unwrap(),
            SurfaceId::Full,
        )
        .unwrap()
    else {
        panic!("plan");
    };
    let schema = operation.input.document();
    for retired in RETIRED_PLAN_PROPERTIES {
        assert!(schema.pointer(&format!("/properties/{retired}")).is_none());
    }
    let wire = schema.to_string();
    for retired in RETIRED_PLAN_WIRE_NAMES {
        assert!(!wire.contains(retired), "{retired}");
    }
    for operation in RETIRED_SEARCH_OPERATIONS {
        assert!(
            registry
                .entry(&EntryId::parse(operation).unwrap(), SurfaceId::Full)
                .is_none()
        );
    }
    let legacy = json!({"description":"criteria","keywords":[],"searches":[],"tiktok_searches":[]});
    assert!(!operation.input.validate(&legacy));
}

#[tokio::test]
async fn valid_plan_store_error_is_not_misclassified_as_validation() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    let failure = registry
        .tool_execute_v2_with_minter(
            &platform,
            &origin,
            SurfaceId::Full,
            "store-failure",
            &ToolExecuteInput {
                calls: vec![prepare_call(
                    &registry,
                    plan(vec![scope(&registry, "youtube.videos.search")]),
                )],
                presentation: PresentationRequest::Requested,
            },
            &|_| {
                Err(onecrew_local_runtime::ResultStoreError::InvalidInput(
                    "search_plan",
                ))
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        failure,
        super::super::execute::ToolExecuteOrchestrationFailure::PresentationUnavailable
    );
}

#[tokio::test]
async fn external_search_without_confirmation_fails_before_dispatch() {
    let registry = production_registry();
    let (_directory, platform, origin) = result_owner();
    for operation in [
        "youtube.channels.search",
        "youtube.videos.search",
        "instagram.users.search",
        "telegram.communities.search",
        "x.posts.search.advanced",
    ] {
        let failure = registry
            .tool_execute_v2_with_minter(
                &platform,
                &origin,
                SurfaceId::Full,
                "no-approval",
                &ToolExecuteInput {
                    calls: vec![call(
                        "search",
                        operation,
                        revision(&registry, operation),
                        json!({}),
                    )],
                    presentation: PresentationRequest::Never,
                },
                &|_| panic!("no approval must not mint"),
            )
            .await
            .unwrap_err();
        assert!(matches!(failure, super::super::execute::ToolExecuteOrchestrationFailure::SearchPlanConfirmationRequired { .. }));
    }
}

#[tokio::test]
async fn prepare_contract_states_the_ttl_and_the_continuation_stays_short_and_localized() {
    let registry = production_registry();
    let RegistryEntry::Operation(prepare) = registry
        .entry(
            &EntryId::parse("onecrew.search.plan.prepare").unwrap(),
            SurfaceId::Full,
        )
        .unwrap()
    else {
        panic!("prepare operation");
    };
    assert_eq!(onecrew_local_runtime::SEARCH_PLAN_TTL_SECONDS % 60, 0);
    for stated in [
        format!(
            "expires {} minutes after confirmation",
            onecrew_local_runtime::SEARCH_PLAN_TTL_SECONDS / 60
        ),
        "page freely with next_cursor".to_owned(),
        "changed criteria need a new confirmed plan".to_owned(),
    ] {
        assert!(
            prepare.metadata.summary.contains(&stated),
            "{stated:?} in {:?}",
            prepare.metadata.summary
        );
    }
    assert!(
        !prepare
            .metadata
            .summary
            .to_ascii_lowercase()
            .contains("budget"),
        "the prepare contract states no plan budget: {:?}",
        prepare.metadata.summary
    );

    let (_directory, platform, origin) = result_owner();
    let output = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "plan-ttl-summary",
            &ToolExecuteInput {
                calls: vec![prepare_call(
                    &registry,
                    plan(vec![scope(&registry, "youtube.channels.search")]),
                )],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let snapshot = platform
        .present_result_with_locale(
            &output.result_ref.unwrap(),
            &origin,
            onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
        )
        .unwrap();
    let ResultStateOutput::Snapshot {
        snapshot: confirmed,
        ..
    } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: snapshot.binding,
            },
        )
        .await
        .unwrap()
    else {
        panic!("confirmed");
    };
    let continuation = platform
        .claim_result_continuation(onecrew_local_runtime::ClaimResultContinuationInput {
            binding: confirmed.binding.clone(),
            expected_continuation_revision: confirmed.continuation.continuation_revision,
            mode: onecrew_local_runtime::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let onecrew_local_runtime::ResultContinuationContext::SearchPlanConfirmed {
        confirmed_plan_ref,
        ..
    } = &continuation.request.context
    else {
        panic!("confirmed plan context");
    };
    assert!(continuation.request.requires_model_context);
    assert_eq!(
        continuation.request.message,
        "已确认搜索计划，请按计划继续搜索。"
    );
    assert!(!continuation.request.message.contains(confirmed_plan_ref));
    assert_eq!(
        confirmed.batch.outcomes[0].summary,
        super::super::search_plan::confirmation_summary(confirmed_plan_ref)
    );
}
