CREATE TABLE discovery_transcript_reads (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 call_number INTEGER NOT NULL CHECK(call_number > 0),
 status TEXT NOT NULL CHECK(status IN ('reading','completed','unavailable','failed')),
 segments_json TEXT NOT NULL CHECK(json_valid(segments_json) AND length(CAST(segments_json AS BLOB)) <= 24000),
 truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
 stop_code TEXT,
 updated_at INTEGER NOT NULL
);
