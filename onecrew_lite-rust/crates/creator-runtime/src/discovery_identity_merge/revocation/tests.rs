use super::*;
use crate::{
    CreatorGateways, DiscoveryIdentityDecision, DiscoveryIdentityFinding,
    DiscoveryIdentityMergeMaterialInput, DiscoveryIdentityQuote, DiscoveryIdentityRelationship,
    DiscoveryReviewerKind, DiscoveryTranscriptSegment, FinishDiscoveryTranscriptInput,
    SubmitDiscoveryIdentityReviewInput,
};

fn submit_original(
    owner: &CreatorRuntime,
    pair: DiscoveryIdentityMergeMaterialInput,
) -> DiscoveryIdentityReviewReceipt {
    let material = owner
        .prepare_discovery_identity_merge_material(pair.clone())
        .unwrap();
    owner.submit_discovery_identity_review("operator", SubmitDiscoveryIdentityReviewInput {
        pair, material_hash: material.material_hash, decision_id: "original".into(),
        reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "identity reviewer".into() },
        decision: DiscoveryIdentityDecision::SameAuthor, rationale: "Both descriptions explicitly identify the other bound native account as their own.".into(),
        findings: material.accounts.each_ref().map(|a| DiscoveryIdentityFinding {
            source_search_hit_id: a.hit.id.clone(), relationship: DiscoveryIdentityRelationship::SelfOwnedAccount,
            citation: DiscoveryIdentityQuote::Description { quote: a.video_description.clone().unwrap() },
        }).to_vec(),
    }).unwrap()
}

fn input() -> RevokeDiscoveryIdentityReviewInput {
    RevokeDiscoveryIdentityReviewInput {
        campaign_id: "merge-review".into(), expected_revision: 1, decision_id: "correction".into(),
        revokes_decision_id: "original".into(),
        reviewer: DiscoveryReviewer { kind: DiscoveryReviewerKind::Agent, label: "correction reviewer".into() },
        rationale: "Withdraw the earlier identity judgment; retain both native accounts and evidence for further review.".into(),
    }
}

#[test]
fn revocation_appends_retains_original_and_allows_a_new_review_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let original = submit_original(&owner, pair.clone());
    let campaign = owner.get_discovery_campaign(&pair.campaign_id).unwrap();
    let counts = owner.count_discovery_candidates(&pair.campaign_id).unwrap();
    let receipt = owner
        .revoke_discovery_identity_review("operator", input())
        .unwrap();
    assert_eq!(receipt.kind, DiscoveryIdentityReviewKind::Revoke);
    assert_eq!(receipt.revokes_decision_id.as_deref(), Some("original"));
    assert_eq!(receipt.candidate_ids, original.candidate_ids);
    assert_eq!(receipt.material_hash, original.material_hash);
    assert_eq!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, "original")
            .unwrap(),
        Some(original)
    );
    assert_eq!(
        owner.get_discovery_campaign(&pair.campaign_id).unwrap(),
        campaign
    );
    assert_eq!(
        owner.count_discovery_candidates(&pair.campaign_id).unwrap(),
        counts
    );
    drop(owner);
    let owner = CreatorRuntime::open(dir.path(), CreatorGateways::default()).unwrap();
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", input())
            .unwrap(),
        receipt
    );
    let mut resubmit = super::super::submission::tests::input(&owner, pair);
    resubmit.decision_id = "new-review".into();
    owner
        .submit_discovery_identity_review("operator", resubmit)
        .unwrap();
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    let (rows, snapshots): (u64, u64) = db
        .query_row(
            "SELECT count(*),count(DISTINCT material_json) FROM discovery_identity_reviews",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((rows, snapshots), (3, 1));
}

#[test]
fn changed_payload_actor_stale_revision_missing_target_and_revoke_of_revoke_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit_original(&owner, pair.clone());
    assert_eq!(
        owner
            .revoke_discovery_identity_review("other-account", input())
            .unwrap_err(),
        "identity_review_account_mismatch"
    );
    let mut stale = input();
    stale.expected_revision = 2;
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", stale)
            .unwrap_err(),
        "identity_review_revision_changed"
    );
    let mut missing = input();
    missing.revokes_decision_id = "missing".into();
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", missing)
            .unwrap_err(),
        "identity_review_not_found"
    );
    let receipt = owner
        .revoke_discovery_identity_review("operator", input())
        .unwrap();
    let mut changed = input();
    changed.rationale.push_str(" Changed.");
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", changed)
            .unwrap_err(),
        "identity_review_idempotency_conflict"
    );
    assert_eq!(
        owner
            .revoke_discovery_identity_review("other-account", input())
            .unwrap_err(),
        "identity_review_idempotency_conflict"
    );
    let mut duplicate = input();
    duplicate.decision_id = "duplicate".into();
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", duplicate)
            .unwrap_err(),
        "identity_review_already_revoked"
    );
    let mut nested = input();
    nested.decision_id = "nested".into();
    nested.revokes_decision_id = receipt.decision_id;
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", nested)
            .unwrap_err(),
        "identity_review_cannot_revoke_revocation"
    );
    for field in [
        "operator_account_id",
        "candidate_ids",
        "material_hash",
        "merged",
        "qualified",
    ] {
        let mut json = serde_json::to_value(input()).unwrap();
        json[field] = serde_json::json!("forged");
        assert!(serde_json::from_value::<RevokeDiscoveryIdentityReviewInput>(json).is_err());
    }
}

#[test]
fn later_native_evidence_does_not_prevent_correction_or_change_original_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    let original = submit_original(&owner, pair.clone());
    let claim = owner
        .claim_discovery_transcript(&pair.campaign_id, 1, "dedicated", &pair.left_search_hit_id)
        .unwrap();
    owner
        .finish_discovery_transcript(FinishDiscoveryTranscriptInput {
            claim,
            segments: vec![DiscoveryTranscriptSegment {
                timestamp: "0:01".into(),
                speaker: String::new(),
                text: "Later evidence for correction.".into(),
            }],
            truncated: false,
            stop_code: None,
        })
        .unwrap();
    let current = owner
        .prepare_discovery_identity_merge_material(pair)
        .unwrap();
    assert_ne!(current.material_hash, original.material_hash);
    let receipt = owner
        .revoke_discovery_identity_review("operator", input())
        .unwrap();
    assert_eq!(receipt.material_hash, original.material_hash);
    assert_eq!(
        owner
            .revoke_discovery_identity_review("operator", input())
            .unwrap(),
        receipt
    );
}

#[tokio::test]
async fn concurrent_identical_revocations_commit_once_and_competing_ids_do_not_repeat_correction() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit_original(&owner, pair.clone());
    let (left, right) = tokio::join!(
        owner.revoke_discovery_identity_review_async("operator".into(), input()),
        owner.revoke_discovery_identity_review_async("operator".into(), input()),
    );
    let receipt = left.unwrap();
    assert_eq!(receipt, right.unwrap());
    assert_eq!(
        owner
            .get_discovery_identity_review_async(
                "operator".into(),
                pair.campaign_id,
                receipt.decision_id.clone()
            )
            .await
            .unwrap(),
        Some(receipt)
    );
    let mut competing = input();
    competing.decision_id = "competing".into();
    assert!(
        owner
            .revoke_discovery_identity_review_async("operator".into(), competing)
            .await
            .is_err()
    );
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn failed_revocation_insert_rolls_back_leaving_original_relation_active() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, pair) = super::super::tests::setup_with_owned_links(dir.path());
    submit_original(&owner, pair.clone());
    let db = rusqlite::Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_revoke_test BEFORE INSERT ON discovery_identity_reviews WHEN NEW.kind='revoke' BEGIN SELECT RAISE(ABORT,'injected_failure'); END;").unwrap();
    assert!(
        owner
            .revoke_discovery_identity_review("operator", input())
            .is_err()
    );
    assert!(
        owner
            .get_discovery_identity_review("operator", &pair.campaign_id, "correction")
            .unwrap()
            .is_none()
    );
    let mut resubmit = super::super::submission::tests::input(&owner, pair.clone());
    resubmit.decision_id = "duplicate".into();
    assert_eq!(
        owner
            .submit_discovery_identity_review("operator", resubmit)
            .unwrap_err(),
        "identity_review_relationship_already_reviewed"
    );
    db.execute_batch("DROP TRIGGER reject_revoke_test;")
        .unwrap();
    owner
        .revoke_discovery_identity_review("operator", input())
        .unwrap();
}
