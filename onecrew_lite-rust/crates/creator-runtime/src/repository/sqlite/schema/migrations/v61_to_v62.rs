//! CRM v61 -> v62 (app 0.6.42): native Reddit profile and content receipts.
//! Rebuild dependent identity tables together, preserving every row. The upgrade
//! owner provides the transaction, lock, backup and schema validation.
use rusqlite::Connection;

const BEFORE: &str = r#"CREATE TEMP TABLE reddit_upgrade_discovery_profile_reads AS SELECT * FROM discovery_profile_reads;
CREATE TEMP TABLE reddit_upgrade_discovery_content_reads AS SELECT * FROM discovery_content_reads;
CREATE TEMP TABLE reddit_upgrade_discovery_profile_identity_matches AS SELECT * FROM discovery_profile_identity_matches;
CREATE TEMP TABLE reddit_upgrade_discovery_content_identity_bindings AS SELECT * FROM discovery_content_identity_bindings;
DROP TABLE discovery_content_identity_bindings;
DROP TABLE discovery_profile_identity_matches;
DROP TABLE discovery_content_reads;
DROP TABLE discovery_profile_reads;"#;
const AFTER: &str = r#"INSERT INTO discovery_profile_reads SELECT * FROM reddit_upgrade_discovery_profile_reads;
INSERT INTO discovery_content_reads SELECT * FROM reddit_upgrade_discovery_content_reads;
INSERT INTO discovery_profile_identity_matches SELECT * FROM reddit_upgrade_discovery_profile_identity_matches;
INSERT INTO discovery_content_identity_bindings SELECT * FROM reddit_upgrade_discovery_content_identity_bindings;
DROP TABLE reddit_upgrade_discovery_profile_reads;
DROP TABLE reddit_upgrade_discovery_content_reads;
DROP TABLE reddit_upgrade_discovery_profile_identity_matches;
DROP TABLE reddit_upgrade_discovery_content_identity_bindings;"#;
const V61: &str = r#"CREATE TABLE discovery_profile_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','tiktok','twitter')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_profile_json TEXT CHECK(native_profile_json IS NULL OR (json_valid(native_profile_json) AND json_type(native_profile_json)='object' AND length(CAST(native_profile_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               UNIQUE(discovery_campaign_id, platform, profile_ref, author_id),
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_profile_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_profile_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_profile_json IS NULL AND stop_code IS NOT NULL))
             );
CREATE TABLE discovery_content_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','twitter')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               source_url TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_content_json TEXT CHECK(native_content_json IS NULL OR (json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_content_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_content_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_content_json IS NULL AND stop_code IS NOT NULL))
             );
CREATE TABLE discovery_profile_identity_matches (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               updated_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id)
             );
CREATE TABLE discovery_content_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_content_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               profile_search_hit_id TEXT NOT NULL REFERENCES discovery_profile_identity_matches(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               created_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id AND search_hit_id=profile_search_hit_id)
             );"#;
const V62: &str = r#"CREATE TABLE discovery_profile_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','tiktok','twitter','reddit')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_profile_json TEXT CHECK(native_profile_json IS NULL OR (json_valid(native_profile_json) AND json_type(native_profile_json)='object' AND length(CAST(native_profile_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               UNIQUE(discovery_campaign_id, platform, profile_ref, author_id),
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_profile_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_profile_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_profile_json IS NULL AND stop_code IS NOT NULL))
             );
CREATE TABLE discovery_content_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','twitter','reddit')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               source_url TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_content_json TEXT CHECK(native_content_json IS NULL OR (json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_content_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_content_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_content_json IS NULL AND stop_code IS NOT NULL))
             );
CREATE TABLE discovery_profile_identity_matches (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               updated_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id)
             );
CREATE TABLE discovery_content_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_content_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               profile_search_hit_id TEXT NOT NULL REFERENCES discovery_profile_identity_matches(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               created_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id AND search_hit_id=profile_search_hit_id)
             );"#;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    rebuild(connection, V62)
}

fn rebuild(connection: &Connection, ddl: &str) -> Result<(), String> {
    connection
        .execute_batch(BEFORE)
        .map_err(|e| e.to_string())?;
    connection.execute_batch(ddl).map_err(|e| e.to_string())?;
    connection.execute_batch(AFTER).map_err(|e| e.to_string())
}

/// Reference construction only. Production never downgrades or rewrites markers.
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
    if sql.contains("'reddit'") {
        rebuild(connection, V61)?;
    }
    Ok(())
}
