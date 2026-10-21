CREATE TABLE discovery_linkedin_experience_reads (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
  expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
  profile_ref TEXT NOT NULL,
  author_handle TEXT NOT NULL,
  account_id TEXT NOT NULL,
  profile_url TEXT NOT NULL,
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
  native_experience_json TEXT CHECK(native_experience_json IS NULL OR (json_valid(native_experience_json) AND json_type(native_experience_json)='object' AND length(CAST(native_experience_json AS BLOB))<=16000)),
  stop_code TEXT,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(discovery_campaign_id,expected_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE,
  CHECK((status='reading' AND native_experience_json IS NULL AND stop_code IS NULL) OR
    (status='completed' AND native_experience_json IS NOT NULL AND stop_code IS NULL) OR
    (status='failed' AND native_experience_json IS NULL AND stop_code IS NOT NULL))
);
