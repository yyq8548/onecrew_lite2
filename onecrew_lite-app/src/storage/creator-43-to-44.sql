CREATE TABLE discovery_content_reads (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 platform TEXT NOT NULL CHECK(platform='bilibili'),
 profile_ref TEXT NOT NULL,
 author_id TEXT NOT NULL,
 source_url TEXT NOT NULL,
 expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
 call_number INTEGER NOT NULL CHECK(call_number > 0),
 status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
 native_content_json TEXT CHECK(native_content_json IS NULL OR (json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB)) <= 16000)),
 stop_code TEXT,
 updated_at INTEGER NOT NULL,
 FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
   REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
 CHECK((status='reading' AND native_content_json IS NULL AND stop_code IS NULL)
   OR (status='completed' AND native_content_json IS NOT NULL AND stop_code IS NULL)
   OR (status='failed' AND native_content_json IS NULL AND stop_code IS NOT NULL))
);
