//! CRM v42 -> v43 (app 0.6.8): budgeted public-profile evidence receipts.
//! Preserve existing observations, approvals, budgets and qualification decisions.
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch("CREATE TABLE discovery_profile_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','tiktok')),
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
             );").map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v42(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads; PRAGMA user_version=42;")
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v37(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v41_to_v42::downgrade_reference_to_v37(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v38(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v41_to_v42::downgrade_reference_to_v38(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v39(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v41_to_v42::downgrade_reference_to_v39(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v40(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v41_to_v42::downgrade_reference_to_v40(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v41(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    connection
        .execute_batch("DROP TABLE discovery_profile_reads;")
        .map_err(|e| e.to_string())?;
    super::v41_to_v42::downgrade_reference_to_v41(connection)
}
