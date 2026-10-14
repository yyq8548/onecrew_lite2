use std::sync::Arc;

use onecrew_creator_runtime::{CreatorSaveIntent, CreatorSaveRouteSelection};
use onecrew_local_runtime::ResultState;
use serde_json::json;

use super::{
    creator_save_support as support,
    fixtures::{call, result_owner, revision},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    v2::contract::{PresentationRequest, ToolExecuteInput, ToolExecutionOutcome},
    v2::creator_save::BATCH_PREPARE,
};

fn seed_creator(
    platform: &onecrew_local_runtime::LocalPlatform,
    entry: onecrew_creator_runtime::CanonicalCreatorSaveInput,
) {
    let creators = platform.creators().expect("Creator owner");
    let prepared = creators
        .prepare_creator_save_batch(vec![entry], CreatorSaveIntent::SaveOrUpdate)
        .expect("seed preparation");
    let row = prepared.rows().into_iter().next().expect("seed row");
    let route_ids = row
        .route_options
        .iter()
        .filter(|route| route.required)
        .map(|route| route.route_id.clone())
        .collect();
    creators
        .execute_creator_save_batch(
            prepared
                .select_routes(vec![CreatorSaveRouteSelection {
                    row_id: row.row_id,
                    route_ids,
                }])
                .expect("seed route selection"),
            "owner-noop-seed".into(),
        )
        .expect("seed save");
}

#[tokio::test]
async fn all_existing_creator_save_is_a_payload_and_does_not_block_later_calls() {
    let (_directory, platform, origin) = result_owner();
    let entries = support::entries(&platform, 1);
    seed_creator(&platform, entries[0].clone());
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let mut candidates = fixture.candidates.clone();
    candidates[0].intent = CreatorSaveIntent::AddNew;
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call-noop-existing",
                BATCH_PREPARE,
                revision(&fixture.registry, BATCH_PREPARE),
                json!({"candidates": candidates}),
            ),
            call(
                "call-after-noop",
                "onecrew.test.read-between",
                revision(&fixture.registry, "onecrew.test.read-between"),
                json!({"value":"after-noop"}),
            ),
        ],
        presentation: PresentationRequest::Requested,
    };
    let result = fixture
        .registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "all-existing", &input)
        .await
        .expect("all-existing no-op execute");
    let snapshot = platform
        .present_result(
            &result.result_ref.expect("requested terminal result"),
            &origin,
        )
        .expect("present terminal no-op result");
    assert!(
        snapshot.pending_action.is_none(),
        "a no-op has no pending review"
    );
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert!(matches!(
        &result.outcomes[0],
        ToolExecutionOutcome::Completed { result, .. }
            if result["summary"]["existing_skipped"] == 1
                && result["summary"]["added"] == 0
                && result["summary"]["updated"] == 0
    ));
    assert!(matches!(
        &result.outcomes[1],
        ToolExecutionOutcome::Completed { result, .. }
            if result["value"] == "onecrew.test.read-between: 1 Creators"
    ));
    assert_eq!(support::saved_count(&platform), 1);
}

#[tokio::test]
async fn a_last_existing_noop_preserves_prior_completed_batch_outcomes() {
    let (_directory, platform, origin) = result_owner();
    let entries = support::entries(&platform, 1);
    seed_creator(&platform, entries[0].clone());
    let fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    let mut candidates = fixture.candidates.clone();
    candidates[0].intent = CreatorSaveIntent::AddNew;
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call-before-noop",
                "onecrew.test.read-before",
                revision(&fixture.registry, "onecrew.test.read-before"),
                json!({"value":"before-noop"}),
            ),
            call(
                "call-last-noop",
                BATCH_PREPARE,
                revision(&fixture.registry, BATCH_PREPARE),
                json!({"candidates": candidates}),
            ),
        ],
        presentation: PresentationRequest::Requested,
    };
    let result = fixture
        .registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "last-noop", &input)
        .await
        .expect("last no-op execute");
    let snapshot = platform
        .present_result(
            &result.result_ref.expect("requested terminal result"),
            &origin,
        )
        .expect("present complete mixed result");
    assert!(snapshot.pending_action.is_none());
    assert!(matches!(snapshot.state, ResultState::Resolved(_)));
    assert_eq!(snapshot.batch.outcomes.len(), 2);
    assert_eq!(
        snapshot.batch.outcomes[0].operation_id,
        "onecrew.test.read-before"
    );
    assert_eq!(snapshot.batch.outcomes[1].operation_id, BATCH_PREPARE);
}

#[tokio::test]
async fn all_blocked_creator_save_stops_later_calls_with_owner_summary_and_no_pending_review() {
    let (_directory, platform, origin) = result_owner();
    let mut entries = support::entries(&platform, 2);
    entries[1].account.external_id = entries[0].account.external_id.clone();
    entries[1].account.handle = entries[0].account.handle.clone();
    let mut fixture = support::registry_with_entries(Arc::clone(&platform), entries);
    fixture.candidates[1].notes = Some("conflicting update".into());
    let input = ToolExecuteInput {
        calls: vec![
            call(
                "call-blocked",
                BATCH_PREPARE,
                revision(&fixture.registry, BATCH_PREPARE),
                json!({"candidates": fixture.candidates}),
            ),
            call(
                "call-must-skip",
                "onecrew.test.read-between",
                revision(&fixture.registry, "onecrew.test.read-between"),
                json!({"value":"must-not-run"}),
            ),
        ],
        presentation: PresentationRequest::Never,
    };
    let result = fixture
        .registry
        .tool_execute_v2(&platform, &origin, SurfaceId::Full, "all-blocked", &input)
        .await
        .expect("all-blocked no-op execute");
    assert!(
        result.result_ref.is_none(),
        "blocked preparation has no pending review"
    );
    let ToolExecutionOutcome::Failed {
        failure,
        review_summary: Some(summary),
        ..
    } = &result.outcomes[0]
    else {
        panic!("blocked preparation must retain a structured failure summary");
    };
    assert_eq!(failure.code.as_str(), "creator_save_noop_blocked");
    assert_eq!(summary.summary.blocked, 2);
    assert!(summary.rows.iter().all(|row| {
        row.reason == Some(onecrew_creator_runtime::CreatorSavePreparedReason::ConflictingUpdate)
    }));
    assert!(matches!(
        &result.outcomes[1],
        ToolExecutionOutcome::Skipped { .. }
    ));
    assert_eq!(support::saved_count(&platform), 0);
    assert!(!matches!(
        result.outcomes[0],
        ToolExecutionOutcome::Completed { .. }
    ));
}
