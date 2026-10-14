use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 70).unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_platform_runs VALUES('upgrade','zhihu','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('zh','upgrade','zhihu','dedicated','keyboard','https://www.zhihu.com/question/456/answer/123','original','Author',3);
      INSERT INTO discovery_search_authors VALUES('zh','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','https://www.zhihu.com/people/creator-123',4);
      INSERT INTO discovery_profile_reads VALUES('zh','upgrade','zhihu','dedicated','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',1,3,'completed','{"preserve":"released Zhihu profile"}',NULL,5);
      INSERT INTO discovery_content_reads VALUES('zh','upgrade','zhihu','dedicated','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','https://www.zhihu.com/question/456/answer/123',1,4,'completed','{"preserve":"released original prose"}',NULL,6);
      PRAGMA user_version=70; COMMIT;
    "#).unwrap();
    (dir, path)
}

#[test]
fn hackernews_upgrade_keeps_paused_history_receipts_identity_children_and_private_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let error = current_schema_error(&path);
    assert!(error.contains("upgrade required"), "{error}");
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        70
    );
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION as u32
    );
    assert_eq!(
        db.query_row("SELECT phase FROM discovery_campaigns", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "paused"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    for table in [
        "discovery_profile_reads",
        "discovery_content_reads",
        "discovery_profile_identity_matches",
        "discovery_content_identity_bindings",
        "discovery_facebook_content_targets",
    ] {
        let sql = |c: &Connection| {
            c.query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [table],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        assert_eq!(sql(&db), sql(&fresh));
    }
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(result.backup_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
}

#[test]
fn hackernews_adjacent_failure_rolls_back_and_marker_only_change_is_rejected() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 70, 71).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    db.pragma_update(None, "user_version", 71).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}
