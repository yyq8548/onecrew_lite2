//! CRM v63 -> v64 (app 0.6.45): preserve the profile search source separately
//! from observed Facebook Reel locators. Startup owns backup and transaction.
use rusqlite::Connection;

const V63: &str = "platform IN ('bilibili','twitter','reddit')";
const V64: &str = "platform IN ('bilibili','twitter','reddit','facebook')";

pub(in crate::repository::sqlite::schema) const SQL: &str = r#"
CREATE TABLE discovery_facebook_content_targets (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL,
  platform TEXT NOT NULL CHECK(platform='facebook'),
  profile_ref TEXT NOT NULL,
  author_id TEXT NOT NULL CHECK(length(author_id) BETWEEN 1 AND 20 AND author_id NOT GLOB '*[^0-9]*' AND substr(author_id,1,1)!='0'),
  profile_url TEXT NOT NULL CHECK(profile_url='https://www.facebook.com/profile.php?id=' || author_id),
  expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
  source_urls_json TEXT CHECK(source_urls_json IS NULL OR (json_valid(source_urls_json) AND json_type(source_urls_json)='array' AND json_array_length(source_urls_json) BETWEEN 1 AND 3 AND length(CAST(source_urls_json AS BLOB)) <= 1024)),
  stop_code TEXT,
  updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
  FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
    REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
  FOREIGN KEY(discovery_campaign_id, expected_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE,
  CHECK((status='reading' AND source_urls_json IS NULL AND stop_code IS NULL)
    OR (status='completed' AND source_urls_json IS NOT NULL AND stop_code IS NULL)
    OR (status='failed' AND source_urls_json IS NULL AND stop_code IS NOT NULL))
);
"#;

fn rebuild(connection: &Connection, from: &str, to: &str) -> Result<(), String> {
    let tables = [
        "discovery_content_reads",
        "discovery_content_identity_bindings",
    ];
    let mut definitions = Vec::new();
    for table in tables {
        definitions.push(
            connection
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE name=?1",
                    [table],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| e.to_string())?,
        );
    }
    if definitions[0].matches(from).count() != 1 {
        return Err("Facebook content upgrade predecessor constraint mismatch".into());
    }
    definitions[0] = definitions[0].replacen(from, to, 1);
    for table in tables {
        connection
            .execute_batch(&format!(
                "CREATE TEMP TABLE facebook_content_upgrade_{table} AS SELECT * FROM {table};"
            ))
            .map_err(|e| e.to_string())?;
    }
    for table in tables.into_iter().rev() {
        connection
            .execute_batch(&format!("DROP TABLE {table};"))
            .map_err(|e| e.to_string())?;
    }
    for (table, ddl) in tables.into_iter().zip(definitions) {
        connection.execute_batch(&ddl).map_err(|e| e.to_string())?;
        connection.execute_batch(&format!("INSERT INTO {table} SELECT * FROM facebook_content_upgrade_{table}; DROP TABLE facebook_content_upgrade_{table};"))
            .map_err(|e|e.to_string())?;
    }
    Ok(())
}

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    rebuild(connection, V63, V64)?;
    connection.execute_batch(SQL).map_err(|e| e.to_string())
}

/// Historical reference construction only; no production downgrade path.
pub(in crate::repository::sqlite) fn strip_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_facebook_content_targets;")
        .map_err(|e| e.to_string())?;
    let ddl: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE name='discovery_content_reads'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if ddl.contains(V64) {
        rebuild(connection, V64, V63)?;
    }
    Ok(())
}
