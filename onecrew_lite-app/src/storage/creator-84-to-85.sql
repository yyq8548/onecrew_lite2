CREATE TABLE discovery_search_positions (
 position INTEGER PRIMARY KEY,
 search_hit_id TEXT NOT NULL UNIQUE REFERENCES discovery_search_hits(id) ON DELETE CASCADE
);
