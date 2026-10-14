use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 84).unwrap();
    db.execute_batch("PRAGMA user_version=84; COMMIT").unwrap();
    (dir, path)
}

#[test]
fn search_order_upgrade_preserves_legacy_facts_and_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let outcome = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(outcome.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        84
    );
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert!(SqliteRepository::open(&path).is_ok());
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    let shape = |db: &Connection| {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_search_positions'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(shape(&db), shape(&fresh));
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn search_order_upgrade_rolls_back_and_rejects_marker_only() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 84, 85).unwrap();
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_search_positions'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}
