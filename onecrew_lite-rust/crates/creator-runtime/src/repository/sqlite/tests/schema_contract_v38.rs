use super::*;
use rusqlite::OptionalExtension;

#[test]
fn discovery_schema_upgrades_v37_without_touching_existing_creator_rows() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("crm.db");
    drop(SqliteRepository::open(&path).unwrap());
    let source = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v37(&source).unwrap();
    source
        .execute_batch(
            "INSERT INTO creators(id,display_name,notes,created_at,updated_at)
               VALUES('legacy-creator','Keep me','original',1,2);",
        )
        .unwrap();
    drop(source);

    let preflight = crate::CreatorRuntime::inspect_database(&path);
    assert_eq!(
        preflight,
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 37,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: crate::CreatorDatabaseUpgradeImpact::default(),
        }
    );
    assert!(SqliteRepository::open(&path).is_err());
    assert_eq!(
        Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        37
    );

    let result = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(result.to_version, schema::CURRENT_SCHEMA_VERSION);
    assert!(std::path::Path::new(&result.backup_path).exists());
    let current = Connection::open(&path).unwrap();
    assert_eq!(
        current
            .query_row(
                "SELECT display_name || ':' || notes FROM creators WHERE id='legacy-creator'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Keep me:original"
    );
    assert_eq!(
        current
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        current
            .query_row("PRAGMA foreign_key_check", [], |row| row
                .get::<_, String>(0))
            .optional()
            .unwrap()
            .is_none()
    );
    assert!(SqliteRepository::open(&path).is_ok());
}
