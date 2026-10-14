use super::*;

fn seed_v42(connection: &Connection) {
    schema::downgrade_reference_to_v42(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        PRAGMA wal_autocheckpoint=0;
        INSERT INTO discovery_campaigns VALUES('profile-upgrade','active',1,30,100,3,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('profile-upgrade',1,'"Creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"bilibili","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('profile-upgrade','bilibili','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','profile-upgrade','bilibili','dedicated','AI tools',
          'https://www.bilibili.com/video/BV1RYqHYnEw8','demo','Creator',3);
        INSERT INTO discovery_search_authors VALUES('seen','123','https://space.bilibili.com/123',4);
    "#).unwrap();
}

fn assert_preserved(connection: &Connection) {
    let observed: (u32, u32, String, i64) = connection
        .query_row(
            "SELECT c.used_calls,v.approved_at,a.author_id,a.observed_at FROM discovery_campaigns c
         JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id
         JOIN discovery_search_hits h ON h.discovery_campaign_id=c.id
         JOIN discovery_search_authors a ON a.search_hit_id=h.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(observed, (3, 1, "123".into(), 4));
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}

#[test]
fn profile_upgrade_preserves_author_wal_and_backup_without_evidence_backfill() {
    let (_dir, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    seed_v42(&connection);
    assert!(matches!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 42,
            ..
        }
    ));
    assert!(SqliteRepository::open(&path).is_err());
    assert_preserved(&connection);
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    assert_preserved(&backup);
    assert_preserved(&connection);
    assert!(
        backup
            .prepare("SELECT * FROM discovery_profile_reads")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM discovery_profile_reads", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn profile_edge_rolls_back_with_the_outer_transaction() {
    let mut connection = Connection::open_in_memory().unwrap();
    schema::initialize(&connection).unwrap();
    seed_v42(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 42, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_profile_reads")
                .is_ok()
        );
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        42
    );
    assert!(
        connection
            .prepare("SELECT * FROM discovery_profile_reads")
            .is_err()
    );
    assert_preserved(&connection);
}
