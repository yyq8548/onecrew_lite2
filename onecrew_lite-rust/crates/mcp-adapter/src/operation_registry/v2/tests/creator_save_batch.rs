use std::sync::{Arc, atomic::Ordering};

use onecrew_local_runtime::{
    CreatorSaveBatchRowStatus as RowStatus, PendingActionView, PresentationSection,
    ResultContinuationContext, ResultSnapshot, ResultState,
};

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
        creator_save::BATCH_PREPARE,
    },
};

fn outcomes(snapshot: &ResultSnapshot) -> &[onecrew_local_runtime::CreatorSaveBatchOutcome] {
    let [PresentationSection::CreatorSaveRows { rows }] = snapshot.presentation.sections.as_slice()
    else {
        panic!("typed receipt rows");
    };
    rows
}

#[tokio::test]
async fn unified_batch_saves_selected_rows_on_the_original_presented_card_and_call() {
    let (_directory, platform, origin) = result_owner();
    let mut entries = support::entries(&platform, 5);
    let source_url = entries[0].account.profile_url.clone();
    entries[0].contact_evidence.push(
        onecrew_creator_runtime::CreatorSaveContactEvidence::ProfileField {
            source: onecrew_creator_runtime::SourceKind::ProfileBio,
            source_url,
            text: "Contact first-row-only@example.com".into(),
        },
    );
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let available = support::prepare_review(&fixture, &platform, &origin).await;
    assert_eq!(support::saved_count(&platform), 0);
    let Some(PendingActionView::CreatorSaveBatch { review }) = &available.pending_action else {
        panic!("review")
    };
    assert_eq!(
        review.rows[0].preview.avatar_url.as_deref(),
        Some("https://example.com/0.jpg")
    );
    // Route IDs are local to their row. Choose a route that actually exists
    // only in the first row, rather than assuming equal route IDs are foreign.
    let foreign_route = review.rows[0]
        .preview
        .routes
        .iter()
        .find(|route| {
            !review.rows[1]
                .preview
                .routes
                .iter()
                .any(|other| other.route_id == route.route_id)
        })
        .expect("the first row has an additional route")
        .route_id
        .clone();
    let mut invalid = support::selections(&available, 4);
    invalid[1].confirmed_route_ids.push(foreign_route);
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: available.binding.clone(),
                    selections: invalid
                }
            )
            .await
            .is_err()
    );
    assert_eq!(
        platform.read_current_result(&available.binding).unwrap(),
        available
    );
    assert_eq!(support::saved_count(&platform), 0);
    let selected = support::selections(&available, 4);
    let ResultStateOutput::Snapshot { snapshot, .. } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: available.binding.clone(),
                selections: selected.clone(),
            },
        )
        .await
        .unwrap()
    else {
        panic!("snapshot")
    };
    assert_eq!(support::saved_count(&platform), 4);
    assert!(
        matches!(&snapshot.state, ResultState::Resolved(value) if value.code == "creator_save_completed")
    );
    assert_eq!(snapshot.binding.result_ref, available.binding.result_ref);
    assert_eq!(snapshot.binding.view, available.binding.view);
    assert_eq!(snapshot.binding.origin, available.binding.origin);
    assert_eq!(snapshot.binding.fingerprint, available.binding.fingerprint);
    assert!(snapshot.binding.revision > available.binding.revision);
    assert!(snapshot.presented);
    assert_eq!(snapshot.locale, available.locale);
    assert!(
        platform
            .present_result(&snapshot.binding.result_ref, &origin)
            .is_err()
    );
    assert!(
        outcomes(&snapshot)[..4]
            .iter()
            .all(|row| row.status == RowStatus::Saved && row.creator_id.is_some())
    );
    assert_eq!(outcomes(&snapshot)[4].status, RowStatus::NotSelected);
    assert_eq!(
        snapshot.batch.outcomes.len(),
        1,
        "one save batch is one original MCP call"
    );
    assert_eq!(snapshot.batch.outcomes[0].call_id, "batch-call");
    assert_eq!(
        snapshot.batch.outcomes[0].disposition,
        onecrew_local_runtime::CallDisposition::Succeeded
    );
    let saved_ids = outcomes(&snapshot)[..4]
        .iter()
        .map(|row| row.creator_id.as_deref().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let resources = &snapshot.batch.outcomes[0].resource_refs;
    assert_eq!(resources.len(), saved_ids.len());
    assert!(
        resources.iter().all(|resource| {
            resource.kind == onecrew_local_runtime::ResultResourceKind::Creator
        })
    );
    assert_eq!(
        resources
            .iter()
            .map(|resource| resource.id.as_str())
            .collect::<std::collections::BTreeSet<_>>(),
        saved_ids,
        "terminal resources contain exactly the saved Creator readbacks"
    );
    assert!(outcomes(&snapshot)[4].creator_id.is_none());
    assert!(
        fixture
            .registry
            .result_state_v2(
                &platform,
                SurfaceId::Full,
                ResultStateInput::ConfirmCreatorSaveBatch {
                    binding: available.binding,
                    selections: selected
                }
            )
            .await
            .is_err()
    );
    assert_eq!(support::saved_count(&platform), 4);
    let claimed = platform
        .claim_result_continuation(onecrew_local_runtime::ClaimResultContinuationInput {
            binding: snapshot.binding,
            expected_continuation_revision: snapshot.continuation.continuation_revision,
            mode: onecrew_local_runtime::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    assert!(claimed.request.requires_model_context);
    let context = serde_json::to_value(&claimed.request.context).unwrap();
    let first = context["outcomes"][0].as_object().unwrap();
    assert_eq!(
        first.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "creator_id",
            "failure_code",
            "operation",
            "reason",
            "row_id",
            "status",
        ]
    );
    assert!(matches!(
        claimed.request.context,
        ResultContinuationContext::CreatorSaveBatch { summary, outcomes }
        if outcomes.len() == 5
            && outcomes[4].status == RowStatus::NotSelected
            && summary.total == 5
            && summary.not_selected == 1
            && summary.added == 4
    ));
}

#[tokio::test]
async fn one_mcp_batch_save_call_prepares_a_full_business_batch_and_reuses_the_owner_ref() {
    let (_directory, platform, origin) = result_owner();
    let count = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
    let fixture = support::registry(Arc::clone(&platform), count);
    let input = ToolExecuteInput {
        calls: vec![call(
            "thousand-rows",
            BATCH_PREPARE,
            revision(&fixture.registry, BATCH_PREPARE),
            serde_json::json!({"candidates": fixture.candidates}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let output = fixture
        .registry
        .tool_execute_v2_with_minter(
            &platform,
            &origin,
            SurfaceId::Full,
            "business-batch",
            &input,
            &|_| panic!("save preparation owns the unique mint"),
        )
        .await
        .unwrap();
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 1);
    assert!(matches!(
        &output.outcomes[..],
        [ToolExecutionOutcome::InputRequired { .. }]
    ));
    let reference = output.result_ref.unwrap();
    assert_eq!(fixture.contexts.lock().unwrap().len(), 1);
    assert_eq!(
        fixture.contexts.lock().unwrap()[0].call_id(),
        "thousand-rows"
    );
    assert!(fixture.contexts.lock().unwrap()[0].call_digest().is_some());
    let snapshot = platform.present_result(&reference, &origin).unwrap();
    let Some(PendingActionView::CreatorSaveBatch { review }) = snapshot.pending_action else {
        panic!("batch review")
    };
    assert_eq!(review.rows.len(), count);
    assert_eq!(
        review.intent,
        onecrew_local_runtime::CreatorSaveIntent::SaveOrUpdate
    );
    assert_eq!(review.summary.total, count);
    assert_eq!(review.summary.eligible_new, count);
    assert_eq!(
        review.summary.eligible_new
            + review.summary.eligible_update
            + review.summary.existing_skipped
            + review.summary.duplicate_skipped
            + review.summary.blocked
            + review.summary.failed,
        review.summary.total,
        "the owner summary partitions every prepared input"
    );
    assert_eq!(review.rows[count - 1].row_id, format!("row-{}", count - 1));
    assert_eq!(support::saved_count(&platform), 0);
}

#[tokio::test]
async fn mixed_preparation_failure_stays_visible_through_confirm_and_continuation() {
    let (_directory, platform, origin) = result_owner();
    let mut entries = support::entries(&platform, 50);
    entries[49].account.display_name = "   ".into();
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let available = support::prepare_review(&fixture, &platform, &origin).await;
    let Some(PendingActionView::CreatorSaveBatch { review }) = &available.pending_action else {
        panic!("mixed review");
    };
    assert_eq!(review.rows.len(), 49);
    assert_eq!(review.failures.len(), 1);
    assert_eq!(review.summary.total, 50);
    assert_eq!(review.summary.failed, 1);
    assert_eq!(
        review.summary.eligible_new
            + review.summary.eligible_update
            + review.summary.existing_skipped
            + review.summary.duplicate_skipped
            + review.summary.blocked
            + review.summary.failed,
        50
    );
    let selected = support::selections(&available, 50);
    let ResultStateOutput::Snapshot { snapshot, .. } = fixture
        .registry
        .result_state_v2(
            &platform,
            SurfaceId::Full,
            ResultStateInput::ConfirmCreatorSaveBatch {
                binding: available.binding,
                selections: selected,
            },
        )
        .await
        .unwrap()
    else {
        panic!("mixed terminal snapshot");
    };
    assert_eq!(support::saved_count(&platform), 49);
    let rows = outcomes(&snapshot);
    assert_eq!(rows.len(), 50);
    let failure = rows
        .iter()
        .find(|row| row.operation == onecrew_local_runtime::CreatorSaveBatchOperation::Failed)
        .expect("the failed preparation row remains in the receipt");
    assert_eq!(
        failure.reason,
        Some(onecrew_local_runtime::CreatorSaveBatchReason::PreparationFailed)
    );
    assert_eq!(snapshot.batch.outcomes.len(), 1);
    assert_eq!(
        snapshot.batch.outcomes[0].disposition,
        onecrew_local_runtime::CallDisposition::Failed {
            code: "creator_save_partial_failure".into()
        }
    );
    let claimed = platform
        .claim_result_continuation(onecrew_local_runtime::ClaimResultContinuationInput {
            binding: snapshot.binding,
            expected_continuation_revision: snapshot.continuation.continuation_revision,
            mode: onecrew_local_runtime::ResultContinuationClaimMode::Pending,
        })
        .unwrap();
    let ResultContinuationContext::CreatorSaveBatch { summary, outcomes } = claimed.request.context
    else {
        panic!("typed mixed continuation");
    };
    assert_eq!(summary.total, 50);
    assert_eq!(summary.added, 49);
    assert_eq!(summary.failed, 1);
    assert_eq!(outcomes.len(), 50);
}

#[tokio::test]
async fn one_candidate_over_the_business_limit_fails_schema_before_the_owner_runs() {
    let (_directory, platform, origin) = result_owner();
    let fixture = support::registry(Arc::clone(&platform), 1);
    let input = ToolExecuteInput {
        calls: vec![call(
            "too-many",
            BATCH_PREPARE,
            revision(&fixture.registry, BATCH_PREPARE),
            serde_json::json!({"candidates": vec![
                fixture.candidates[0].clone();
                onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS + 1
            ]}),
        )],
        presentation: PresentationRequest::Requested,
    };
    let error = fixture
        .registry
        .tool_execute_v2(
            &platform,
            &origin,
            SurfaceId::Full,
            "too-many-selections",
            &input,
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        super::super::execute::ToolExecuteOrchestrationFailure::Prevalidation { .. }
    ));
    assert_eq!(fixture.prepares.load(Ordering::SeqCst), 0);
    assert!(fixture.contexts.lock().unwrap().is_empty());
    assert_eq!(support::saved_count(&platform), 0);
}
