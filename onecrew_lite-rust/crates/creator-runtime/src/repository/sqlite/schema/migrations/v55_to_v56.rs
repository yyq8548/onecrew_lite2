//! CRM v55 -> v56 (app 0.6.27): append-only source-bound identity review ledger.
//! No existing account, qualification, source or target is merged or backfilled.
use rusqlite::Connection;

const SQL: &str = "CREATE TABLE discovery_identity_reviews (
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  decision_id TEXT NOT NULL CHECK(length(decision_id) BETWEEN 1 AND 128),
  operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  kind TEXT NOT NULL CHECK(kind IN ('same_author','different_authors','insufficient','revoke')),
  left_candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  right_candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  material_hash TEXT NOT NULL CHECK(length(material_hash)=64),
  request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
  input_json TEXT NOT NULL CHECK(json_valid(input_json) AND json_type(input_json)='object' AND length(CAST(input_json AS BLOB)) <= 65536),
  material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object' AND length(CAST(material_json AS BLOB)) <= 3145728),
  receipt_json TEXT NOT NULL CHECK(json_valid(receipt_json) AND json_type(receipt_json)='object' AND length(CAST(receipt_json AS BLOB)) <= 65536),
  revokes_decision_id TEXT,
  created_at INTEGER NOT NULL CHECK(created_at >= 0),
  PRIMARY KEY(discovery_campaign_id, decision_id),
  CHECK(left_candidate_id <> right_candidate_id),
  CHECK((kind='revoke' AND revokes_decision_id IS NOT NULL AND length(revokes_decision_id) BETWEEN 1 AND 128 AND revokes_decision_id<>decision_id)
    OR (kind<>'revoke' AND revokes_decision_id IS NULL)),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE,
  FOREIGN KEY(discovery_campaign_id, revokes_decision_id)
    REFERENCES discovery_identity_reviews(discovery_campaign_id, decision_id)
);
CREATE UNIQUE INDEX discovery_identity_reviews_one_revocation
  ON discovery_identity_reviews(discovery_campaign_id, revokes_decision_id)
  WHERE kind='revoke';
CREATE INDEX discovery_identity_reviews_campaign_revision
  ON discovery_identity_reviews(discovery_campaign_id, criteria_revision, created_at);
CREATE TRIGGER discovery_identity_reviews_no_update
  BEFORE UPDATE ON discovery_identity_reviews
  BEGIN SELECT RAISE(ABORT, 'identity_review_is_immutable'); END;";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SQL)
        .map_err(|error| error.to_string())
}

/// Empty reference only, never a production downgrade or ledger erasure API.
pub(in crate::repository::sqlite) fn strip_identity_reviews_reference(
    connection: &Connection,
) -> Result<(), String> {
    connection
        .execute_batch("DROP TABLE IF EXISTS discovery_identity_reviews;")
        .map_err(|error| error.to_string())
}
