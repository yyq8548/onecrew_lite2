use super::*;

fn seed_v39(connection: &Connection) {
    schema::downgrade_reference_to_v39(connection).unwrap();
    connection.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        PRAGMA wal_autocheckpoint=0;
        INSERT INTO discovery_campaigns VALUES('transcript-upgrade','active',1,30,100,7,1,2);
        INSERT INTO discovery_campaign_revisions VALUES('transcript-upgrade',1,'"AI creators"','1.8.8',
          '{"min_followers":10000,"requires_ai_tool_demo":true}',
          '[{"platform":"youtube","profile_ref":"dedicated","keywords":["AI tools"]}]',1);
        INSERT INTO discovery_platform_runs VALUES('transcript-upgrade','youtube','dedicated','completed',NULL,NULL,1,NULL,2);
        INSERT INTO discovery_search_hits VALUES('seen','transcript-upgrade','youtube','dedicated','AI tools',
          'https://www.youtube.com/watch?v=LR04bU_yV5k','AI demo','Creator',3);
        INSERT INTO discovery_candidates VALUES('candidate','transcript-upgrade',1,'youtube','dedicated','UC1234567890',
          'Creator','https://www.youtube.com/channel/UC1234567890','pending',
          '{"source_urls":["https://www.youtube.com/watch?v=LR04bU_yV5k"]}',3,5);
        INSERT INTO discovery_search_completions VALUES('transcript-upgrade','youtube','dedicated','AI tools',3);
        INSERT INTO discovery_video_completions VALUES('seen',5);
    "#).unwrap();
}

#[test]
fn transcript_upgrade_preserves_v39_wal_receipts_and_independent_backup() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let source = Connection::open(&path).unwrap();
    seed_v39(&source);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 39,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: crate::CreatorDatabaseUpgradeImpact::default(),
        }
    );
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup = Connection::open(&result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        39
    );
    for connection in [&source, &backup] {
        assert_eq!(
            connection
                .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            7
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
                    "SELECT COUNT(*) FROM discovery_video_completions",
                    [],
                    |r| r.get::<_, u32>(0)
                )
                .unwrap(),
            1
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
                .query_row("SELECT verdict FROM discovery_candidates", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    assert!(
        backup
            .prepare("SELECT * FROM discovery_transcript_reads")
            .is_err()
    );
    assert_eq!(
        source
            .query_row("SELECT COUNT(*) FROM discovery_transcript_reads", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
}

#[test]
fn transcript_migration_ddl_rolls_back_with_outer_chain() {
    let mut connection = Connection::open_in_memory().unwrap();
    schema::initialize(&connection).unwrap();
    seed_v39(&connection);
    {
        let transaction = connection.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 39, schema::CURRENT_SCHEMA_VERSION).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_transcript_reads")
                .is_ok()
        );
    }
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        39
    );
    assert!(
        connection
            .prepare("SELECT * FROM discovery_transcript_reads")
            .is_err()
    );
    assert_eq!(
        connection
            .query_row("SELECT used_calls FROM discovery_campaigns", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        7
    );
}
