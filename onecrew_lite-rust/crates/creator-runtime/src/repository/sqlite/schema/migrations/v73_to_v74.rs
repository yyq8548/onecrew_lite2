//! CRM v73 -> v74 (app 0.6.70): native author/content receipts for Pixiv,
//! LinkedIn and Douyin. The startup owner supplies locking and a verified backup.
use rusqlite::Connection;
const PROFILE_PREDECESSOR: &str = "platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest')";
const PROFILE_CURRENT: &str = "platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin','douyin')";
const CONTENT_PREDECESSOR: &str = "platform IN ('bilibili','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest')";
const CONTENT_CURRENT: &str = "platform IN ('bilibili','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin','douyin')";
const TABLES: [&str; 5] = [
    "discovery_profile_reads",
    "discovery_content_reads",
    "discovery_profile_identity_matches",
    "discovery_content_identity_bindings",
    "discovery_facebook_content_targets",
];

pub(in crate::repository::sqlite::schema) fn migrate(db: &Connection) -> Result<(), String> {
    rebuild(db, false)
}

fn rebuild(db: &Connection, reference: bool) -> Result<(), String> {
    let mut ddl = Vec::new();
    let mut objects = Vec::new();
    for table in TABLES {
        ddl.push(
            db.query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?,
        );
        let mut statement = db.prepare("SELECT sql FROM sqlite_master WHERE tbl_name=?1 AND type IN ('index','trigger') AND sql IS NOT NULL ORDER BY type,name").map_err(|e| e.to_string())?;
        objects.extend(
            statement
                .query_map([table], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?,
        );
    }
    for (index, old, new) in [
        (0, PROFILE_PREDECESSOR, PROFILE_CURRENT),
        (1, CONTENT_PREDECESSOR, CONTENT_CURRENT),
    ] {
        let (from, to) = if reference { (new, old) } else { (old, new) };
        if ddl[index].matches(from).count() != 1 {
            return Err("native receipt v73 predecessor constraint mismatch".into());
        }
        ddl[index] = ddl[index].replacen(from, to, 1);
    }
    db.execute_batch("SAVEPOINT native_receipt_v74")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        for table in TABLES {
            db.execute_batch(&format!(
                "CREATE TEMP TABLE native_v74_{table} AS SELECT * FROM {table}"
            ))
            .map_err(|e| e.to_string())?;
        }
        for table in TABLES.into_iter().rev() {
            db.execute_batch(&format!("DROP TABLE {table}"))
                .map_err(|e| e.to_string())?;
        }
        for (table, sql) in TABLES.into_iter().zip(ddl) {
            db.execute_batch(&sql).map_err(|e| e.to_string())?;
            db.execute_batch(&format!("INSERT INTO {table} SELECT * FROM native_v74_{table}; DROP TABLE native_v74_{table}")).map_err(|e| e.to_string())?;
        }
        // Recreate explicit indexes and triggers after copying rows; historical
        // insert triggers must not run a second time during a schema upgrade.
        for sql in objects {
            db.execute_batch(&sql).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => db
            .execute_batch("RELEASE native_receipt_v74")
            .map_err(|e| e.to_string()),
        Err(error) => {
            db.execute_batch("ROLLBACK TO native_receipt_v74; RELEASE native_receipt_v74")
                .map_err(|e| format!("{error}; migration rollback failed: {e}"))?;
            Err(error)
        }
    }
}

/// Only builds frozen historical references; never downgrades customer data.
pub(in crate::repository::sqlite) fn strip_reference(db: &Connection) -> Result<(), String> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='discovery_profile_reads'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if sql.contains(PROFILE_CURRENT) {
        rebuild(db, true)?;
    }
    Ok(())
}
