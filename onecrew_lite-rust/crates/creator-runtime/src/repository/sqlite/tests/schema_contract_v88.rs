use super::*;

#[test]
fn query_round_upgrade_preserves_v89_backup_and_rolls_back() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 66, 89).unwrap();
    db.execute_batch("PRAGMA user_version=89; COMMIT; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 89, 90).unwrap();
    db.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name='discovery_query_rounds'", [], |r| r.get::<_,i64>(0)).unwrap(),0);
    let before = schema_contract_v67::snapshot(&db);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db),before);
    assert_eq!(schema_contract_v67::snapshot(&Connection::open(result.backup_path).unwrap()),before);
    SqliteRepository::open(&path).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM discovery_query_rounds",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}

#[test]
fn target_decrease_upgrade_preserves_v88_and_backup() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 66, 88).unwrap();
    db.execute_batch("PRAGMA user_version=88; COMMIT; PRAGMA foreign_keys=ON;").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(schema_contract_v67::snapshot(&Connection::open(result.backup_path).unwrap()), before);
    SqliteRepository::open(&path).unwrap();
    let sql: String = db.query_row("SELECT sql FROM sqlite_master WHERE name='discovery_target_increases'", [], |r|r.get(0)).unwrap();
    assert!(sql.contains("new_target_count != previous_target_count"));
}

#[test]
fn target_decrease_migration_rollback_preserves_constraint() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 66, 88).unwrap();
    db.execute_batch("COMMIT; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 88, 89).unwrap();
    db.execute_batch("ROLLBACK;").unwrap();
    let sql: String = db.query_row("SELECT sql FROM sqlite_master WHERE name='discovery_target_increases'", [], |r|r.get(0)).unwrap();
    assert!(sql.contains("new_target_count > previous_target_count"));
}

#[test]
fn wechat_upgrade_preserves_v87_and_backup() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 87).unwrap();
    db.execute_batch("PRAGMA user_version=87; COMMIT; PRAGMA foreign_keys=ON;")
        .unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(
        schema_contract_v67::snapshot(&Connection::open(result.backup_path).unwrap()),
        before
    );
    SqliteRepository::open(&path).unwrap();
    db.execute("INSERT INTO contact_locators(id,channel,kind,normalized_value,display_value,created_at,updated_at) VALUES ('wx-fixture','wechat','handle','example_wx','Example_Wx',1,1)", []).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT channel FROM contact_locators WHERE id='wx-fixture'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "wechat"
    );
}

#[test]
fn wechat_migration_rollback_restores_released_constraints() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 87).unwrap();
    db.execute_batch("COMMIT; BEGIN IMMEDIATE;").unwrap();
    schema::apply_forward_migrations(&db, 87, 88).unwrap();
    db.execute_batch("ROLLBACK;").unwrap();
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='contact_locators'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!sql.contains("'wechat'"));
}
