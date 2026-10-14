use super::*;

fn predecessor() -> (tempfile::TempDir, PathBuf) {
    let (dir, path) = schema_contract_v67::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 66, 77).unwrap();
    db.execute_batch("INSERT INTO discovery_platform_runs VALUES('upgrade','google','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('web-history','upgrade','google','dedicated','keyboard','https://www.youtube.com/watch?v=abcdefghijk','Search lead','',2);
      INSERT INTO discovery_web_source_reads(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,stop_code,updated_at)
      VALUES('web-history','upgrade','google','dedicated','https://www.youtube.com/watch?v=abcdefghijk',1,2,'youtube','abcdefghijk','failed','opencli_invalid_output',2);
      PRAGMA user_version=77; COMMIT;").unwrap();
    (dir, path)
}

#[test]
fn native_x_source_upgrade_preserves_receipts_backup_and_restart() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let old: String=db.query_row("SELECT json_array(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,native_author_id,native_material_json,stop_code,updated_at) FROM discovery_web_source_reads",[],|r|r.get(0)).unwrap();
    assert!(current_schema_error(&path).contains("upgrade required"));
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    for connection in [&db, &backup] {
        let row:String=connection.query_row("SELECT json_array(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,native_author_id,native_material_json,stop_code,updated_at) FROM discovery_web_source_reads",[],|r|r.get(0)).unwrap();
        assert_eq!(row, old);
    }
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        77
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
    let ddl = |connection: &Connection| {
        connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name='discovery_web_source_reads'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    };
    assert_eq!(ddl(&db), ddl(&fresh));
}

#[test]
fn native_x_source_edge_preserves_indexes_and_triggers_without_replaying_inserts() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;
      CREATE TABLE migration_probe(id TEXT);
      CREATE INDEX web_source_probe_index ON discovery_web_source_reads(source_url);
      CREATE TRIGGER web_source_probe_insert AFTER INSERT ON discovery_web_source_reads BEGIN INSERT INTO migration_probe VALUES(NEW.search_hit_id); END;").unwrap();
    schema::apply_forward_migrations(&db, 77, 78).unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('web_source_probe_index','web_source_probe_insert')",[],|r|r.get::<_,i64>(0)).unwrap(),2);
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
}

#[test]
fn native_x_source_forward_step_is_transactional_and_rejects_inconsistent_ids() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE")
        .unwrap();
    schema::apply_forward_migrations(&db, 77, 78).unwrap();
    db.execute_batch("INSERT INTO discovery_search_hits VALUES('x-source','upgrade','google','dedicated','keyboard','https://x.com/creator/status/123','Search lead','',2)").unwrap();
    let sql="INSERT INTO discovery_web_source_reads(search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,updated_at)
      VALUES('x-source','upgrade','google','dedicated','https://x.com/creator/status/123',1,4,'twitter',?1,'reading',3)";
    for id in ["0", "0123", "abcdefghijk", "1".repeat(21).as_str()] {
        assert!(db.execute(sql, [id]).is_err());
    }
    db.execute(sql, ["123"]).unwrap();
    db.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_web_source_reads", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        db.query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_web_source_reads'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap()
        .contains("CHECK(target_platform='youtube')")
    );
}
