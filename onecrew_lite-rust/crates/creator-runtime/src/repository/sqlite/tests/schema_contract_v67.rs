use super::*;

pub(super) fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    // Frozen v66 constructed through its historical adjacent edges.
    let (dir, path) = schema_contract_v63::predecessor();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&db, 62, 66).unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_platform_runs VALUES('upgrade','facebook','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('fb','upgrade','facebook','dedicated','AI tools','https://www.facebook.com/profile.php?id=123','caption','Author',3);
      INSERT INTO discovery_search_authors VALUES('fb','123','https://www.facebook.com/profile.php?id=123',4);
      INSERT INTO discovery_profile_reads VALUES('fb','upgrade','facebook','dedicated','123',1,2,'completed','{}',NULL,5);
      INSERT INTO discovery_facebook_content_targets VALUES('fb','upgrade','facebook','dedicated','123','https://www.facebook.com/profile.php?id=123',1,3,'completed','["https://www.facebook.com/reel/456"]',NULL,6);
      INSERT INTO discovery_contact_lookups VALUES('candidate','upgrade',1,'search','{"preserve":"released receipt"}');
    "#).unwrap();
    db.execute_batch(
        "PRAGMA user_version=66; UPDATE discovery_campaigns SET phase='paused'; COMMIT;",
    )
    .unwrap();
    (dir, path)
}

pub(super) fn snapshot(db: &Connection) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    if db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_douyin_author_reads')",
            [],
            |r| r.get::<_, bool>(0),
        )
        .unwrap()
    {
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM discovery_douyin_author_reads",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0,
            "upgrading historical fixtures must never create source-author requests"
        );
    }
    if db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_web_source_reads')",
            [],
            |r| r.get::<_, bool>(0),
        )
        .unwrap()
    {
        assert_eq!(
            db.query_row("SELECT count(*) FROM discovery_web_source_reads", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    if db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_facebook_identity_reads')", [], |r|r.get::<_,bool>(0)).unwrap() {
        assert_eq!(db.query_row("SELECT count(*) FROM discovery_facebook_identity_reads", [], |r|r.get::<_,i64>(0)).unwrap(),0);
    }
    if db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_facebook_original_attempts')", [], |r| r.get::<_,bool>(0)).unwrap() {
        assert_eq!(db.query_row("SELECT count(*) FROM discovery_facebook_original_attempts", [], |r| r.get::<_,i64>(0)).unwrap(), 0, "upgrade must not fabricate paid original reads");
    }
    // Compare all predecessor rows; additive v75/v76/v79/v81 receipts are checked by their
    // own schema contract, rather than treating a new empty table as data loss.
    if db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='discovery_search_positions')",
            [],
            |r| r.get::<_, bool>(0),
        )
        .unwrap()
    {
        assert_eq!(
            db.query_row("SELECT count(*) FROM discovery_search_positions", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "upgrade must not invent historical search order"
        );
    }
    for table in ["gmail_bulk_batches", "gmail_bulk_items", "gmail_send_attempts", "discovery_query_rounds"] {
        if db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name=?1)", [table], |r|r.get::<_,bool>(0)).unwrap() {
            assert_eq!(db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r|r.get::<_,i64>(0)).unwrap(), 0, "historical fixtures must not gain outbound mail");
        }
    }
    let names: Vec<String> = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT IN ('discovery_query_rounds','gmail_bulk_batches','gmail_bulk_items','gmail_send_attempts','discovery_douyin_author_reads','discovery_web_source_reads','discovery_facebook_identity_reads','discovery_facebook_original_attempts','discovery_search_positions') ORDER BY name").unwrap()
        .query_map([], |r| r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    names
        .into_iter()
        .map(|table| {
            let selection = if table == "discovery_candidates" {
                "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at"
            } else { "*" };
            let mut statement = db
                .prepare(&format!("SELECT {selection} FROM {table} ORDER BY 1"))
                .unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| (0..columns).map(|i| row.get(i)).collect())
                .unwrap()
                .collect::<Result<Vec<Vec<rusqlite::types::Value>>, _>>()
                .unwrap();
            (table, rows)
        })
        .collect()
}

#[test]
fn instagram_upgrade_preserves_receipts_x_identity_children_pause_budget_and_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = snapshot(&db);
    assert!(current_schema_error(&path).contains("upgrade required"));
    assert_eq!(snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        66
    );
    assert_eq!(snapshot(&backup), before);
    assert_eq!(snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT phase,used_calls,max_calls FROM discovery_campaigns",
            [],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?
            ))
        )
        .unwrap(),
        ("paused".into(), 4, 6)
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_content_reads'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(sql.contains("'instagram'"));
    let fresh = Connection::open_in_memory().unwrap();
    schema::initialize(&fresh).unwrap();
    let fresh_sql: String = fresh
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_content_reads'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(sql, fresh_sql);
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn instagram_adjacent_edge_rollback_preserves_children_and_marker_only_is_rejected() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    let before = snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 66, 67).unwrap();
        assert_eq!(snapshot(&tx), before);
        assert!(
            tx.execute_batch("SELECT * FROM injected_missing_table;")
                .is_err()
        );
        // The failed outer operation does not commit the edge.
    }
    assert_eq!(snapshot(&db), before);
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(snapshot(&db), before);
}
