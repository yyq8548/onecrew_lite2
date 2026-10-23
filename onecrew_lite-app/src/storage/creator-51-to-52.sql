CREATE TABLE discovery_video_read_failures (
  search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
  call_number INTEGER NOT NULL CHECK(call_number > 0),
  stop_code TEXT NOT NULL CHECK(length(stop_code) BETWEEN 1 AND 128),
  updated_at INTEGER NOT NULL
);
