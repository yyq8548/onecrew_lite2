use super::*;

#[tokio::test]
async fn request_edit_is_a_nested_typed_state_action_with_server_owned_continuation() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-edit",
    )
    .await;
    let instruction = "Use only verified public contact routes.";
    let edited = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::RequestEdit {
                input: RequestResultEditInput {
                    binding: available.binding,
                    instruction: instruction.to_owned(),
                },
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = edited else {
        panic!("edit snapshot")
    };
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
    assert!(snapshot.pending_action.is_none());
    assert_eq!(
        snapshot.continuation.state,
        ResultContinuationState::Pending
    );
    let claimed = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ClaimContinuation {
                input: ClaimResultContinuationInput {
                    binding: snapshot.binding,
                    expected_continuation_revision: snapshot.continuation.continuation_revision,
                    mode: ResultContinuationClaimMode::Pending,
                },
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::ClaimedContinuation { claimed } = claimed else {
        panic!("edit continuation claim")
    };
    assert_eq!(
        claimed.request.context,
        ResultContinuationContext::EditRequested
    );
    assert!(claimed.request.message.contains(instruction));
    assert_eq!(
        serde_json::to_value(&claimed.request.context).unwrap(),
        serde_json::json!({"kind":"edit_requested"})
    );
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChangedInput {
    value: u64,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ChangedOutput {
    value: u64,
}

#[tokio::test]
async fn resume_whole_batch_prevalidation_failure_calls_no_owner_and_closes_failed() {
    let original_calls = Arc::new(AtomicUsize::new(0));
    let mut original_builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.resume-after",
            "onecrew.test.resume-first",
        ]));
    for (id, approval) in [
        ("onecrew.test.resume-first", ApprovalClass::AttendedPrompt),
        ("onecrew.test.resume-after", ApprovalClass::None),
    ] {
        let mut operation_policy = if approval == ApprovalClass::AttendedPrompt {
            attended_policy()
        } else {
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            )
        };
        if approval == ApprovalClass::None {
            operation_policy.approval_request = None;
        }
        let counter = Arc::clone(&original_calls);
        original_builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                move |_context, input| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("original owners stay pending"),
            )
            .unwrap();
    }
    let original = original_builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let execute = ToolExecuteInput {
        calls: vec![
            call(
                "call.resume-first",
                "onecrew.test.resume-first",
                revision(&original, "onecrew.test.resume-first"),
                json!({"value":"first"}),
            ),
            call(
                "call.resume-after",
                "onecrew.test.resume-after",
                revision(&original, "onecrew.test.resume-after"),
                json!({"value":"after"}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };
    let result_ref = original
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-resume-prevalidation",
            &execute,
        )
        .await
        .unwrap()
        .result_ref
        .unwrap();
    let available = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref,
            locale: Default::default(),
        },
    )
    .unwrap()
    .snapshot;

    let resumed_calls = Arc::new(AtomicUsize::new(0));
    let mut changed_builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.resume-after",
            "onecrew.test.resume-first",
        ]));
    for (id, approval) in [
        ("onecrew.test.resume-first", ApprovalClass::AttendedPrompt),
        ("onecrew.test.resume-after", ApprovalClass::None),
    ] {
        let mut operation_policy = if approval == ApprovalClass::AttendedPrompt {
            attended_policy()
        } else {
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            )
        };
        if approval == ApprovalClass::None {
            operation_policy.approval_request = None;
        }
        let counter = Arc::clone(&resumed_calls);
        changed_builder
            .register_operation::<ChangedInput, ChangedOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                move |_context, input| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(ChangedOutput { value: input.value })
                    }
                },
                |_| unreachable!("changed owners must not run"),
            )
            .unwrap();
    }
    let changed = changed_builder.build().unwrap();
    let output = changed
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = output else {
        panic!("failed resume must return terminal snapshot")
    };
    assert!(matches!(snapshot.state, ResultState::Failed(_)));
    assert_eq!(
        snapshot.continuation.state,
        ResultContinuationState::Pending
    );
    assert_eq!(original_calls.load(Ordering::SeqCst), 0);
    assert_eq!(resumed_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancel_installs_bounded_cancelled_continuation() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-cancel",
    )
    .await;
    let output = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Cancel {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = output else {
        panic!("cancel must return snapshot")
    };
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert_eq!(
        snapshot.continuation.state,
        ResultContinuationState::Pending
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
    let claimed = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: snapshot.binding,
            expected_continuation_revision: snapshot.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    assert!(matches!(
        claimed.request.context,
        ResultContinuationContext::Cancelled
    ));
    assert!(!claimed.request.requires_model_context);
    assert_eq!(
        claimed.request.message,
        "Cancelled. This action will not be executed."
    );
}

#[tokio::test]
async fn cancel_and_edit_continuations_follow_the_presented_locale() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    for (batch_id, edit) in [("batch-zh-cancel", false), ("batch-zh-edit", true)] {
        let input = ToolExecuteInput {
            calls: vec![call(
                "call.confirm",
                "onecrew.test.confirm",
                revision(&registry, "onecrew.test.confirm"),
                json!({"value":"approved"}),
            )],
            presentation: PresentationRequest::Never,
        };
        let result_ref = registry
            .tool_execute_v2(&platform, &origin, SurfaceId::Full, batch_id, &input)
            .await
            .unwrap()
            .result_ref
            .unwrap();
        let available = platform
            .present_result_with_locale(
                &result_ref,
                &origin,
                onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
            )
            .unwrap();
        let input = if edit {
            ResultStateInput::RequestEdit {
                input: RequestResultEditInput {
                    binding: available.binding,
                    instruction: "只保留公开邮箱".to_owned(),
                },
            }
        } else {
            ResultStateInput::Cancel {
                binding: available.binding,
            }
        };
        let ResultStateOutput::Snapshot { snapshot, .. } = registry
            .result_state_v2(&platform, SurfaceId::Full, input)
            .await
            .unwrap()
        else {
            panic!("terminal snapshot")
        };
        let claimed = platform
            .claim_result_continuation(ClaimResultContinuationInput {
                binding: snapshot.binding,
                expected_continuation_revision: snapshot.continuation.continuation_revision,
                mode: ResultContinuationClaimMode::Pending,
            })
            .unwrap();
        assert!(!claimed.request.requires_model_context);
        assert_eq!(
            claimed.request.message,
            if edit {
                "用户要求修改，原操作未执行。修改要求：\n\n只保留公开邮箱"
            } else {
                "已取消，不执行此操作。"
            }
        );
    }
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn state_claim_and_complete_are_thin_owner_projections() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-continuation",
    )
    .await;
    let terminal = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding,
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = terminal else {
        panic!("confirm must return snapshot")
    };
    let claimed = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ClaimContinuation {
                input: ClaimResultContinuationInput {
                    binding: snapshot.binding,
                    expected_continuation_revision: snapshot.continuation.continuation_revision,
                    mode: ResultContinuationClaimMode::Pending,
                },
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::ClaimedContinuation { claimed } = claimed else {
        panic!("claim output")
    };
    let completed = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::CompleteContinuation {
                input: CompleteResultContinuationInput {
                    binding: claimed.binding,
                    expected_continuation_revision: claimed.continuation_revision,
                    lease_id: claimed.lease_id,
                    outcome: ResultContinuationCompletionOutcome::Delivered,
                },
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::CompletedContinuation { completed } = completed else {
        panic!("complete output")
    };
    assert_eq!(
        completed.continuation.state,
        ResultContinuationState::Delivered
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
}
