CREATE TABLE discovery_library_identity_versions (
  discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
  source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
  source_hash TEXT NOT NULL CHECK(length(source_hash)=64 AND source_hash NOT GLOB '*[^0-9a-f]*'),
  identity_revision INTEGER NOT NULL CHECK(identity_revision BETWEEN 1 AND 9007199254740991),
  identity_hash TEXT NOT NULL CHECK(length(identity_hash)=64 AND identity_hash NOT GLOB '*[^0-9a-f]*'),
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(discovery_campaign_id, criteria_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
