use super::*;

#[tokio::test]
async fn required_input_mints_confirmation_even_when_presentation_is_never() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let attended_counter = Arc::clone(&owner_calls);
    let later_counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.after-confirm",
            "onecrew.test.confirm",
        ]));
    let mut attended_policy = policy(ApprovalClass::AttendedPrompt, EffectClass::DeleteLocal);
    attended_policy.approval_request =
        Some(ApprovalRequest::new("Confirm the staged operation?").unwrap());
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.confirm"),
            attended_policy,
            move |_context, input| {
                let attended_counter = Arc::clone(&attended_counter);
                async move {
                    attended_counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("attended owner must remain pending"),
        )
        .unwrap();
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.after-confirm"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
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
    let opaque_value = format!("owner-envelope-sentinel-{}", "x".repeat(600));
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call.confirm",
                "onecrew.test.confirm",
                revision(&registry, "onecrew.test.confirm"),
                json!({"value":opaque_value}),
            ),
            call(
                "call.after",
                "onecrew.test.after-confirm",
                revision(&registry, "onecrew.test.after-confirm"),
                json!({"value":"later"}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };

    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "batch-confirm", &input)
        .await
        .unwrap();
    let result_ref = result
        .result_ref
        .expect("required input must mint despite presentation never");
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::InputRequired { .. }
    ));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);

    let snapshot = platform.present_result(&result_ref, &origin).unwrap();
    assert_eq!(snapshot.binding.view.kind, ResultViewKind::Confirmation);
    assert!(matches!(snapshot.state, ResultState::Available));
    assert!(snapshot.pending_action.is_some());
    let public_snapshot = serde_json::to_string(&snapshot).unwrap();
    assert!(public_snapshot.contains("owner-envelope-sentinel"));
    assert!(!public_snapshot.contains("owner_payload"));

    let submitted = platform
        .submit_result_confirmation(&snapshot.binding)
        .unwrap();
    assert!(!format!("{submitted:?}").contains("owner-envelope-sentinel"));
    let (_, _, owner_payload) = submitted.into_generic_parts().unwrap();
    let ResumeEnvelope::V1 { pending, remaining } = decode_resume_envelope(owner_payload).unwrap();
    assert_eq!(pending.call.call_id.as_str(), "call.confirm");
    assert_eq!(pending.call.operation_id.as_str(), "onecrew.test.confirm");
    assert_eq!(
        pending.call.arguments["value"],
        json!(format!("owner-envelope-sentinel-{}", "x".repeat(600)))
    );
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].call.call_id.as_str(), "call.after");
    assert_ne!(pending.request_id, remaining[0].request_id);
}

#[tokio::test]
async fn requested_completed_result_is_read_only_and_stores_no_raw_output() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.requested",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.requested"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            move |_context, _input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput {
                        value: "RAW_PROVIDER_OUTPUT_SENTINEL".to_owned(),
                    })
                }
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let requested = ToolExecuteInput {
        calls: vec![call(
            "call.requested",
            "onecrew.test.requested",
            revision(&registry, "onecrew.test.requested"),
            json!({"value":"safe preview"}),
        )],
        presentation: PresentationRequest::Requested,
    };

    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-requested",
            &requested,
        )
        .await
        .unwrap();
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::Completed { result, .. }
            if result["value"] == "RAW_PROVIDER_OUTPUT_SENTINEL"
    ));
    let result_ref = result.result_ref.expect("requested result reference");
    let snapshot = platform.present_result(&result_ref, &origin).unwrap();
    assert_eq!(snapshot.binding.view.kind, ResultViewKind::ReadOnly);
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert!(snapshot.pending_action.is_none());
    let stored = serde_json::to_string(&snapshot).unwrap();
    assert!(stored.contains("RAW_PROVIDER_OUTPUT_SENTINEL"));
    assert!(
        !serde_json::to_string(&snapshot.batch)
            .unwrap()
            .contains("RAW_PROVIDER_OUTPUT_SENTINEL")
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn if_required_completed_result_does_not_mint() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.if-required",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.if-required"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, input| async move {
                Ok::<_, TestOwnerError>(TestOutput { value: input.value })
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.if-required",
            "onecrew.test.if-required",
            revision(&registry, "onecrew.test.if-required"),
            json!({"value":"complete"}),
        )],
        presentation: PresentationRequest::IfRequired,
    };

    let result = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-if-required",
            &input,
        )
        .await
        .unwrap();
    assert!(result.result_ref.is_none());
    assert_eq!(platform.cleanup_results().unwrap().active, 0);
}

#[tokio::test]
async fn requested_failures_mint_failed_and_unknown_initial_states() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.requested-failure",
            "onecrew.test.requested-unknown",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.requested-failure"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, _input| async { Err::<TestOutput, _>(TestOwnerError) },
            |_| {
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse("known_failure").unwrap(),
                    true,
                    None,
                )
            },
        )
        .unwrap();
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.requested-unknown"),
            policy(ApprovalClass::OwnerGuarded, EffectClass::ExternalWrite),
            |_context, _input| async { Err::<TestOutput, _>(TestOwnerError) },
            |_| {
                ClassifiedOwnerFailure::unknown(
                    PublicFailureCode::parse("effect_unknown").unwrap(),
                    None,
                )
            },
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();

    for (call_id, operation_id, expected_unknown) in [
        ("call.failure", "onecrew.test.requested-failure", false),
        ("call.unknown", "onecrew.test.requested-unknown", true),
    ] {
        let input = ToolExecuteInput {
            calls: vec![call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value":"test"}),
            )],
            presentation: PresentationRequest::Requested,
        };
        let result_ref = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &format!("batch-{call_id}"),
                &input,
            )
            .await
            .unwrap()
            .result_ref
            .unwrap();
        let snapshot = platform.present_result(&result_ref, &origin).unwrap();
        assert_eq!(snapshot.binding.view.kind, ResultViewKind::ReadOnly);
        if expected_unknown {
            assert!(matches!(snapshot.state, ResultState::Unknown(_)));
        } else {
            assert!(matches!(snapshot.state, ResultState::Failed(_)));
        }
    }
}

#[tokio::test]
async fn sensitive_operation_refuses_requested_presentation_before_owner() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&owner_calls);
    let mut operation_policy = OperationPolicy::read_only(
        EffectClass::ReadLocal,
        ConnectionRequirement::None,
        crate::operation_registry::execution::ExecutionDeclaration::Sequential,
    );
    operation_policy.secret = SecretClass::Transient;
    operation_policy.sensitive_input_paths = vec!["/value".to_owned()];
    operation_policy.view_kind = OperationViewKind::ReadOnly;
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.sensitive",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.sensitive"),
            operation_policy,
            move |_context, input| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("unsafe presentation must stop before owner"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.sensitive",
            "onecrew.test.sensitive",
            revision(&registry, "onecrew.test.sensitive"),
            json!({"value":"must-not-project"}),
        )],
        presentation: PresentationRequest::Requested,
    };

    assert_eq!(
        registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                "batch-sensitive",
                &input,
            )
            .await,
        Err(ToolExecuteOrchestrationFailure::PresentationUnavailable)
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
    assert_eq!(platform.cleanup_results().unwrap().active, 0);
}

#[tokio::test]
async fn result_reference_is_absent_after_local_platform_restart() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.restart",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.restart"),
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            ),
            |_context, input| async move {
                Ok::<_, TestOwnerError>(TestOutput { value: input.value })
            },
            |_| unreachable!("test owner succeeds"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: vec![call(
            "call.restart",
            "onecrew.test.restart",
            revision(&registry, "onecrew.test.restart"),
            json!({"value":"complete"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result_ref = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "batch-restart", &input)
        .await
        .unwrap()
        .result_ref
        .unwrap();
    drop(platform);
    let restarted = onecrew_local_runtime::LocalPlatform::open(directory.path()).unwrap();
    assert_eq!(
        restarted.present_result(&result_ref, &origin).unwrap_err(),
        ResultStoreError::Absent
    );
}
