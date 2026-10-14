use super::*;

pub(super) fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = current_database();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    schema::strip_facebook_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 62).unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_campaigns VALUES('upgrade','active',1,30,6,4,1,2);
      INSERT INTO discovery_campaign_revisions VALUES('upgrade',1,'"Creators"','1.8.8','{"min_followers":10000}',
        '[{"platform":"twitter","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
      INSERT INTO discovery_platform_runs VALUES('upgrade','twitter','dedicated','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('seen','upgrade','twitter','dedicated','AI tools','https://x.com/i/status/123','demo','Creator',3);
      INSERT INTO discovery_search_authors VALUES('seen','creator_1','https://x.com/creator_1',4);
      INSERT INTO discovery_identity_reads VALUES('seen','upgrade','twitter','dedicated','creator_1','https://x.com/i/status/123','123',1,2,'completed','987654321','https://x.com/creator_1/status/123',NULL,4);
      INSERT INTO discovery_profile_reads VALUES('seen','upgrade','twitter','dedicated','creator_1',1,3,'completed','{"name":"Creator"}',NULL,5);
      INSERT INTO discovery_profile_identity_matches VALUES('seen','seen','987654321',5);
      INSERT INTO discovery_content_reads VALUES('seen','upgrade','twitter','dedicated','creator_1','https://x.com/i/status/123',1,4,'completed','{"text":"original"}',NULL,6);
      INSERT INTO discovery_content_identity_bindings VALUES('seen','seen','seen','987654321',6);
      INSERT INTO discovery_candidates VALUES('candidate','upgrade',1,'twitter','dedicated','987654321','Creator','https://x.com/creator_1','pending','{"author_identity_kind":"native_id"}',5,5);
      INSERT INTO discovery_manual_approvals VALUES('upgrade',1,'candidate',7);
    "#).unwrap();
    (dir, path)
}

pub(super) fn snapshot(db: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    [
        "discovery_campaigns",
        "discovery_campaign_revisions",
        "discovery_candidates",
        "discovery_manual_approvals",
        "discovery_profile_reads",
        "discovery_content_reads",
        "discovery_identity_reads",
        "discovery_profile_identity_matches",
        "discovery_content_identity_bindings",
    ]
    .into_iter()
    .map(|table| {
        let selection = if table == "discovery_candidates" {
            "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at"
        } else { "*" };
        let mut statement = db
            .prepare(&format!("SELECT {selection} FROM {table} ORDER BY 1"))
            .unwrap();
        let columns = statement.column_count();
        statement
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    })
    .collect()
}

#[test]
fn facebook_upgrade_preserves_rows_x_bindings_approval_budget_and_backup() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = snapshot(&db);
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        62
    );
    assert_eq!(snapshot(&backup), before);
    assert_eq!(snapshot(&db), before);
    assert!(SqliteRepository::open(&path).is_ok());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_profile_reads'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(ddl.contains("'facebook'"));
}

#[test]
fn facebook_upgrade_rolls_back_and_rejects_marker_only_change() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let before = snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 62, 63).unwrap();
        assert_eq!(snapshot(&tx), before);
    }
    assert_eq!(snapshot(&db), before);
    db.pragma_update(None, "user_version", 63).unwrap();
    assert!(schema::initialize(&db).is_err());
    assert_eq!(snapshot(&db), before);
}
