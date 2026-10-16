CREATE TABLE discovery_video_identity_bindings (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_video_completions(search_hit_id) ON DELETE CASCADE,
 discovery_campaign_id TEXT NOT NULL,
 criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
 created_at INTEGER NOT NULL,
 FOREIGN KEY(discovery_campaign_id, criteria_revision)
   REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
CREATE TABLE discovery_library_source_versions (
 discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
 source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
 snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64 AND snapshot_hash NOT GLOB '*[^0-9a-f]*'),
 updated_at INTEGER NOT NULL,
 FOREIGN KEY(discovery_campaign_id, criteria_revision)
   REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
);
