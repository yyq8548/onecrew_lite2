use super::*;
fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 75).unwrap();
    db.execute_batch("PRAGMA user_version=75; COMMIT").unwrap();
    (dir, path)
}
#[test]
fn web_source_upgrade_preserves_all_history_and_private_backup_with_no_provider_reads() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        75
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_web_source_reads", [], |r| r
            .get::<_, i64>(0))
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
        "discovery_web_source_reads",
        "discovery_web_source_reads_campaign",
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
fn web_source_edge_rolls_back_and_marker_alone_never_repairs_a_predecessor() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 75, 76).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_web_source_reads'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}
#[test]
fn web_source_table_rejects_false_native_identity_and_null_completed_material() {
    let (_dir, path) = predecessor();
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 75, 76).unwrap();
    db.execute_batch("INSERT INTO discovery_platform_runs VALUES('upgrade','google','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('web','upgrade','google','dedicated','AI tools','https://www.youtube.com/watch?v=abcdefghijk','summary','',3)").unwrap();
    for (author, body) in [
        (Some("Display Name"), Some("{}")),
        (Some("UCabcdefghijklmnopqrstuv"), None),
        (None, Some("{}")),
        (
            Some("UCabcdefghijklmnopqrstuv"),
            Some("{\"channelId\":\"UCabcdefghijklmnopqrstuv\",\"videoId\":\"wrong\"}"),
        ),
    ] {
        assert!(db.execute("INSERT INTO discovery_web_source_reads VALUES('web','upgrade','google','dedicated','https://www.youtube.com/watch?v=abcdefghijk',1,2,'youtube','abcdefghijk','completed',?1,?2,NULL,4)",rusqlite::params![author,body]).is_err());
    }
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_web_source_reads", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
