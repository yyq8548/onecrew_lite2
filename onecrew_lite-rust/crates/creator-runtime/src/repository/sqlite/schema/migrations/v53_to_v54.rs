//! CRM v53 -> v54 (app 0.6.21): source-bound review receipts for native X posts.
//! Existing v1 YouTube reviews and all Candidate/Campaign facts are copied unchanged.
use rusqlite::Connection;

const SQL: &str = "ALTER TABLE discovery_evidence_reviews RENAME TO discovery_evidence_reviews_v53;
             DROP INDEX discovery_evidence_reviews_by_candidate;
             CREATE TABLE discovery_evidence_reviews (
               id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
               operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
               discovery_campaign_id TEXT NOT NULL,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
               profile_ref TEXT NOT NULL CHECK(length(profile_ref) BETWEEN 1 AND 256),
               request_id TEXT NOT NULL CHECK(length(trim(request_id)) BETWEEN 1 AND 128),
               request_hash TEXT NOT NULL CHECK(length(request_hash)=64 AND request_hash NOT GLOB '*[^0-9a-f]*'),
               material_hash TEXT NOT NULL CHECK(length(material_hash)=64 AND material_hash NOT GLOB '*[^0-9a-f]*'),
               material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object'
                 AND coalesce(json_extract(material_json,'$.format_version'),0) IN (1,2) AND length(CAST(material_json AS BLOB)) <= 1048576),
               policy_version TEXT NOT NULL CHECK(length(trim(policy_version)) BETWEEN 1 AND 64),
               reviewer_json TEXT NOT NULL CHECK(json_valid(reviewer_json) AND json_type(reviewer_json)='object' AND length(CAST(reviewer_json AS BLOB)) <= 2048),
               findings_json TEXT NOT NULL CHECK(json_valid(findings_json) AND json_type(findings_json)='array' AND length(CAST(findings_json AS BLOB)) <= 65536),
               verdict TEXT NOT NULL CHECK(verdict IN ('pending','qualified','rejected')),
               assessment_json TEXT NOT NULL CHECK(json_valid(assessment_json) AND json_type(assessment_json)='object' AND length(CAST(assessment_json AS BLOB)) <= 16384),
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               UNIQUE(operator_account_id, discovery_campaign_id, request_id),
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             INSERT INTO discovery_evidence_reviews
               SELECT * FROM discovery_evidence_reviews_v53;
             DROP TABLE discovery_evidence_reviews_v53;
             CREATE INDEX discovery_evidence_reviews_by_candidate
               ON discovery_evidence_reviews(discovery_campaign_id, candidate_id, created_at, id);";

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(SQL)
        .map_err(|error| error.to_string())
}

/// Builds a v53 test reference while preserving v1 receipts; v2 has no v53 meaning.
pub(in crate::repository::sqlite) fn restore_review_v1_reference(
    connection: &Connection,
) -> Result<(), String> {
    let unsupported: i64 = connection
        .query_row(
            "SELECT count(*) FROM discovery_evidence_reviews
         WHERE json_extract(material_json,'$.format_version') IS NOT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if unsupported != 0 {
        return Err("review_reference_contains_v2".into());
    }
    connection
        .execute_batch(
            "ALTER TABLE discovery_evidence_reviews RENAME TO discovery_evidence_reviews_v54;
        DROP INDEX discovery_evidence_reviews_by_candidate;",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(super::v50_to_v51::EVIDENCE_REVIEW_V51_SQL)
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "INSERT INTO discovery_evidence_reviews
        SELECT * FROM discovery_evidence_reviews_v54;
        DROP TABLE discovery_evidence_reviews_v54;",
        )
        .map_err(|error| error.to_string())
}
