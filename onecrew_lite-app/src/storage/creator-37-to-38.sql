CREATE TABLE discovery_campaigns (
 id TEXT PRIMARY KEY,
 phase TEXT NOT NULL CHECK(phase IN ('draft','active','paused','completed','archived')),
 current_revision INTEGER NOT NULL CHECK(current_revision >= 1),
 target_count INTEGER NOT NULL CHECK(target_count >= 1),
 max_calls INTEGER NOT NULL CHECK(max_calls >= 1),
 used_calls INTEGER NOT NULL DEFAULT 0 CHECK(used_calls >= 0 AND used_calls <= max_calls),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL
);
CREATE INDEX discovery_campaigns_phase_updated ON discovery_campaigns(phase, updated_at DESC);
CREATE TABLE discovery_campaign_revisions (
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 revision INTEGER NOT NULL CHECK(revision >= 1),
 brief_json TEXT NOT NULL CHECK(json_valid(brief_json)),
 opencli_version TEXT NOT NULL,
 criteria_json TEXT NOT NULL CHECK(json_valid(criteria_json)),
 scope_json TEXT NOT NULL CHECK(json_valid(scope_json)),
 approved_at INTEGER,
 PRIMARY KEY(discovery_campaign_id, revision)
);
CREATE TABLE discovery_platform_runs (
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 platform TEXT NOT NULL,
 profile_ref TEXT NOT NULL,
 status TEXT NOT NULL CHECK(status IN ('unsearched','searching','completed','technical_stop','risk_control_stop','unauthorized','budget_exhausted')),
 cursor TEXT,
 active_query TEXT,
 calls_used INTEGER NOT NULL DEFAULT 0 CHECK(calls_used >= 0),
 stop_code TEXT,
 updated_at INTEGER NOT NULL,
 PRIMARY KEY(discovery_campaign_id, platform, profile_ref)
);
CREATE TABLE discovery_candidates (
 id TEXT PRIMARY KEY,
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
 platform TEXT NOT NULL,
 source_profile_ref TEXT NOT NULL,
 platform_account_id TEXT NOT NULL,
 display_name TEXT NOT NULL,
 profile_url TEXT NOT NULL,
 verdict TEXT NOT NULL CHECK(verdict IN ('pending','qualified','rejected')),
 evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 UNIQUE(discovery_campaign_id, platform, platform_account_id),
 FOREIGN KEY(discovery_campaign_id, platform, source_profile_ref)
   REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE
);
CREATE INDEX discovery_candidates_by_verdict ON discovery_candidates(discovery_campaign_id, verdict, updated_at DESC);
CREATE TABLE discovery_search_hits (
 id TEXT PRIMARY KEY,
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 platform TEXT NOT NULL,
 profile_ref TEXT NOT NULL,
 search_keyword TEXT NOT NULL,
 source_url TEXT NOT NULL,
 title TEXT NOT NULL,
 author_label TEXT NOT NULL,
 captured_at INTEGER NOT NULL,
 UNIQUE(discovery_campaign_id, platform, profile_ref, source_url),
 FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
   REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE
);
CREATE INDEX discovery_search_hits_by_campaign ON discovery_search_hits(discovery_campaign_id, platform, profile_ref, id);
CREATE TABLE discovery_pending_decisions (
 id TEXT PRIMARY KEY,
 discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
 expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
 kind TEXT NOT NULL,
 payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
 state TEXT NOT NULL CHECK(state IN ('pending','resolved')),
 created_at INTEGER NOT NULL,
 resolved_at INTEGER
);
CREATE INDEX discovery_pending_decisions_open ON discovery_pending_decisions(discovery_campaign_id, created_at) WHERE state = 'pending';
