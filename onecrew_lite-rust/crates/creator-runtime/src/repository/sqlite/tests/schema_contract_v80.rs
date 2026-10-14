use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 79).unwrap();
    db.execute_batch("PRAGMA user_version=79; COMMIT").unwrap();
    (dir, path)
}
fn facts(db: &Connection) -> String {
    db.query_row("SELECT json_array(id,phase,current_revision,used_calls,max_calls) FROM discovery_campaigns",[],|r|r.get(0)).unwrap()
}
fn triggers(db: &Connection) -> Vec<(String, String)> {
    db.prepare("SELECT name,sql FROM sqlite_master WHERE name LIKE 'discovery_facebook_native_profile_%' ORDER BY name").unwrap()
        .query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().collect::<Result<_,_>>().unwrap()
}
#[test]
fn facebook_native_profile_forward_upgrade_preserves_history_and_private_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let old = facts(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    let upgrade = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgrade.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        79
    );
    assert_eq!(facts(&db), old);
    assert_eq!(facts(&backup), old);
    assert!(triggers(&backup).is_empty());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&upgrade.backup_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    assert_eq!(triggers(&db).len(), 2);
    assert_eq!(triggers(&db), triggers(&fresh));
    assert!(SqliteRepository::open(&path).is_ok());
    assert_eq!(facts(&db), old);
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
fn facebook_native_profile_edge_rolls_back_and_rejects_marker_only_upgrade() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let old = facts(&db);
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 79, 80).unwrap();
    assert_eq!(triggers(&db).len(), 2);
    db.execute_batch("ROLLBACK").unwrap();
    assert!(triggers(&db).is_empty());
    assert_eq!(facts(&db), old);
    db.pragma_update(None, "user_version", 80).unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}
