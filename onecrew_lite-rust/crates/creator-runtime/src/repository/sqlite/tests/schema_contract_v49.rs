use super::*;

#[test]
fn native_content_edge_is_adjacent_atomic_and_rejects_marker_only_repair() {
    let mut db = Connection::open_in_memory().unwrap();
    schema::initialize(&db).unwrap();
    schema::strip_library_source_reference(&db).unwrap();
    db.execute_batch("DROP TABLE discovery_content_identity_bindings; PRAGMA user_version=48;")
        .unwrap();
    {
        let transaction = db.transaction().unwrap();
        schema::apply_forward_migrations(&transaction, 48, 49).unwrap();
        assert!(
            transaction
                .prepare("SELECT * FROM discovery_content_identity_bindings")
                .is_ok()
        );
    }
    assert!(
        db.prepare("SELECT * FROM discovery_content_identity_bindings")
            .is_err()
    );
    db.pragma_update(None, "user_version", schema::CURRENT_SCHEMA_VERSION)
        .unwrap();
    assert!(schema::initialize(&db).is_err());
}
