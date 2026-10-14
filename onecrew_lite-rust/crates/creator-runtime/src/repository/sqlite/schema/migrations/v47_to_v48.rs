//! CRM v47 -> v48 (app 0.6.13): bind fresh native profiles to observed post authors.
//! Historical handle-only profiles, candidates, approvals and budgets stay unchanged.
use rusqlite::Connection;

const PROFILE_IDENTITY_MATCHES_V48_SQL: &str = "CREATE TABLE discovery_profile_identity_matches (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               updated_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id)
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(PROFILE_IDENTITY_MATCHES_V48_SQL)
        .map_err(|e| e.to_string())
}

/// Empty predecessor reference only, never a production downgrade.
pub(in crate::repository::sqlite) fn strip_profile_identity_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_profile_identity_matches;")
        .map_err(|e| e.to_string())
}
