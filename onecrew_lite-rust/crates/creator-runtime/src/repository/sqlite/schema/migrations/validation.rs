use rusqlite::Connection;

use crate::repository::sqlite::schema::{CURRENT_SCHEMA_VERSION, user_version};

/// Historical reference only; never called by production upgrades.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v32(
    connection: &Connection,
) -> Result<(), String> {
    super::strip_content_reference(connection)?;
    super::downgrade_reference_to_v33(connection)?;
    if user_version(connection)? != 33 {
        return Ok(());
    }
    connection.execute_batch(r#"PRAGMA foreign_keys = OFF;
DROP TABLE tiktok_one_account_bindings;
CREATE TABLE platform_accounts_v33_copy AS SELECT * FROM platform_accounts;
DROP TABLE platform_accounts;
CREATE TABLE platform_accounts (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube')),
               target_kind TEXT NOT NULL CHECK(target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               external_id TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(platform, target_kind, external_id)
             );
INSERT INTO platform_accounts SELECT * FROM platform_accounts_v33_copy;
DROP TABLE platform_accounts_v33_copy;
CREATE TABLE account_refs_v33_copy AS SELECT * FROM account_refs;
DROP TABLE account_refs;
CREATE TABLE account_refs (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube')),
               kind TEXT NOT NULL CHECK(kind IN ('profile','service_credential')),
               label TEXT NOT NULL,
               active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
               availability TEXT NOT NULL DEFAULT 'available'
                 CHECK(availability IN ('available','unavailable')),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
INSERT INTO account_refs SELECT * FROM account_refs_v33_copy;
DROP TABLE account_refs_v33_copy;
CREATE TABLE observation_sources_v33_copy AS SELECT * FROM observation_sources;
DROP TABLE observation_sources;
CREATE TABLE observation_sources (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN (
                 'initial_platform_account','profile_bio','profile_website','profile_location',
                 'video_description','channel_description','link_hub','manual_business_email',
                 'community_administrator','manual_entry','contact_route_resolution',
                 'creator_import'
               )),
               platform TEXT CHECK(platform IS NULL OR platform IN ('x','instagram','telegram','youtube')),
               source_url TEXT,
               external_ref TEXT,
               account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               metadata_json TEXT NOT NULL DEFAULT '{}',
               observed_at INTEGER NOT NULL
             );
INSERT INTO observation_sources SELECT * FROM observation_sources_v33_copy;
DROP TABLE observation_sources_v33_copy;
CREATE UNIQUE INDEX unique_creator_import_source
               ON observation_sources(external_ref) WHERE kind = 'creator_import';
CREATE TABLE resolved_targets_v33_copy AS SELECT * FROM resolved_targets;
DROP TABLE resolved_targets;
CREATE TABLE resolved_targets (
               id TEXT PRIMARY KEY,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube')),
               target_kind TEXT NOT NULL CHECK(target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               external_target_id TEXT NOT NULL,
               platform_account_id TEXT REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               metadata_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(account_ref_id, platform, target_kind, external_target_id)
             );
INSERT INTO resolved_targets SELECT * FROM resolved_targets_v33_copy;
DROP TABLE resolved_targets_v33_copy;
CREATE TABLE creator_candidate_snapshots_v33_copy AS SELECT * FROM creator_candidate_snapshots;
DROP TABLE creator_candidate_snapshots;
CREATE TABLE creator_candidate_snapshots (
               discovery_id TEXT NOT NULL CHECK(length(trim(discovery_id)) > 0),
               candidate_id TEXT NOT NULL CHECK(length(trim(candidate_id)) > 0),
               origin_kind TEXT NOT NULL CHECK(origin_kind IN ('campaign_discovery','community_administrator','ad_hoc_search')),
               origin_json TEXT NOT NULL CHECK(json_valid(origin_json) AND json_type(origin_json) = 'object'),
               candidate_json TEXT NOT NULL CHECK(json_valid(candidate_json) AND json_type(candidate_json) = 'object'),
               account_platform TEXT NOT NULL CHECK(account_platform IN ('x','instagram','telegram','youtube')),
               account_target_kind TEXT NOT NULL CHECK(account_target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               account_external_id TEXT NOT NULL CHECK(length(trim(account_external_id)) > 0),
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
                 AND json_type(candidate_json, '$.account.external_id') = 'text'
                 AND json_extract(candidate_json, '$.account.external_id') = account_external_id),
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
INSERT INTO creator_candidate_snapshots SELECT * FROM creator_candidate_snapshots_v33_copy;
DROP TABLE creator_candidate_snapshots_v33_copy;
CREATE INDEX creator_candidate_snapshots_by_campaign
               ON creator_candidate_snapshots(campaign_id, updated_at DESC);
CREATE TABLE contact_locators_v33_copy AS SELECT * FROM contact_locators;
DROP TABLE contact_locators;
CREATE TABLE contact_locators (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','website','link_hub')),
               kind TEXT NOT NULL CHECK(kind IN ('email_address','handle','url','invite_link','channel_id')),
               normalized_value TEXT NOT NULL,
               display_value TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(channel, kind, normalized_value)
             );
INSERT INTO contact_locators SELECT * FROM contact_locators_v33_copy;
DROP TABLE contact_locators_v33_copy;
CREATE TABLE conversations_v33_copy AS SELECT * FROM conversations;
DROP TABLE conversations;
CREATE TABLE conversations (
               id TEXT PRIMARY KEY,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               resolved_target_id TEXT NOT NULL REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','website','link_hub')),
               provider_thread_id TEXT NOT NULL,
               cursor TEXT,
               unread_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(account_ref_id, resolved_target_id, channel, provider_thread_id)
             );
INSERT INTO conversations SELECT * FROM conversations_v33_copy;
DROP TABLE conversations_v33_copy;
PRAGMA user_version = 32;
PRAGMA foreign_keys = ON;"#).map_err(|error| error.to_string())
}

/// Transform the empty current reference schema into the immediate v31
/// predecessor by removing the Phase C auth-connection and mailbox mapping
/// tables. This is test-only and never a production downgrade path.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v31(
    connection: &Connection,
) -> Result<(), String> {
    downgrade_reference_to_v32(connection)?;
    let version = user_version(connection)?;
    if version == 31 {
        return Ok(());
    }
    if version != 32 {
        return Ok(());
    }
    let non_local_provider_rows = connection
        .query_row(
            "SELECT COUNT(*) FROM mailbox_refs WHERE provider NOT IN ('resend')",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(|error| error.to_string())?;
    if non_local_provider_rows != 0 {
        return Err("cannot build a v31 reference while non-Resend mailboxes exist".into());
    }
    connection
        .execute_batch(
            "PRAGMA foreign_keys = OFF;
             DROP INDEX mailbox_identity_mappings_by_connection;
             DROP TABLE mailbox_identity_mappings;
             DROP TABLE email_auth_connections;
             CREATE TABLE mailbox_refs_rebuild (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL CHECK(provider = 'resend'),
               label TEXT NOT NULL,
               email_address TEXT NOT NULL,
               display_name TEXT,
               capabilities_json TEXT NOT NULL,
               available INTEGER NOT NULL,
               last_error TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             INSERT INTO mailbox_refs_rebuild
               (id, provider, label, email_address, display_name, capabilities_json,
                available, last_error, created_at, updated_at)
             SELECT id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at
             FROM mailbox_refs;
             DROP TABLE mailbox_refs;
             CREATE TABLE mailbox_refs (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL CHECK(provider = 'resend'),
               label TEXT NOT NULL,
               email_address TEXT NOT NULL,
               display_name TEXT,
               capabilities_json TEXT NOT NULL,
               available INTEGER NOT NULL,
               last_error TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             INSERT INTO mailbox_refs
               (id, provider, label, email_address, display_name, capabilities_json,
                available, last_error, created_at, updated_at)
             SELECT id, provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at
             FROM mailbox_refs_rebuild;
             DROP TABLE mailbox_refs_rebuild;
             PRAGMA foreign_keys = ON;
             PRAGMA user_version = 31;",
        )
        .map_err(|error| error.to_string())
}

/// Transform the empty current reference schema into the immediate v30
/// predecessor by restoring the legacy task-run origin CHECK and invariant.
/// This helper is test-only and mirrors the v30 -> v31 table rebuild without
/// introducing a production downgrade path.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v30(
    connection: &Connection,
) -> Result<(), String> {
    let version = user_version(connection)?;
    if version == 30 {
        return Ok(());
    }
    if version > 31 && version <= CURRENT_SCHEMA_VERSION {
        downgrade_reference_to_v31(connection)?;
    }
    if user_version(connection)? != 31 {
        return Ok(());
    }
    connection
        .execute_batch(
            "PRAGMA legacy_alter_table = ON;
             DROP INDEX reply_triage_one_per_email;
             DROP INDEX reply_triage_one_per_message;
             DROP INDEX reply_triage_open;
             ALTER TABLE reply_triage RENAME TO reply_triage_v31;
             DROP INDEX task_attention_open;
             DROP INDEX task_attention_recency;
             DROP INDEX task_attention_by_task_recency;
             ALTER TABLE task_attention RENAME TO task_attention_v31;
             DROP INDEX task_runs_claim_key;
             DROP INDEX one_open_task_run_per_task;
             DROP INDEX task_runs_by_task_recency;
             DROP INDEX task_runs_task_identity;
             ALTER TABLE task_runs RENAME TO task_runs_v31;
             CREATE TABLE task_runs (
               id TEXT PRIMARY KEY,
               task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
               scheduled_for INTEGER NOT NULL,
               state TEXT NOT NULL CHECK(state IN ('claimed','reported_done','reported_failed','expired')),
               idempotency_key TEXT NOT NULL,
               claimed_at INTEGER NOT NULL,
               lease_expires_at INTEGER NOT NULL,
               report_summary TEXT,
               evidence_json TEXT CHECK(evidence_json IS NULL OR (json_valid(evidence_json) AND json_type(evidence_json) = 'object')),
               finished_at INTEGER,
               origin TEXT NOT NULL CHECK(origin IN ('envoy','dispatcher','external')),
               chat_session_id TEXT,
               CHECK(origin = 'envoy' OR chat_session_id IS NULL),
               CHECK((state = 'claimed' AND report_summary IS NULL AND evidence_json IS NULL AND finished_at IS NULL)
                  OR (state = 'expired' AND report_summary IS NULL AND evidence_json IS NULL AND finished_at IS NOT NULL)
                  OR (state IN ('reported_done','reported_failed') AND report_summary IS NOT NULL AND evidence_json IS NOT NULL AND finished_at IS NOT NULL))
             );
             INSERT INTO task_runs
               (id, task_id, scheduled_for, state, idempotency_key, claimed_at,
                lease_expires_at, report_summary, evidence_json, finished_at, origin,
                chat_session_id)
             SELECT id, task_id, scheduled_for, state, idempotency_key, claimed_at,
                    lease_expires_at, report_summary, evidence_json, finished_at,
                    origin, chat_session_id
             FROM task_runs_v31;
             DROP TABLE task_runs_v31;
             CREATE UNIQUE INDEX task_runs_claim_key ON task_runs(idempotency_key);
             CREATE UNIQUE INDEX one_open_task_run_per_task
               ON task_runs(task_id) WHERE state = 'claimed';
             CREATE INDEX task_runs_by_task_recency
               ON task_runs(task_id, claimed_at DESC);
             CREATE UNIQUE INDEX task_runs_task_identity
               ON task_runs(task_id, id);
             CREATE TABLE reply_triage (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','dm')),
               email_id TEXT REFERENCES emails(id) ON DELETE CASCADE,
               message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
               intent TEXT NOT NULL CHECK(intent IN ('interested','question','negotiating','declined','irrelevant','other')),
               summary TEXT NOT NULL,
               suggested_reply TEXT,
               task_run_id TEXT REFERENCES task_runs(id) ON DELETE SET NULL,
               created_at INTEGER NOT NULL,
               acknowledged_at INTEGER,
               CHECK((channel = 'email' AND email_id IS NOT NULL AND message_id IS NULL)
                  OR (channel = 'dm' AND message_id IS NOT NULL AND email_id IS NULL))
             );
             INSERT INTO reply_triage
               (id, channel, email_id, message_id, intent, summary, suggested_reply, task_run_id,
                created_at, acknowledged_at)
             SELECT id, channel, email_id, message_id, intent, summary, suggested_reply, task_run_id,
                    created_at, acknowledged_at
             FROM reply_triage_v31;
             DROP TABLE reply_triage_v31;
             CREATE UNIQUE INDEX reply_triage_one_per_email ON reply_triage(email_id) WHERE email_id IS NOT NULL;
             CREATE UNIQUE INDEX reply_triage_one_per_message ON reply_triage(message_id) WHERE message_id IS NOT NULL;
             CREATE INDEX reply_triage_open ON reply_triage(created_at DESC) WHERE acknowledged_at IS NULL;
             CREATE TABLE task_attention (
               sequence INTEGER PRIMARY KEY AUTOINCREMENT,
               id TEXT NOT NULL UNIQUE,
               task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
               kind TEXT NOT NULL CHECK(kind IN ('run_failed','dispatch_blocked')),
               task_run_id TEXT,
               occurrence_key TEXT NOT NULL UNIQUE CHECK(length(occurrence_key) BETWEEN 1 AND 192),
               reason_code TEXT CHECK(reason_code IS NULL OR (
                 length(reason_code) BETWEEN 1 AND 64 AND reason_code NOT GLOB '*[^a-z0-9_]*' AND substr(reason_code, 1, 1) BETWEEN 'a' AND 'z')),
               message TEXT CHECK(message IS NULL OR (
                 length(trim(message)) BETWEEN 1 AND 512
               )),
               occurred_at INTEGER NOT NULL CHECK(occurred_at >= 0),
               resolved_at INTEGER CHECK(resolved_at IS NULL OR resolved_at >= occurred_at),
               acknowledged_at INTEGER CHECK(acknowledged_at IS NULL OR acknowledged_at >= occurred_at),
               FOREIGN KEY(task_id, task_run_id) REFERENCES task_runs(task_id, id) ON DELETE CASCADE,
               CHECK((kind = 'run_failed' AND task_run_id IS NOT NULL AND reason_code IS NULL AND message IS NULL)
                  OR (kind = 'dispatch_blocked' AND task_run_id IS NULL AND reason_code IS NOT NULL AND message IS NOT NULL
                      AND ((reason_code = 'codex_cli_missing' AND message = '未找到可用的 Codex CLI')
                        OR (reason_code = 'plugin_install_failed' AND message = 'Codex 插件安装或更新失败')
                        OR (reason_code = 'mcp_unavailable' AND message = '本地 MCP 未就绪')
                        OR (reason_code = 'session_start_failed' AND message = '任务会话启动失败'))))
             );
             INSERT INTO task_attention
               (sequence, id, task_id, kind, task_run_id, occurrence_key, reason_code, message,
                occurred_at, resolved_at, acknowledged_at)
             SELECT sequence, id, task_id, kind, task_run_id, occurrence_key, reason_code, message,
                    occurred_at, resolved_at, acknowledged_at
             FROM task_attention_v31;
             DROP TABLE task_attention_v31;
             CREATE INDEX task_attention_open ON task_attention(sequence DESC)
               WHERE acknowledged_at IS NULL AND resolved_at IS NULL;
             CREATE INDEX task_attention_recency ON task_attention(sequence DESC);
             CREATE INDEX task_attention_by_task_recency ON task_attention(task_id, sequence DESC);
             PRAGMA legacy_alter_table = OFF;
             PRAGMA user_version = 30;",
        )
        .map_err(|error| error.to_string())
}

/// Transform the empty current reference schema into the immediate v29
/// predecessor by removing only the additive v30 locator-ownership index.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v29(
    connection: &Connection,
) -> Result<(), String> {
    let version = user_version(connection)?;
    if !(29..=CURRENT_SCHEMA_VERSION).contains(&version) {
        return Ok(());
    }
    if version > 31 && version <= CURRENT_SCHEMA_VERSION {
        downgrade_reference_to_v31(connection)?;
    }
    if user_version(connection)? == 31 {
        downgrade_reference_to_v30(connection)?;
    }
    if user_version(connection)? == 30 {
        connection
            .execute_batch(
                "DROP INDEX creator_contact_routes_one_per_locator;
                 PRAGMA user_version = 29;",
            )
            .map_err(|error| error.to_string())?;
    }
    if user_version(connection)? == 29 {
        return Ok(());
    }
    Ok(())
}

/// Transform the empty current reference schema into the immediate v28
/// predecessor by removing only the additive v29 private continuation table
/// and indexes.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v28(
    connection: &Connection,
) -> Result<(), String> {
    let version = user_version(connection)?;
    if !(29..=CURRENT_SCHEMA_VERSION).contains(&version) {
        return Ok(());
    }
    if version > 29 {
        downgrade_reference_to_v29(connection)?;
    }
    connection
        .execute_batch(
            "DROP TABLE mailbox_sync_incremental_continuations;
             DROP INDEX creator_origins_by_creator_recency;
             DROP INDEX locator_observations_by_locator_recency;
             DROP INDEX locator_observations_by_creator_source;
             DROP INDEX locator_observations_by_platform_account_source;
             DROP INDEX touchpoints_by_creator_recency;
             DROP INDEX email_threads_by_creator;
             PRAGMA user_version = 28;",
        )
        .map_err(|error| error.to_string())
}

/// Transform the empty current reference schema through v28 into the earliest
/// supported predecessor. The frozen v27 -> v28 edge is additive, so this
/// removes only its empty group table, nullable Creator column, and index after
/// first removing the v29 indexes.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v27(
    connection: &Connection,
) -> Result<(), String> {
    let version = user_version(connection)?;
    if !(28..=CURRENT_SCHEMA_VERSION).contains(&version) {
        return Ok(());
    }
    let group_rows = connection
        .query_row("SELECT COUNT(*) FROM creator_groups", [], |row| {
            row.get::<_, u64>(0)
        })
        .map_err(|error| error.to_string())?;
    let assigned_rows = connection
        .query_row(
            "SELECT COUNT(*) FROM creators WHERE group_id IS NOT NULL",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map_err(|error| error.to_string())?;
    if group_rows != 0 || assigned_rows != 0 {
        return Err("cannot build a v27 reference while v28 Creator groups exist".into());
    }
    // Do not partially downgrade a v29 fixture when the v28 -> v27 preflight
    // above rejects it.
    downgrade_reference_to_v28(connection)?;
    connection
        .execute_batch(
            "DROP INDEX creators_by_group;
             ALTER TABLE creators DROP COLUMN group_id;
             DROP TABLE creator_groups;
             PRAGMA user_version = 27;",
        )
        .map_err(|error| error.to_string())
}

/// Transform the current reference schema into the immediate v35 predecessor
/// by restoring the frozen Campaign tables, the campaign-bound provenance
/// tables, and the campaign-attributed audit log from `reference_v35`. Rows
/// are carried over unchanged, so a fixture that already holds v36-only
/// provenance is rejected before any write instead of being partially
/// downgraded. Test-only reference reconstruction, never a production
/// downgrade path.
pub(in crate::repository::sqlite) fn downgrade_reference_to_v35(
    connection: &Connection,
) -> Result<(), String> {
    let version = user_version(connection)?;
    if version != CURRENT_SCHEMA_VERSION && version != 36 {
        return Ok(());
    }
    let origin_rows = connection.query_row(
        "SELECT COUNT(*) FROM creator_origins WHERE kind='platform_search' OR (kind='community_administrator' AND source_creator_id IS NULL)", [], |row| row.get::<_, u64>(0),
    ).map_err(|error| error.to_string())?;
    let snapshot_rows = if version == 36 {
        connection.query_row("SELECT COUNT(*) FROM creator_candidate_snapshots WHERE origin_kind='platform_search' OR source_kind='operator_community'", [], |row| row.get::<_, u64>(0)).map_err(|error| error.to_string())?
    } else {
        0
    };
    if origin_rows != 0 || snapshot_rows != 0 {
        return Err("cannot build a v35 reference while v36-only provenance rows exist".into());
    }
    super::downgrade_reference_to_v36(connection)?;
    connection
        .execute_batch(&format!(
            "PRAGMA foreign_keys = OFF;
             {campaign_objects}
             CREATE TABLE creator_candidate_snapshots_v36_copy AS
               SELECT * FROM creator_candidate_snapshots;
             DROP TABLE creator_candidate_snapshots;
             {snapshots}
             INSERT INTO creator_candidate_snapshots
               (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                account_platform, account_target_kind, account_external_id, account_handle,
                account_display_name, administrator_role, community_external_id, community_title,
                community_handle, community_target_kind, source_kind, campaign_id, account_ref_id,
                source_creator_id, source_contact_route_snapshot_id, source_contact_route_id,
                source_resolved_target_id, created_at, updated_at)
             SELECT discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                    account_platform, account_target_kind, account_external_id, account_handle,
                    account_display_name, administrator_role, community_external_id, community_title,
                    community_handle, community_target_kind, source_kind, NULL, account_ref_id,
                    source_creator_id, source_contact_route_snapshot_id, source_contact_route_id,
                    source_resolved_target_id, created_at, updated_at
             FROM creator_candidate_snapshots_v36_copy;
             DROP TABLE creator_candidate_snapshots_v36_copy;
             CREATE TABLE creator_origins_v36_copy AS SELECT * FROM creator_origins;
             DROP TABLE creator_origins;
             {origins}
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, campaign_id, source_creator_id,
                source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                administrator_role, community_external_id, community_title, community_handle,
                community_target_kind, import_content_sha256, imported_creator_id, discovered_at)
             SELECT id, kind, creator_id, source_id, discovery_id, NULL, source_creator_id,
                    source_contact_route_id, operator_account_ref_id, source_resolved_target_id,
                    administrator_role, community_external_id, community_title, community_handle,
                    community_target_kind, import_content_sha256, imported_creator_id, discovered_at
             FROM creator_origins_v36_copy;
             DROP TABLE creator_origins_v36_copy;
             CREATE TABLE audit_events_v36_copy AS SELECT * FROM audit_events;
             DROP TABLE audit_events;
             {audit}
             INSERT INTO audit_events
               (id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                contact_locator_id, contact_route_id, email_id, campaign_id, payload_json,
                created_at)
             SELECT id, kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id,
                    contact_locator_id, contact_route_id, email_id, NULL, payload_json,
                    created_at
             FROM audit_events_v36_copy;
             DROP TABLE audit_events_v36_copy;
             PRAGMA user_version = 35;
             PRAGMA foreign_keys = ON;",
            campaign_objects = super::reference_v35::V35_CAMPAIGN_OBJECTS_SQL,
            snapshots = super::reference_v35::V35_CREATOR_CANDIDATE_SNAPSHOTS_SQL,
            origins = super::reference_v35::V35_CREATOR_ORIGINS_SQL,
            audit = super::reference_v35::V35_AUDIT_EVENTS_SQL,
        ))
        .map_err(|error| error.to_string())
}
