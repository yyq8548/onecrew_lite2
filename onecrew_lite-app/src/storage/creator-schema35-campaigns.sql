CREATE TABLE campaigns (
 id TEXT PRIMARY KEY, name TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('configuring','running','reviewing','archived')),
 goal TEXT NOT NULL, regions_json TEXT NOT NULL DEFAULT '[]', platforms_json TEXT NOT NULL DEFAULT '[]',
 keywords_json TEXT NOT NULL DEFAULT '[]', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE campaign_delete_operations (
 id TEXT PRIMARY KEY, campaign_id TEXT NOT NULL, plan_fingerprint TEXT NOT NULL,
 impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
 status TEXT NOT NULL CHECK(status IN ('prepared','completed')),
 result_json TEXT CHECK(result_json IS NULL OR (json_valid(result_json) AND json_type(result_json) = 'object')),
 expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 CHECK((status = 'prepared' AND result_json IS NULL) OR (status = 'completed' AND result_json IS NOT NULL))
);
CREATE INDEX campaign_delete_operations_lookup ON campaign_delete_operations(campaign_id, status, expires_at);
CREATE TABLE campaign_account_bindings (
 id TEXT PRIMARY KEY, campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
 account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
 purpose TEXT NOT NULL, strategy_json TEXT NOT NULL DEFAULT '{}', created_at INTEGER NOT NULL,
 UNIQUE(campaign_id, account_ref_id, purpose)
);
CREATE TABLE campaign_templates (
 id TEXT PRIMARY KEY, name TEXT NOT NULL, goal TEXT NOT NULL,
 regions_json TEXT NOT NULL DEFAULT '[]', platforms_json TEXT NOT NULL DEFAULT '[]', keywords_json TEXT NOT NULL DEFAULT '[]',
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, revision INTEGER NOT NULL DEFAULT 1
);
CREATE TABLE campaign_template_delete_operations (
 id TEXT PRIMARY KEY, template_id TEXT NOT NULL, plan_fingerprint TEXT NOT NULL,
 impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
 status TEXT NOT NULL CHECK(status IN ('prepared','completed')),
 result_json TEXT CHECK(result_json IS NULL OR (json_valid(result_json) AND json_type(result_json) = 'object')),
 expires_at INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
 CHECK((status = 'prepared' AND result_json IS NULL) OR (status = 'completed' AND result_json IS NOT NULL))
);
CREATE INDEX campaign_template_delete_operations_lookup ON campaign_template_delete_operations(template_id, status, expires_at);
