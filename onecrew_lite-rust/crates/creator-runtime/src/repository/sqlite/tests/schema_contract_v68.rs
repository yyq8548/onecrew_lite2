use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 67).unwrap();
    db.execute_batch("PRAGMA user_version=67; COMMIT").unwrap();
    (dir, path)
}

#[test]
fn weibo_upgrade_preserves_all_v67_receipts_facebook_children_pause_and_backup() {
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
        67
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
        assert!(sql(&db).contains("'weibo'"));
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
fn weibo_adjacent_edge_rolls_back_and_version_marker_alone_is_rejected() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 67, 68).unwrap();
        assert_eq!(schema_contract_v67::snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table")
                .is_err()
        );
    }
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    db.pragma_update(None, "user_version", 68).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(schema_contract_v67::snapshot(&db), before);
}
