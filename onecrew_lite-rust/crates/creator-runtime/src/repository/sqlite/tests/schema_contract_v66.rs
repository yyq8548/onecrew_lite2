use super::*;

#[test]
fn contact_lookup_upgrade_preserves_v65_and_backup_without_searching() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let db = Connection::open(&path).unwrap();
    schema::strip_instagram_reference(&db).unwrap();
    db.execute_batch(
        "DROP TABLE discovery_contact_lookups; PRAGMA user_version=65;
      INSERT INTO discovery_campaigns VALUES('old','paused',1,30,NULL,31,1,2);
      INSERT INTO discovery_campaign_revisions VALUES('old',1,'\"Product\"','1.8.8','{}','[]',1);
      INSERT INTO discovery_recommendation_policies VALUES('old',1,'score_and_contact_v1');",
    )
    .unwrap();
    assert!(SqliteRepository::open(&path).is_err());
    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let saved: (String, Option<u32>, u32) = db
        .query_row(
            "SELECT phase,max_calls,used_calls FROM discovery_campaigns",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(saved, ("paused".into(), None, 31));
    assert_eq!(
        db.query_row("SELECT count(*) FROM discovery_contact_lookups", [], |r| {
            r.get::<_, u32>(0)
        })
        .unwrap(),
        0
    );
    assert!(SqliteRepository::open(&path).is_ok());
    let backup = Connection::open(result.backup_path).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        65
    );
}
