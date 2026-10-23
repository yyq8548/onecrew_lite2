CREATE TABLE discovery_facebook_content_targets (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL,
  platform TEXT NOT NULL CHECK(platform='facebook'),
  profile_ref TEXT NOT NULL,
  author_id TEXT NOT NULL CHECK(length(author_id) BETWEEN 1 AND 20 AND author_id NOT GLOB '*[^0-9]*' AND substr(author_id,1,1)!='0'),
  profile_url TEXT NOT NULL CHECK(profile_url='https://www.facebook.com/profile.php?id=' || author_id),
  expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
  source_urls_json TEXT CHECK(source_urls_json IS NULL OR (json_valid(source_urls_json) AND json_type(source_urls_json)='array' AND json_array_length(source_urls_json) BETWEEN 1 AND 3 AND length(CAST(source_urls_json AS BLOB)) <= 1024)),
  stop_code TEXT,
  updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
  FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
    REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
  FOREIGN KEY(discovery_campaign_id, expected_revision)
    REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE,
  CHECK((status='reading' AND source_urls_json IS NULL AND stop_code IS NULL)
    OR (status='completed' AND source_urls_json IS NOT NULL AND stop_code IS NULL)
    OR (status='failed' AND source_urls_json IS NULL AND stop_code IS NOT NULL))
);
