use super::*;
use rusqlite::params;

#[test]
fn identity_review_upgrade_preserves_sources_budget_qualification_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = crate::discovery_identity_merge::tests::setup(dir.path());
    let target_input = crate::IncreaseDiscoveryTargetInput {
        campaign_id: input.campaign_id.clone(),
        expected_revision: 1,
        expected_target_count: 200,
        new_target_count: 250,
        decision_id: "preserve-target-decision".into(),
    };
    let budget_input = crate::IncreaseDiscoveryBudgetInput {
        campaign_id: input.campaign_id.clone(),
        expected_revision: 1,
        expected_max_calls: 4,
        new_max_calls: 5,
        decision_id: "preserve-budget-decision".into(),
    };
    owner
        .increase_discovery_target("operator", target_input.clone())
        .unwrap();
    owner
        .increase_discovery_budget("operator", budget_input.clone())
        .unwrap();
    let expected_campaign = owner.get_discovery_campaign(&input.campaign_id).unwrap();
    let expected_counts = owner
        .count_discovery_candidates(&input.campaign_id)
        .unwrap();
    let expected_material = owner
        .prepare_discovery_identity_merge_material(input.clone())
        .unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_identity_reviews_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 55).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 55,
            ..
        }
    ));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        55
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_identity_reviews")
            .is_err()
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    schema::initialize(&db).unwrap();
    for ledger in ["discovery_target_increases", "discovery_budget_increases"] {
        let query = format!("SELECT count(*) FROM {ledger}");
        assert_eq!(
            backup
                .query_row(&query, [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(db.query_row(&query, [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    }
    drop(db);
    let reopened =
        crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_campaign(&input.campaign_id).unwrap(),
        expected_campaign
    );
    assert_eq!(
        reopened
            .count_discovery_candidates(&input.campaign_id)
            .unwrap(),
        expected_counts
    );
    assert_eq!(
        reopened
            .increase_discovery_target("operator", target_input)
            .unwrap(),
        expected_campaign
    );
    assert_eq!(
        reopened
            .increase_discovery_budget("operator", budget_input)
            .unwrap(),
        expected_campaign
    );
    assert_eq!(
        reopened
            .prepare_discovery_identity_merge_material(input)
            .unwrap()
            .material_hash,
        expected_material.material_hash
    );
}

#[test]
fn identity_review_edge_rolls_back_and_marker_alone_cannot_upgrade() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_identity_reviews_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 55).unwrap();
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 55, 56).unwrap();
        assert!(
            tx.prepare("SELECT * FROM discovery_identity_reviews")
                .is_ok()
        );
        assert!(
            tx.execute_batch("INSERT INTO nonexistent_table VALUES(1)")
                .is_err()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_identity_reviews")
            .is_err()
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        55
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}

#[test]
fn identity_ledger_binds_distinct_accounts_and_audit_request_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = crate::discovery_identity_merge::tests::setup(dir.path());
    let pair = owner
        .prepare_discovery_identity_merge_material(input)
        .unwrap();
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let insert = |decision: &str,
                  kind: &str,
                  right: &str,
                  revokes: Option<&str>,
                  hash: &str,
                  json: &str| {
        db.execute("INSERT INTO discovery_identity_reviews (
          discovery_campaign_id,decision_id,operator_account_id,criteria_revision,kind,
          left_candidate_id,right_candidate_id,material_hash,request_hash,input_json,material_json,receipt_json,revokes_decision_id,created_at
        ) VALUES('merge-review',?1,'operator',1,?2,?3,?4,?5,?5,?6,'{}','{}',?7,100)",
        params![decision, kind, pair.accounts[0].candidate_id, right, hash, json, revokes])
    };
    let right = &pair.accounts[1].candidate_id;
    let hash = "a".repeat(64);
    assert!(
        insert(
            "same-account",
            "same_author",
            &pair.accounts[0].candidate_id,
            None,
            &hash,
            "{}"
        )
        .is_err()
    );
    assert!(insert("bad-hash", "same_author", right, None, "x", "{}").is_err());
    assert!(insert("bad-json", "same_author", right, None, &hash, "[]").is_err());
    assert!(
        insert(
            "oversized",
            "same_author",
            right,
            None,
            &hash,
            &serde_json::json!({"text":"x".repeat(65536)}).to_string()
        )
        .is_err()
    );
    assert!(
        insert(
            "missing-original",
            "revoke",
            right,
            Some("missing"),
            &hash,
            "{}"
        )
        .is_err()
    );
    insert("merge", "same_author", right, None, &hash, "{}").unwrap();
    assert!(
        db.execute(
            "UPDATE discovery_identity_reviews SET kind='different_authors'",
            []
        )
        .is_err()
    );
    assert!(insert("merge", "same_author", right, None, &hash, "{}").is_err());
    assert!(
        insert(
            "revoke-without-reference",
            "revoke",
            right,
            None,
            &hash,
            "{}"
        )
        .is_err()
    );
    assert!(
        insert(
            "not-revoke",
            "same_author",
            right,
            Some("merge"),
            &hash,
            "{}"
        )
        .is_err()
    );
    insert("revoke", "revoke", right, Some("merge"), &hash, "{}").unwrap();
    assert!(insert("revoke-again", "revoke", right, Some("merge"), &hash, "{}").is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_identity_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
