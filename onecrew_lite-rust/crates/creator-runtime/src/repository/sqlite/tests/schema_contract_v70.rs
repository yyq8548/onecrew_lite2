use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 69).unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_platform_runs VALUES('upgrade','weibo','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('wb','upgrade','weibo','dedicated','keyboard','https://weibo.com/12345/Abc123','original','Author',3);
      INSERT INTO discovery_search_authors VALUES('wb','12345','https://weibo.com/u/12345',4);
      INSERT INTO discovery_profile_reads VALUES('wb','upgrade','weibo','dedicated','12345',1,3,'completed','{"preserve":"weibo native receipt"}',NULL,5);
      INSERT INTO discovery_content_reads VALUES('wb','upgrade','weibo','dedicated','12345','https://weibo.com/12345/Abc123',1,4,'completed','{"preserve":"weibo original content"}',NULL,6);
      PRAGMA user_version=69; COMMIT;
    "#).unwrap();
    (dir, path)
}

#[test]
fn zhihu_upgrade_preserves_v69_weibo_facebook_children_pause_receipts_and_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        69
    );
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
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
    for table in ["discovery_profile_reads", "discovery_content_reads"] {
        let sql = |c: &Connection| {
            c.query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [table],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        assert!(sql(&db).contains("'zhihu'"));
        assert_eq!(sql(&db), sql(&fresh));
    }
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn zhihu_adjacent_edge_rolls_back_and_marker_only_upgrade_is_rejected() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 69, 70).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    db.pragma_update(None, "user_version", 70).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}
