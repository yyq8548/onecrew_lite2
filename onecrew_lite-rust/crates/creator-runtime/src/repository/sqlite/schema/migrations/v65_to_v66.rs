//! CRM v65 -> v66 (app 0.6.55): durable score-gated contact lookups.
use rusqlite::Connection;
pub(in crate::repository::sqlite::schema) const SQL: &str = r#"
CREATE TABLE discovery_contact_lookups (
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 discovery_campaign_id TEXT NOT NULL,
 criteria_revision INTEGER NOT NULL,
 method TEXT NOT NULL CHECK(method IN ('maigret','search')),
 receipt_json TEXT NOT NULL CHECK(json_valid(receipt_json)),
 PRIMARY KEY(candidate_id,criteria_revision,method),
 FOREIGN KEY(discovery_campaign_id,criteria_revision)
 REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE
);
"#;
pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(SQL).map_err(|e| e.to_string())
}
pub(in crate::repository::sqlite) fn strip_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_contact_lookups;")
        .map_err(|e| e.to_string())
}
