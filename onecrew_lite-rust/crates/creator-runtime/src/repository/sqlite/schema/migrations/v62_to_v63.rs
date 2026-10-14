//! CRM v62 -> v63 (app 0.6.44): observed numeric Facebook profile receipts.
//! Preflight validates the predecessor. Rebuild the profile and both dependent
//! identity tables together; the startup owner supplies backup and transaction.
use rusqlite::Connection;

const V62: &str = "platform IN ('bilibili','tiktok','twitter','reddit')";
const V63: &str = "platform IN ('bilibili','tiktok','twitter','reddit','facebook')";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    rebuild(connection, V62, V63)
}

fn rebuild(connection: &Connection, from: &str, to: &str) -> Result<(), String> {
    let tables = [
        "discovery_profile_reads",
        "discovery_profile_identity_matches",
        "discovery_content_identity_bindings",
    ];
    let mut ddl = Vec::new();
    for table in tables {
        ddl.push(
            connection
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE name=?1",
                    [table],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?,
        );
    }
    if ddl[0].matches(from).count() != 1 {
        return Err("Facebook profile upgrade predecessor constraint mismatch".into());
    }
    ddl[0] = ddl[0].replacen(from, to, 1);
    for table in tables {
        connection
            .execute_batch(&format!(
                "CREATE TEMP TABLE facebook_upgrade_{table} AS SELECT * FROM {table};"
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
        connection.execute_batch(&format!("INSERT INTO {table} SELECT * FROM facebook_upgrade_{table}; DROP TABLE facebook_upgrade_{table};")).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reference construction only; production never downgrades user data.
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
    if sql.contains(V63) {
        rebuild(connection, V63, V62)?;
    }
    Ok(())
}
