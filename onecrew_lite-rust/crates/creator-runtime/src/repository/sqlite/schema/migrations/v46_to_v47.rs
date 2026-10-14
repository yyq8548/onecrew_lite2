//! CRM v46 -> v47 (app 0.6.12): additive native post-author identity receipts.
//! No historical identity backfill, qualification change, or budget rewrite.
use rusqlite::Connection;

const IDENTITY_READS_V47_SQL: &str = "CREATE TABLE discovery_identity_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform='twitter'),
               profile_ref TEXT NOT NULL,
               author_handle TEXT NOT NULL,
               source_url TEXT NOT NULL,
               post_id TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               account_id TEXT,
               observed_source_url TEXT,
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND account_id IS NULL AND observed_source_url IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND account_id IS NOT NULL AND observed_source_url IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND account_id IS NULL AND observed_source_url IS NULL AND stop_code IS NOT NULL))
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(IDENTITY_READS_V47_SQL)
        .map_err(|e| e.to_string())
}

/// Empty predecessor reference only; never a production downgrade.
pub(in crate::repository::sqlite) fn strip_identity_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_identity_reads;")
        .map_err(|e| e.to_string())
}
