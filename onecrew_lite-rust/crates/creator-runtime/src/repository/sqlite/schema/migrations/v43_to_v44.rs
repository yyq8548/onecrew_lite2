//! CRM v43 -> v44 (app 0.6.9): one author-bound public-video receipt per hit.
//! Existing Campaign criteria, approvals, budgets, candidates and receipts survive unchanged.
use rusqlite::Connection;

pub(super) const CONTENT_READS_SQL: &str = "CREATE TABLE discovery_content_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform='bilibili'),
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
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(CONTENT_READS_SQL)
        .map_err(|e| e.to_string())
}

/// Historical schema comparison only. The earlier migration bodies stay frozen.
pub(in crate::repository::sqlite) fn strip_content_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_content_reads;")
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v43(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_content_reads; PRAGMA user_version=43;")
        .map_err(|e| e.to_string())
}
