//! Real Web router and admission over isolated, authenticated owner data.
use super::ServiceState;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use onecrew_creator_runtime::RevokeDiscoveryIdentityReviewInput;
use onecrew_creator_sync::test_support::{
    TestHttpProbe, TestHttpResponse, with_test_http_responses,
};
use onecrew_local_domain::CreatorSyncCredentialMetadata;
use onecrew_local_runtime::{LocalPlatform, discovery_identity_test_support as fixture};
use onecrew_local_vault::LocalVault;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tower::ServiceExt;

fn admission_responses(metadata: &CreatorSyncCredentialMetadata) -> Vec<TestHttpResponse> {
    let installation = json!({
        "installation_id": metadata.installation_id,
        "credential_id": metadata.credential_id,
        "access_user_id": metadata.access_user_id,
        "access_device_id": metadata.access_device_id,
        "source_instance_id": metadata.source_instance_id,
        "access_pairing_id": metadata.access_pairing_id,
        "client_platform": "macos", "app_version": metadata.app_version,
        "status": "active", "revision": metadata.revision,
        "created_at": metadata.created_at, "updated_at": metadata.updated_at,
        "last_seen_at": metadata.last_seen_at, "revoked_at": null
    });
    let probe = json!({
        "installation_id": metadata.installation_id,
        "credential_id": metadata.credential_id,
        "access_user_id": metadata.access_user_id,
        "access_device_id": metadata.access_device_id,
        "source_instance_id": metadata.source_instance_id,
        "access_pairing_id": metadata.access_pairing_id,
        "protocol": "reacher.creator_sync", "protocol_version": metadata.protocol_version,
        "capabilities": ["creator_snapshots:write_chunked_quarantine"],
        "accepted_bundle": metadata.accepted_bundle
    });
    vec![
        TestHttpResponse {
            method: "GET",
            path: format!("/installations/{}", metadata.installation_id),
            status: 200,
            body: json!({"installation": installation}),
        },
        TestHttpResponse {
            method: "GET",
            path: "/creator-sync/credential".into(),
            status: 200,
            body: probe,
        },
    ]
}

fn service(data: &Path, responses: Vec<TestHttpResponse>) -> (Arc<ServiceState>, TestHttpProbe) {
    let vault = LocalVault::open(data.join("agent-platform.db")).unwrap();
    let (sync, probe) = with_test_http_responses(vault, responses);
    let state = ServiceState::new(data.into(), &[], "/test/onecrew".into(), "test".into());
    assert!(state.creator_sync.set(Arc::new(sync)).is_ok());
    state.startup.start(data.into()).unwrap();
    state.browser.set_port(23459).unwrap();
    (state, probe)
}

async fn command(
    state: &Arc<ServiceState>,
    name: &str,
    input: Value,
    csrf: bool,
) -> (StatusCode, Value) {
    let session = crate::web::router(state.clone())
        .oneshot(
            Request::builder()
                .uri("/api/session")
                .header("host", "127.0.0.1:23459")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(session.status(), StatusCode::OK);
    let set_cookie = session.headers()["set-cookie"].to_str().unwrap().to_owned();
    let session_wire: Value =
        serde_json::from_slice(&to_bytes(session.into_body(), 4096).await.unwrap()).unwrap();
    let mut request = Request::builder()
        .method("POST")
        .uri(format!("/api/commands/{name}"))
        .header("host", "127.0.0.1:23459")
        .header("origin", "http://127.0.0.1:23459")
        .header("cookie", set_cookie.split(';').next().unwrap())
        .header("content-type", "application/json");
    if csrf {
        request = request.header(
            "x-onecrew-csrf",
            session_wire["result"]["csrf"].as_str().unwrap(),
        );
    }
    let response = crate::web::router(state.clone())
        .oneshot(
            request
                .body(Body::from(json!({"input":input}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

async fn success(state: &Arc<ServiceState>, name: &str, input: Value) -> Value {
    let (status, wire) = command(state, name, input, true).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(wire["ok"], true, "{name}: {wire}");
    wire["result"].clone()
}

#[tokio::test]
async fn authenticated_identity_review_http_merges_replays_reads_and_revokes() {
    let data = tempfile::tempdir().unwrap();
    let seed = LocalPlatform::open(data.path()).unwrap();
    let metadata = fixture::authenticate(&seed, "fixture-operator");
    let pair = fixture::saved_pair(&seed);
    let owner = seed.creators().unwrap();
    let before = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let (state, probe) = service(data.path(), admission_responses(&metadata));
    let pair_json = serde_json::to_value(&pair).unwrap();
    let (status, _) = command(
        &state,
        "onecrew_discovery_identity_review_submit",
        json!({}),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(probe.remaining_responses(), 2);
    let material = success(&state, "onecrew_discovery_identity_material_get", pair_json).await;
    assert_eq!(probe.remaining_responses(), 0);
    let canonical = owner
        .prepare_discovery_identity_merge_material(pair.clone())
        .unwrap();
    assert_eq!(material, serde_json::to_value(&canonical).unwrap());
    let input = serde_json::to_value(fixture::submission(pair.clone(), canonical)).unwrap();
    let receipt = success(
        &state,
        "onecrew_discovery_identity_review_submit",
        input.clone(),
    )
    .await;
    assert_eq!(receipt["operator_account_id"], "fixture-operator");
    assert_eq!(
        receipt,
        success(&state, "onecrew_discovery_identity_review_submit", input).await
    );
    let read = json!({"campaign_id":pair.campaign_id,"decision_id":"merge"});
    assert_eq!(
        receipt,
        success(
            &state,
            "onecrew_discovery_identity_review_get",
            read.clone()
        )
        .await
    );
    let query = json!({"campaign_id":pair.campaign_id,"expected_revision":1});
    let merged = success(
        &state,
        "onecrew_discovery_identity_groups_get",
        query.clone(),
    )
    .await;
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
    let revoke = serde_json::to_value(RevokeDiscoveryIdentityReviewInput {
        campaign_id: pair.campaign_id.clone(),
        expected_revision: 1,
        decision_id: "revoke".into(),
        revokes_decision_id: "merge".into(),
        reviewer: fixture::reviewer(),
        rationale: "Withdraw the fixture review.".into(),
    })
    .unwrap();
    let (status, _) = command(
        &state,
        "onecrew_discovery_identity_review_revoke",
        revoke.clone(),
        false,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap()
            .counts
            .identity_groups,
        1
    );

    let other = fixture::authenticate(&seed, "other-operator");
    let (other_state, other_probe) = service(data.path(), admission_responses(&other));
    for (name, input) in [
        ("onecrew_discovery_identity_review_get", read.clone()),
        ("onecrew_discovery_identity_review_revoke", revoke.clone()),
    ] {
        let (status, wire) = command(&other_state, name, input, true).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(wire["ok"], false);
        assert!(
            wire["error"]
                .as_str()
                .unwrap()
                .starts_with("identity_review_")
        );
    }
    assert_eq!(other_probe.remaining_responses(), 0);
    assert_eq!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap()
            .counts
            .identity_groups,
        1
    );

    let restored = fixture::authenticate(&seed, "fixture-operator");
    let (restored_state, restored_probe) = service(data.path(), admission_responses(&restored));
    let revoked = success(
        &restored_state,
        "onecrew_discovery_identity_review_revoke",
        revoke,
    )
    .await;
    assert_eq!(revoked["kind"], "revoke");
    let split = success(
        &restored_state,
        "onecrew_discovery_identity_groups_get",
        query,
    )
    .await;
    assert_eq!(split["counts"]["identity_groups"], 2);
    assert_ne!(split["snapshot_hash"], merged["snapshot_hash"]);
    assert_eq!(
        receipt,
        success(
            &restored_state,
            "onecrew_discovery_identity_review_get",
            read
        )
        .await
    );
    assert_eq!(restored_probe.remaining_responses(), 0);
    assert_eq!(
        before,
        owner.get_discovery_campaign(&pair.campaign_id).unwrap()
    );
}

#[tokio::test]
async fn authenticated_identity_review_http_rejects_mismatched_remote_identity() {
    let data = tempfile::tempdir().unwrap();
    let seed = LocalPlatform::open(data.path()).unwrap();
    let metadata = fixture::authenticate(&seed, "fixture-operator");
    let pair = fixture::saved_pair(&seed);
    let owner = seed.creators().unwrap();
    let before = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let mut responses = admission_responses(&metadata);
    responses[1].body["access_user_id"] = json!("different-operator");
    let (state, probe) = service(data.path(), responses);
    let (status, wire) = command(
        &state,
        "onecrew_discovery_identity_material_get",
        serde_json::to_value(pair.clone()).unwrap(),
        true,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(wire["ok"], false);
    assert!(
        wire["error"]
            .as_str()
            .unwrap()
            .starts_with("login_required:")
    );
    assert_eq!(probe.remaining_responses(), 0);
    assert_eq!(
        before,
        owner.get_discovery_campaign(&pair.campaign_id).unwrap()
    );
    assert_eq!(
        owner
            .get_discovery_identity_groups(&pair.campaign_id, 1)
            .unwrap()
            .counts
            .identity_groups,
        2
    );
}
