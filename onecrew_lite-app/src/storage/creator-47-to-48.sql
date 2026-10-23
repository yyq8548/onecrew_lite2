CREATE TABLE discovery_profile_identity_matches (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
 identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
 account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
 updated_at INTEGER NOT NULL,
 CHECK(search_hit_id=identity_search_hit_id)
);
