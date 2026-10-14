use super::*;

fn predecessor() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let db = Connection::open(&path).unwrap();
    schema::strip_contacts_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 64).unwrap();
    db.execute_batch(r#"
      INSERT INTO discovery_campaigns VALUES('old','paused',1,30,200,31,1,2);
      INSERT INTO discovery_campaign_revisions VALUES('old',1,'"Product"','1.8.8','{"language":"Chinese"}','[]',1);
    "#).unwrap();
    (dir, path)
}

#[test]
fn contact_upgrade_preserves_old_approval_budget_pause_and_backup() {
    let (_dir, path) = predecessor();
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    let saved: (String, u32, u32, i64) = db.query_row("SELECT phase,max_calls,used_calls,approved_at FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(saved, ("paused".into(), 200, 31, 1));
    let policy: String = db.query_row("SELECT policy FROM discovery_recommendation_policies WHERE discovery_campaign_id='old'", [], |r| r.get(0)).unwrap();
    assert_eq!(policy, "legacy");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_contact_observations",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        64
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
            .get::<_, u32>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn contact_upgrade_rolls_back_and_rejects_marker_only() {
    let (_dir, path) = predecessor();
    let mut db = Connection::open(&path).unwrap();
    {
        let tx = db.transaction().unwrap();
        schema::apply_forward_migrations(&tx, 64, 65).unwrap();
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM discovery_recommendation_policies",
                [],
                |r| r.get::<_, u32>(0)
            )
            .unwrap(),
            1
        );
    }
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='discovery_contact_observations'",
            [],
            |r| r.get::<_, u32>(0)
        )
        .unwrap(),
        0
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
}
