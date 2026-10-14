use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::time::timeout;

use super::fixtures::{
    TestInput, TestOutput, TestOwnerError, call, metadata, policy, result_owner, revision,
};
use crate::operation_registry::{
    approval::ApprovalRequest,
    descriptor::{ApprovalClass, ConnectionRequirement, EffectClass, SurfaceId},
    execution::{
        BoundedPublicValue, ClassifiedOwnerFailure, ExecuteEffectState, ExecuteFailurePhase,
        ExecutionDeclaration, PublicFailureCode, RecoveryDirective,
    },
    registration::{OperationPolicy, RegistryBuilder},
    v2::{
        contract::{PresentationRequest, ToolExecuteInput, ToolExecutionOutcome, ToolInputKind},
        execute::{SKIPPED_AFTER_STOP, ToolExecuteOrchestrationFailure},
    },
};

#[tokio::test]
async fn whole_batch_prevalidation_failure_calls_no_owner() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.first",
            "onecrew.test.invalid",
        ]));
    for id in ["onecrew.test.first", "onecrew.test.invalid"] {
        let counter = Arc::clone(&owner_calls);
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
                move |_context, input| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("test owner succeeds"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call.first",
                "onecrew.test.first",
                revision(&registry, "onecrew.test.first"),
                json!({"value":"valid"}),
            ),
            call(
                "call.invalid",
                "onecrew.test.invalid",
                revision(&registry, "onecrew.test.invalid"),
                json!({"value":7}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };

    let failure = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-prevalidation",
            &input,
        )
        .await
        .unwrap_err();
    let ToolExecuteOrchestrationFailure::Prevalidation {
        call_id,
        operation_id,
        failure,
    } = failure
    else {
        panic!("expected invocation prevalidation failure")
    };
    assert_eq!(call_id.as_str(), "call.invalid");
    assert_eq!(operation_id.as_str(), "onecrew.test.invalid");
    assert_eq!(failure.code.as_str(), "input_schema_rejected");
    assert_eq!(failure.phase, ExecuteFailurePhase::BeforeInvocation);
    assert_eq!(failure.effect, ExecuteEffectState::NotInvoked);
    assert!(failure.dm_failure.is_none());
    assert!(
        serde_json::to_value(&failure)
            .unwrap()
            .get("dm_failure")
            .is_none()
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn ordered_owner_guarded_successes_receive_distinct_bounded_request_ids() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&observed);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.record",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.record"),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            move |context, input| {
                let recorder = Arc::clone(&recorder);
                async move {
                    recorder
                        .lock()
                        .unwrap()
                        .push((context.request_id, input.value.clone()));
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let revision = revision(&registry, "onecrew.test.record");
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call.alpha",
                "onecrew.test.record",
                revision.clone(),
                json!({"value":"alpha"}),
            ),
            call(
                "call.beta",
                "onecrew.test.record",
                revision,
                json!({"value":"beta"}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };

    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-request-ordered",
            &input,
        )
        .await
        .unwrap();
    assert!(result.batch_id.starts_with("batch-"));
    assert_eq!(result.batch_id.len(), 38);
    assert!(result.result_ref.is_none());
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::Completed { call_id, result, .. }
            if call_id.as_str() == "call.alpha" && result == &json!({"value":"alpha"})
    ));
    assert!(matches!(
        &result.outcomes[1],
        ToolExecutionOutcome::Completed { call_id, result, .. }
            if call_id.as_str() == "call.beta" && result == &json!({"value":"beta"})
    ));
    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[0].1, "alpha");
    assert_eq!(observed[1].1, "beta");
    assert_ne!(observed[0].0, observed[1].0);
    assert!(
        observed
            .iter()
            .all(|(request_id, _)| { request_id.starts_with("mcp-v2-") && request_id.len() <= 64 })
    );
}

#[tokio::test]
async fn attended_prompt_stops_batch_without_owner_or_rmcp_approval() {
    let first_calls = Arc::new(AtomicUsize::new(0));
    let attended_calls = Arc::new(AtomicUsize::new(0));
    let later_calls = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.after",
            "onecrew.test.attended",
            "onecrew.test.before",
        ]));

    for (id, approval, effect, calls) in [
        (
            "onecrew.test.before",
            ApprovalClass::None,
            EffectClass::ReadLocal,
            Arc::clone(&first_calls),
        ),
        (
            "onecrew.test.attended",
            ApprovalClass::AttendedPrompt,
            EffectClass::DeleteLocal,
            Arc::clone(&attended_calls),
        ),
        (
            "onecrew.test.after",
            ApprovalClass::None,
            EffectClass::ReadLocal,
            Arc::clone(&later_calls),
        ),
    ] {
        let mut operation_policy = policy(approval, effect);
        if approval == ApprovalClass::AttendedPrompt {
            operation_policy.approval_request =
                Some(ApprovalRequest::new("Confirm the bounded test action?").unwrap());
        }
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                move |_context, input| {
                    let calls = Arc::clone(&calls);
                    async move {
                        calls.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("test owner succeeds"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: [
            ("call.before", "onecrew.test.before"),
            ("call.attended", "onecrew.test.attended"),
            ("call.after", "onecrew.test.after"),
        ]
        .into_iter()
        .map(|(call_id, operation_id)| {
            call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value":call_id}),
            )
        })
        .collect(),
        presentation: PresentationRequest::Never,
    };

    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-attended",
            &input,
        )
        .await
        .unwrap();
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::Completed { call_id, .. } if call_id.as_str() == "call.before"
    ));
    assert!(matches!(
        &result.outcomes[1],
        ToolExecutionOutcome::InputRequired {
            call_id,
            input_kind: ToolInputKind::Confirmation,
            ..
        } if call_id.as_str() == "call.attended"
    ));
    assert!(matches!(
        &result.outcomes[2],
        ToolExecutionOutcome::Skipped { call_id, reason, .. }
            if call_id.as_str() == "call.after" && reason == SKIPPED_AFTER_STOP
    ));
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(attended_calls.load(Ordering::SeqCst), 0);
    assert_eq!(later_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn known_failure_and_unknown_effect_map_to_distinct_terminal_outcomes() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.after-failure",
            "onecrew.test.known-failure",
            "onecrew.test.unknown-effect",
        ]));
    let later_calls = Arc::new(AtomicUsize::new(0));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.known-failure"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async { Err::<TestOutput, _>(TestOwnerError) },
            |_| {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("known_failure").unwrap(),
                    true,
                    Some(RecoveryDirective::CorrectInput),
                )
                .with_provider_status(Some(400))
            },
        )
        .unwrap();
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.unknown-effect"),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            |_context, _input| async { Err::<TestOutput, _>(TestOwnerError) },
            |_| {
                ClassifiedOwnerFailure::unknown(
                    PublicFailureCode::parse("effect_unknown").unwrap(),
                    Some(RecoveryDirective::ReadStatus {
                        reference: BoundedPublicValue::parse("audit-test").unwrap(),
                    }),
                )
                .with_provider_status(Some(503))
            },
        )
        .unwrap();
    let later_counter = Arc::clone(&later_calls);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.after-failure"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                ExecutionDeclaration::Sequential,
            ),
            move |_context, input| {
                let later_counter = Arc::clone(&later_counter);
                async move {
                    later_counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("later owner must be skipped"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();

    let run = |call_id: &str, operation_id: &str| ToolExecuteInput {
        calls: vec![
            call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value":"test"}),
            ),
            call(
                "call.after",
                "onecrew.test.after-failure",
                revision(&registry, "onecrew.test.after-failure"),
                json!({"value":"must-not-run"}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };
    let known = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-known",
            &run("call.known", "onecrew.test.known-failure"),
        )
        .await
        .unwrap();
    let unknown = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-unknown",
            &run("call.unknown", "onecrew.test.unknown-effect"),
        )
        .await
        .unwrap();

    assert!(matches!(
        &known.outcomes[0],
        ToolExecutionOutcome::Failed { failure, .. }
            if failure.code.as_str() == "known_failure"
                && failure.effect == ExecuteEffectState::KnownNoEffect
                && failure.phase == ExecuteFailurePhase::AfterInvocation
                && failure.retry_safe
                && failure.provider_status == Some(400)
                && failure.recovery == Some(RecoveryDirective::CorrectInput)
    ));
    assert!(matches!(
        &unknown.outcomes[0],
        ToolExecutionOutcome::Unknown { failure, .. }
            if failure.code.as_str() == "effect_unknown"
                && failure.effect == ExecuteEffectState::Unknown
                && failure.phase == ExecuteFailurePhase::AfterInvocation
                && !failure.retry_safe
                && failure.provider_status == Some(503)
                && matches!(failure.recovery, Some(RecoveryDirective::ReadStatus { .. }))
    ));
    assert!(matches!(
        &known.outcomes[1],
        ToolExecutionOutcome::Skipped { reason, .. } if reason == SKIPPED_AFTER_STOP
    ));
    assert!(matches!(
        &unknown.outcomes[1],
        ToolExecutionOutcome::Skipped { reason, .. } if reason == SKIPPED_AFTER_STOP
    ));
    assert_eq!(later_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn invalid_batch_id_fails_before_owner() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.presentation",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.presentation"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                ExecutionDeclaration::Sequential,
            ),
            move |_context, input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let executable = ToolExecuteInput {
        calls: vec![call(
            "call.presentation",
            "onecrew.test.presentation",
            revision(&registry, "onecrew.test.presentation"),
            json!({"value":"test"}),
        )],
        presentation: PresentationRequest::Never,
    };
    for invalid_request_id in ["", "   ", "line\nbreak"] {
        assert_eq!(
            registry
                .tool_execute_v2(
                    &platform,
                    &origin,
                    SurfaceId::Full,
                    invalid_request_id,
                    &executable,
                )
                .await,
            Err(ToolExecuteOrchestrationFailure::InvalidBatchRequestId)
        );
    }
    let oversized_request_id = "x".repeat(257);
    assert_eq!(
        registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &oversized_request_id,
                &executable,
            )
            .await,
        Err(ToolExecuteOrchestrationFailure::InvalidBatchRequestId)
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

/// The MCP seam proves overlap with a gate rather than elapsed-time guesses.
/// Four calls must reach the owner before the test opens the gate; a serial
/// executor therefore fails this test without making the test depend on a
/// provider's response latency.
#[tokio::test]
async fn independent_reads_overlap_with_a_four_call_bound_and_keep_input_order() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let (started_tx, mut started_rx) = tokio::sync::mpsc::unbounded_channel();
    let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.independent",
        ]));
    let mut independent_policy = policy(ApprovalClass::None, EffectClass::ReadLocal);
    independent_policy.execution = ExecutionDeclaration::IndependentRead;
    let active_for_owner = Arc::clone(&active);
    let maximum_for_owner = Arc::clone(&maximum_active);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.independent"),
            independent_policy,
            move |_context, input| {
                let active = Arc::clone(&active_for_owner);
                let maximum_active = Arc::clone(&maximum_for_owner);
                let started_tx = started_tx.clone();
                let mut gate_rx = gate_rx.clone();
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum_active.fetch_max(current, Ordering::SeqCst);
                    started_tx.send(input.value.clone()).unwrap();
                    while !*gate_rx.borrow() {
                        gate_rx.changed().await.unwrap();
                    }
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("independent test owner succeeds"),
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: (0..8)
            .map(|index| {
                call(
                    &format!("call-{index}"),
                    "onecrew.test.independent",
                    revision(&registry, "onecrew.test.independent"),
                    json!({"value": format!("value-{index}")}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Never,
    };
    let control = crate::server::McpAdapterUpdateControl::default();
    let registry_for_execute = Arc::clone(&registry);
    let platform_for_execute = Arc::clone(&platform);
    let origin_for_execute = origin.clone();
    let owned_control = control.clone();
    let caller = tokio::spawn(async move {
        owned_control
            .run_owned(async move {
                registry_for_execute
                    .tool_execute_v2(
                        &platform_for_execute,
                        &origin_for_execute,
                        SurfaceId::Full,
                        "independent-barrier",
                        &input,
                    )
                    .await
                    .map_err(|_| rmcp::ErrorData::internal_error("test execute failed", None))
            })
            .await
    });
    for _ in 0..4 {
        timeout(Duration::from_millis(250), started_rx.recv())
            .await
            .expect("four independent reads must reach the owner")
            .expect("owner start channel remains open");
    }
    assert_eq!(maximum_active.load(Ordering::SeqCst), 4);
    assert_eq!(active.load(Ordering::SeqCst), 4);
    assert!(started_rx.try_recv().is_err());
    assert!(control.pause(Duration::from_millis(10)).await.is_err());
    gate_tx.send(true).unwrap();

    let result = caller.await.unwrap().unwrap();
    let values = result
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ToolExecutionOutcome::Completed { result, .. } => result
                .get("value")
                .and_then(Value::as_str)
                .unwrap()
                .to_owned(),
            other => panic!("independent read unexpectedly stopped: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        (0..8)
            .map(|index| format!("value-{index}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(control.pause(Duration::from_secs(1)).await.is_ok());
}

#[tokio::test]
async fn a_mixed_batch_keeps_independent_reads_on_the_serial_path() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.mixed-first",
            "onecrew.test.mixed-sequential",
            "onecrew.test.mixed-last",
        ]));
    for (id, declaration) in [
        (
            "onecrew.test.mixed-first",
            ExecutionDeclaration::IndependentRead,
        ),
        (
            "onecrew.test.mixed-sequential",
            ExecutionDeclaration::Sequential,
        ),
        (
            "onecrew.test.mixed-last",
            ExecutionDeclaration::IndependentRead,
        ),
    ] {
        let active_for_owner = Arc::clone(&active);
        let maximum_for_owner = Arc::clone(&maximum_active);
        let mut operation_policy = policy(ApprovalClass::None, EffectClass::ReadLocal);
        operation_policy.execution = declaration;
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                move |_context, input| {
                    let active = Arc::clone(&active_for_owner);
                    let maximum_active = Arc::clone(&maximum_for_owner);
                    async move {
                        let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                        maximum_active.fetch_max(current, Ordering::SeqCst);
                        tokio::task::yield_now().await;
                        active.fetch_sub(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("mixed test owner succeeds"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: [
            ("call-first", "onecrew.test.mixed-first"),
            ("call-sequential", "onecrew.test.mixed-sequential"),
            ("call-last", "onecrew.test.mixed-last"),
        ]
        .into_iter()
        .map(|(call_id, operation_id)| {
            call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value": call_id}),
            )
        })
        .collect(),
        presentation: PresentationRequest::Never,
    };
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "mixed-serial", &input)
        .await
        .unwrap();
    assert_eq!(result.outcomes.len(), 3);
    assert_eq!(maximum_active.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_immediate_independent_failure_skips_only_calls_never_polled() {
    let started = Arc::new(Mutex::new(Vec::new()));
    let failure_observed = Arc::new(tokio::sync::Notify::new());
    let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.failure-stop",
        ]));
    let started_for_owner = Arc::clone(&started);
    let failure_for_owner = Arc::clone(&failure_observed);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.failure-stop"),
            {
                let mut operation_policy = policy(ApprovalClass::None, EffectClass::ReadLocal);
                operation_policy.execution = ExecutionDeclaration::IndependentRead;
                operation_policy
            },
            move |_context, input| {
                let started = Arc::clone(&started_for_owner);
                let failure_observed = Arc::clone(&failure_for_owner);
                let mut gate_rx = gate_rx.clone();
                let index = input
                    .value
                    .strip_prefix("value-")
                    .and_then(|value| value.parse::<usize>().ok())
                    .expect("failure-stop fixture uses indexed values");
                async move {
                    started.lock().unwrap().push(index);
                    if index == 0 {
                        failure_observed.notify_one();
                        return Err(TestOwnerError);
                    }
                    while !*gate_rx.borrow() {
                        gate_rx.changed().await.unwrap();
                    }
                    Ok(TestOutput {
                        value: format!("completed-{index}"),
                    })
                }
            },
            |_| {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("immediate_read_failure").unwrap(),
                    true,
                    None,
                )
            },
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: (0..8)
            .map(|index| {
                call(
                    &format!("call-{index}"),
                    "onecrew.test.failure-stop",
                    revision(&registry, "onecrew.test.failure-stop"),
                    json!({"value": format!("value-{index}")}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Never,
    };
    let control = crate::server::McpAdapterUpdateControl::default();
    let registry_for_execute = Arc::clone(&registry);
    let platform_for_execute = Arc::clone(&platform);
    let origin_for_execute = origin.clone();
    let owned_control = control.clone();
    let caller = tokio::spawn(async move {
        owned_control
            .run_owned(async move {
                registry_for_execute
                    .tool_execute_v2(
                        &platform_for_execute,
                        &origin_for_execute,
                        SurfaceId::Full,
                        "failure-stop-barrier",
                        &input,
                    )
                    .await
                    .map_err(|_| rmcp::ErrorData::internal_error("test execute failed", None))
            })
            .await
    });
    timeout(Duration::from_millis(250), failure_observed.notified())
        .await
        .expect("the first owner failure is observed");
    gate_tx.send(true).unwrap();
    let result = caller.await.unwrap().unwrap();
    let started = started.lock().unwrap().clone();
    assert!(!started.is_empty());
    assert!(started.len() <= 4);
    for (index, outcome) in result.outcomes.iter().enumerate() {
        if index == 0 {
            assert!(matches!(outcome, ToolExecutionOutcome::Failed { .. }));
        } else if started.contains(&index) {
            assert!(matches!(outcome, ToolExecutionOutcome::Completed { .. }));
        } else {
            assert!(matches!(outcome, ToolExecutionOutcome::Skipped { .. }));
        }
    }
}

#[tokio::test]
async fn cancellation_keeps_started_owner_work_owned_until_terminal() {
    let active = Arc::new(AtomicUsize::new(0));
    let settled = Arc::new(Mutex::new(Vec::new()));
    let terminal = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = Arc::new(tokio::sync::Notify::new());
    let (gate_tx, gate_rx) = tokio::sync::watch::channel(false);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.cancellable-read",
        ]));
    let active_for_owner = Arc::clone(&active);
    let settled_for_owner = Arc::clone(&settled);
    let terminal_for_owner = Arc::clone(&terminal);
    let started_for_owner = Arc::clone(&started);
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.cancellable-read"),
            {
                let mut operation_policy = policy(ApprovalClass::None, EffectClass::ReadLocal);
                operation_policy.execution = ExecutionDeclaration::IndependentRead;
                operation_policy
            },
            move |_context, _input| {
                let active = Arc::clone(&active_for_owner);
                let settled = Arc::clone(&settled_for_owner);
                let terminal = Arc::clone(&terminal_for_owner);
                let started = Arc::clone(&started_for_owner);
                let mut gate_rx = gate_rx.clone();
                async move {
                    struct EffectGuard {
                        active: Arc<AtomicUsize>,
                        settled: Arc<Mutex<Vec<&'static str>>>,
                        terminal: Arc<AtomicBool>,
                    }
                    impl Drop for EffectGuard {
                        fn drop(&mut self) {
                            self.active.fetch_sub(1, Ordering::SeqCst);
                            self.settled.lock().unwrap().push(
                                if self.terminal.load(Ordering::SeqCst) {
                                    "terminal"
                                } else {
                                    "unknown"
                                },
                            );
                        }
                    }
                    active.fetch_add(1, Ordering::SeqCst);
                    let _guard = EffectGuard {
                        active,
                        settled,
                        terminal,
                    };
                    started.notify_one();
                    while !*gate_rx.borrow() {
                        gate_rx.changed().await.unwrap();
                    }
                    _guard.terminal.store(true, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        value: "completed-after-cancel".to_owned(),
                    })
                }
            },
            |_| unreachable!("cancellable owner never returns"),
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call-cancellable",
            "onecrew.test.cancellable-read",
            revision(&registry, "onecrew.test.cancellable-read"),
            json!({"value":"pending"}),
        )],
        presentation: PresentationRequest::Never,
    };
    let control = crate::server::McpAdapterUpdateControl::default();
    let registry_for_execute = Arc::clone(&registry);
    let platform_for_execute = Arc::clone(&platform);
    let origin_for_execute = origin.clone();
    let owned_control = control.clone();
    let caller = tokio::spawn(async move {
        owned_control
            .run_owned(async move {
                registry_for_execute
                    .tool_execute_v2(
                        &platform_for_execute,
                        &origin_for_execute,
                        SurfaceId::Full,
                        "cancellable-read",
                        &input,
                    )
                    .await
                    .map_err(|_| rmcp::ErrorData::internal_error("test execute failed", None))
            })
            .await
    });
    started.notified().await;
    assert_eq!(active.load(Ordering::SeqCst), 1);
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(control.pause(Duration::from_millis(10)).await.is_err());
    gate_tx.send(true).unwrap();
    assert!(control.pause(Duration::from_secs(1)).await.is_ok());
    assert_eq!(active.load(Ordering::SeqCst), 0);
    assert!(terminal.load(Ordering::SeqCst));
    assert_eq!(settled.lock().unwrap().as_slice(), ["terminal"]);
}
