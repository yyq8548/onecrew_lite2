use super::*;

#[test]
fn language_sampling_upgrade_preserves_campaign_and_retains_v58_backup() {
    let dir = tempfile::tempdir().unwrap();
    let (owner, hit) = crate::discovery_review::tests::setup(dir.path());
    let before = owner.get_discovery_campaign("review").unwrap();
    let material = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    drop(owner);
    let path = dir.path().join("crm.db");
    let db = Connection::open(&path).unwrap();
    schema::strip_reddit_reference(&db).unwrap();
    db.execute_batch("DROP TABLE discovery_manual_approvals; DROP TABLE discovery_language_samples; PRAGMA user_version=58;")
        .unwrap();
    drop(db);
    assert!(SqliteRepository::open(&path).is_err());
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        58
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let owner = crate::CreatorRuntime::open(dir.path(), crate::CreatorGateways::default()).unwrap();
    assert_eq!(owner.get_discovery_campaign("review").unwrap(), before);
    assert_eq!(
        owner
            .prepare_discovery_review_material("review", 1, &hit)
            .unwrap(),
        material
    );
    let db = Connection::open(path).unwrap();
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM discovery_language_samples", [], |r| r
            .get::<_, u32>(0))
            .unwrap(),
        0
    );
}
