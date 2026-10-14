//! CRM v66 -> v67 (app 0.6.57): author-bound Instagram profile/content receipts.
//! Preserve every receipt and both dependent X identity tables. The startup
//! upgrade owner supplies the exclusive lock, WAL-consistent backup and outer
//! transaction; this edge does not change campaign authorization or run reads.
use rusqlite::Connection;

const PROFILE_66: &str = "platform IN ('bilibili','tiktok','twitter','reddit','facebook')";
const PROFILE_67: &str =
    "platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram')";
const CONTENT_66: &str = "platform IN ('bilibili','twitter','reddit','facebook')";
const CONTENT_67: &str = "platform IN ('bilibili','twitter','reddit','facebook','instagram')";

pub(in crate::repository::sqlite::schema) fn migrate(
    connection: &Connection,
) -> Result<(), String> {
    rebuild(connection, false)
}

fn rebuild(connection: &Connection, reference_only: bool) -> Result<(), String> {
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
            connection
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE name=?1",
                    [table],
                    |r| r.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?,
        );
    }
    for (index, old, new) in [(0, PROFILE_66, PROFILE_67), (1, CONTENT_66, CONTENT_67)] {
        let (from, to) = if reference_only {
            (new, old)
        } else {
            (old, new)
        };
        if ddl[index].matches(from).count() != 1 {
            return Err("Instagram receipt upgrade predecessor constraint mismatch".into());
        }
        ddl[index] = ddl[index].replacen(from, to, 1);
    }
    for table in tables {
        connection
            .execute_batch(&format!(
                "CREATE TEMP TABLE instagram_upgrade_{table} AS SELECT * FROM {table};"
            ))
            .map_err(|e| e.to_string())?;
    }
    for table in tables.into_iter().rev() {
        connection
            .execute_batch(&format!("DROP TABLE {table};"))
            .map_err(|e| e.to_string())?;
    }
    for (table, sql) in tables.into_iter().zip(ddl) {
        connection.execute_batch(&sql).map_err(|e| e.to_string())?;
        connection.execute_batch(&format!(
            "INSERT INTO {table} SELECT * FROM instagram_upgrade_{table}; DROP TABLE instagram_upgrade_{table};"
        )).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Frozen-predecessor reference construction only. Never downgrade user data.
pub(in crate::repository::sqlite) fn strip_reference(
    connection: &Connection,
) -> Result<(), String> {
    let sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_profile_reads'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if sql.contains(PROFILE_67) {
        rebuild(connection, true)?;
    }
    Ok(())
}
