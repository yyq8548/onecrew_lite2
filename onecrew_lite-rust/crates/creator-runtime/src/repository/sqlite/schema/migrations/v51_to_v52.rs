//! CRM v51 -> v52 (app 0.6.18): source-level YouTube read failures.
//! Historical platform stops stay unchanged; no failure is recast as completed evidence.
use rusqlite::Connection;

const SQL: &str = "CREATE TABLE discovery_video_read_failures (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  stop_code TEXT NOT NULL CHECK(length(stop_code) BETWEEN 1 AND 128),
  updated_at INTEGER NOT NULL
);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SQL)
        .map_err(|error| error.to_string())
}

/// Empty predecessor reference only; never a production downgrade.
pub(in crate::repository::sqlite) fn strip_video_read_failures_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_video_read_failures;")
        .map_err(|error| error.to_string())
}
