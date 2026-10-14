//! CRM v41 -> v42 (app 0.6.7): observed author identity on discovery search hits.
//! Preserve every historical hit and checkpoint; never infer missing authors.
use rusqlite::Connection;

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "CREATE TABLE discovery_search_authors (
           search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
           author_id TEXT NOT NULL CHECK(length(author_id) BETWEEN 1 AND 100),
           author_url TEXT NOT NULL CHECK(length(author_url) BETWEEN 1 AND 2048),
           observed_at INTEGER NOT NULL
         );",
        )
        .map_err(|error| error.to_string())
}

fn remove_authors(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_search_authors;")
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v41(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_authors(connection)?;
    connection
        .pragma_update(None, "user_version", 41)
        .map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v40(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_authors(connection)?;
    super::v40_to_v41::downgrade_reference_to_v40(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v39(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_authors(connection)?;
    super::v40_to_v41::downgrade_reference_to_v39(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v38(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_authors(connection)?;
    super::v40_to_v41::downgrade_reference_to_v38(connection)
}

pub(in crate::repository::sqlite) fn downgrade_reference_to_v37(
    connection: &Connection,
) -> Result<(), String> {
    if super::super::user_version(connection)? != super::super::CURRENT_SCHEMA_VERSION {
        return Ok(());
    }
    remove_authors(connection)?;
    super::v40_to_v41::downgrade_reference_to_v37(connection)
}
