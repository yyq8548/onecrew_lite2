use super::*;

#[test]
fn language_v2_upgrade_preserves_v60_campaign_and_backup() {
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
    db.execute_batch("ALTER TABLE discovery_language_samples DROP COLUMN sample_format_version;")
        .unwrap();
    db.pragma_update(None, "user_version", 60).unwrap();
    let legacy = serde_json::json!({"author_id":material.native_author_id,"source_url":material.hit.source_url,
        "text":"original","comments":[{"author_id":"old-reader-id","source_url":"comment","text":"中文评论"}]}).to_string();
    db.execute("INSERT INTO discovery_language_samples VALUES('review',1,?1,?2,1,1,'completed',?3,NULL,123)",
        rusqlite::params![material.candidate_id,hit,legacy]).unwrap();
    drop(db);
    assert!(SqliteRepository::open(&path).is_err());
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        60
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
        db.query_row(
            "SELECT sample_json,sample_format_version FROM discovery_language_samples",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
        )
        .unwrap(),
        (legacy, 1)
    );
    let migrated = owner
        .prepare_discovery_review_material("review", 1, &hit)
        .unwrap();
    assert_eq!(migrated.language_samples[0].comments[0].text, "中文评论");
    assert!(
        serde_json::to_value(&migrated).unwrap()["language_samples"][0]["comments"][0]
            .get("author_id")
            .is_none()
    );
}
