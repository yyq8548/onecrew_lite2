//! Frozen CRM v35 DDL (app 0.5.23) for every object the v35 -> v36 edge drops
//! or rebuilds: the five Campaign tables, the two campaign-bound provenance
//! tables, and the campaign-attributed audit log. Copied verbatim from the
//! released v35 schema so predecessor fixtures and the v35 validator are
//! reconstructed from this record rather than from the current schema. Never
//! an application downgrade API.

/// The five Campaign tables with their two lookup indexes.
pub(super) const V35_CAMPAIGN_OBJECTS_SQL: &str = r#"
             CREATE TABLE campaigns (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               status TEXT NOT NULL CHECK(status IN ('configuring','running','reviewing','archived')),
               goal TEXT NOT NULL,
               regions_json TEXT NOT NULL DEFAULT '[]',
               platforms_json TEXT NOT NULL DEFAULT '[]',
               keywords_json TEXT NOT NULL DEFAULT '[]',
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               revision INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE campaign_delete_operations (
               id TEXT PRIMARY KEY,
               campaign_id TEXT NOT NULL,
               plan_fingerprint TEXT NOT NULL,
               impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
               status TEXT NOT NULL CHECK(status IN ('prepared','completed')),
               result_json TEXT CHECK(result_json IS NULL OR (json_valid(result_json) AND json_type(result_json) = 'object')),
               expires_at INTEGER NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               CHECK((status = 'prepared' AND result_json IS NULL)
                  OR (status = 'completed' AND result_json IS NOT NULL))
             );
             CREATE INDEX campaign_delete_operations_lookup
               ON campaign_delete_operations(campaign_id, status, expires_at);
             CREATE TABLE campaign_account_bindings (
               id TEXT PRIMARY KEY,
               campaign_id TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               purpose TEXT NOT NULL,
               strategy_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL,
               UNIQUE(campaign_id, account_ref_id, purpose)
             );
             CREATE TABLE campaign_templates (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               goal TEXT NOT NULL,
               regions_json TEXT NOT NULL DEFAULT '[]',
               platforms_json TEXT NOT NULL DEFAULT '[]',
               keywords_json TEXT NOT NULL DEFAULT '[]',
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               revision INTEGER NOT NULL DEFAULT 1
             );
             CREATE TABLE campaign_template_delete_operations (
               id TEXT PRIMARY KEY,
               template_id TEXT NOT NULL,
               plan_fingerprint TEXT NOT NULL,
               impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
               status TEXT NOT NULL CHECK(status IN ('prepared','completed')),
               result_json TEXT CHECK(result_json IS NULL OR (json_valid(result_json) AND json_type(result_json) = 'object')),
               expires_at INTEGER NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               CHECK((status = 'prepared' AND result_json IS NULL)
                  OR (status = 'completed' AND result_json IS NOT NULL))
             );
             CREATE INDEX campaign_template_delete_operations_lookup
               ON campaign_template_delete_operations(template_id, status, expires_at);
"#;

/// `creator_candidate_snapshots` with campaign-bound provenance kinds and the
/// campaign lookup index.
pub(super) const V35_CREATOR_CANDIDATE_SNAPSHOTS_SQL: &str = r#"
             CREATE TABLE creator_candidate_snapshots (
               discovery_id TEXT NOT NULL CHECK(length(trim(discovery_id)) > 0),
               candidate_id TEXT NOT NULL CHECK(length(trim(candidate_id)) > 0),
               origin_kind TEXT NOT NULL CHECK(origin_kind IN ('campaign_discovery','community_administrator','ad_hoc_search')),
               origin_json TEXT NOT NULL CHECK(json_valid(origin_json) AND json_type(origin_json) = 'object'),
               candidate_json TEXT NOT NULL CHECK(json_valid(candidate_json) AND json_type(candidate_json) = 'object'),
               account_platform TEXT NOT NULL CHECK(account_platform IN ('x','instagram','telegram','youtube','tiktok')),
               account_target_kind TEXT NOT NULL CHECK(account_target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               account_external_id TEXT CHECK(
                 (account_external_id IS NOT NULL AND length(trim(account_external_id)) > 0)
                 OR (account_external_id IS NULL AND account_platform = 'tiktok'
                   AND account_handle IS NOT NULL AND length(trim(account_handle)) > 0)),
               account_handle TEXT CHECK(account_handle IS NULL OR length(trim(account_handle)) > 0),
               account_display_name TEXT NOT NULL CHECK(length(trim(account_display_name)) > 0),
               administrator_role TEXT CHECK(administrator_role IN ('owner','administrator')),
               community_external_id TEXT,
               community_title TEXT,
               community_handle TEXT,
               community_target_kind TEXT CHECK(community_target_kind IN ('channel','supergroup')),
               source_kind TEXT NOT NULL CHECK(source_kind IN ('campaign_discovery','campaign_community','creator_contact_route','ad_hoc_search')),
               campaign_id TEXT REFERENCES campaigns(id) ON DELETE CASCADE,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               source_creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               source_contact_route_snapshot_id TEXT
                 CHECK(source_contact_route_snapshot_id IS NULL
                   OR length(trim(source_contact_route_snapshot_id)) > 0),
               source_contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE SET NULL,
               source_resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(discovery_id, candidate_id),
               CHECK(json_type(candidate_json, '$.candidate_id') = 'text'
                 AND json_extract(candidate_json, '$.candidate_id') = candidate_id
                 AND json_type(candidate_json, '$.origin.discovery_id') = 'text'
                 AND json_extract(candidate_json, '$.origin.discovery_id') = discovery_id
                 AND json_type(candidate_json, '$.origin.kind') = 'text'
                 AND json_extract(candidate_json, '$.origin.kind') = origin_kind
                 AND json_type(candidate_json, '$.account.platform') = 'text'
                 AND json_extract(candidate_json, '$.account.platform') = account_platform
                 AND json_type(candidate_json, '$.account.target_kind') = 'text'
                 AND json_extract(candidate_json, '$.account.target_kind') = account_target_kind
                 AND json_type(candidate_json, '$.account.external_id') IS CASE WHEN account_external_id IS NULL THEN 'null' ELSE 'text' END
                 AND json_extract(candidate_json, '$.account.external_id') IS account_external_id),
               CHECK(
                 (origin_kind = 'campaign_discovery'
                   AND source_kind = 'campaign_discovery'
                   AND campaign_id IS NOT NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_snapshot_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND source_resolved_target_id IS NULL)
                 OR
                 (origin_kind = 'ad_hoc_search'
                   AND source_kind = 'ad_hoc_search'
                   AND campaign_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_snapshot_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND source_resolved_target_id IS NULL)
                 OR
                 (origin_kind = 'community_administrator'
                   AND administrator_role IS NOT NULL
                   AND community_external_id IS NOT NULL
                   AND length(trim(community_external_id)) > 0
                   AND community_title IS NOT NULL
                   AND length(trim(community_title)) > 0
                   AND (community_handle IS NULL OR length(trim(community_handle)) > 0)
                   AND community_target_kind IS NOT NULL
                   AND account_platform = 'telegram'
                   AND account_target_kind IN ('user','bot')
                   AND ((source_kind = 'campaign_community'
                     AND campaign_id IS NOT NULL
                     AND source_creator_id IS NULL
                     AND source_contact_route_snapshot_id IS NULL
                     AND source_contact_route_id IS NULL
                     AND source_resolved_target_id IS NULL)
                   OR (source_kind = 'creator_contact_route'
                     AND campaign_id IS NULL
                     AND source_creator_id IS NOT NULL
                     AND source_contact_route_snapshot_id IS NOT NULL
                     AND source_resolved_target_id IS NOT NULL))))
               ,CHECK(source_contact_route_id IS NULL
                 OR source_contact_route_id = source_contact_route_snapshot_id)
             );
             CREATE INDEX creator_candidate_snapshots_by_campaign
               ON creator_candidate_snapshots(campaign_id, updated_at DESC);
"#;

/// `creator_origins` with campaign-bound provenance kinds and its six indexes.
pub(super) const V35_CREATOR_ORIGINS_SQL: &str = r#"
             CREATE TABLE creator_origins (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('campaign_discovery','community_administrator','manual_entry','ad_hoc_search','creator_import')),
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               source_id TEXT NOT NULL REFERENCES observation_sources(id) ON DELETE RESTRICT,
               discovery_id TEXT CHECK(discovery_id IS NULL OR length(trim(discovery_id)) > 0),
               campaign_id TEXT REFERENCES campaigns(id) ON DELETE CASCADE,
               source_creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               source_contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE SET NULL,
               operator_account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               source_resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               administrator_role TEXT CHECK(administrator_role IN ('owner','administrator')),
               community_external_id TEXT,
               community_title TEXT,
               community_handle TEXT,
               community_target_kind TEXT CHECK(community_target_kind IN ('channel','supergroup')),
               import_content_sha256 TEXT REFERENCES creator_import_receipts(content_sha256) ON DELETE RESTRICT,
               imported_creator_id TEXT CHECK(imported_creator_id IS NULL OR length(trim(imported_creator_id)) > 0),
               discovered_at INTEGER NOT NULL,
               CHECK(
                 (kind = 'campaign_discovery'
                   AND campaign_id IS NOT NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND operator_account_ref_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL
                   AND discovery_id IS NOT NULL)
                 OR
                 (kind = 'community_administrator'
                   AND operator_account_ref_id IS NOT NULL
                   AND administrator_role IS NOT NULL
                   AND community_external_id IS NOT NULL
                   AND length(trim(community_external_id)) > 0
                   AND community_title IS NOT NULL
                   AND length(trim(community_title)) > 0
                   AND (community_handle IS NULL OR length(trim(community_handle)) > 0)
                   AND community_target_kind IS NOT NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL
                   AND ((campaign_id IS NOT NULL
                     AND source_creator_id IS NULL
                     AND source_contact_route_id IS NULL
                     AND source_resolved_target_id IS NULL)
                   OR (campaign_id IS NULL
                     AND source_creator_id IS NOT NULL
                     AND source_resolved_target_id IS NOT NULL))
                   AND discovery_id IS NOT NULL)
                 OR
                 (kind = 'manual_entry'
                   AND discovery_id IS NULL
                   AND campaign_id IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND operator_account_ref_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL)
                 OR
                 (kind = 'ad_hoc_search'
                   AND discovery_id IS NOT NULL
                   AND campaign_id IS NULL
                   AND operator_account_ref_id IS NOT NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NULL
                   AND imported_creator_id IS NULL)
                 OR
                 (kind = 'creator_import'
                   AND discovery_id IS NULL
                   AND campaign_id IS NULL
                   AND source_creator_id IS NULL
                   AND source_contact_route_id IS NULL
                   AND operator_account_ref_id IS NULL
                   AND source_resolved_target_id IS NULL
                   AND administrator_role IS NULL
                   AND community_external_id IS NULL
                   AND community_title IS NULL
                   AND community_handle IS NULL
                   AND community_target_kind IS NULL
                   AND import_content_sha256 IS NOT NULL
                   AND imported_creator_id IS NOT NULL)
               )
             );
             CREATE INDEX creator_origins_by_creator_recency
               ON creator_origins(creator_id, discovered_at DESC, source_id);
             CREATE UNIQUE INDEX unique_campaign_creator_origin
               ON creator_origins(campaign_id, discovery_id, creator_id)
               WHERE kind = 'campaign_discovery';
             CREATE UNIQUE INDEX unique_community_administrator_origin
               ON creator_origins(
                 source_creator_id, source_contact_route_id, operator_account_ref_id,
                 source_resolved_target_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator';
             CREATE UNIQUE INDEX unique_campaign_community_administrator_origin
               ON creator_origins(
                 campaign_id, operator_account_ref_id, community_external_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator' AND campaign_id IS NOT NULL;
             CREATE UNIQUE INDEX unique_manual_creator_origin
               ON creator_origins(creator_id) WHERE kind = 'manual_entry';
             CREATE UNIQUE INDEX unique_ad_hoc_search_origin
               ON creator_origins(operator_account_ref_id, discovery_id, creator_id)
               WHERE kind = 'ad_hoc_search';
             CREATE UNIQUE INDEX unique_creator_import_origin
               ON creator_origins(import_content_sha256, imported_creator_id)
               WHERE kind = 'creator_import';
"#;

/// `audit_events` with the nullable Campaign attribution column.
pub(super) const V35_AUDIT_EVENTS_SQL: &str = r#"
             CREATE TABLE audit_events (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               mailbox_ref_id TEXT REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               contact_locator_id TEXT REFERENCES contact_locators(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               email_id TEXT REFERENCES emails(id) ON DELETE RESTRICT,
               campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,
               payload_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL
             );
"#;
