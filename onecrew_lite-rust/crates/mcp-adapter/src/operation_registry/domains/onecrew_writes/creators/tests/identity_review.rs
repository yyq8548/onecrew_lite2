use crate::operation_registry::descriptor::SurfaceId;
use crate::operation_registry::domains::onecrew_writes::creators::test_support::{
    execute_current, operation, registry,
};
use crate::operation_registry::execution::{ExecutionContext, project_execute_failure};
use onecrew_creator_runtime::{
    GetDiscoveryIdentityGroupsInput, RevokeDiscoveryIdentityReviewInput,
};
use onecrew_local_runtime::{LocalPlatform, discovery_identity_test_support as fixture};
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn authenticated_identity_review_mcp_executes_owner_merge_readback_and_revoke() {
    let dir = tempfile::tempdir().unwrap();
    let platform = Arc::new(LocalPlatform::open(dir.path()).unwrap());
    fixture::authenticate(&platform, "fixture-operator");
    let pair = fixture::saved_pair(&platform);
    let owner = platform.creators().unwrap();
    let before = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let registry = registry(platform.clone());
    let execute = |id: &str, args: Value| {
        execute_current(
            operation(&registry, id),
            ExecutionContext {
                surface: SurfaceId::Full,
                request_id: "fixture-peer".into(),
                connection: None,
                confirmed: true,
            },
            args,
        )
    };
    let material = execute(
        "onecrew.discovery.identity.material.get",
        serde_json::to_value(&pair).unwrap(),
    )
    .unwrap();
    let canonical = owner
        .prepare_discovery_identity_merge_material(pair.clone())
        .unwrap();
    assert_eq!(material, serde_json::to_value(&canonical).unwrap());
    let input = fixture::submission(pair.clone(), canonical);
    let receipt = execute(
        "onecrew.discovery.identity.review.submit",
        serde_json::to_value(&input).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["operator_account_id"], "fixture-operator");
    assert_eq!(receipt["kind"], "same_author");
    assert_eq!(
        execute(
            "onecrew.discovery.identity.review.submit",
            serde_json::to_value(&input).unwrap()
        )
        .unwrap(),
        receipt
    );
    let query = GetDiscoveryIdentityGroupsInput {
        campaign_id: pair.campaign_id.clone(),
        expected_revision: 1,
    };
    let merged = execute(
        "onecrew.discovery.identity.groups.get",
        serde_json::to_value(&query).unwrap(),
    )
    .unwrap();
    assert_eq!(merged["counts"]["identity_groups"], 1);
    assert_eq!(
        merged,
        serde_json::to_value(
            owner
                .get_discovery_identity_groups(&pair.campaign_id, 1)
                .unwrap()
        )
        .unwrap()
    );
    let read = json!({"campaign_id":pair.campaign_id,"decision_id":"merge"});
    assert_eq!(
        execute("onecrew.discovery.identity.review.get", read.clone()).unwrap(),
        receipt
    );
    fixture::authenticate(&platform, "other-operator");
    let error = execute("onecrew.discovery.identity.review.get", read.clone()).unwrap_err();
    assert_eq!(
        project_execute_failure(&error).code.as_str(),
        "discovery_read_unavailable"
    );
    fixture::authenticate(&platform, "fixture-operator");
    let revoke = RevokeDiscoveryIdentityReviewInput {
        campaign_id: pair.campaign_id.clone(),
        expected_revision: 1,
        decision_id: "revoke".into(),
        revokes_decision_id: "merge".into(),
        reviewer: fixture::reviewer(),
        rationale: "Withdraw the fixture review.".into(),
    };
    let revoked = execute(
        "onecrew.discovery.identity.review.revoke",
        serde_json::to_value(&revoke).unwrap(),
    )
    .unwrap();
    assert_eq!(revoked["kind"], "revoke");
    let split = execute(
        "onecrew.discovery.identity.groups.get",
        serde_json::to_value(query).unwrap(),
    )
    .unwrap();
    assert_eq!(split["counts"]["identity_groups"], 2);
    assert_ne!(split["snapshot_hash"], merged["snapshot_hash"]);
    assert_eq!(
        execute("onecrew.discovery.identity.review.get", read).unwrap(),
        receipt
    );
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        before
    );
}
