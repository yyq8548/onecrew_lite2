use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 76).unwrap();
    db.execute_batch("PRAGMA user_version=76; COMMIT").unwrap();
    (dir, path)
}

#[test]
fn candidate_source_upgrade_keeps_history_children_backup_and_actual_native_origin() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = schema_contract_v67::snapshot(&db);
    let error = current_schema_error(&path);
    assert!(error.contains("upgrade required"), "{error}");
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        76
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(schema_contract_v67::snapshot(&backup), before);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_candidates WHERE source_platform!=platform",
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
    assert!(SqliteRepository::open(&path).is_ok());
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    let ddl = |db: &Connection| {
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_candidates'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    assert_eq!(ddl(&db), ddl(&fresh));
}

#[test]
fn candidate_source_edge_preserves_indexes_triggers_without_repeating_historical_effects() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;
      CREATE TABLE migration_probe(id TEXT);
      CREATE INDEX candidate_probe_index ON discovery_candidates(display_name);
      CREATE TRIGGER candidate_probe_insert AFTER INSERT ON discovery_candidates BEGIN INSERT INTO migration_probe VALUES(NEW.id); END;").unwrap();
    let before = schema_contract_v67::snapshot(&db);
    schema::apply_forward_migrations(&db, 76, 77).unwrap();
    assert_eq!(schema_contract_v67::snapshot(&db), before);
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('candidate_probe_index','candidate_probe_insert')",[],|r|r.get::<_,i64>(0)).unwrap(),2);
    assert_eq!(
        db.query_row("SELECT count(*) FROM migration_probe", [], |r| r
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
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM pragma_table_info('discovery_candidates') WHERE name='source_platform'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}

#[test]
fn candidate_source_foreign_key_requires_actual_search_origin_without_native_search_run() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 76, 77).unwrap();
    db.execute_batch("INSERT INTO discovery_platform_runs VALUES('upgrade','google','dedicated','completed',NULL,NULL,1,NULL,2)").unwrap();
    let sql="INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
      VALUES('web-author','upgrade',1,'youtube','dedicated','UCabcdefghijklmnopqrstuv','Actual creator','https://www.youtube.com/channel/UCabcdefghijklmnopqrstuv','pending','{}',3,3,?1)";
    assert!(db.execute(sql, ["yahoo"]).is_err());
    assert!(db.execute(sql, ["youtube"]).is_err());
    db.execute(sql, ["google"]).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_platform_runs WHERE platform='youtube'",
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
    db.execute_batch("ROLLBACK").unwrap();
}
