use std::sync::{Arc, atomic::Ordering};

use onecrew_local_runtime::{
    CreatorSavePreparedStatus, PendingActionView, ResultContinuationContext, ResultState,
};
use serde_json::json;

use super::{
    creator_save_support as support,
    fixtures::{call, result_owner, revision},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    v2::{
        contract::{
            PresentationRequest, ResultStateInput, ResultStateOutput, ToolExecuteInput,
            ToolExecutionOutcome,
        },
        creator_save::PREPARE,
    },
};

#[tokio::test]
async fn single_prepare_returns_the_owners_one_row_batch_review_without_saving() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    let input = ToolExecuteInput {
        calls: vec![call(
            "save-1",
            PREPARE,
            revision(&fixture.registry, PREPARE),
            serde_json::to_value(&fixture.candidates[0]).unwrap(),
        )],
        presentation: PresentationRequest::Requested,
    };
    let result = fixture
        .registry
        .tool_execute_v2_with_minter(
            &platform,
            &origin,
            SurfaceId::Full,
            "single-prepare",
            &input,
            &|_| {
                panic!(
                    "a sealed review uses the result owner's mint, not the generic payload minter"
                )
            },
        )
        .await
        .unwrap();
    assert!(matches!(
        &result.outcomes[..],
        [ToolExecutionOutcome::InputRequired { .. }]
    ));
    let ToolExecutionOutcome::InputRequired {
        review_summary: Some(summary),
        ..
    } = &result.outcomes[0]
    else {
        panic!("Creator review summary must be model-visible on execute");
    };
    assert_eq!(summary.summary.total, 1);
    assert_eq!(summary.rows.len(), 1);
    assert_eq!(summary.rows[0].row_id, "row-0");
    assert_eq!(
        summary.rows[0].status,
        CreatorSavePreparedStatus::EligibleNew
    );
    assert!(summary.rows[0].reason.is_none());
    assert!(summary.failures.is_empty());
    let wire = serde_json::to_string(&result).unwrap();
    assert!(wire.contains("review_summary"));
    assert!(!wire.contains("creator_save_summary"));
    assert!(!wire.contains("creator_id"));
    assert!(!wire.contains("routes"));
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.contexts.lock().unwrap().len(), 1);
    assert_eq!(fixture.contexts.lock().unwrap()[0].call_id(), "save-1");
    assert!(fixture.contexts.lock().unwrap()[0].call_digest().is_some());
    assert_eq!(support::saved_count(&platform), 0);
    let snapshot = platform
        .present_result(&result.result_ref.unwrap(), &origin)
        .unwrap();
    assert!(matches!(snapshot.state, ResultState::Available));
    assert_eq!(
        snapshot.presentation.kind.as_str(),
        "creator_save_batch_confirmation"
    );
    let Some(PendingActionView::CreatorSaveBatch { review }) = &snapshot.pending_action else {
        panic!("one row still uses the batch-only confirmation contract");
    };
    assert_eq!(review.rows.len(), 1);
    let preview = &review.rows[0].preview;
    assert_eq!(review.rows[0].row_id, "row-0");
    assert_eq!(preview.notes.as_deref(), Some("Reviewed 中文"));
    assert_eq!(preview.score, Some(80));
    assert_eq!(preview.follower_counts["x"], 39617);
    assert_eq!(preview.routes.len(), 2);
    assert!(serde_json::to_vec(preview).unwrap().len() < 4096);

    let required = support::selections(&snapshot, 1);
    let optional = preview
        .routes
        .iter()
        .find(|route| !route.required)
        .unwrap()
        .route_id
        .clone();
    for routes in [
        vec![],
        vec![optional.clone()],
        vec![required[0].confirmed_route_ids[0].clone(), "foreign".into()],
        vec![required[0].confirmed_route_ids[0].clone(); 2],
    ] {
        let mut invalid = required.clone();
        invalid[0].confirmed_route_ids = routes;
        assert!(
            fixture
                .registry
                .result_state_v2(
                    &platform,
                    SurfaceId::Full,
                    ResultStateInput::ConfirmCreatorSaveBatch {
                        binding: snapshot.binding.clone(),
                        selections: invalid
                    }
                )
                .await
                .is_err()
        );
        assert_eq!(support::saved_count(&platform), 0);
        assert_eq!(
            platform.read_current_result(&snapshot.binding).unwrap(),
            snapshot
        );
    }
    let mut selected = required;
    selected[0].confirmed_route_ids.push(optional);
    let ResultStateOutput::Snapshot {
        snapshot: terminal, ..
    } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: snapshot.binding.clone(),
                selections: selected.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("terminal snapshot")
    };
    assert!(
        matches!(&terminal.state, ResultState::Resolved(resolution) if resolution.code == "creator_save_completed")
    );
    assert!(terminal.pending_action.is_none());
    assert_eq!(support::saved_count(&platform), 1);
    assert_eq!(terminal.binding.result_ref, snapshot.binding.result_ref);
    assert!(terminal.presented);
    assert_eq!(terminal.batch.outcomes.len(), 1);
    assert_eq!(terminal.batch.outcomes[0].call_id, "save-1");
    assert_eq!(
        terminal.batch.outcomes[0].disposition,
        onecrew_local_runtime::CallDisposition::Succeeded
    );
    let [resource] = terminal.batch.outcomes[0].resource_refs.as_slice() else {
        panic!("a single saved Creator has exactly one resource");
    };
    let resource_id = resource.id.clone();
    assert_eq!(
        resource.kind,
        onecrew_local_runtime::ResultResourceKind::Creator
    );
    let saved = platform
        .creators()
        .unwrap()
        .list_creators(Default::default())
        .unwrap();
    assert_eq!(resource_id, saved.items[0].id.as_str());
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: snapshot.binding,
                    selections: selected
                }
            )
            .await
            .is_err()
    );
    assert_eq!(support::saved_count(&platform), 1);
    let claimed = platform
        .claim_result_continuation(onecrew_local_runtime::ClaimResultContinuationInput {
            binding: terminal.binding,
            expected_continuation_revision: terminal.continuation.continuation_revision,
            mode: onecrew_local_runtime::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::CreatorSaveBatch { outcomes, .. } = claimed.request.context
    else {
        panic!("typed save continuation");
    };
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].row_id, "row-0");
    assert_eq!(
        outcomes[0].creator_id.as_deref(),
        Some(resource_id.as_str())
    );
}

#[tokio::test]
async fn cancel_and_generic_confirm_never_submit_creator_save() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    let snapshot = support::review(&platform, &origin, 1).await;
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::Confirm {
                    binding: snapshot.binding.clone()
                }
            )
            .await
            .is_err()
    );
    fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::Cancel {
                binding: snapshot.binding.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(support::saved_count(&platform), 0);
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: snapshot.binding.clone(),
                    selections: support::selections(&snapshot, 1)
                }
            )
            .await
            .is_err()
    );
    assert_eq!(support::saved_count(&platform), 0);
}

#[test]
fn creator_save_state_rejects_retired_single_action_and_forged_owner_authority() {
    for value in [
        json!({"action":"confirm_creator_save","binding":{},"confirmed_route_ids":[]}),
        json!({"action":"confirm_creator_save_batch","binding":{},"selections":[],"operation_id":"forged"}),
        json!({"action":"confirm_creator_save_batch","binding":{},"selections":[],"confirmed":true}),
    ] {
        assert!(serde_json::from_value::<ResultStateInput>(value).is_err());
    }
}

#[tokio::test]
async fn expired_prepared_review_cannot_be_presented_or_reconstructed_as_a_save() {
    let (_directory, platform, origin) = result_owner();
    let entries = support::entries(&platform, 1);
    let result_ref = onecrew_local_runtime::test_support::mint_expired_creator_save_review(
        &platform,
        entries,
        onecrew_local_runtime::CreatorSavePreparationContext {
            origin: origin.clone(),
            call_id: "expired".into(),
            operation_id: PREPARE.into(),
        },
        onecrew_local_runtime::CreatorSaveIntent::SaveOrUpdate,
    )
    .await
    .unwrap();
    assert!(matches!(
        platform.present_result(&result_ref, &origin),
        Err(onecrew_local_runtime::ResultStoreError::Expired)
    ));
    assert_eq!(support::saved_count(&platform), 0);
}
