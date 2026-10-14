use super::*;

#[test]
fn identity_edge_preserves_v46_budget_sources_and_independent_backup() {
    let (_dir, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v46(&connection).unwrap();
    connection.execute_batch(r#"
        INSERT INTO discovery_campaigns VALUES('identity-upgrade','active',1,30,6,4,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('identity-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"twitter","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('identity-upgrade','twitter','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','identity-upgrade','twitter','dedicated','AI tools',
          'https://x.com/i/status/123','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','creator_1','https://x.com/creator_1',4);
    "#).unwrap();
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 46,
            ..
        }
    ));
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        46
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_identity_reads")
            .is_err()
    );
    for db in [&connection, &backup] {
        assert_eq!(
            db.query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            db.query_row("SELECT author_id FROM discovery_search_authors", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "creator_1"
        );
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM discovery_identity_reads", [], |r| r
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn identity_edge_outer_transaction_rolls_back_and_marker_alone_cannot_upgrade() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::downgrade_reference_to_v46(&db).unwrap();
    {
        let transaction = db.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 46, 47).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_identity_reads")
                .is_ok()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_identity_reads")
            .is_err()
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        46
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}
