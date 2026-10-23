CREATE TABLE discovery_source_targets (
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL,
  source_platform TEXT NOT NULL,
  target_count INTEGER NOT NULL CHECK(target_count BETWEEN 1 AND 10000),
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(discovery_campaign_id,criteria_revision,source_platform),
  FOREIGN KEY(discovery_campaign_id,criteria_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE
);

CREATE TABLE discovery_web_author_pages (
  search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  discovery_campaign_id TEXT NOT NULL,
  criteria_revision INTEGER NOT NULL,
  requested_url TEXT NOT NULL,
  page_kind TEXT NOT NULL CHECK(page_kind IN ('article','author')),
  profile_ref TEXT NOT NULL,
  call_number INTEGER NOT NULL CHECK(call_number>0),
  status TEXT NOT NULL CHECK(status IN ('reading','awaiting_resolution','completed','failed')),
  material_json TEXT CHECK(material_json IS NULL OR (json_valid(material_json) AND json_type(material_json)='object')),
  resolution_json TEXT CHECK(resolution_json IS NULL OR (json_valid(resolution_json) AND json_type(resolution_json)='object')),
  stop_code TEXT,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY(search_hit_id,requested_url),
  FOREIGN KEY(discovery_campaign_id,criteria_revision) REFERENCES discovery_campaign_revisions(discovery_campaign_id,revision) ON DELETE CASCADE,
  CHECK((status='reading' AND resolution_json IS NULL AND stop_code IS NULL) OR
    (status='awaiting_resolution' AND material_json IS NOT NULL AND resolution_json IS NULL AND stop_code IS NULL) OR
    (status='completed' AND material_json IS NOT NULL AND resolution_json IS NOT NULL AND stop_code IS NULL) OR
    (status='failed' AND stop_code IS NOT NULL))
);

CREATE TABLE discovery_web_author_edges (
  native_search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  parent_search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  article_url TEXT NOT NULL,
  author_page_url TEXT,
  author_name TEXT NOT NULL,
  attribution_quote TEXT NOT NULL,
  native_source_url TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE(parent_search_hit_id,native_source_url)
);
