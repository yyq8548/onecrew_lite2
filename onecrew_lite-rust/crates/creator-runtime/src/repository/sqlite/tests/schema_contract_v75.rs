use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 74).unwrap();
    db.execute_batch("PRAGMA user_version=74; COMMIT").unwrap();
    (dir, path)
}

#[test]
fn source_author_upgrade_preserves_history_and_private_backup_without_creating_reads() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        74
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_douyin_author_reads",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    for name in [
        "discovery_douyin_author_reads",
        "discovery_douyin_author_reads_campaign",
    ] {
        let sql = |db: &Connection| {
            db.query_row("SELECT sql FROM sqlite_master WHERE name=?1", [name], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
        };
        assert_eq!(sql(&db), sql(&fresh));
    }
    assert!(SqliteRepository::open(&path).is_ok());
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
fn source_author_edge_rolls_back_and_marker_only_upgrade_fails_closed() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 74, 75).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_douyin_author_reads'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.pragma_update(None, "user_version", 75).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}

#[test]
fn source_author_table_rejects_null_completion_identity_and_unbound_video() {
    let (_dir, path) = predecessor();
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 74, 75).unwrap();
    db.execute_batch("INSERT INTO discovery_platform_runs VALUES('upgrade','douyin','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('dy','upgrade','douyin','dedicated','AI tools','https://www.douyin.com/video/123','original','Creator',3);").unwrap();
    for (status, author, url, stop) in [
        ("completed", Some("MS4wLjABAAAA12345678"), None, None),
        (
            "completed",
            None,
            Some("https://www.douyin.com/user/MS4wLjABAAAA12345678"),
            None,
        ),
        (
            "completed",
            Some("12345678"),
            Some("https://www.douyin.com/user/12345678"),
            None,
        ),
        ("failed", None, None, None),
        ("reading", Some("MS4wLjABAAAA12345678"), None, None),
    ] {
        assert!(db.execute("INSERT INTO discovery_douyin_author_reads VALUES('dy','upgrade','douyin','dedicated','https://www.douyin.com/video/123','123',1,5,?1,?2,?3,NULL,?4,6)",rusqlite::params![status,author,url,stop as Option<&str>]).is_err());
    }
    assert!(db.execute_batch("INSERT INTO discovery_douyin_author_reads VALUES('dy','upgrade','douyin','dedicated','https://www.douyin.com/video/456','123',1,5,'reading',NULL,NULL,NULL,NULL,6)").is_err());
    db.execute_batch("INSERT INTO discovery_douyin_author_reads VALUES('dy','upgrade','douyin','dedicated','https://www.douyin.com/video/123','123',1,5,'reading',NULL,NULL,NULL,NULL,6);
      UPDATE discovery_douyin_author_reads SET status='completed',author_id='MS4wLjABAAAA12345678',profile_url='https://www.douyin.com/user/MS4wLjABAAAA12345678'; COMMIT;").unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}
