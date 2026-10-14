use super::*;

#[test]
fn review_encoding_upgrade_preserves_old_receipts_and_has_a_readable_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, hit) = crate::discovery_review::tests::setup(dir.path());
    let campaign = owner.get_discovery_campaign("review").unwrap();
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    let input = crate::SubmitDiscoveryReviewInput {
        campaign_id: "review".into(),
        expected_revision: 1,
        search_hit_id: hit.clone(),
        material_hash: material.material_hash.clone(),
        request_id: "legacy-encoding".into(),
        reviewer: crate::DiscoveryReviewer {
            kind: crate::DiscoveryReviewerKind::Agent,
            label: "fixture".into(),
        },
        findings: [
            crate::DiscoveryReviewCriterion::OwnAiToolDemo,
            crate::DiscoveryReviewCriterion::PublicToolUrl,
        ]
        .into_iter()
        .map(|criterion| crate::DiscoveryReviewFinding {
            criterion,
            outcome: crate::DiscoveryReviewOutcome::Insufficient,
            rationale: "No saved support".into(),
            citations: vec![],
            tool: None,
            country_code: None,
            match_level: None,
        })
        .collect(),
    };
    let receipt = owner
        .submit_discovery_review("account", input.clone())
        .unwrap();
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_reddit_reference(&db).unwrap();
    let before: String = db
        .query_row(
            "SELECT assessment_json FROM discovery_evidence_reviews",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Version 57 has exactly the same SQL layout, but cannot emit the new JSON
    // findings. This reference contains only released legacy review encodings.
    db.execute_batch(
        "DROP TABLE discovery_manual_approvals; DROP TABLE discovery_language_samples;",
    )
    .unwrap();
    db.pragma_update(None, "user_version", 57).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        57
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(db);
    let reopened =
        crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    let after: String = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT assessment_json FROM discovery_evidence_reviews",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
    assert_eq!(
        reopened.submit_discovery_review("account", input).unwrap(),
        receipt
    );
    assert_eq!(reopened.get_discovery_campaign("review").unwrap(), campaign);
    assert_eq!(
        reopened
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap(),
        material
    );
}
