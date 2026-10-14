use super::*;

#[test]
fn manual_approval_upgrade_preserves_v59_candidates_and_has_no_implicit_approvals() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, _) = crate::discovery_review::tests::setup(dir.path());
    let before = owner.get_discovery_campaign("review").unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_reddit_reference(&db).unwrap();
    db.execute_batch("ALTER TABLE discovery_language_samples DROP COLUMN sample_format_version;")
        .unwrap();
    db.execute_batch("DROP TABLE discovery_manual_approvals; PRAGMA user_version=59")
        .unwrap();
    drop(db);
    assert!(SqliteRepository::open(&path).is_err());
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        59
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let owner = crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(owner.get_discovery_campaign("review").unwrap(), before);
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_manual_approvals", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}
