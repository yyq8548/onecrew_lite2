CREATE TABLE discovery_search_authors (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
 author_id TEXT NOT NULL CHECK(length(author_id) BETWEEN 1 AND 100),
 author_url TEXT NOT NULL CHECK(length(author_url) BETWEEN 1 AND 2048),
 observed_at INTEGER NOT NULL
);
