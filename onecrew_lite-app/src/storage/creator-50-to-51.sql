CREATE TABLE discovery_evidence_reviews (
 id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
 operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
 discovery_campaign_id TEXT NOT NULL,
 criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
 search_hit_id TEXT NOT NULL REFERENCES discovery_video_identity_bindings(search_hit_id) ON DELETE CASCADE,
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
 profile_ref TEXT NOT NULL CHECK(length(profile_ref) BETWEEN 1 AND 256),
 request_id TEXT NOT NULL CHECK(length(trim(request_id)) BETWEEN 1 AND 128),
 request_hash TEXT NOT NULL CHECK(length(request_hash)=64 AND request_hash NOT GLOB '*[^0-9a-f]*'),
 material_hash TEXT NOT NULL CHECK(length(material_hash)=64 AND material_hash NOT GLOB '*[^0-9a-f]*'),
 material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object'
   AND json_extract(material_json,'$.format_version') IS 1 AND length(CAST(material_json AS BLOB)) <= 1048576),
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
CREATE INDEX discovery_evidence_reviews_by_candidate
 ON discovery_evidence_reviews(discovery_campaign_id, candidate_id, created_at, id);
