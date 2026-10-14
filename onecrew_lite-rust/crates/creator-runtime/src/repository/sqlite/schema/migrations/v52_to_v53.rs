//! CRM v52 -> v53 (app 0.6.19): audited increases to a Campaign's read budget.
//! Approved criteria, scopes, revision, evidence and spent calls are unchanged.
use rusqlite::Connection;

const SQL: &str = "CREATE TABLE discovery_budget_increases (
  id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
  operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  previous_max_calls INTEGER NOT NULL CHECK(previous_max_calls BETWEEN 1 AND 10000),
  new_max_calls INTEGER NOT NULL CHECK(new_max_calls BETWEEN 1 AND 10000 AND new_max_calls > previous_max_calls),
  used_calls_at_decision INTEGER NOT NULL CHECK(used_calls_at_decision >= 0),
  created_at INTEGER NOT NULL CHECK(created_at >= 0),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SQL)
        .map_err(|error| error.to_string())
}

/// Empty predecessor reference only; never a production downgrade.
pub(in crate::repository::sqlite) fn strip_budget_increases_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE discovery_budget_increases;")
        .map_err(|error| error.to_string())
}
