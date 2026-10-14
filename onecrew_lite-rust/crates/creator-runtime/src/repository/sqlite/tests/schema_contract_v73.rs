use super::*;
#[test]
fn contact_routes_upgrade_preserves_paused_data_and_creates_verified_backup() {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 72).unwrap();
    db.execute_batch("PRAGMA user_version=72; COMMIT").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_contact_observations'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(ddl.contains("'hidden_email','social_profile'"));
    drop(dir);
}
