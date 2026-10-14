use super::*;

fn seed_v43(connection: &Connection) {
    schema::downgrade_reference_to_v43(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        PRAGMA wal_autocheckpoint=0;
        INSERT INTO discovery_campaigns VALUES('content-upgrade','active',1,30,6,4,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('content-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"bilibili","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('content-upgrade','bilibili','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','content-upgrade','bilibili','dedicated','AI tools',
          'https://www.bilibili.com/video/BV1RYqHYnEw8','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','123','https://space.bilibili.com/123',4);
        INSERT INTO discovery_profile_reads VALUES('seen','content-upgrade','bilibili','dedicated','123',1,4,
          'completed','{"mid":"123","profile_url":"https://space.bilibili.com/123","name":"Creator","follower_count":15000}',NULL,5);
    "#).unwrap();
}

#[test]
fn content_upgrade_preserves_profile_receipt_budget_and_backup() {
    let (_dir, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    seed_v43(&connection);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 43,
            ..
        }
    ));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        43
    );
    assert!(
        backup
            .prepare("SELECT * FROM discovery_content_reads")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT native_profile_json FROM discovery_profile_reads",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "{\"mid\":\"123\",\"profile_url\":\"https://space.bilibili.com/123\",\"name\":\"Creator\",\"follower_count\":15000}"
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM discovery_content_reads", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn content_edge_rolls_back_with_outer_transaction() {
    let mut connection = Connection::open_in_memory().unwrap();
    schema::initialize(&connection).unwrap();
    seed_v43(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 43, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_content_reads")
                .is_ok()
        );
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        43
    );
    assert!(
        connection
            .prepare("SELECT * FROM discovery_content_reads")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        4
    );
}
