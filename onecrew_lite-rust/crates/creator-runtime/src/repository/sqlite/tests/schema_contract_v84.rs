use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 83).unwrap();
    db.execute_batch("PRAGMA user_version=83; COMMIT").unwrap();
    (dir, path)
}

fn contacts_shape(db: &Connection) -> String {
    db.query_row(
        "SELECT sql FROM sqlite_master WHERE name='discovery_contact_observations'",
        [],
        |r| r.get(0),
    )
    .unwrap()
}

fn campaign_facts(db: &Connection) -> String {
    db.query_row("SELECT json_array(id,phase,current_revision,used_calls,max_calls) FROM discovery_campaigns", [], |r| r.get(0)).unwrap()
}

#[test]
fn messaging_upgrade_preserves_campaign_and_matches_fresh_schema() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = campaign_facts(&db);
    let shape = contacts_shape(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    let outcome = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(outcome.backup_path).unwrap();
    assert_eq!(contacts_shape(&backup), shape);
    assert_eq!(campaign_facts(&backup), before);
    assert_eq!(campaign_facts(&db), before);
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    assert_eq!(contacts_shape(&db), contacts_shape(&fresh));
    assert!(contacts_shape(&db).contains("'whatsapp','wechat','wechat_official'"));
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn messaging_upgrade_rolls_back_and_marker_only_is_rejected() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = contacts_shape(&db);
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 83, 84).unwrap();
    assert_ne!(contacts_shape(&db), before);
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(contacts_shape(&db), before);
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}
