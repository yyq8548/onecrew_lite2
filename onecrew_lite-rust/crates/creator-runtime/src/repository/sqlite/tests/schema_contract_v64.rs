use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let (dir, path) = super::schema_contract_v63::predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let tx = db.transaction().unwrap();
    schema::apply_forward_migrations(&tx, 62, 63).unwrap();
    tx.pragma_update(None, "user_version", 63).unwrap();
    tx.commit().unwrap();
    (dir, path)
}

#[test]
fn facebook_content_upgrade_keeps_original_sources_x_bindings_approval_and_budget() {
    let (_dir, path) = predecessor();
    let db = Connection::open(&path).unwrap();
    let before = super::schema_contract_v63::snapshot(&db);
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(super::schema_contract_v63::snapshot(&db), before);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        63
    );
    assert_eq!(super::schema_contract_v63::snapshot(&backup), before);
    assert_eq!(super::schema_contract_v63::snapshot(&db), before);
    assert!(SqliteRepository::open(&path).is_ok());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_facebook_content_targets",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_content_reads'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(ddl.contains("'facebook'"));
}

#[test]
fn facebook_content_upgrade_rolls_back_and_marker_only_is_rejected() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    let before = super::schema_contract_v63::snapshot(&db);
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 63, 64).unwrap();
        assert_eq!(super::schema_contract_v63::snapshot(&tx), before);
    }
    assert_eq!(super::schema_contract_v63::snapshot(&db), before);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_facebook_content_targets'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    db.pragma_update(None, "user_version", 64).unwrap();
    assert!(schema::initialize(&db).is_err());
    assert_eq!(super::schema_contract_v63::snapshot(&db), before);
}

#[test]
fn facebook_target_receipt_constraints_keep_numeric_profile_and_bound_state() {
    let (_dir, path) = current_database();
    let db = Connection::open(path).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_campaigns VALUES('facebook','active',1,4,8,2,1,2);
      INSERT INTO discovery_campaign_revisions VALUES('facebook',1,'"Shampoo"','1.8.8','{}',
        '[{"platform":"facebook","profile_ref":"test","keywords":["shampoo"]}]',1);
      INSERT INTO discovery_platform_runs VALUES('facebook','facebook','test','completed',NULL,NULL,1,NULL,2);
      INSERT INTO discovery_search_hits VALUES('fb','facebook','facebook','test','shampoo','https://www.facebook.com/profile.php?id=123','caption','Author',3);
      INSERT INTO discovery_search_authors VALUES('fb','123','https://www.facebook.com/profile.php?id=123',4);
      INSERT INTO discovery_profile_reads VALUES('fb','facebook','facebook','test','123',1,2,'completed','{}',NULL,5);
    "#).unwrap();
    let insert = "INSERT INTO discovery_facebook_content_targets VALUES('fb','facebook','facebook','test','123',?1,1,3,?2,?3,?4,6)";
    for (profile, state, urls, stop) in [
        (
            "https://www.facebook.com/profile.php?id=999",
            "reading",
            None,
            None,
        ),
        (
            "https://www.facebook.com/profile.php?id=123",
            "completed",
            None,
            None,
        ),
        (
            "https://www.facebook.com/profile.php?id=123",
            "completed",
            Some("[]"),
            None,
        ),
        (
            "https://www.facebook.com/profile.php?id=123",
            "completed",
            Some("[1,2,3,4]"),
            None,
        ),
        (
            "https://www.facebook.com/profile.php?id=123",
            "reading",
            Some("[\"https://www.facebook.com/reel/456\"]"),
            None,
        ),
        (
            "https://www.facebook.com/profile.php?id=123",
            "failed",
            None,
            None,
        ),
    ] {
        assert!(
            db.execute(
                insert,
                rusqlite::params![profile, state, urls, stop as Option<&str>]
            )
            .is_err()
        );
    }
    db.execute(
        insert,
        rusqlite::params![
            "https://www.facebook.com/profile.php?id=123",
            "completed",
            "[\"https://www.facebook.com/reel/456\"]",
            None::<String>
        ],
    )
    .unwrap();
    assert!(
        db.execute(
            insert,
            rusqlite::params![
                "https://www.facebook.com/profile.php?id=123",
                "reading",
                None::<String>,
                None::<String>
            ]
        )
        .is_err()
    );
}
