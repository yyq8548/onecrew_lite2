//! CRM v68 -> v69 (app 0.6.59): author-bound Bluesky receipts. Existing
//! startup owner provides lock, preflight, WAL backup and outer transaction.
use rusqlite::Connection;
const PROFILE_68: &str =
    "platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo')";
const PROFILE_69: &str =
    "platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo','bluesky')";
const CONTENT_68: &str =
    "platform IN ('bilibili','twitter','reddit','facebook','instagram','weibo')";
const CONTENT_69: &str =
    "platform IN ('bilibili','twitter','reddit','facebook','instagram','weibo','bluesky')";

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, false)
}

fn rebuild(db: &Connection, reference: bool) -> Result<(), String> {
    let tables = [
        "discovery_profile_reads",
        "discovery_content_reads",
        "discovery_profile_identity_matches",
        "discovery_content_identity_bindings",
        "discovery_facebook_content_targets",
    ];
    let mut ddl = Vec::new();
    for table in tables {
        ddl.push(
            db.query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [table],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?,
        );
    }
    for (index, old, new) in [(0, PROFILE_68, PROFILE_69), (1, CONTENT_68, CONTENT_69)] {
        let (from, to) = if reference { (new, old) } else { (old, new) };
        if ddl[index].matches(from).count() != 1 {
            return Err("Bluesky receipt predecessor constraint mismatch".into());
        }
        ddl[index] = ddl[index].replacen(from, to, 1);
    }
    for table in tables {
        db.execute_batch(&format!(
            "CREATE TEMP TABLE bluesky_upgrade_{table} AS SELECT * FROM {table};"
        ))
        .map_err(|e| e.to_string())?;
    }
    for table in tables.into_iter().rev() {
        db.execute_batch(&format!("DROP TABLE {table};"))
            .map_err(|e| e.to_string())?;
    }
    for (table, sql) in tables.into_iter().zip(ddl) {
        db.execute_batch(&sql).map_err(|e| e.to_string())?;
        db.execute_batch(&format!("INSERT INTO {table} SELECT * FROM bluesky_upgrade_{table}; DROP TABLE bluesky_upgrade_{table};")).map_err(|e|e.to_string())?;
    }
    Ok(())
}

/// Historical reference construction only. Never an application downgrade.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_profile_reads'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if sql.contains(PROFILE_69) {
        rebuild(db, true)?;
    }
    Ok(())
}
