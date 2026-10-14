//! CRM v38 -> v39 (app 0.6.4): durable discovery read completions.
//! Preserve attempts, results, approvals and stop states. Backfill only facts
//! supported by stored hits/source URLs; unknown old reads are not invented.

use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(
        "CREATE TABLE discovery_search_completions (
           discovery_campaign_id TEXT NOT NULL,
           platform TEXT NOT NULL,
           profile_ref TEXT NOT NULL,
           search_keyword TEXT NOT NULL,
           completed_at INTEGER NOT NULL,
           PRIMARY KEY(discovery_campaign_id, platform, profile_ref, search_keyword),
           FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
             REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE
         );
         CREATE TABLE discovery_video_completions (
           search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
           completed_at INTEGER NOT NULL
         );
         INSERT INTO discovery_search_completions
           SELECT discovery_campaign_id,platform,profile_ref,search_keyword,MIN(captured_at)
           FROM discovery_search_hits
           GROUP BY discovery_campaign_id,platform,profile_ref,search_keyword;
         INSERT INTO discovery_video_completions
           SELECT h.id,MIN(c.updated_at) FROM discovery_search_hits h
           JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
             AND c.platform=h.platform
           JOIN json_each(c.evidence_json, '$.source_urls') s ON s.value=h.source_url
           WHERE h.platform='youtube' GROUP BY h.id;"
    ).map_err(|error| error.to_string())
}

fn remove_current_receipts(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "DROP TABLE discovery_video_completions;
         DROP TABLE discovery_search_completions;",
        )
        .map_err(|error| error.to_string())
}

/// Validator/test reference reconstruction only; never a production downgrade.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v38(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_current_receipts(connection)?;
    connection
        .pragma_update(None, "user_version", 38)
        .map_err(|error| error.to_string())
}

/// Adapt the current reference before invoking the immutable older helper,
/// whose guard expects the current marker. No customer database uses this path.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v37(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_current_receipts(connection)?;
    super::v37_to_v38::downgrade_reference_to_v37(connection)
}
