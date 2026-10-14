//! CRM v58 -> v59 (app 0.6.31): bounded original-language sampling receipts.
//! Existing evidence and budgets remain unchanged.
use rusqlite::Connection;

pub(in crate::repository::sqlite) const SQL: &str = "CREATE TABLE discovery_language_samples (
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL,
  candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 3),
  call_number INTEGER NOT NULL CHECK(call_number >= 1),
  status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
  sample_json TEXT CHECK(sample_json IS NULL OR (json_valid(sample_json) AND json_type(sample_json)='object')),
  stop_code TEXT,
  created_at INTEGER NOT NULL,
  PRIMARY KEY(discovery_campaign_id,criteria_revision,candidate_id,ordinal),
  FOREIGN KEY(discovery_campaign_id,criteria_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision)
);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(SQL).map_err(|e| e.to_string())
}

pub(in crate::repository::sqlite) fn strip_language_samples_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_language_samples;")
        .map_err(|e| e.to_string())
}
