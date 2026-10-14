use std::{
    collections::BTreeSet,
    sync::{Arc, atomic::Ordering},
};

use onecrew_local_runtime::{
    CallDisposition, CreatorSaveBatchRowStatus, PendingActionView, PresentationSection,
    ResultContinuationState, ResultState,
};

use super::{
    creator_save_support as support,
    fixtures::{call, result_owner, revision},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    v2::{
        contract::{PresentationRequest, ResultStateInput, ResultStateOutput, ToolExecuteInput},
        creator_save::{BATCH_PREPARE, PREPARE},
    },
};

#[tokio::test]
async fn seven_large_reads_leave_prefix_budget_for_the_creator_review_tail() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry_with_large_reads(Arc::clone(&platform), 1);
    let reads = (0..7).map(|index| {
        call(
            &format!("native-read-{index}"),
            "onecrew.test.native-read",
            revision(&fixture.registry, "onecrew.test.native-read"),
            serde_json::json!({"value": format!("query-{index}")}),
        )
    });
    let save = call(
        "creator-review",
        PREPARE,
        revision(&fixture.registry, PREPARE),
        serde_json::to_value(&fixture.candidates[0]).unwrap(),
    );
    let calls = reads.chain(std::iter::once(save)).collect::<Vec<_>>();
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "seven-reads-before-review",
            &ToolExecuteInput {
                calls,
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();

    assert_eq!(output.presentation_failure, None);
    let result_ref = output
        .result_ref
        .expect("the exact Creator review remains mintable");
    let snapshot = platform.present_result(&result_ref, &origin).unwrap();
    assert!(matches!(&snapshot.state, ResultState::Available));
    assert!(matches!(
        snapshot.pending_action.as_ref(),
        Some(PendingActionView::CreatorSaveBatch { .. })
    ));
    assert!(matches!(
        snapshot.batch.outcomes.last(),
        Some(onecrew_local_runtime::ExecutedCallOutcome {
            disposition: CallDisposition::NotRun { .. },
            ..
        })
    ));
    assert!(
        snapshot.batch.outcomes[..7]
            .iter()
            .all(|outcome| outcome.disposition == CallDisposition::Succeeded)
    );
    let prefix = &snapshot.presentation.sections[..snapshot.completed_sections_count];
    let prefix_bytes = serde_json::to_vec(prefix).unwrap();
    assert!(
        prefix_bytes.len() <= 64 * 1024,
        "readonly prefix is {} bytes",
        prefix_bytes.len()
    );
}

#[tokio::test]
async fn resumed_reads_keep_the_completed_prefix_bounded_before_next_review() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry_with_large_reads(Arc::clone(&platform), 2);
    let first_save = call(
        "first-save",
        PREPARE,
        revision(&fixture.registry, PREPARE),
        serde_json::to_value(&fixture.candidates[0]).unwrap(),
    );
    let reads = (0..6).map(|index| {
        call(
            &format!("native-read-{index}"),
            "onecrew.test.native-read",
            revision(&fixture.registry, "onecrew.test.native-read"),
            serde_json::json!({"value": format!("query-{index}")}),
        )
    });
    let second_save = call(
        "second-save",
        PREPARE,
        revision(&fixture.registry, PREPARE),
        serde_json::to_value(&fixture.candidates[1]).unwrap(),
    );
    let calls = std::iter::once(first_save)
        .chain(reads)
        .chain(std::iter::once(second_save))
        .collect::<Vec<_>>();
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "resume-seven-reads-before-review",
            &ToolExecuteInput {
                calls,
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let first = platform
        .present_result(
            &output.result_ref.expect("first review is presentable"),
            &origin,
        )
        .unwrap();
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
        panic!("resumed review")
    };

    assert!(matches!(&second.state, ResultState::Available));
    assert!(matches!(
        second.pending_action.as_ref(),
        Some(PendingActionView::CreatorSaveBatch { .. })
    ));
    assert_eq!(support::saved_count(&platform), 1);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 2);
    assert!(
        second.batch.outcomes[..7]
            .iter()
            .all(|outcome| outcome.disposition == CallDisposition::Succeeded)
    );
    assert!(matches!(
        second.batch.outcomes.last(),
        Some(outcome) if matches!(outcome.disposition, CallDisposition::NotRun { .. })
    ));
    let prefix = &second.presentation.sections[..second.completed_sections_count];
    let prefix_bytes = serde_json::to_vec(prefix).unwrap();
    assert!(
        prefix_bytes.len() <= 64 * 1024,
        "resumed readonly prefix is {} bytes",
        prefix_bytes.len()
    );
}

#[tokio::test]
async fn large_creator_receipts_are_exact_while_following_reads_stay_bounded() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry_with_large_reads_and_names(Arc::clone(&platform), 50, 1_600);
    let reads = (0..2).map(|index| {
        call(
            &format!("native-read-{index}"),
            "onecrew.test.native-read",
            revision(&fixture.registry, "onecrew.test.native-read"),
            serde_json::json!({"value": format!("query-{index}")}),
        )
    });
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "large-receipts-followed-by-reads",
            &ToolExecuteInput {
                calls: std::iter::once(call(
                    "creator-batch",
                    BATCH_PREPARE,
                    revision(&fixture.registry, BATCH_PREPARE),
                    serde_json::json!({"candidates": fixture.candidates.clone()}),
                ))
                .chain(reads)
                .collect(),
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let first = platform
        .present_result(
            &output.result_ref.expect("batch review is presentable"),
            &origin,
        )
        .unwrap();
    let ResultStateOutput::Snapshot {
        snapshot: terminal, ..
    } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: first.binding.clone(),
                selections: support::selections(&first, 50),
            },
        )
        .await
        .unwrap()
    else {
        panic!("completed Creator batch")
    };

    assert!(matches!(&terminal.state, ResultState::Resolved(_)));
    assert_eq!(support::saved_count(&platform), 50);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
    assert!(
        terminal
            .batch
            .outcomes
            .iter()
            .all(|outcome| outcome.disposition == CallDisposition::Succeeded)
    );
    let rows = terminal
        .presentation
        .sections
        .iter()
        .find_map(|section| match section {
            PresentationSection::CreatorSaveRows { rows } => Some(rows),
            _ => None,
        })
        .expect("exact Creator receipts remain visible");
    assert_eq!(rows.len(), 50);
    assert!(rows.iter().all(|row| {
        row.display_name
            .as_deref()
            .is_some_and(|name| name.len() >= 1_600)
    }));
    assert!(serde_json::to_vec(rows).unwrap().len() > 64 * 1024);
}

#[tokio::test]
async fn reads_and_two_sealed_saves_advance_one_card_and_keep_complete_receipts() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 2);
    let read = |id, operation| {
        call(
            id,
            operation,
            revision(&fixture.registry, operation),
            serde_json::json!({"value":"inspect saved count"}),
        )
    };
    let save = |id, index| {
        call(
            id,
            PREPARE,
            revision(&fixture.registry, PREPARE),
            serde_json::to_value(&fixture.candidates[index]).unwrap(),
        )
    };
    let input = ToolExecuteInput {
        calls: vec![
            read("read-1", "onecrew.test.read-before"),
            save("save-1", 0),
            read("read-2", "onecrew.test.read-between"),
            save("save-2", 1),
        ],
        presentation: PresentationRequest::Requested,
    };
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "mixed-owned-batch",
            &input,
        )
        .await
        .unwrap();
    let first = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
    assert_eq!(
        *fixture.reads.lock().unwrap(),
        [("onecrew.test.read-before".into(), 0)]
    );
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
        panic!("second owned review")
    };
    assert!(matches!(second.state, ResultState::Available));
    assert_eq!(
        *fixture.reads.lock().unwrap(),
        [
            ("onecrew.test.read-before".into(), 0),
            ("onecrew.test.read-between".into(), 1)
        ]
    );
    assert_eq!(
        fixture.prepares.load(Ordering::SeqCst),
        2,
        "confirmation never reruns an already prepared binding"
    );
    assert_eq!(support::saved_count(&platform), 1);
    assert_eq!(second.binding.result_ref, first.binding.result_ref);
    assert_eq!(second.binding.view, first.binding.view);
    assert_eq!(second.binding.fingerprint, first.binding.fingerprint);
    assert!(second.binding.revision > first.binding.revision);
    assert!(second.presented);
    assert_eq!(
        second.continuation.state,
        ResultContinuationState::NotRequested
    );
    assert_eq!(
        second
            .batch
            .outcomes
            .iter()
            .map(|row| row.call_id.as_str())
            .collect::<Vec<_>>(),
        ["read-1", "save-1", "read-2", "save-2"]
    );
    assert!(
        second.batch.outcomes[..3]
            .iter()
            .all(|row| row.disposition == CallDisposition::Succeeded)
    );
    assert!(matches!(
        second.batch.outcomes[3].disposition,
        CallDisposition::NotRun { .. }
    ));
    let completed = platform.completed_result_sections(&second.binding).unwrap();
    assert!(completed.iter().any(|section| matches!(section, PresentationSection::CreatorSaveRows { rows }
        if rows.len() == 1 && rows[0].status == CreatorSaveBatchRowStatus::Saved && !rows[0].confirmed_route_ids.is_empty())));
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: first.binding,
                    selections: support::selections(&second, 1)
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
        panic!("complete batch")
    };
    assert!(matches!(terminal.state, ResultState::Resolved(_)));
    assert_eq!(support::saved_count(&platform), 2);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.reads.lock().unwrap().len(), 2);
    assert!(
        terminal
            .batch
            .outcomes
            .iter()
            .all(|row| row.disposition == CallDisposition::Succeeded)
    );
    assert_eq!(terminal.binding.result_ref, second.binding.result_ref);
    assert!(terminal.presentation.sections.starts_with(&completed));
    assert_eq!(
        terminal.continuation.state,
        ResultContinuationState::Pending
    );
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
    assert_eq!(rows.len(), 2);
    let ids = rows
        .iter()
        .map(|row| row.creator_id.as_deref().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 2);
    let resources = terminal
        .batch
        .outcomes
        .iter()
        .flat_map(|call| call.resource_refs.iter())
        .map(|resource| resource.id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(resources, ids);
}

#[tokio::test]
async fn a_same_id_peer_call_with_changed_fields_cannot_execute_sealed_authority() {
    use crate::operation_registry::{
        descriptor::{EntryId, RegistryEntry},
        v2::resume::{ResumeCall, encode_owner_resume_envelope, owner_call_digest},
    };
    use onecrew_local_runtime::{ExecutedBatch, ExecutedCallOutcome, OwnerReviewContext};

    for changed in [
        "arguments",
        "request_id",
        "schema_revision",
        "connection",
        "surface",
    ] {
        let (_directory, platform, origin) = result_owner();
        let fixture = support::registry(Arc::clone(&platform), 1);
        let original = ResumeCall {
            request_id: format!("mcp-v2-00-{}", "a".repeat(24)),
            call: call(
                "same-call-id",
                PREPARE,
                revision(&fixture.registry, PREPARE),
                serde_json::to_value(&fixture.candidates[0]).unwrap(),
            ),
        };
        let Some(RegistryEntry::Operation(operation)) = fixture
            .registry
            .entry(&EntryId::parse(PREPARE).unwrap(), SurfaceId::Full)
        else {
            panic!("save operation")
        };
        let context = OwnerReviewContext::new(
            origin.clone(),
            "same-call-id",
            PREPARE,
            Some(owner_call_digest(SurfaceId::Full, &original, operation).unwrap()),
        )
        .unwrap();
        let review = onecrew_local_runtime::test_support::prepare_creator_save_review(
            &platform,
            support::entries(&platform, 1),
            context,
            onecrew_local_runtime::CreatorSaveIntent::SaveOrUpdate,
        )
        .await
        .unwrap();
        let mut altered = original.clone();
        let mut peer_surface = SurfaceId::Full;
        match changed {
            "arguments" => {
                altered.call.arguments.insert(
                    "notes".into(),
                    serde_json::json!("A changed body with the same call ID"),
                );
            }
            "request_id" => {
                altered.request_id = format!("mcp-v2-00-{}", "b".repeat(24));
            }
            "schema_revision" => {
                altered.call.schema_revision = "sha256:changed-schema".into();
            }
            "connection" => {
                altered.call.connection = Some(
                    crate::operation_registry::descriptor::ConnectionRef::OperatorAccount {
                        toolkit: crate::operation_registry::descriptor::OperatorToolkitId::X,
                        operator_account_id: "00000000-0000-0000-0000-000000000002".into(),
                    },
                );
            }
            "surface" => {
                peer_surface = SurfaceId::Compact;
            }
            _ => unreachable!(),
        }
        let peer = encode_owner_resume_envelope(peer_surface, altered, Vec::new()).unwrap();
        let reference = platform
            .mint_owner_review(
                review,
                ExecutedBatch {
                    outcomes: vec![ExecutedCallOutcome {
                        call_id: "same-call-id".into(),
                        operation_id: PREPARE.into(),
                        disposition: CallDisposition::NotRun {
                            reason: "confirmation_required".into(),
                        },
                        summary: "Creator save requires confirmation.".into(),
                        resource_refs: Vec::new(),
                    }],
                },
                Some(peer),
                Vec::new(),
            )
            .unwrap();
        let available = platform.present_result(&reference, &origin).unwrap();
        let ResultStateOutput::Snapshot { snapshot, .. } = fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: available.binding.clone(),
                    selections: support::selections(&available, 1),
                },
            )
            .await
            .unwrap()
        else {
            panic!("closed corrupted review")
        };
        assert!(
            matches!(&snapshot.state, ResultState::Failed(failure) if failure.code == "owner_resume_call_mismatch"),
            "{changed}: {:?}",
            snapshot.state
        );
        assert_eq!(support::saved_count(&platform), 0, "{changed}");
        assert_eq!(
            fixture.prepares.load(Ordering::SeqCst),
            0,
            "validation must not rerun the preparer"
        );
        assert!(snapshot.batch.outcomes[0].resource_refs.is_empty());
    }
}

#[tokio::test]
async fn second_review_capacity_failure_keeps_the_first_save_and_its_full_receipt() {
    use crate::operation_registry::v2::creator_save::BATCH_PREPARE;
    use onecrew_local_runtime::{
        ExecutedBatch, ExecutedCallOutcome, ExecutedResultInput, InitialResultState,
        OwnerActionPayload, PendingUserAction, PresentationField, PresentationProjection,
        ResultPresentationKind, ResultStoreError, ResultViewKind,
    };

    let (_directory, platform, origin) = result_owner();
    // One small save plus a full-cap batch save. The batch keeps its former
    // byte weight through longer descriptions, so it is still too large to be
    // re-admitted once the filler below exhausts the shared budget.
    let mut entries = support::entries(
        &platform,
        onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS + 1,
    );
    for entry in &mut entries[1..] {
        entry.profile.description = Some("x".repeat(20 * 1024));
    }
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "capacity-between-reviews",
            &ToolExecuteInput {
                calls: vec![
                    call(
                        "small-save",
                        PREPARE,
                        revision(&fixture.registry, PREPARE),
                        serde_json::to_value(&fixture.candidates[0]).unwrap(),
                    ),
                    call(
                        "large-save",
                        BATCH_PREPARE,
                        revision(&fixture.registry, BATCH_PREPARE),
                        serde_json::json!({"candidates":&fixture.candidates[1..]}),
                    ),
                ],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let available = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
    assert_eq!(support::saved_count(&platform), 0);
    // Fill the real shared byte budget with valid ordinary results. The next
    // large review must fail its actual capacity admission after the first
    // small save; no fake minter or error-injection interface is used.
    let mut reached_capacity = false;
    for index in 0..2000 {
        let filler = platform.mint_execution_result(ExecutedResultInput {
            origin: origin.clone(),
            view_kind: ResultViewKind::Confirmation,
            batch: ExecutedBatch {
                outcomes: vec![ExecutedCallOutcome {
                    call_id: format!("filler-{index}"),
                    operation_id: "onecrew.test.capacity".into(),
                    disposition: CallDisposition::NotRun {
                        reason: "confirmation_required".into(),
                    },
                    summary: "Capacity fixture awaiting confirmation.".into(),
                    resource_refs: Vec::new(),
                }],
            },
            presentation: PresentationProjection {
                kind: ResultPresentationKind::new("simple_operation").unwrap(),
                title: "Capacity fixture".into(),
                summary: None,
                sections: vec![PresentationSection::Fields {
                    heading: None,
                    fields: (0..7)
                        .map(|_| PresentationField {
                            label: "Bounded padding".into(),
                            value: "x".repeat(8192),
                        })
                        .collect(),
                }],
            },
            pending_action: Some(PendingUserAction::Confirm {
                prompt: "Capacity fixture".into(),
                label: "Confirm".into(),
                payload: OwnerActionPayload::new(vec![b'x'; 16 * 1024]).unwrap(),
            }),
            initial_state: InitialResultState::Available,
        });
        match filler {
            Ok(_) => {}
            Err(ResultStoreError::Capacity(_)) => {
                reached_capacity = true;
                break;
            }
            Err(error) => panic!("invalid capacity fixture: {error}"),
        }
    }
    assert!(
        reached_capacity,
        "the real result-store byte budget was filled"
    );
    let ResultStateOutput::Snapshot { snapshot, .. } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: available.binding.clone(),
                selections: support::selections(&available, 1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("closed capacity failure")
    };
    assert!(
        matches!(&snapshot.state, ResultState::Failed(failure) if failure.code == "resume_next_review_unavailable")
    );
    assert_eq!(support::saved_count(&platform), 1);
    assert_eq!(snapshot.binding.result_ref, available.binding.result_ref);
    assert_eq!(
        snapshot.batch.outcomes[0].disposition,
        CallDisposition::Succeeded
    );
    assert!(matches!(
        snapshot.batch.outcomes[1].disposition,
        CallDisposition::NotRun { .. }
    ));
    assert_eq!(snapshot.batch.outcomes[0].resource_refs.len(), 1);
    assert!(snapshot.presentation.sections.iter().any(|section| matches!(section, PresentationSection::CreatorSaveRows { rows }
        if rows.len() == 1 && rows[0].status == CreatorSaveBatchRowStatus::Saved && !rows[0].confirmed_route_ids.is_empty())));
}

#[tokio::test]
async fn generic_confirmation_can_precede_a_full_batch_save_and_keeps_full_receipts() {
    use crate::operation_registry::v2::creator_save::BATCH_PREPARE;
    use onecrew_local_runtime::{
        ClaimResultContinuationInput, ResultContinuationClaimMode, ResultContinuationContext,
    };

    let (_directory, platform, origin) = result_owner();
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let fixture = support::registry(Arc::clone(&platform), count);
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "generic-before-thousand",
            &ToolExecuteInput {
                calls: vec![
                    call(
                        "generic-first",
                        "onecrew.test.generic-write",
                        revision(&fixture.registry, "onecrew.test.generic-write"),
                        serde_json::json!({"value":"generic write completed"}),
                    ),
                    call(
                        "save-thousand",
                        BATCH_PREPARE,
                        revision(&fixture.registry, BATCH_PREPARE),
                        serde_json::json!({"candidates":fixture.candidates}),
                    ),
                ],
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let first = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
    assert_eq!(fixture.generic_writes.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 0);
    let ResultStateOutput::Snapshot {
        snapshot: review, ..
    } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Confirm {
                binding: first.binding.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("save review")
    };
    assert!(matches!(review.state, ResultState::Available));
    assert_eq!(review.binding.result_ref, first.binding.result_ref);
    assert_eq!(fixture.generic_writes.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(support::saved_count(&platform), 0);
    let chosen = support::selections(&review, count);
    assert_eq!(chosen.len(), count);
    let ResultStateOutput::Snapshot {
        snapshot: terminal, ..
    } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: review.binding.clone(),
                selections: chosen,
            },
        )
        .await
        .unwrap()
    else {
        panic!("complete full-batch result")
    };
    assert!(matches!(terminal.state, ResultState::Resolved(_)));
    assert_eq!(terminal.batch.outcomes.len(), 2);
    assert_eq!(terminal.batch.outcomes[1].resource_refs.len(), count);
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
    assert_eq!(rows.len(), count);
    assert!(
        rows.iter()
            .all(|row| row.status == CreatorSaveBatchRowStatus::Saved
                && !row.confirmed_route_ids.is_empty())
    );
    let continuation = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: terminal.binding,
            expected_continuation_revision: terminal.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::Execution { outcomes } = continuation.request.context else {
        panic!("mixed execution continuation")
    };
    // A full business batch saves fewer Creators than the generic 64-ref
    // budget, so the continuation carries every one and truncates nothing.
    assert_eq!(outcomes[1].resource_refs.len(), count);
    assert!(outcomes[1].summary.contains(&count.to_string()));
    // Nothing is truncated at this size: the whole batch fits inside the 64-ref
    // budget, so the summary stays the owner's own text with no "showing 64 of
    // N" note appended.
    assert!(!outcomes[1].summary.contains("complete receipts"));
    assert!(!outcomes[1].summary.contains("完整回执"));
    assert_eq!(fixture.generic_writes.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn cancelling_a_later_review_retains_the_completed_save_in_the_continuation() {
    use onecrew_local_runtime::{
        ClaimResultContinuationInput, ResultContinuationClaimMode, ResultContinuationContext,
    };
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 2);
    let output = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "cancel-second-save",
            &ToolExecuteInput {
                calls: fixture
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| {
                        call(
                            &format!("save-{index}"),
                            PREPARE,
                            revision(&fixture.registry, PREPARE),
                            serde_json::to_value(candidate).unwrap(),
                        )
                    })
                    .collect(),
                presentation: PresentationRequest::Requested,
            },
        )
        .await
        .unwrap();
    let first = platform
        .present_result(&output.result_ref.unwrap(), &origin)
        .unwrap();
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
    let completed = platform.completed_result_sections(&second.binding).unwrap();
    let ResultStateOutput::Snapshot {
        snapshot: cancelled,
        ..
    } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Cancel {
                binding: second.binding.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("cancelled remaining work")
    };
    assert_eq!(support::saved_count(&platform), 1);
    assert!(cancelled.presentation.sections.starts_with(&completed));
    let claimed = platform
        .claim_result_continuation(ClaimResultContinuationInput {
            binding: cancelled.binding,
            expected_continuation_revision: cancelled.continuation.continuation_revision,
            mode: ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::Execution { outcomes } = claimed.request.context else {
        panic!("completed prefix must remain in model context")
    };
    assert_eq!(outcomes[0].disposition, CallDisposition::Succeeded);
    assert_eq!(outcomes[0].resource_refs.len(), 1);
    assert!(matches!(
        outcomes[1].disposition,
        CallDisposition::NotRun { .. }
    ));
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: second.binding.clone(),
                    selections: support::selections(&second, 1)
                },
            )
            .await
            .is_err()
    );
    assert_eq!(support::saved_count(&platform), 1);
}
