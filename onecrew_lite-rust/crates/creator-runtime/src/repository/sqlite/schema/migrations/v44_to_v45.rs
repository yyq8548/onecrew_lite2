//! CRM v44 -> v45 (app 0.6.10): accept X public-profile receipts.
//! The handle remains a mutable locator; qualification still requires stable identity.
use rusqlite::Connection;

const PROFILE_READS_V45_SQL: &str = "CREATE TABLE discovery_profile_reads (
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
             );";

const COPY_PROFILE_READS: &str = "INSERT INTO discovery_profile_reads
             (search_hit_id, discovery_campaign_id, platform, profile_ref, author_id,
              expected_revision, call_number, status, native_profile_json, stop_code, updated_at)
             SELECT search_hit_id, discovery_campaign_id, platform, profile_ref, author_id,
                    expected_revision, call_number, status, native_profile_json, stop_code, updated_at
             FROM discovery_profile_reads_v44;";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    // The outer upgrade orchestrator owns the transaction and version stamp.
    // No table references discovery_profile_reads, so renaming it does not retarget children.
    connection
        .execute_batch("ALTER TABLE discovery_profile_reads RENAME TO discovery_profile_reads_v44;")
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(PROFILE_READS_V45_SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(COPY_PROFILE_READS)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch("DROP TABLE discovery_profile_reads_v44;")
        .map_err(|e| e.to_string())
}

/// Build a predecessor schema for contract tests. The table is empty there.
pub(in crate::repository::sqlite) fn strip_twitter_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v42_to_v43::migrate(connection)
}
