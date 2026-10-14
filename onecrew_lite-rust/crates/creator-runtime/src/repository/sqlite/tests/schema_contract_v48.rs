use super::*;

#[test]
fn native_profile_edge_preserves_legacy_receipts_and_backup_without_backfill() {
    let (_dir, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::strip_library_source_reference(&connection).unwrap();
    connection
        .execute_batch("DROP TABLE discovery_content_identity_bindings;")
        .unwrap();
    connection
        .execute_batch("DROP TABLE discovery_profile_identity_matches; PRAGMA user_version=47;")
        .unwrap();
    connection.execute_batch(r#"
        INSERT INTO discovery_campaigns VALUES('native-upgrade','active',1,30,6,3,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('native-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"twitter","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('native-upgrade','twitter','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','native-upgrade','twitter','dedicated','AI tools',
          'https://x.com/i/status/123','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','creator_1','https://x.com/creator_1',4);
        INSERT INTO discovery_identity_reads VALUES('seen','native-upgrade','twitter','dedicated','creator_1',
          'https://x.com/i/status/123','123',1,2,'completed','987654321','https://x.com/creator_1/status/123',NULL,4);
        INSERT INTO discovery_profile_reads VALUES('seen','native-upgrade','twitter','dedicated','creator_1',1,3,
          'completed','{"handle":"creator_1","profile_url":"https://x.com/creator_1","name":"Creator","follower_count":15000,"identity_kind":"mutable_handle"}',NULL,5);
        INSERT INTO discovery_candidates VALUES('legacy-candidate','native-upgrade',1,'twitter','dedicated','creator_1',
          'Creator','https://x.com/creator_1','pending','{"author_identity_kind":"mutable_handle"}',5,5);
    "#).unwrap();
    let original: String = connection
        .query_row(
            "SELECT native_profile_json FROM discovery_profile_reads",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        47
    );
    for db in [&connection, &backup] {
        assert_eq!(
            db.query_row(
                "SELECT native_profile_json FROM discovery_profile_reads",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            original
        );
        assert_eq!(
            db.query_row("SELECT used_calls FROM discovery_campaigns", [], |row| row
                .get::<_, u32>(
                0
            ))
            .unwrap(),
            3
        );
        assert_eq!(db.query_row("SELECT platform_account_id || ':' || verdict || ':' || evidence_json FROM discovery_candidates",[],|row|row.get::<_,String>(0)).unwrap(),
            "creator_1:pending:{\"author_identity_kind\":\"mutable_handle\"}");
        assert_eq!(
            db.query_row(
                "SELECT account_id FROM discovery_identity_reads",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "987654321"
        );
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM discovery_profile_identity_matches",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
        0
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_profile_identity_matches")
            .is_err()
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn native_profile_edge_rolls_back_and_rejects_marker_only_upgrade() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_library_source_reference(&db).unwrap();
    db.execute_batch("DROP TABLE discovery_content_identity_bindings;")
        .unwrap();
    db.execute_batch("DROP TABLE discovery_profile_identity_matches; PRAGMA user_version=47;")
        .unwrap();
    {
        let transaction = db.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 47, 48).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_profile_identity_matches")
                .is_ok()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_profile_identity_matches")
            .is_err()
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}
