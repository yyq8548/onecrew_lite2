//! Isolated authenticated owner composition; no provider or customer data is used.
use super::*;
use crate::discovery_identity_test_support::{authenticate, reviewer, saved_pair};
use onecrew_creator_runtime::{
    DiscoveryIdentityDecision, DiscoveryIdentityFinding, DiscoveryIdentityQuote,
    DiscoveryIdentityRelationship, DiscoveryIdentityReviewKind,
};

#[tokio::test]
async fn authenticated_merge_and_revoke_preserve_sources_qualification_budget_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(dir.path()).unwrap();
    authenticate(&platform, "fixture-operator");
    let pair = saved_pair(&platform);
    let owner = platform.creators().unwrap();
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let candidates = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let query = GetDiscoveryIdentityGroupsInput {
        campaign_id: pair.campaign_id.clone(),
        expected_revision: 1,
    };
    let before = platform
        .get_discovery_identity_groups(query.clone())
        .await
        .unwrap();
    assert_eq!(before.counts.identity_groups, 2);
    let material = platform
        .prepare_discovery_identity_merge_material(pair.clone())
        .await
        .unwrap();
    let input = SubmitDiscoveryIdentityReviewInput {
        pair: pair.clone(), material_hash: material.material_hash, decision_id: "merge".into(), reviewer: reviewer(),
        decision: DiscoveryIdentityDecision::SameAuthor,
        rationale: "Both saved author-owned descriptions explicitly identify the other bound account as their own.".into(),
        findings: material.accounts.each_ref().map(|account| DiscoveryIdentityFinding {
            source_search_hit_id: account.hit.id.clone(), relationship: DiscoveryIdentityRelationship::SelfOwnedAccount,
            citation: DiscoveryIdentityQuote::Description { quote: account.video_description.clone().unwrap() },
        }).to_vec(),
    };
    let receipt = platform
        .submit_discovery_identity_review(input.clone())
        .await
        .unwrap();
    assert_eq!(receipt.operator_account_id, "fixture-operator");
    assert_eq!(receipt.kind, DiscoveryIdentityReviewKind::SameAuthor);
    assert_eq!(
        platform
            .submit_discovery_identity_review(input)
            .await
            .unwrap(),
        receipt
    );
    let merged = platform
        .get_discovery_identity_groups(query.clone())
        .await
        .unwrap();
    assert_eq!(merged.counts.identity_groups, 1);
    assert_eq!(merged.counts.native_accounts, 2);
    assert_ne!(merged.snapshot_hash, before.snapshot_hash);
    for member in before.groups.iter().flat_map(|group| &group.members) {
        assert!(merged.groups[0].members.contains(member));
    }
    let read = GetDiscoveryIdentityReviewInput {
        campaign_id: pair.campaign_id.clone(),
        decision_id: "merge".into(),
    };
    assert_eq!(
        platform
            .get_discovery_identity_review(read.clone())
            .await
            .unwrap(),
        Some(receipt.clone())
    );
    authenticate(&platform, "other-operator");
    assert_eq!(
        platform
            .get_discovery_identity_review(read.clone())
            .await
            .unwrap_err(),
        "identity_review_read_unavailable"
    );
    let revoke = RevokeDiscoveryIdentityReviewInput {
        campaign_id: pair.campaign_id.clone(),
        expected_revision: 1,
        decision_id: "revoke".into(),
        revokes_decision_id: "merge".into(),
        reviewer: reviewer(),
        rationale: "Withdraw this fixture identity judgment.".into(),
    };
    assert_eq!(
        platform
            .revoke_discovery_identity_review(revoke.clone())
            .await
            .unwrap_err(),
        "identity_review_revoke_outcome_unknown"
    );
    assert_eq!(
        platform
            .get_discovery_identity_groups(query.clone())
            .await
            .unwrap(),
        merged
    );
    authenticate(&platform, "fixture-operator");
    let revoked = platform
        .revoke_discovery_identity_review(revoke.clone())
        .await
        .unwrap();
    assert_eq!(revoked.kind, DiscoveryIdentityReviewKind::Revoke);
    let split = platform
        .get_discovery_identity_groups(query.clone())
        .await
        .unwrap();
    assert_eq!(split.counts.identity_groups, 2);
    assert_ne!(split.snapshot_hash, merged.snapshot_hash);
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        candidates
    );
    drop(owner);
    drop(platform);
    let reopened = LocalPlatform::open(dir.path()).unwrap();
    assert_eq!(
        reopened.get_discovery_identity_groups(query).await.unwrap(),
        split
    );
    assert_eq!(
        reopened.get_discovery_identity_review(read).await.unwrap(),
        Some(receipt)
    );
    assert_eq!(
        reopened
            .revoke_discovery_identity_review(revoke)
            .await
            .unwrap(),
        revoked
    );
    assert_eq!(
        reopened
            .creators()
            .unwrap()
            .get_discovery_campaign(&pair.campaign_id)
            .unwrap(),
        campaign
    );
}
