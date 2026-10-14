//! CRM v45 -> v46 (app 0.6.11): accept bounded X recent-content receipts.
//! Existing Bilibili receipts, Campaign approvals, candidates and budgets remain unchanged.
use rusqlite::Connection;

const CONTENT_READS_V46_SQL: &str = "CREATE TABLE discovery_content_reads (
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
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    // The service upgrade owner holds the transaction and writes the version.
    connection
        .execute_batch("ALTER TABLE discovery_content_reads RENAME TO discovery_content_reads_v45;")
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(CONTENT_READS_V46_SQL)
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "INSERT INTO discovery_content_reads
        (search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,source_url,
         expected_revision,call_number,status,native_content_json,stop_code,updated_at)
        SELECT search_hit_id,discovery_campaign_id,platform,profile_ref,author_id,source_url,
               expected_revision,call_number,status,native_content_json,stop_code,updated_at
        FROM discovery_content_reads_v45;",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch("DROP TABLE discovery_content_reads_v45;")
        .map_err(|error| error.to_string())
}

/// Empty predecessor reference used only in migration contract tests.
pub(in crate::repository::sqlite) fn strip_twitter_content_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_content_reads;")
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(super::v43_to_v44::CONTENT_READS_SQL)
        .map_err(|error| error.to_string())
}
