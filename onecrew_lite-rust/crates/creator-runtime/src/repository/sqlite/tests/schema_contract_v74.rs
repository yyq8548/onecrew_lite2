use super::*;
fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 73).unwrap();
    db.execute_batch("PRAGMA user_version=73; COMMIT").unwrap();
    (dir, path)
}
#[test]
fn native_receipts_upgrade_preserves_rows_pause_budget_identity_and_private_backup() {
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
        73
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(
        db.query_row(
            "SELECT phase,used_calls,max_calls FROM discovery_campaigns",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?
            ))
        )
        .unwrap(),
        ("paused".into(), 4, 6)
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
        let sql = |db: &Connection| {
            db.query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [table],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
        };
        assert_eq!(sql(&db), sql(&fresh));
        if table.ends_with("reads") {
            for platform in ["pixiv", "linkedin", "douyin"] {
                assert!(sql(&db).contains(platform));
            }
        }
    }
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
fn native_receipts_adjacent_rollback_preserves_children_and_rejects_marker_only_upgrade() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 73, 74).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}
#[test]
fn native_receipts_edge_preserves_explicit_indexes_and_triggers_without_replaying_inserts() {
    let (_dir, path) = predecessor();
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE INDEX fixture_native_author ON discovery_profile_reads(author_id); CREATE TABLE fixture_insert_count(count INTEGER); INSERT INTO fixture_insert_count VALUES(0); CREATE TRIGGER fixture_profile_insert AFTER INSERT ON discovery_profile_reads BEGIN UPDATE fixture_insert_count SET count=count+1; END;").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let objects:Vec<(String,String)>=db.prepare("SELECT name,sql FROM sqlite_master WHERE name IN ('fixture_native_author','fixture_profile_insert') ORDER BY name").unwrap().query_map([],|r| Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 73, 74).unwrap();
    db.execute_batch("COMMIT").unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let after:Vec<(String,String)>=db.prepare("SELECT name,sql FROM sqlite_master WHERE name IN ('fixture_native_author','fixture_profile_insert') ORDER BY name").unwrap().query_map([],|r| Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(after, objects);
}
