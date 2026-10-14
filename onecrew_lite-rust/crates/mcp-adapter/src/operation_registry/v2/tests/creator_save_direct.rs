//! Model-visible preparation never supplies confirmation authority.
use std::sync::{Arc, atomic::Ordering};

use super::{
    creator_save_support as support,
    fixtures::{call, production_registry, result_owner, revision},
};
use crate::operation_registry::{
    descriptor::{EntryId, SurfaceId},
    v2::{
        contract::{PresentationRequest, ToolExecuteInput},
        creator_save::{BATCH_PREPARE, PREPARE},
        execute::ToolExecuteOrchestrationFailure,
    },
};

#[test]
fn retired_direct_confirm_operations_are_absent_on_every_surface() {
    let registry = production_registry();
    for id in [
        "onecrew.creator.candidate.save.confirm",
        "onecrew.creator.candidate.batch.save.confirm",
        "onecrew.creator.shortlist.add",
        "onecrew.creator.shortlist.list",
        "onecrew.creator.shortlist.remove",
    ] {
        for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
            assert!(
                registry
                    .entry(&EntryId::parse(id).unwrap(), surface)
                    .is_none(),
                "{id} on {surface:?}"
            );
        }
    }
}

#[tokio::test]
async fn single_and_batch_saves_advance_two_reviews_without_shrinking_the_call_envelope() {
    use super::super::contract::{ResultStateInput, ResultStateOutput};
    use onecrew_local_runtime::{CreatorSaveBatchRowStatus, PresentationSection, ResultState};
    for operation in [PREPARE, BATCH_PREPARE] {
        let (_directory, platform, origin) = result_owner();
        let fixture = support::registry(Arc::clone(&platform), 2);
        let input = ToolExecuteInput {
            calls: fixture
                .candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let arguments = if operation == PREPARE {
                        serde_json::to_value(candidate).unwrap()
                    } else {
                        serde_json::json!({"candidates":[candidate]})
                    };
                    call(
                        &format!("save-{index}"),
                        operation,
                        revision(&fixture.registry, operation),
                        arguments,
                    )
                })
                .collect(),
            presentation: PresentationRequest::Requested,
        };
        let output = fixture
            .registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                "two-save-reviews",
                &input,
            )
            .await
            .unwrap();
        let first = platform
            .present_result(&output.result_ref.unwrap(), &origin)
            .unwrap();
        assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
        assert_eq!(support::saved_count(&platform), 0);
        let ResultStateOutput::Snapshot {
            snapshot: second, ..
        } = fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: first.binding.clone(),
                    selections: support::selections(&first, 1),
                },
            )
            .await
            .unwrap()
        else {
            panic!("second review")
        };
        assert!(matches!(second.state, ResultState::Available));
        assert_eq!(second.binding.result_ref, first.binding.result_ref);
        assert_eq!(second.binding.fingerprint, first.binding.fingerprint);
        assert_eq!(second.binding.view, first.binding.view);
        assert!(second.binding.revision > first.binding.revision);
        assert_eq!(fixture.prepares.load(Ordering::SeqCst), 2);
        assert_eq!(support::saved_count(&platform), 1);
        let completed = platform.completed_result_sections(&second.binding).unwrap();
        assert!(completed.iter().any(
            |section| matches!(section, PresentationSection::CreatorSaveRows { rows }
            if rows.len() == 1 && rows[0].status == CreatorSaveBatchRowStatus::Saved)
        ));
        assert!(
            fixture
                .registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::ConfirmCreatorSaveBatch {
                        binding: first.binding.clone(),
                        selections: support::selections(&first, 1)
                    }
                )
                .await
                .is_err()
        );
        let ResultStateOutput::Snapshot {
            snapshot: terminal, ..
        } = fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: second.binding.clone(),
                    selections: support::selections(&second, 1),
                },
            )
            .await
            .unwrap()
        else {
            panic!("complete result")
        };
        assert!(matches!(terminal.state, ResultState::Resolved(_)));
        assert_eq!(support::saved_count(&platform), 2);
        assert_eq!(terminal.binding.result_ref, second.binding.result_ref);
        assert!(terminal.presentation.sections.starts_with(&completed));
        let rows = terminal
            .presentation
            .sections
            .iter()
            .filter_map(|section| match section {
                PresentationSection::CreatorSaveRows { rows } => Some(rows.as_slice()),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(
            rows.len(),
            2,
            "both original safe receipts survive the final result"
        );
        assert!(
            rows.iter()
                .all(|row| row.status == CreatorSaveBatchRowStatus::Saved)
        );
    }
}

#[tokio::test]
async fn repeated_explicit_prepare_never_automatically_confirms_the_previous_review() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    let input = ToolExecuteInput {
        calls: vec![call(
            "prepare",
            PREPARE,
            revision(&fixture.registry, PREPARE),
            serde_json::to_value(&fixture.candidates[0]).unwrap(),
        )],
        presentation: PresentationRequest::Never,
    };
    for request_id in ["first-prepare", "explicit-retry"] {
        let output = fixture
            .registry
            .tool_execute_v2(&platform, &origin, SurfaceId::Full, request_id, &input)
            .await
            .unwrap();
        let snapshot = platform
            .present_result(&output.result_ref.unwrap(), &origin)
            .unwrap();
        assert!(matches!(
            snapshot.state,
            onecrew_local_runtime::ResultState::Available
        ));
        assert_eq!(support::saved_count(&platform), 0);
    }
    let contexts = fixture.contexts.lock().unwrap();
    assert_eq!(contexts.len(), 2);
    assert_ne!(contexts[0].call_digest(), contexts[1].call_digest());
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn invalid_save_schema_and_unavailable_surface_are_refused_before_preparation() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    let valid = serde_json::to_value(&fixture.candidates[0]).unwrap();
    let mut forged_confirmed = valid.clone();
    forged_confirmed["confirmed"] = serde_json::json!(true);
    let mut forged_fingerprint = valid.clone();
    forged_fingerprint["fingerprint"] = serde_json::json!("forged");
    for arguments in [
        serde_json::json!({"discovery_id":"old","candidate_id":"old"}),
        serde_json::json!({"selection_ref":"00000000-0000-0000-0000-000000000001","tags":null,"quality":{}}),
        forged_confirmed,
        forged_fingerprint,
    ] {
        let input = ToolExecuteInput {
            calls: vec![call(
                "invalid",
                PREPARE,
                revision(&fixture.registry, PREPARE),
                arguments,
            )],
            presentation: PresentationRequest::Never,
        };
        assert!(matches!(
            fixture
                .registry
                .tool_execute_v2(&platform, &origin, SurfaceId::Full, "invalid-save", &input)
                .await,
            Err(ToolExecuteOrchestrationFailure::Prevalidation { .. })
        ));
    }
    let input = ToolExecuteInput {
        calls: vec![call(
            "wrong-surface",
            PREPARE,
            revision(&fixture.registry, PREPARE),
            valid,
        )],
        presentation: PresentationRequest::Never,
    };
    assert!(matches!(
        fixture
            .registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Compact,
                "wrong-surface",
                &input
            )
            .await,
        Err(ToolExecuteOrchestrationFailure::Prevalidation { .. })
    ));
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 0);
    assert!(fixture.contexts.lock().unwrap().is_empty());
    assert_eq!(support::saved_count(&platform), 0);
}

#[tokio::test]
async fn creator_tag_schema_rejects_single_and_batch_inputs_before_preparation() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    for operation_id in [PREPARE, BATCH_PREPARE] {
        for tags in [
            serde_json::json!(["AI", "YouTube", "近期活跃", "AI编程", "中文"]),
            serde_json::json!(["AI", "AI"]),
            serde_json::json!(["1234567890123"]),
            serde_json::json!([""]),
        ] {
            let mut candidate = serde_json::to_value(&fixture.candidates[0]).unwrap();
            candidate["tags"] = tags;
            let arguments = if operation_id == PREPARE {
                candidate
            } else {
                serde_json::json!({"candidates":[candidate]})
            };
            let input = ToolExecuteInput {
                calls: vec![call(
                    "invalid-tags",
                    operation_id,
                    revision(&fixture.registry, operation_id),
                    arguments,
                )],
                presentation: PresentationRequest::Never,
            };
            assert!(matches!(
                fixture
                    .registry
                    .tool_execute_v2(&platform, &origin, SurfaceId::Full, "tag-schema", &input)
                    .await,
                Err(ToolExecuteOrchestrationFailure::Prevalidation { .. })
            ));
        }
    }
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 0);
    assert_eq!(support::saved_count(&platform), 0);
}

#[tokio::test]
async fn production_prepare_distinguishes_input_rejection_from_later_read_failure() {
    use crate::operation_registry::{
        RegistryOwners, build_registry,
        descriptor::RegistryEntry,
        execution::{ExecuteEffectState, ExecuteFailurePhase, RecoveryDirective},
        v2::contract::ToolExecutionOutcome,
    };

    let (_directory, platform, origin) = result_owner();
    let registry = build_registry(
        RegistryOwners {
            creators: platform.creators().unwrap(),
            platform: Arc::clone(&platform),
            operator_accounts: Arc::new(
                crate::accounts::PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
            ),
        },
        std::sync::Arc::new(
            onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"),
        ),
    )
    .unwrap();
    // There is no saved operator session. A valid target reaches read admission
    // and fails there; an invalid target must stop before that boundary.
    let valid = serde_json::to_value(support::candidate(0)).unwrap();
    let mut invalid = valid.clone();
    invalid["target"]["handle"] = serde_json::json!("invalid handle sentinel");
    for (index, (operation_id, arguments)) in [
        (PREPARE, invalid.clone()),
        // All candidates are validated before any read, including when the
        // first candidate would otherwise fail missing-session admission.
        (
            BATCH_PREPARE,
            serde_json::json!({"candidates":[valid.clone(), invalid]}),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(RegistryEntry::Operation(operation)) =
            registry.entry(&EntryId::parse(operation_id).unwrap(), SurfaceId::Full)
        else {
            panic!("production Creator preparation operation");
        };
        let validator = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(operation.input.document())
            .unwrap();
        assert!(
            validator.is_valid(&arguments),
            "runtime rejection must follow schema acceptance"
        );
        let input = ToolExecuteInput {
            calls: vec![call(
                "invalid-target",
                operation_id,
                revision(&registry, operation_id),
                arguments,
            )],
            presentation: PresentationRequest::Requested,
        };
        let output = registry
            .tool_execute_v2(
                &platform,
                &origin,
                SurfaceId::Full,
                &format!("input-rejection-{index}"),
                &input,
            )
            .await
            .unwrap();
        if let Some(reference) = &output.result_ref {
            let diagnostic = platform.present_result(reference, &origin).unwrap();
            assert!(diagnostic.pending_action.is_none());
            assert_eq!(
                diagnostic.binding.view.kind,
                onecrew_local_runtime::ResultViewKind::ReadOnly
            );
            assert!(matches!(
                diagnostic.state,
                onecrew_local_runtime::ResultState::Failed(_)
            ));
            assert!(
                platform
                    .submit_result_review(
                        &diagnostic.binding,
                        onecrew_local_runtime::OwnerReviewSelection::Confirm
                    )
                    .is_err(),
                "a requested failure presentation must not mint confirmation authority"
            );
        }
        assert!(output.presentation_failure.is_none());
        let [ToolExecutionOutcome::Failed { failure, .. }] = output.outcomes.as_slice() else {
            panic!(
                "invalid runtime input must be a known failure: {:?}",
                output.outcomes
            );
        };
        assert_eq!(failure.code.as_str(), "creator_save_input_invalid");
        assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
        assert_eq!(failure.effect, ExecuteEffectState::KnownNoEffect);
        assert_eq!(failure.recovery, Some(RecoveryDirective::CorrectInput));
        assert!(!failure.retry_safe);
        let wire = serde_json::to_value(failure).unwrap();
        assert_eq!(wire["effect"], "known_no_effect");
        assert_eq!(
            wire["message"],
            format!(
                "Candidate {}: Creator save target is invalid. No Creator save was submitted.",
                index + 1,
            )
        );
        assert!(
            !serde_json::to_string(failure)
                .unwrap()
                .contains("invalid handle sentinel")
        );
        assert_eq!(support::saved_count(&platform), 0);
    }

    let input = ToolExecuteInput {
        calls: vec![call(
            "missing-session",
            PREPARE,
            revision(&registry, PREPARE),
            valid,
        )],
        presentation: PresentationRequest::Requested,
    };
    let output = registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "read-admission-failure",
            &input,
        )
        .await
        .unwrap();
    let [
        ToolExecutionOutcome::Unknown {
            failure,
            review_summary: Some(summary),
            ..
        },
    ] = output.outcomes.as_slice()
    else {
        panic!(
            "later preparation failure retains the owner's conservative effect class: {:?}",
            output.outcomes
        );
    };
    assert_eq!(failure.code.as_str(), "creator_save_noop_unknown");
    assert_eq!(summary.summary.total, 1);
    assert_eq!(summary.summary.failed, 1);
    assert!(summary.rows.is_empty());
    assert_eq!(summary.failures.len(), 1);
    assert_eq!(
        summary.failures[0].reason,
        onecrew_creator_runtime::CreatorSavePreparedReason::PreparationFailed
    );
    assert_eq!(failure.phase, ExecuteFailurePhase::AfterInvocation);
    assert_eq!(failure.effect, ExecuteEffectState::Unknown);
    assert_eq!(serde_json::to_value(failure).unwrap()["effect"], "unknown");
    let message = serde_json::to_value(failure).unwrap()["message"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(!message.is_empty());
    assert!(!message.contains("credential health may have changed"));
    if let Some(reference) = &output.result_ref {
        let diagnostic = platform.present_result(reference, &origin).unwrap();
        assert!(diagnostic.pending_action.is_none());
        assert!(matches!(
            diagnostic.state,
            onecrew_local_runtime::ResultState::Unknown(_)
        ));
        assert!(
            platform
                .submit_result_review(
                    &diagnostic.binding,
                    onecrew_local_runtime::OwnerReviewSelection::Confirm
                )
                .is_err()
        );
    }
    assert_eq!(support::saved_count(&platform), 0);
}

#[tokio::test]
async fn eight_generic_calls_still_execute_independently_of_creator_batch_size() {
    use super::fixtures::{TestInput, TestOutput, TestOwnerError, metadata, policy};
    use crate::operation_registry::{
        descriptor::{ApprovalClass, EffectClass},
        registration::RegistryBuilder,
    };
    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls = Arc::clone(&counter);
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            "onecrew.test.generic",
        ]));
    builder
        .register_operation::<TestInput, TestOutput, TestOwnerError, _, _, _>(
            metadata("onecrew.test.generic"),
            policy(ApprovalClass::None, EffectClass::ReadLocal),
            move |_, input| {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(TestOutput { value: input.value })
                }
            },
            |_| unreachable!("generic read fixture"),
        )
        .unwrap();
    let registry = builder.build().unwrap();
    let (_directory, platform, origin) = result_owner();
    let input = ToolExecuteInput {
        calls: (0..8)
            .map(|index| {
                call(
                    &format!("generic-{index}"),
                    "onecrew.test.generic",
                    revision(&registry, "onecrew.test.generic"),
                    serde_json::json!({"value":index.to_string()}),
                )
            })
            .collect(),
        presentation: PresentationRequest::Never,
    };
    let input: ToolExecuteInput =
        serde_json::from_value(serde_json::to_value(input).unwrap()).unwrap();
    let result = registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "eight-generic", &input)
        .await
        .unwrap();
    assert_eq!(result.outcomes.len(), 8);
    assert!(result.outcomes.iter().all(|outcome| matches!(
        outcome,
        super::super::contract::ToolExecutionOutcome::Completed { .. }
    )));
    assert_eq!(counter.load(Ordering::SeqCst), 8);
    let mut oversized = serde_json::to_value(&input).unwrap();
    let mut ninth = oversized["calls"][0].clone();
    ninth["call_id"] = serde_json::json!("generic-8");
    oversized["calls"].as_array_mut().unwrap().push(ninth);
    assert!(serde_json::from_value::<ToolExecuteInput>(oversized).is_err());
}
