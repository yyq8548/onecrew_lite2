use super::*;
use crate::{
    DiscoveryReviewCriterion, DiscoveryReviewFinding, DiscoveryReviewOutcome, DiscoveryReviewer,
    DiscoveryReviewerKind, SubmitDiscoveryReviewInput,
};

#[test]
fn native_review_edge_preserves_v1_receipt_and_changes_only_review_constraint() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, hit) = crate::discovery_review::tests::setup(dir.path());
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let receipt = owner
        .submit_discovery_review(
            "account",
            SubmitDiscoveryReviewInput {
                campaign_id: "review".into(),
                expected_revision: 1,
                search_hit_id: hit,
                material_hash: material.material_hash,
                request_id: "v1-before-upgrade".into(),
                reviewer: DiscoveryReviewer {
                    kind: DiscoveryReviewerKind::Agent,
                    label: "fixture reviewer".into(),
                },
                findings: vec![
                    DiscoveryReviewFinding {
                        criterion: DiscoveryReviewCriterion::OwnAiToolDemo,
                        outcome: DiscoveryReviewOutcome::Insufficient,
                        rationale: "No transcript evidence.".into(),
                        citations: vec![],
                        tool: None,
                        match_level: None,
                        country_code: None,
                    },
                    DiscoveryReviewFinding {
                        criterion: DiscoveryReviewCriterion::PublicToolUrl,
                        outcome: DiscoveryReviewOutcome::Insufficient,
                        rationale: "No linked page evidence.".into(),
                        citations: vec![],
                        tool: None,
                        match_level: None,
                        country_code: None,
                    },
                ],
            },
        )
        .unwrap();
    drop(owner);
    let mut db = Connection::open(dir.path().join("crm.db")).unwrap();
    let before: String = db
        .query_row(
            "SELECT assessment_json FROM discovery_evidence_reviews",
            [],
            |row| row.get(0),
        )
        .unwrap();
    schema::restore_review_v1_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 53).unwrap();
    assert!(schema::initialize(&db).is_err());
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let tx = db.transaction().unwrap();
    schema::apply_forward_migrations(&tx, 53, schema::CURRENT_SCHEMA_VERSION).unwrap();
    tx.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    tx.commit().unwrap();
    schema::initialize(&db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT assessment_json FROM discovery_evidence_reviews",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        before
    );
    let after: crate::DiscoveryReviewReceipt = serde_json::from_str(&before).unwrap();
    assert_eq!(after, receipt);
    let fk: String = db.query_row("SELECT \"table\" FROM pragma_foreign_key_list('discovery_evidence_reviews') WHERE \"from\"='search_hit_id'",[],
        |row| row.get(0)).unwrap();
    assert_eq!(fk, "discovery_search_hits");
}
