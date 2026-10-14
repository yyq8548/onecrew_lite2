use super::*;

#[tokio::test]
async fn confirmation_returns_transient_dm_evidence_without_changing_effect_or_replaying() {
    use crate::operation_registry::{
        execution::{ExecuteEffectState, project_classified_owner_failure},
        v2::batch_resume::classify_submitted_owner_failure,
    };
    use onecrew_local_runtime::{
        DmDiagnostic, DmFailure, OwnerExecutionEffect, OwnerExecutionFailure,
    };
    for message in ["login 429 timeout", "sent successfully"] {
        for (owner_effect, projected_effect, code) in [
            (
                OwnerExecutionEffect::KnownNoEffect,
                ExecuteEffectState::KnownNoEffect,
                "outreach_preflight_rejected",
            ),
            (
                OwnerExecutionEffect::Applied,
                ExecuteEffectState::Applied,
                "email_send_recording_incomplete",
            ),
            (
                OwnerExecutionEffect::Unknown,
                ExecuteEffectState::Unknown,
                "email_send_outcome_unknown",
            ),
        ] {
            let (_directory, platform, origin) = result_owner();
            let owner_calls = Arc::new(AtomicUsize::new(0));
            let counter = Arc::clone(&owner_calls);
            let mut diagnostic = DmDiagnostic::provider("x", "x_chat", message);
            diagnostic.code = Some("451".into());
            diagnostic.http_status = Some(403);
            let dm_failure = DmFailure {
                stage: "capability",
                diagnostic,
            };
            let owner_failure = OwnerExecutionFailure {
                code,
                message: "The confirmed owner call could not finish.",
                effect: owner_effect,
                dm_failure: Some(Box::new(dm_failure.clone())),
            };
            let projected = project_classified_owner_failure(classify_submitted_owner_failure(
                owner_failure.clone(),
            ));
            assert_eq!(projected.effect, projected_effect);
            assert!(!projected.retry_safe);
            assert_eq!(projected.recovery, None);
            let mut builder =
                RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
                    "onecrew.test.confirm-dm-failure",
                ]));
            builder
                .register_operation::<TestInput, TestOutput, OwnerExecutionFailure, _, _, _>(
                    metadata("onecrew.test.confirm-dm-failure"),
                    attended_policy(),
                    move |_context, _input| {
                        let counter = Arc::clone(&counter);
                        let failure = owner_failure.clone();
                        async move {
                            counter.fetch_add(1, Ordering::SeqCst);
                            Err::<TestOutput, _>(failure)
                        }
                    },
                    classify_submitted_owner_failure,
                )
                .unwrap();
            let registry = builder.build().unwrap();
            let available = execute_and_present_confirmation(
                &registry,
                &platform,
                &origin,
                "onecrew.test.confirm-dm-failure",
                "confirm-dm-failure",
            )
            .await;
            assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
            let output = registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::Confirm {
                        binding: available.binding,
                    },
                )
                .await
                .unwrap();
            let wire = serde_json::to_value(&output).unwrap();
            assert_eq!(wire["dm_failure"]["diagnostic"]["code"], "451");
            assert_eq!(wire["dm_failure"]["stage"], "capability");
            assert!(wire.get("failure").is_none());
            let schema = serde_json::to_value(schemars::schema_for!(ResultStateOutput)).unwrap();
            assert!(
                jsonschema::options()
                    .build(&schema)
                    .unwrap()
                    .is_valid(&wire)
            );
            let ResultStateOutput::Snapshot {
                snapshot,
                dm_failure: Some(failure),
            } = output
            else {
                panic!("confirmed failure preserves the terminal snapshot and diagnostic")
            };
            assert_eq!(failure.as_ref(), &dm_failure);
            assert_eq!(
                matches!(
                    snapshot.batch.outcomes[0].disposition,
                    onecrew_local_runtime::CallDisposition::Unknown { .. }
                ),
                !matches!(owner_effect, OwnerExecutionEffect::KnownNoEffect)
            );
            let summary = snapshot.presentation.summary.as_deref().unwrap();
            match owner_effect {
                OwnerExecutionEffect::KnownNoEffect => {
                    assert!(summary.contains("did not complete"))
                }
                OwnerExecutionEffect::Applied => {
                    assert!(summary.contains("was carried out"));
                    assert!(summary.contains("Do not run"));
                }
                OwnerExecutionEffect::Unknown => {
                    assert!(!summary.contains("was carried out"));
                    assert!(summary.contains("could not be verified"));
                    assert!(summary.contains("Do not run"));
                    assert!(summary.contains("recorded status"));
                }
            }
            let readback = registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::ReadCurrent {
                        binding: snapshot.binding,
                    },
                )
                .await
                .unwrap();
            let readback = serde_json::to_value(readback).unwrap();
            assert!(
                readback.get("dm_failure").is_none(),
                "diagnostics are not persisted with ResultSnapshot"
            );
            assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test]
async fn a_pending_confirmation_states_its_prompt_once_on_the_card() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-prompt-once",
    )
    .await;
    let Some(onecrew_local_runtime::PendingActionView::Confirm { prompt, label }) =
        available.pending_action.as_ref()
    else {
        panic!("an attended call presents a confirm action")
    };
    assert_eq!(prompt, "Confirm the staged operation?");
    assert_eq!(label, "Confirm");
    // The App renders `pending_action.prompt` in its own box. The projection
    // used to repeat it as a warning notice, showing one sentence twice.
    let sections = serde_json::to_string(&available.presentation.sections).unwrap();
    assert!(
        !sections.contains(prompt.as_str()),
        "the prompt is the pending action's copy alone: {sections}"
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn result_present_is_once_origin_bound_and_model_output_has_no_actions() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.present",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.present"),
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
            "call.present",
            "onecrew.test.present",
            revision(&registry, "onecrew.test.present"),
            json!({"value":"complete"}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result_ref = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "batch-present", &input)
        .await
        .unwrap()
        .result_ref
        .unwrap();

    let presented = result_present_v2(
        &platform,
        &origin,
        ResultPresentInput {
            result_ref: result_ref.clone(),
            locale: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(presented.model.status, ResultPresentStatus::Resolved);
    let model = serde_json::to_string(&presented.model).unwrap();
    assert!(!model.contains("snapshot"));
    assert!(!model.contains("pending_action"));
    assert_eq!(
        result_present_v2(
            &platform,
            &origin,
            ResultPresentInput {
                result_ref: result_ref.clone(),
                locale: Default::default(),
            },
        )
        .unwrap_err(),
        ResultStateFailure::Replay
    );
    let guessed = "rr_00000000000000000000000000000000".parse().unwrap();
    assert_eq!(
        result_present_v2(
            &platform,
            &origin,
            ResultPresentInput {
                result_ref: guessed,
                locale: Default::default(),
            },
        )
        .unwrap_err(),
        ResultStateFailure::Absent
    );
    let other_origin = ResultOrigin::new("mcp:other").unwrap();
    assert_eq!(
        result_present_v2(
            &platform,
            &other_origin,
            ResultPresentInput {
                result_ref,
                locale: Default::default()
            },
        )
        .unwrap_err(),
        ResultStateFailure::OriginMismatch
    );
    assert_eq!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Compact,
                ResultStateInput::ReadCurrent {
                    binding: presented.snapshot.binding,
                },
            )
            .await
            .unwrap_err(),
        ResultStateFailure::OriginMismatch
    );
}

#[tokio::test]
async fn confirm_dispatches_exactly_once_and_installs_execution_continuation() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-confirm-state",
    )
    .await;

    let output = registry
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
        panic!("confirmation must return the authoritative snapshot")
    };
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert_eq!(snapshot.batch.outcomes.len(), 1);
    assert!(matches!(
        snapshot.batch.outcomes[0].disposition,
        onecrew_local_runtime::CallDisposition::Succeeded
    ));
    assert_eq!(
        snapshot.continuation.state,
        ResultContinuationState::Pending
    );
    // The owner's own return value is its bookkeeping, not the card's news.
    // A confirmed write reports that it happened; what it handed back stays
    // out of the card that says so.
    assert!(
        !serde_json::to_string(&snapshot)
            .unwrap()
            .contains("completed:approved")
    );
}

#[tokio::test]
async fn confirm_combines_prior_and_resumed_outcomes_in_original_order() {
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.after-confirm",
            "onecrew.test.before-confirm",
            "onecrew.test.middle-confirm",
        ]));
    for (id, approval) in [
        ("onecrew.test.before-confirm", ApprovalClass::None),
        ("onecrew.test.middle-confirm", ApprovalClass::AttendedPrompt),
        ("onecrew.test.after-confirm", ApprovalClass::None),
    ] {
        let operation_policy = if approval == ApprovalClass::AttendedPrompt {
            attended_policy()
        } else {
            OperationPolicy::read_only(
                EffectClass::ReadLocal,
                ConnectionRequirement::None,
                crate::operation_registry::execution::ExecutionDeclaration::Sequential,
            )
        };
        let recorder = Arc::clone(&observed);
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                operation_policy,
                move |_context, input| {
                    let recorder = Arc::clone(&recorder);
                    async move {
                        recorder.lock().unwrap().push(input.value.clone());
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("test owners succeed"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let execute = ToolExecuteInput {
        calls: [
            ("call.before", "onecrew.test.before-confirm", "before"),
            ("call.middle", "onecrew.test.middle-confirm", "middle"),
            ("call.after", "onecrew.test.after-confirm", "after"),
        ]
        .into_iter()
        .map(|(call_id, operation_id, value)| {
            call(
                call_id,
                operation_id,
                revision(&registry, operation_id),
                json!({"value":value}),
            )
        })
        .collect(),
        presentation: PresentationRequest::Never,
    };
    let result_ref = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-combined",
            &execute,
        )
        .await
        .unwrap()
        .result_ref
        .unwrap();
    assert_eq!(&*observed.lock().unwrap(), &["before"]);
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
    let output = registry
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
        panic!("confirm output")
    };
    assert_eq!(&*observed.lock().unwrap(), &["before", "middle", "after"]);
    assert_eq!(
        snapshot
            .batch
            .outcomes
            .iter()
            .map(|outcome| outcome.call_id.as_str())
            .collect::<Vec<_>>(),
        ["call.before", "call.middle", "call.after"]
    );
    assert!(snapshot.batch.outcomes.iter().all(|outcome| matches!(
        outcome.disposition,
        onecrew_local_runtime::CallDisposition::Succeeded
    )));
}

#[tokio::test]
async fn forged_and_stale_confirmation_bindings_call_no_owner() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let registry = success_attended_registry(Arc::clone(&owner_calls));
    let (_directory, platform, origin) = result_owner();
    let available = execute_and_present_confirmation(
        &registry,
        &platform,
        &origin,
        "onecrew.test.confirm",
        "batch-binding",
    )
    .await;

    let mut forged = available.binding.clone();
    forged.fingerprint = format!("sha256:{}", "0".repeat(64))
        .parse::<ResultFingerprint>()
        .unwrap();
    assert_eq!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm { binding: forged },
            )
            .await
            .unwrap_err(),
        ResultStateFailure::FingerprintMismatch
    );
    let mut stale = available.binding;
    stale.revision -= 1;
    assert!(matches!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm { binding: stale },
            )
            .await
            .unwrap_err(),
        ResultStateFailure::RevisionConflict { .. }
    ));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn each_attended_call_requires_its_own_confirmation_on_the_same_result() {
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.attended-one",
            "onecrew.test.attended-two",
        ]));
    for id in ["onecrew.test.attended-one", "onecrew.test.attended-two"] {
        let counter = Arc::clone(&owner_calls);
        builder
            .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
                metadata(id),
                attended_policy(),
                move |_context, input| {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Ok::<_, TestOwnerError>(TestOutput { value: input.value })
                    }
                },
                |_| unreachable!("fixture writes succeed after their own confirmations"),
            )
            .unwrap();
    }
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: [
            ("call.attended-one", "onecrew.test.attended-one"),
            ("call.attended-two", "onecrew.test.attended-two"),
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
    let result_ref = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "batch-two-attended",
            &input,
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
    let first_binding = available.binding.clone();
    let output = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: available.binding.clone(),
            },
        )
        .await
        .unwrap();
    let ResultStateOutput::Snapshot { snapshot, .. } = output else {
        panic!("first confirmation advances to the second review")
    };
    assert!(matches!(snapshot.state, ResultState::Available));
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.binding.result_ref, first_binding.result_ref);
    assert_eq!(snapshot.binding.view, first_binding.view);
    assert_eq!(snapshot.binding.fingerprint, first_binding.fingerprint);
    assert!(snapshot.binding.revision > first_binding.revision);
    assert!(snapshot.presented);
    assert_eq!(
        snapshot.continuation.state,
        onecrew_local_runtime::ResultContinuationState::NotRequested
    );
    assert!(matches!(
        snapshot.batch.outcomes[0].disposition,
        onecrew_local_runtime::CallDisposition::Succeeded
    ));
    assert!(matches!(
        snapshot.batch.outcomes[1].disposition,
        onecrew_local_runtime::CallDisposition::NotRun { .. }
    ));
    assert!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm {
                    binding: first_binding
                }
            )
            .await
            .is_err()
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 1);
    let second_binding = snapshot.binding.clone();
    let ResultStateOutput::Snapshot {
        snapshot: terminal, ..
    } = registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: second_binding.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("terminal result")
    };
    assert!(matches!(terminal.state, ResultState::Resolved(_)));
    assert_eq!(terminal.binding.result_ref, second_binding.result_ref);
    assert_eq!(owner_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        terminal.continuation.state,
        onecrew_local_runtime::ResultContinuationState::Pending
    );
    assert!(
        registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm {
                    binding: second_binding
                }
            )
            .await
            .is_err()
    );
    assert_eq!(owner_calls.load(Ordering::SeqCst), 2);
}
