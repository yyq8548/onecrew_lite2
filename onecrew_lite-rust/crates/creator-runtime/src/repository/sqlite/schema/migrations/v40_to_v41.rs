//! CRM v40 -> v41 (app 0.6.6): source-bound, budgeted public-page receipts.
//! Preserve all prior facts without fabricating historical page content.

use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE discovery_public_page_reads (
           candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
           requested_url TEXT NOT NULL,
           search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
           call_number INTEGER NOT NULL CHECK(call_number > 0),
           status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
           source_excerpt TEXT NOT NULL CHECK(length(CAST(source_excerpt AS BLOB)) <= 16000),
           document_url TEXT,
           title TEXT NOT NULL,
           markdown_excerpt TEXT NOT NULL CHECK(length(CAST(markdown_excerpt AS BLOB)) <= 20000),
           truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
           stop_code TEXT,
           updated_at INTEGER NOT NULL,
           PRIMARY KEY(candidate_id, requested_url)
         );",
        )
        .map_err(|error| error.to_string())
}

fn remove_pages(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_public_page_reads;")
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v40(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_pages(connection)?;
    connection
        .pragma_update(None, "user_version", 40)
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v39(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_pages(connection)?;
    super::v39_to_v40::downgrade_reference_to_v39(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v38(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_pages(connection)?;
    super::v39_to_v40::downgrade_reference_to_v38(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v37(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_pages(connection)?;
    super::v39_to_v40::downgrade_reference_to_v37(connection)
}
