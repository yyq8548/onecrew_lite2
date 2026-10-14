//! CRM v56 -> v57 (app 0.6.28): independent Library identity publication versions.
//! Existing sources, reviews, eligibility and budgets are preserved unchanged.
use rusqlite::Connection;

const SQL: &str = "CREATE TABLE discovery_library_identity_versions (
  discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
  source_hash TEXT NOT NULL CHECK(length(source_hash)=64 AND source_hash NOT GLOB '*[^0-9a-f]*'),
  identity_revision INTEGER NOT NULL CHECK(identity_revision BETWEEN 1 AND 9007199254740991),
  identity_hash TEXT NOT NULL CHECK(length(identity_hash)=64 AND identity_hash NOT GLOB '*[^0-9a-f]*'),
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SQL)
        .map_err(|error| error.to_string())
}

/// Empty reference only; never a production downgrade or data-erasure API.
pub(in crate::repository::sqlite) fn strip_library_identity_versions_reference(
    connection: &Connection,
) -> Result<(), String> {
    super::v58_to_v59::strip_language_samples_reference(connection)?;
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_library_identity_versions;")
        .map_err(|error| error.to_string())
}
