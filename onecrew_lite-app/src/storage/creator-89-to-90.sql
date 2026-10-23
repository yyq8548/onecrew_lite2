CREATE TABLE discovery_query_rounds (
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL CHECK(revision > 0),
 round INTEGER NOT NULL CHECK(round > 0),
 operator_account_id TEXT NOT NULL,
 scopes_json TEXT NOT NULL CHECK(json_valid(scopes_json)),
 used_calls INTEGER NOT NULL CHECK(used_calls >= 0),
 created_at INTEGER NOT NULL,
 PRIMARY KEY(discovery_campaign_id,revision,round)
);
