CREATE TABLE discovery_public_page_reads (
 candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
 requested_url TEXT NOT NULL,
 search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
 call_number INTEGER NOT NULL CHECK(call_number > 0),
 status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
 source_excerpt TEXT NOT NULL CHECK(length(CAST(source_excerpt AS BLOB)) <= 16000),
 document_url TEXT,
 title TEXT NOT NULL,
 markdown_excerpt TEXT NOT NULL CHECK(length(CAST(markdown_excerpt AS BLOB)) <= 20000),
 truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
 stop_code TEXT,
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(candidate_id, requested_url)
);
