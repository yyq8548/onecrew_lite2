use super::*;

#[test]
fn source_failure_edge_upgrades_v51_without_recasting_old_facts() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_budget_increases_reference(&db).unwrap();
    schema::strip_video_read_failures_reference(&db).unwrap();
    db.pragma_update(None, "user_version", 51).unwrap();
    assert!(schema::initialize(&db).is_err());

    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let tx = db.transaction().unwrap();
    schema::apply_forward_migrations(&tx, 51, schema::CURRENT_SCHEMA_VERSION).unwrap();
    tx.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    tx.commit().unwrap();

    schema::initialize(&db).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM discovery_video_read_failures",
            [],
            |row| { row.get::<_, i64>(0) }
        )
        .unwrap(),
        0
    );
}
