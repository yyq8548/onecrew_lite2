use super::*;
use crate::repository::DiscoveryCampaignRepository;

fn seed_v38(connection: &Connection) {
    schema::downgrade_reference_to_v38(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        PRAGMA wal_autocheckpoint=0;
        INSERT INTO discovery_campaigns VALUES('receipt-upgrade','active',1,30,100,7,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('receipt-upgrade',1,'"AI creators"','1.8.8',
          '{"min_followers":10000}',
          '[{"platform":"youtube","profile_ref":"dedicated","keywords":["AI tools","AI demos"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('receipt-upgrade','youtube','dedicated','completed',NULL,NULL,3,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','receipt-upgrade','youtube','dedicated','AI tools',
          'https://www.youtube.com/watch?v=00000000001','AI demo','Creator',3);
        INSERT INTO discovery_search_hits VALUES('unread','receipt-upgrade','youtube','dedicated','AI tools',
          'https://www.youtube.com/watch?v=00000000002','Another demo','Creator',4);
        INSERT INTO discovery_candidates VALUES('candidate','receipt-upgrade',1,'youtube','dedicated','UC1234567890',
          'Creator','https://www.youtube.com/channel/UC1234567890','pending',
          '{"source_urls":["https://www.youtube.com/watch?v=00000000001"]}',3,5);
    "#).unwrap();
}

#[test]
fn discovery_receipts_upgrade_v38_preserves_wal_facts_budget_and_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let source = Connection::open(&path).unwrap();
    seed_v38(&source);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 38,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: crate::CreatorDatabaseUpgradeImpact::default(),
        }
    );
    assert!(SqliteRepository::open(&path).is_err());
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        38
    );
    assert_eq!(
        backup
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |row| row
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        7
    );
    assert_eq!(
        backup
            .query_row("SELECT COUNT(*) FROM discovery_search_hits", [], |row| row
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        2
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        source
            .query_row(
                "SELECT search_keyword FROM discovery_search_completions",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "AI tools"
    );
    assert_eq!(
        source
            .query_row(
                "SELECT search_hit_id FROM discovery_video_completions",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "seen"
    );
    let repository = SqliteRepository::open(&path).unwrap();
    let campaign = repository
        .get_discovery_campaign("receipt-upgrade")
        .unwrap();
    assert_eq!(campaign.used_calls, 7);
    assert_eq!(campaign.max_calls, Some(100));
    assert_eq!(campaign.platform_runs[0].calls_used, 3);
    assert_eq!(campaign.approved_at, Some(1));
    assert_eq!(
        repository
            .next_discovery_search_keyword("receipt-upgrade", &campaign.scopes[0])
            .unwrap()
            .as_deref(),
        Some("AI demos")
    );
    assert_eq!(
        repository
            .next_discovery_video_hit("receipt-upgrade")
            .unwrap()
            .unwrap()
            .id,
        "unread"
    );
    assert_eq!(
        repository
            .count_discovery_candidates("receipt-upgrade")
            .unwrap()
            .pending,
        1
    );
}

#[test]
fn discovery_receipts_edge_rolls_back_with_outer_transaction() {
    let mut connection = Connection::open_in_memory().unwrap();
    super::super::schema::initialize(&connection).unwrap();
    seed_v38(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 38, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert_eq!(
            transaction
                .query_row(
                    "SELECT COUNT(*) FROM discovery_video_completions",
                    [],
                    |row| row.get::<_, u32>(0)
                )
                .unwrap(),
            1
        );
        // Dropping the uncommitted chain must remove both DDL and backfilled rows.
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        38
    );
    assert!(
        connection
            .prepare("SELECT * FROM discovery_video_completions")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |row| row
                .get::<_, u32>(
                0
            ))
            .unwrap(),
        7
    );
}
