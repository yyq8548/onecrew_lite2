//! CRM v49 -> v50 (app 0.6.16): exact video attribution and Library source versions.
//! Historical receipts, candidates, criteria, budgets and verdicts are preserved.
use rusqlite::Connection;

const LIBRARY_SOURCE_V50_SQL: &str = "CREATE TABLE discovery_video_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_video_completions(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
               created_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_library_source_versions (
               discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
               snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64 AND snapshot_hash NOT GLOB '*[^0-9a-f]*'),
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(LIBRARY_SOURCE_V50_SQL)
        .map_err(|e| e.to_string())
}

/// Empty predecessor reference only; never a production downgrade.
pub(in crate::repository::sqlite) fn strip_library_source_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection.execute_batch("DROP TABLE discovery_library_source_versions; DROP TABLE discovery_video_identity_bindings;")
        .map_err(|e| e.to_string())
}
