use super::*;

#[test]
fn target_ledger_upgrade_preserves_campaign_without_increasing_target() {
    let dir = tempfile::tempdir().unwrap();
    let owner = crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    let input = crate::CreateDiscoveryCampaignInput {
        id: "target-upgrade".into(),
        brief: "Shoe creators".into(),
        opencli_version: "1.8.8".into(),
        criteria: serde_json::json!({"min_subscribers": 10000, "category": "shoes"}),
        scopes: vec![crate::DiscoveryPlatformScope {
            platform: "youtube".into(),
            profile_ref: "dedicated".into(),
            keywords: vec!["shoes".into()],
        }],
        target_count: 30,
        max_calls: Some(400),
    };
    owner.create_discovery_campaign(input).unwrap();
    let expected = owner
        .approve_discovery_campaign("target-upgrade", 1)
        .unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_target_increases_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 54).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 54,
            ..
        }
    ));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        54
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_target_increases")
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_target_increases", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    schema::initialize(&db).unwrap();
    drop(db);
    let reopened =
        crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_campaign("target-upgrade").unwrap(),
        expected
    );
}

#[test]
fn target_ledger_edge_rolls_back_and_rejects_marker_only_repair() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_target_increases_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 54).unwrap();
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 54, 55).unwrap();
        assert!(
            tx.prepare("SELECT * FROM discovery_target_increases")
                .is_ok()
        );
        assert!(
            tx.execute_batch("INSERT INTO nonexistent_table VALUES(1)")
                .is_err()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_target_increases")
            .is_err()
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        54
    );
    db.pragma_update(None, "user_version", 55).unwrap();
    assert!(schema::initialize(&db).is_err());
}
