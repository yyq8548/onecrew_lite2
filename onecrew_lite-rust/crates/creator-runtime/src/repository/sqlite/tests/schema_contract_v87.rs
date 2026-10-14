use super::*;

#[test]
fn gmail_bulk_upgrade_preserves_v86_and_has_backup() {
    let (_dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 86).unwrap();
    db.execute_batch("PRAGMA user_version=86; COMMIT; PRAGMA foreign_keys=ON;")
        .unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let outcome = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(outcome.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        86
    );
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    SqliteRepository::open(&path).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM gmail_bulk_batches", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn gmail_bulk_migration_rolls_back() {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE mailbox_refs(id TEXT,provider TEXT); CREATE TABLE idempotency_records(key TEXT,mailbox_ref_id TEXT,action TEXT,state TEXT);").unwrap();
    db.execute_batch("BEGIN IMMEDIATE").unwrap();
    schema::apply_forward_migrations(&db, 86, 87).unwrap();
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name LIKE 'gmail_%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn gmail_bulk_upgrade_locks_old_uncertain_gmail_actions_only() {
    let db=Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE mailbox_refs(id TEXT,provider TEXT); CREATE TABLE idempotency_records(key TEXT,mailbox_ref_id TEXT,action TEXT,state TEXT);
        INSERT INTO mailbox_refs VALUES ('gmail','gmail'),('legacy','resend');
        INSERT INTO idempotency_records VALUES ('uncertain','gmail','manual_email_send','remote_started'),('other','legacy','email_send','remote_started'),('new','gmail','email_send','in_progress');").unwrap();
    schema::apply_forward_migrations(&db,86,87).unwrap();
    let keys=db.prepare("SELECT idempotency_key FROM gmail_send_attempts").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(keys,vec!["uncertain"]);
}
