use super::*;
use rusqlite::params;

fn backup_count(directory: &std::path::Path) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("crm.db.backup-")
        })
        .count()
}

fn seed_discovery(db: &Connection) {
    let has_source_platform: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('discovery_candidates') WHERE name='source_platform')",
        [], |r| r.get(0)).unwrap();
    let candidate_columns = if has_source_platform {
        ",source_platform"
    } else {
        ""
    };
    let candidate_values = if has_source_platform {
        ",'youtube'"
    } else {
        ""
    };
    db.execute_batch(&r#"
        INSERT INTO discovery_campaigns VALUES('campaign','active',1,30,6,4,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('campaign',1,'"Creators"','1.8.8',
          '{"min_followers":10000,"requires_ai_tool_demo":true}',
          '[{"platform":"youtube","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('campaign','youtube','dedicated','completed',NULL,NULL,4,NULL,2);
        INSERT INTO discovery_search_hits VALUES('hit','campaign','youtube','dedicated','AI tools',
          'https://www.youtube.com/watch?v=abcdefghijk','demo','Creator',3);
        INSERT INTO discovery_video_completions VALUES('hit',5);
        INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at{candidate_columns}) VALUES('candidate','campaign',1,'youtube','dedicated','channel',
          'Creator','https://www.youtube.com/channel/channel','pending','{"note":"preserve original bytes"}',5,5{candidate_values});
        INSERT INTO discovery_video_identity_bindings VALUES('hit','campaign',1,'candidate','channel',5);
        INSERT INTO discovery_library_source_versions VALUES('campaign',1,7,
          'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',6);
    "#.replace("{candidate_columns}", candidate_columns).replace("{candidate_values}", candidate_values)).unwrap();
}

fn preserved(db: &Connection) -> (i64, String, String, i64, String, String) {
    db.query_row("SELECT d.used_calls,c.evidence_json,c.verdict,v.source_revision,b.native_author_id,r.criteria_json
        FROM discovery_campaigns d JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
        JOIN discovery_library_source_versions v ON v.discovery_campaign_id=d.id
        JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id
        JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=d.id AND r.revision=d.current_revision",
        [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap()
}

#[test]
fn review_ledger_upgrade_preserves_facts_backup_and_restart_without_backfill() {
    let (dir, path) = current_database();
    let db = Connection::open(&path).unwrap();
    schema::strip_budget_increases_reference(&db).unwrap();
    schema::strip_video_read_failures_reference(&db).unwrap();
    schema::strip_review_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 50).unwrap();
    seed_discovery(&db);
    let expected = preserved(&db);
    assert!(matches!(crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired { from_version: 50, impact, .. }
            if impact == crate::CreatorDatabaseUpgradeImpact::default()));
    assert_eq!(backup_count(dir.path()), 0);
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(preserved(&db), expected);
    assert!(
        db.prepare("SELECT * FROM discovery_evidence_reviews")
            .is_err()
    );
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(backup_count(dir.path()), 1);
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        50
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_evidence_reviews")
            .is_err()
    );
    assert_eq!(preserved(&backup), expected);
    assert_eq!(preserved(&db), expected);
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_evidence_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // initialize validates exact fresh-current structure, not just user_version.
    schema::initialize(&db).unwrap();
    let repeated = crate::CreatorRuntime::upgrade_database(&path).unwrap_err();
    assert_eq!(repeated.support_code, "LOCAL_DATA_ALREADY_CURRENT");
    assert!(!repeated.storage_upgraded);
    assert_eq!(backup_count(dir.path()), 1);
    drop(db);
    assert!(SqliteRepository::open(&path).is_ok());
    assert_eq!(preserved(&Connection::open(&path).unwrap()), expected);
}

#[test]
fn review_ledger_edge_rolls_back_and_rejects_marker_only_repair() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_budget_increases_reference(&db).unwrap();
    schema::strip_video_read_failures_reference(&db).unwrap();
    schema::strip_review_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 50).unwrap();
    seed_discovery(&db);
    let expected = preserved(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 50, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert!(
            tx.prepare("SELECT * FROM discovery_evidence_reviews")
                .is_ok()
        );
        // A subsequent statement failure cannot commit this adjacent step.
        assert!(
            tx.execute_batch("INSERT INTO nonexistent_table VALUES(1)")
                .is_err()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_evidence_reviews")
            .is_err()
    );
    assert_eq!(preserved(&db), expected);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        50
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}

const INSERT: &str = "INSERT INTO discovery_evidence_reviews VALUES
    (?1,'account','campaign',1,'hit','candidate','channel','dedicated',?2,?3,?3,?4,
    'policy-v1','{}',?5,'pending','{}',10)";

#[test]
fn review_ledger_enforces_unique_account_request_identity_and_bounded_encoding() {
    let db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    seed_discovery(&db);
    let hash = "a".repeat(64);
    let material = r#"{"format_version":1}"#;
    db.execute(INSERT, params!["record", "request", hash, material, "[]"])
        .unwrap();
    assert!(
        db.execute(
            INSERT,
            params!["other-record", "request", hash, material, "[]"]
        )
        .is_err()
    );
    db.execute(
        &INSERT.replace("'account'", "'another-account'"),
        params!["separate", "request", hash, material, "[]"],
    )
    .unwrap();
    for bad_hash in ["g".repeat(64), "a".repeat(63), "A".repeat(64)] {
        assert!(
            db.execute(INSERT, params!["bad", "bad", bad_hash, material, "[]"])
                .is_err()
        );
    }
    for bad_material in [
        "null".to_owned(),
        "{}".into(),
        "[]".into(),
        "broken".into(),
        r#"{"format_version":3}"#.into(),
        format!(
            r#"{{"format_version":1,"text":"{}"}}"#,
            "中".repeat(350_000)
        ),
    ] {
        assert!(
            db.execute(INSERT, params!["bad", "bad", hash, bad_material, "[]"])
                .is_err(),
            "accepted invalid material: {:?}",
            bad_material.chars().take(80).collect::<String>()
        );
    }
    for findings in [
        "{}".to_owned(),
        "broken".into(),
        format!("[\"{}\"]", "a".repeat(65_536)),
    ] {
        assert!(
            db.execute(INSERT, params!["bad", "bad", hash, material, findings])
                .is_err()
        );
    }
    for broken in [
        INSERT.replace("'hit'", "'unbound-hit'"),
        INSERT.replace("'candidate'", "'missing-candidate'"),
        INSERT.replace("'campaign',1", "'campaign',2"),
        INSERT.replace("'pending'", "'guessed'"),
        INSERT.replace("'policy-v1'", "''"),
        INSERT.replace("'policy-v1','{}'", "'policy-v1','[]'"),
    ] {
        assert!(
            db.execute(&broken, params!["bad", "bad", hash, material, "[]"])
                .is_err()
        );
    }
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_evidence_reviews", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(preserved(&db).2, "pending");
}
