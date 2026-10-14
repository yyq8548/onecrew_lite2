use super::*;

fn seed_v41(connection: &Connection) {
    schema::downgrade_reference_to_v41(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        PRAGMA wal_autocheckpoint=0;
        INSERT INTO discovery_campaigns VALUES('author-upgrade','active',1,30,100,3,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('author-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"bilibili","profile_ref":"dedicated","keywords":["ecommerce"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('author-upgrade','bilibili','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','author-upgrade','bilibili','dedicated','ecommerce',
          'https://www.bilibili.com/video/BV1RYqHYnEw8','demo','Same name',3);
        INSERT INTO discovery_search_completions VALUES('author-upgrade','bilibili','dedicated','ecommerce',3);
    "#).unwrap();
}

fn assert_preserved(connection: &Connection) {
    assert_eq!(
        connection
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT approved_at FROM discovery_campaign_revisions",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT author_label FROM discovery_search_hits WHERE id='seen'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "Same name"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM discovery_search_completions",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn author_upgrade_preserves_wal_and_backup_without_name_based_backfill() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let source = Connection::open(&path).unwrap();
    seed_v41(&source);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 41,
            ..
        }
    ));
    assert!(SqliteRepository::open(&path).is_err());
    assert_preserved(&source);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        41
    );
    assert_preserved(&backup);
    assert_preserved(&source);
    assert!(
        backup
            .prepare("SELECT * FROM discovery_search_authors")
            .is_err()
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM discovery_search_authors", [], |r| r
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn author_edge_rolls_back_with_the_outer_transaction() {
    let mut connection = Connection::open_in_memory().unwrap();
    schema::initialize(&connection).unwrap();
    seed_v41(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 41, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_search_authors")
                .is_ok()
        );
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        41
    );
    assert!(
        connection
            .prepare("SELECT * FROM discovery_search_authors")
            .is_err()
    );
    assert_preserved(&connection);
}
