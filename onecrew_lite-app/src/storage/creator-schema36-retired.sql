CREATE TABLE tiktok_one_account_bindings (
 account_ref_id TEXT PRIMARY KEY REFERENCES account_refs(id) ON DELETE CASCADE,
 connection_id TEXT NOT NULL CHECK(length(trim(connection_id)) > 0),
 cloud_account_id TEXT NOT NULL CHECK(length(trim(cloud_account_id)) > 0),
 installation_id TEXT NOT NULL CHECK(length(trim(installation_id)) > 0),
 access_user_id TEXT NOT NULL CHECK(length(trim(access_user_id)) > 0),
 created_at INTEGER NOT NULL,
 updated_at INTEGER NOT NULL,
 UNIQUE(installation_id, access_user_id, connection_id, cloud_account_id)
);
CREATE TABLE creator_candidate_snapshots (
 discovery_id TEXT NOT NULL CHECK(length(trim(discovery_id)) > 0),
 candidate_id TEXT NOT NULL CHECK(length(trim(candidate_id)) > 0),
 origin_kind TEXT NOT NULL CHECK(origin_kind IN ('platform_search','community_administrator')),
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
 source_kind TEXT NOT NULL CHECK(source_kind IN ('platform_search','operator_community','creator_contact_route')),
 account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
 source_creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
 source_contact_route_snapshot_id TEXT
   CHECK(source_contact_route_snapshot_id IS NULL OR length(trim(source_contact_route_snapshot_id)) > 0),
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
   (origin_kind = 'platform_search'
     AND source_kind = 'platform_search'
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
     AND ((source_kind = 'operator_community'
       AND source_creator_id IS NULL
       AND source_contact_route_snapshot_id IS NULL
       AND source_contact_route_id IS NULL
       AND source_resolved_target_id IS NULL)
     OR (source_kind = 'creator_contact_route'
       AND source_creator_id IS NOT NULL
       AND source_contact_route_snapshot_id IS NOT NULL
       AND source_resolved_target_id IS NOT NULL))))
 ,CHECK(source_contact_route_id IS NULL OR source_contact_route_id = source_contact_route_snapshot_id)
);
