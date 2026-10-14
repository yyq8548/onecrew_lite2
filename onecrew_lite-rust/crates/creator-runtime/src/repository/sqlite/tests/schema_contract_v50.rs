use super::*;

#[test]
fn library_source_upgrade_preserves_legacy_completion_without_guessing_identity() {
    let (_dir, path) = current_database();
    let db = Connection::open(&path).unwrap();
    schema::strip_library_source_reference(&db).unwrap();
    db.execute_batch(
        r#"
        PRAGMA user_version=49;
        INSERT INTO discovery_campaigns VALUES('legacy','active',1,30,6,3,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('legacy',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"youtube","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('legacy','youtube','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','legacy','youtube','dedicated','AI tools',
          'https://www.youtube.com/watch?v=abcdefghijk','demo','Creator',3);
        INSERT INTO discovery_video_completions VALUES('seen',5);
    "#,
    )
    .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        49
    );
    for connection in [&db, &backup] {
        assert_eq!(
            connection
                .query_row(
                    "SELECT search_hit_id FROM discovery_video_completions",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "seen"
        );
        assert_eq!(
            connection
                .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    for table in [
        "discovery_video_identity_bindings",
        "discovery_library_source_versions",
    ] {
        assert_eq!(
            db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(backup.prepare(&format!("SELECT * FROM {table}")).is_err());
    }
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn library_source_edge_rolls_back_and_marker_only_upgrade_is_rejected() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_library_source_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 49).unwrap();
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 49, 50).unwrap();
        assert!(
            tx.prepare("SELECT * FROM discovery_library_source_versions")
                .is_ok()
        );
        assert!(
            tx.prepare("SELECT * FROM discovery_video_identity_bindings")
                .is_ok()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_library_source_versions")
            .is_err()
    );
    assert!(
        db.prepare("SELECT * FROM discovery_video_identity_bindings")
            .is_err()
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}
