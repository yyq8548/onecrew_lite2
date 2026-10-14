use super::*;

const TABLES: &[&str] = &[
    "discovery_campaigns",
    "discovery_campaign_revisions",
    "discovery_candidates",
    "discovery_manual_approvals",
    "discovery_profile_reads",
    "discovery_content_reads",
    "discovery_identity_reads",
    "discovery_profile_identity_matches",
    "discovery_content_identity_bindings",
];

fn snapshot(db: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
    TABLES
        .iter()
        .map(|table| {
            let selection = if *table == "discovery_candidates" {
                "id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at"
            } else { "*" };
            let mut stmt = db
                .prepare(&format!("SELECT {selection} FROM {table} ORDER BY 1"))
                .unwrap();
            let columns = stmt.column_count();
            stmt.query_map([], |r| (0..columns).map(|i| r.get(i)).collect())
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        })
        .collect()
}

fn seed(db: &Connection) {
    db.execute_batch(r#"
        INSERT INTO discovery_campaigns VALUES('upgrade','active',1,30,6,4,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}', '[{"platform":"twitter","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('upgrade','twitter','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','upgrade','twitter','dedicated','AI tools',
          'https://x.com/i/status/123','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','creator_1','https://x.com/creator_1',4);
        INSERT INTO discovery_identity_reads VALUES('seen','upgrade','twitter','dedicated','creator_1',
          'https://x.com/i/status/123','123',1,2,'completed','987654321','https://x.com/creator_1/status/123',NULL,4);
        INSERT INTO discovery_profile_reads VALUES('seen','upgrade','twitter','dedicated','creator_1',1,3,
          'completed','{"handle":"creator_1","profile_url":"https://x.com/creator_1","name":"Creator","follower_count":15000,"identity_kind":"native_id","account_id":"987654321"}',NULL,5);
        INSERT INTO discovery_profile_identity_matches VALUES('seen','seen','987654321',5);
        INSERT INTO discovery_content_reads VALUES('seen','upgrade','twitter','dedicated','creator_1',
          'https://x.com/i/status/123',1,4,'completed','{"text":"original"}',NULL,6);
        INSERT INTO discovery_content_identity_bindings VALUES('seen','seen','seen','987654321',6);
        INSERT INTO discovery_candidates VALUES('candidate','upgrade',1,'twitter','dedicated','987654321',
          'Creator','https://x.com/creator_1','pending','{"author_identity_kind":"native_id"}',5,5);
        INSERT INTO discovery_manual_approvals VALUES('upgrade',1,'candidate',7);
    "#).unwrap();
}

#[test]
fn reddit_upgrade_preserves_x_identity_children_budget_approval_and_backup() {
    let (_dir, path) = current_database();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    schema::strip_reddit_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 61).unwrap();
    seed(&db);
    let before = snapshot(&db);
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        61
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
}

#[test]
fn reddit_upgrade_rollback_retains_parent_and_child_rows() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_reddit_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 61).unwrap();
    seed(&db);
    let before = snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 61, 62).unwrap();
        assert_eq!(snapshot(&tx), before);
        // Roll back without committing, as the owner does after postflight failure.
    }
    assert_eq!(snapshot(&db), before);
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_profile_reads'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!ddl.contains("'reddit'"));
    db.pragma_update(None, "user_version", 62).unwrap();
    assert!(
        schema::initialize(&db).is_err(),
        "marker-only upgrades must be rejected"
    );
}
