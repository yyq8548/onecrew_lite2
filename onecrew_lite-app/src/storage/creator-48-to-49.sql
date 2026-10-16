CREATE TABLE discovery_content_identity_bindings (
 search_hit_id TEXT PRIMARY KEY REFERENCES discovery_content_reads(search_hit_id) ON DELETE CASCADE,
 identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
 profile_search_hit_id TEXT NOT NULL REFERENCES discovery_profile_identity_matches(search_hit_id) ON DELETE CASCADE,
 account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
 created_at INTEGER NOT NULL,
 CHECK(search_hit_id=identity_search_hit_id AND search_hit_id=profile_search_hit_id)
);
