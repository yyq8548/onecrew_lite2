use super::*;

#[test]
fn library_identity_versions_reject_invalid_parent_hash_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = crate::discovery_identity_merge::tests::setup(dir.path());
    drop(owner);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let insert = |campaign: &str, criteria: i64, source: i64, identity: i64, hash: &str| {
        db.execute(
            "INSERT INTO discovery_library_identity_versions VALUES(?1,?2,?3,?5,?4,?5,100)",
            rusqlite::params![campaign, criteria, source, identity, hash],
        )
    };
    let hash = "a".repeat(64);
    assert!(insert("missing", 1, 1, 1, &hash).is_err());
    assert!(insert(&input.campaign_id, 2, 1, 1, &hash).is_err());
    for (source, identity) in [
        (0, 1),
        (1, 0),
        (9_007_199_254_740_992, 1),
        (1, 9_007_199_254_740_992),
    ] {
        assert!(insert(&input.campaign_id, 1, source, identity, &hash).is_err());
    }
    for invalid in ["short".to_owned(), "g".repeat(64)] {
        assert!(insert(&input.campaign_id, 1, 1, 1, &invalid).is_err());
    }
    insert(&input.campaign_id, 1, 1, 1, &hash).unwrap();
    assert!(insert(&input.campaign_id, 1, 2, 2, &hash).is_err());
    drop(db);
    let db = Connection::open(dir.path().join("crm.db")).unwrap();
    schema::initialize(&db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT identity_revision FROM discovery_library_identity_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn library_identity_version_upgrade_preserves_source_versions_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, input) = crate::discovery_identity_merge::tests::setup(dir.path());
    // This fixture represents a released v56 approval, before contact recommendations existed.
    Connection::open(dir.path().join("crm.db"))
        .unwrap()
        .execute(
            "UPDATE discovery_recommendation_policies SET policy='legacy'",
            [],
        )
        .unwrap();
    let campaign = owner.get_discovery_campaign(&input.campaign_id).unwrap();
    let sources = owner
        .prepare_discovery_library_snapshot(&input.campaign_id, 1)
        .unwrap();
    let groups = owner
        .get_discovery_identity_groups(&input.campaign_id, 1)
        .unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_library_identity_versions_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 56).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 56,
            ..
        }
    ));
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        56
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_library_identity_versions")
            .is_err()
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_library_identity_versions",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    schema::initialize(&db).unwrap();
    drop(db);
    let reopened =
        crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(
        reopened.get_discovery_campaign(&input.campaign_id).unwrap(),
        campaign
    );
    assert_eq!(
        reopened
            .prepare_discovery_library_snapshot(&input.campaign_id, 1)
            .unwrap(),
        sources
    );
    assert_eq!(
        reopened
            .get_discovery_identity_groups(&input.campaign_id, 1)
            .unwrap(),
        groups
    );
}

#[test]
fn library_identity_version_edge_rolls_back_and_rejects_marker_only_upgrade() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_library_identity_versions_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 56).unwrap();
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 56, 57).unwrap();
        assert!(
            tx.prepare("SELECT * FROM discovery_library_identity_versions")
                .is_ok()
        );
        assert!(
            tx.execute_batch("INSERT INTO nonexistent_table VALUES(1)")
                .is_err()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_library_identity_versions")
            .is_err()
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        56
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}
