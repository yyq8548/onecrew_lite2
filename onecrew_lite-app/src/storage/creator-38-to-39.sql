CREATE TABLE discovery_search_completions (
 discovery_campaign_id TEXT NOT NULL,
 platform TEXT NOT NULL,
 profile_ref TEXT NOT NULL,
 search_keyword TEXT NOT NULL,
 completed_at INTEGER NOT NULL,
 PRIMARY KEY(discovery_campaign_id, platform, profile_ref, search_keyword),
 FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
   REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE
);
CREATE TABLE discovery_video_completions (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
 completed_at INTEGER NOT NULL
);
INSERT INTO discovery_search_completions
 SELECT discovery_campaign_id,platform,profile_ref,search_keyword,MIN(captured_at)
 FROM discovery_search_hits
 GROUP BY discovery_campaign_id,platform,profile_ref,search_keyword;
INSERT INTO discovery_video_completions
 SELECT h.id,MIN(c.updated_at) FROM discovery_search_hits h
 JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
   AND c.platform=h.platform
 JOIN json_each(c.evidence_json, '$.source_urls') s ON s.value=h.source_url
 WHERE h.platform='youtube' GROUP BY h.id;
