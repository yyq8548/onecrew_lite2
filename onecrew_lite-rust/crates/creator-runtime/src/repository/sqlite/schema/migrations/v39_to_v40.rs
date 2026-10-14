//! CRM v39 -> v40 (app 0.6.5): bounded, budgeted transcript read receipts.
//! Existing candidates, approvals and budgets are preserved without backfill.

use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(
        "CREATE TABLE discovery_transcript_reads (
           search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
           candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
           call_number INTEGER NOT NULL CHECK(call_number > 0),
           status TEXT NOT NULL CHECK(status IN ('reading','completed','unavailable','failed')),
           segments_json TEXT NOT NULL CHECK(json_valid(segments_json) AND length(CAST(segments_json AS BLOB)) <= 24000),
           truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
           stop_code TEXT,
           updated_at INTEGER NOT NULL
         );"
    ).map_err(|error| error.to_string())
}

fn remove_transcripts(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_transcript_reads;")
        .map_err(|error| error.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v39(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_transcripts(connection)?;
    connection
        .pragma_update(None, "user_version", 39)
        .map_err(|error| error.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v38(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_transcripts(connection)?;
    super::v38_to_v39::downgrade_reference_to_v38(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v37(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_transcripts(connection)?;
    super::v38_to_v39::downgrade_reference_to_v37(connection)
}
