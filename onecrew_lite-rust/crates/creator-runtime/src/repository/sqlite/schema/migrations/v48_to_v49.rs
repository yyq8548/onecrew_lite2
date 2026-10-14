//! CRM v48 -> v49 (app 0.6.14): reserve native-author content bindings.
//! Historical content, candidates, author evidence and budgets stay unchanged.
use rusqlite::Connection;

const CONTENT_IDENTITY_BINDINGS_V49_SQL: &str = "CREATE TABLE discovery_content_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_content_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               profile_search_hit_id TEXT NOT NULL REFERENCES discovery_profile_identity_matches(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               created_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id AND search_hit_id=profile_search_hit_id)
             );";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(CONTENT_IDENTITY_BINDINGS_V49_SQL)
        .map_err(|e| e.to_string())
}

/// Empty predecessor reference only, never a production downgrade.
pub(in crate::repository::sqlite) fn strip_content_identity_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_content_identity_bindings;")
        .map_err(|e| e.to_string())
}
