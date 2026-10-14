use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 78).unwrap();
    db.execute_batch("PRAGMA user_version=78; COMMIT").unwrap();
    (dir, path)
}
fn preserved(db: &Connection) -> String {
    db.query_row("SELECT json_array(id,phase,current_revision,used_calls,max_calls) FROM discovery_campaigns",[],|r|r.get(0)).unwrap()
}
#[test]
fn facebook_identity_forward_upgrade_preserves_history_and_private_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let old = preserved(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        78
    );
    assert_eq!(preserved(&db), old);
    assert_eq!(preserved(&backup), old);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_facebook_identity_reads",
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&upgraded.backup_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    let ddl = |c: &Connection| {
        c.query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_facebook_identity_reads'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(ddl(&db), ddl(&fresh));
    assert!(SqliteRepository::open(&path).is_ok());
}
#[test]
fn facebook_identity_edge_rolls_back_and_marker_only_database_is_rejected() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let old = preserved(&db);
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 78, 79).unwrap();
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_facebook_identity_reads'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(preserved(&db), old);
    db.pragma_update(None, "user_version", 79).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}
