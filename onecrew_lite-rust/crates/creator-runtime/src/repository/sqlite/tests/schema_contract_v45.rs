use super::*;

fn seed_v44(connection: &Connection) {
    schema::strip_twitter_content_reference(connection).unwrap();
    schema::strip_twitter_reference(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA user_version=44;
        INSERT INTO discovery_campaigns VALUES('profile-upgrade','active',1,30,6,4,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('profile-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"bilibili","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('profile-upgrade','bilibili','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','profile-upgrade','bilibili','dedicated','AI tools',
          'https://www.bilibili.com/video/BV1RYqHYnEw8','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','123','https://space.bilibili.com/123',4);
        INSERT INTO discovery_profile_reads VALUES('seen','profile-upgrade','bilibili','dedicated','123',1,4,
          'completed','{"mid":"123","profile_url":"https://space.bilibili.com/123","name":"Creator","follower_count":15000}',NULL,5);
    "#).unwrap();
}

#[test]
fn x_profile_edge_preserves_v44_receipt_budget_and_backup() {
    let (_dir, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    seed_v44(&connection);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 44,
            ..
        }
    ));
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        44
    );
    let receipt = |db: &Connection| -> String {
        db.query_row(
            "SELECT native_profile_json FROM discovery_profile_reads WHERE search_hit_id='seen'",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(receipt(&connection), receipt(&backup));
    assert_eq!(
        connection
            .query_row(
                "SELECT used_calls FROM discovery_campaigns WHERE id='profile-upgrade'",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
        4
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        schema::CURRENT_SCHEMA_VERSION
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn x_profile_edge_rolls_back_with_outer_transaction() {
    let mut connection = Connection::open_in_memory().unwrap();
    schema::initialize(&connection).unwrap();
    seed_v44(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 44, 45).unwrap();
        let definition: String = transaction
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE name='discovery_profile_reads'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(definition.contains("'twitter'"));
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        44
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM discovery_profile_reads", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
    let definition: String = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='discovery_profile_reads'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!definition.contains("'twitter'"));
}
