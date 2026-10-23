CREATE TABLE discovery_recommendation_policies (
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  policy TEXT NOT NULL CHECK(policy IN ('legacy','score_and_contact_v1')),
  PRIMARY KEY(discovery_campaign_id, criteria_revision),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
INSERT INTO discovery_recommendation_policies
  SELECT discovery_campaign_id, revision, 'legacy' FROM discovery_campaign_revisions;
CREATE TABLE discovery_contact_observations (
  candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 128),
  kind TEXT NOT NULL CHECK(kind IN ('email','telegram')),
  original_value TEXT NOT NULL CHECK(length(original_value) BETWEEN 1 AND 2048),
  normalized_value TEXT NOT NULL CHECK(length(normalized_value) BETWEEN 1 AND 2048),
  source_url TEXT NOT NULL CHECK(length(source_url) BETWEEN 1 AND 2048),
  source_excerpt TEXT NOT NULL CHECK(length(CAST(source_excerpt AS BLOB)) BETWEEN 1 AND 4096),
  captured_at INTEGER NOT NULL CHECK(captured_at > 0),
  attribution TEXT NOT NULL CHECK(attribution IN ('author_published','unconfirmed')),
  PRIMARY KEY(candidate_id, criteria_revision, normalized_value, source_url),
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
CREATE INDEX discovery_contacts_author_revision
  ON discovery_contact_observations(discovery_campaign_id, criteria_revision, native_author_id);
