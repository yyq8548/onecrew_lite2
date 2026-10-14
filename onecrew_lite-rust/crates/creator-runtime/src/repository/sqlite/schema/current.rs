use rusqlite::Connection;

#[cfg(test)]
pub(in crate::repository::sqlite) const NO_SHARED_CONTACT_ROUTE_TRIGGERS_SQL: &str = "
CREATE TRIGGER prevent_shared_creator_contact_locator_insert
BEFORE INSERT ON creator_contact_routes
WHEN NOT EXISTS (
  SELECT 1 FROM creator_contact_routes existing
  WHERE existing.locator_id = NEW.locator_id
    AND existing.creator_id = NEW.creator_id
)
AND EXISTS (
  SELECT 1 FROM creator_contact_routes existing
  WHERE existing.locator_id = NEW.locator_id
    AND existing.creator_id != NEW.creator_id
)
BEGIN
  SELECT RAISE(ABORT, 'contact route already belongs to another Creator');
END;
CREATE TRIGGER prevent_shared_creator_contact_locator_update
BEFORE UPDATE OF creator_id, locator_id ON creator_contact_routes
WHEN (NEW.creator_id != OLD.creator_id OR NEW.locator_id != OLD.locator_id)
AND EXISTS (
  SELECT 1 FROM creator_contact_routes existing
  WHERE existing.locator_id = NEW.locator_id
    AND existing.creator_id != NEW.creator_id
    AND existing.id != OLD.id
)
BEGIN
  SELECT RAISE(ABORT, 'contact route already belongs to another Creator');
END;";

pub(super) fn create_current_schema_unchecked(connection: &Connection) -> Result<(), String> {
    // Keep the leading comma and closing parenthesis attached to group_id:
    // SQLite's ADD COLUMN stores that exact separator in sqlite_schema.
    // Matching it keeps the frozen v28 table identity identical in fresh
    // current databases and throughout the retained migration chain.
    connection
        .execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE creator_groups (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL COLLATE NOCASE UNIQUE
                 CHECK(length(trim(name)) BETWEEN 1 AND 64),
               revision INTEGER NOT NULL CHECK(revision >= 1),
               position INTEGER NOT NULL CHECK(position >= 0),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE creators (
               id TEXT PRIMARY KEY,
               display_name TEXT NOT NULL,
               notes TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               archived_at INTEGER,
               score INTEGER CHECK(score IS NULL OR (score BETWEEN 0 AND 100)),
               assessment TEXT,
               follower_counts_json TEXT NOT NULL DEFAULT '{}'
                 CHECK(json_valid(follower_counts_json) AND json_type(follower_counts_json) = 'object'),
               pinned_at INTEGER
             , group_id TEXT REFERENCES creator_groups(id) ON DELETE SET NULL);
             CREATE INDEX creators_by_group ON creators(group_id);
             CREATE TABLE creator_relationships (
               creator_id TEXT PRIMARY KEY REFERENCES creators(id) ON DELETE RESTRICT,
               stage TEXT NOT NULL CHECK(stage IN ('prospect','engaged','collaborating','archived')),
               last_touch_at INTEGER,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE platform_accounts (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
               target_kind TEXT NOT NULL CHECK(target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               external_id TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(platform, target_kind, external_id)
             );
             CREATE TABLE creator_account_links (
               id TEXT PRIMARY KEY,
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               platform_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               state TEXT NOT NULL CHECK(state IN ('claimed','confirmed','rejected')),
               confidence TEXT NOT NULL CHECK(confidence IN ('exact','asserted','possible')),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(creator_id, platform_account_id)
             );
             CREATE UNIQUE INDEX one_confirmed_creator_per_platform_account
               ON creator_account_links(platform_account_id) WHERE state = 'confirmed';
             CREATE TABLE account_refs (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
               kind TEXT NOT NULL CHECK(kind IN ('profile','service_credential')),
               label TEXT NOT NULL,
               active INTEGER NOT NULL DEFAULT 1 CHECK(active IN (0,1)),
               availability TEXT NOT NULL DEFAULT 'available'
                 CHECK(availability IN ('available','unavailable')),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE observation_sources (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN (
                 'initial_platform_account','profile_bio','profile_website','profile_location',
                 'video_description','channel_description','link_hub','manual_business_email',
                 'community_administrator','manual_entry','contact_route_resolution',
                 'creator_import'
               )),
               platform TEXT CHECK(platform IS NULL OR platform IN ('x','instagram','telegram','youtube','tiktok')),
               source_url TEXT,
               external_ref TEXT,
               account_ref_id TEXT REFERENCES account_refs(id) ON DELETE RESTRICT,
               metadata_json TEXT NOT NULL DEFAULT '{}',
               observed_at INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX unique_creator_import_source
               ON observation_sources(external_ref) WHERE kind = 'creator_import';
             CREATE TABLE creator_import_operations (
               id TEXT PRIMARY KEY,
               content_sha256 TEXT NOT NULL,
               bundle_json TEXT CHECK(bundle_json IS NULL OR (json_valid(bundle_json) AND json_type(bundle_json) = 'object')),
               plan_fingerprint TEXT NOT NULL,
               impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
               status TEXT NOT NULL CHECK(status IN ('prepared','completed')),
               result_json TEXT CHECK(result_json IS NULL OR (json_valid(result_json) AND json_type(result_json) = 'object')),
               expires_at INTEGER NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               CHECK((status = 'prepared' AND bundle_json IS NOT NULL AND result_json IS NULL)
                  OR (status = 'completed' AND bundle_json IS NULL AND result_json IS NOT NULL))
             );
             CREATE UNIQUE INDEX one_prepared_creator_import_per_digest
               ON creator_import_operations(content_sha256) WHERE status = 'prepared';
             CREATE INDEX creator_import_operations_expiry
               ON creator_import_operations(status, expires_at);
             CREATE TABLE creator_import_receipts (
               content_sha256 TEXT PRIMARY KEY,
               format TEXT NOT NULL CHECK(format = 'reacher.creator_bundle'),
               format_version INTEGER NOT NULL CHECK(format_version > 0),
               profile TEXT NOT NULL CHECK(profile = 'portable_public'),
               snapshot_kind TEXT NOT NULL CHECK(snapshot_kind = 'full'),
               creator_count INTEGER NOT NULL CHECK(creator_count BETWEEN 0 AND 10000),
               impact_json TEXT NOT NULL CHECK(json_valid(impact_json) AND json_type(impact_json) = 'object'),
               source_id TEXT REFERENCES observation_sources(id) ON DELETE RESTRICT,
               imported_at INTEGER NOT NULL
             );
             CREATE TABLE platform_account_observations (
               id TEXT PRIMARY KEY,
               platform_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               source_id TEXT NOT NULL REFERENCES observation_sources(id) ON DELETE RESTRICT,
               handle TEXT,
               display_name TEXT,
               profile_url TEXT,
               public_json TEXT NOT NULL DEFAULT '{}',
               observed_at INTEGER NOT NULL
             );
             CREATE INDEX platform_account_observations_latest
               ON platform_account_observations(platform_account_id, observed_at DESC);
             CREATE TABLE contact_locators (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','tiktok','website','link_hub','wechat')),
               kind TEXT NOT NULL CHECK(kind IN ('email_address','handle','url','invite_link','channel_id')),
               normalized_value TEXT NOT NULL,
               display_value TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(channel, kind, normalized_value)
             );
             CREATE TABLE creator_contact_routes (
               id TEXT PRIMARY KEY,
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               locator_id TEXT NOT NULL REFERENCES contact_locators(id) ON DELETE RESTRICT,
               confidence TEXT NOT NULL CHECK(confidence IN ('exact','asserted','possible')),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(creator_id, locator_id)
             );
             CREATE UNIQUE INDEX creator_contact_routes_one_per_locator
               ON creator_contact_routes(locator_id);
             CREATE TRIGGER prevent_shared_creator_contact_locator_insert
             BEFORE INSERT ON creator_contact_routes
             WHEN NOT EXISTS (
               SELECT 1 FROM creator_contact_routes existing
               WHERE existing.locator_id = NEW.locator_id
                 AND existing.creator_id = NEW.creator_id
             )
             AND EXISTS (
               SELECT 1 FROM creator_contact_routes existing
               WHERE existing.locator_id = NEW.locator_id
                 AND existing.creator_id != NEW.creator_id
             )
             BEGIN
               SELECT RAISE(ABORT, 'contact route already belongs to another Creator');
             END;
             CREATE TRIGGER prevent_shared_creator_contact_locator_update
             BEFORE UPDATE OF creator_id, locator_id ON creator_contact_routes
             WHEN (NEW.creator_id != OLD.creator_id OR NEW.locator_id != OLD.locator_id)
             AND EXISTS (
               SELECT 1 FROM creator_contact_routes existing
               WHERE existing.locator_id = NEW.locator_id
                 AND existing.creator_id != NEW.creator_id
                 AND existing.id != OLD.id
             )
             BEGIN
               SELECT RAISE(ABORT, 'contact route already belongs to another Creator');
             END;
             CREATE TABLE creator_contact_route_dependencies (
               parent_contact_route_id TEXT NOT NULL REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               child_contact_route_id TEXT NOT NULL REFERENCES creator_contact_routes(id) ON DELETE CASCADE,
               kind TEXT NOT NULL CHECK(kind IN ('link_hub','community_administrator')),
               created_at INTEGER NOT NULL,
               PRIMARY KEY(parent_contact_route_id, child_contact_route_id),
               CHECK(parent_contact_route_id != child_contact_route_id)
             );
             CREATE INDEX creator_contact_route_dependencies_by_child
               ON creator_contact_route_dependencies(child_contact_route_id);
             CREATE TRIGGER prevent_creator_contact_route_dependency_cycle_insert
             BEFORE INSERT ON creator_contact_route_dependencies
             WHEN EXISTS (
               WITH RECURSIVE descendants(route_id) AS (
                 SELECT child_contact_route_id
                 FROM creator_contact_route_dependencies
                 WHERE parent_contact_route_id = NEW.child_contact_route_id
                 UNION
                 SELECT dependency.child_contact_route_id
                 FROM creator_contact_route_dependencies dependency
                 JOIN descendants ON dependency.parent_contact_route_id = descendants.route_id
               )
               SELECT 1 FROM descendants WHERE route_id = NEW.parent_contact_route_id
             )
             BEGIN
               SELECT RAISE(ABORT, 'contact route dependency cycle');
             END;
             CREATE TRIGGER prevent_creator_contact_route_dependency_cycle_update
             BEFORE UPDATE OF parent_contact_route_id, child_contact_route_id
             ON creator_contact_route_dependencies
             WHEN EXISTS (
               WITH RECURSIVE descendants(route_id) AS (
                 SELECT child_contact_route_id
                 FROM creator_contact_route_dependencies
                 WHERE parent_contact_route_id = NEW.child_contact_route_id
                   AND NOT (
                     parent_contact_route_id = OLD.parent_contact_route_id
                     AND child_contact_route_id = OLD.child_contact_route_id
                   )
                 UNION
                 SELECT dependency.child_contact_route_id
                 FROM creator_contact_route_dependencies dependency
                 JOIN descendants ON dependency.parent_contact_route_id = descendants.route_id
                 WHERE NOT (
                   dependency.parent_contact_route_id = OLD.parent_contact_route_id
                   AND dependency.child_contact_route_id = OLD.child_contact_route_id
                 )
               )
               SELECT 1 FROM descendants WHERE route_id = NEW.parent_contact_route_id
             )
             BEGIN
               SELECT RAISE(ABORT, 'contact route dependency cycle');
             END;
             CREATE TABLE creator_contact_route_delete_operations (
               id TEXT PRIMARY KEY,
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               contact_route_id TEXT NOT NULL,
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
             CREATE INDEX creator_contact_route_delete_operations_lookup
               ON creator_contact_route_delete_operations(contact_route_id, status, expires_at);
             CREATE TABLE locator_observations (
               id TEXT PRIMARY KEY,
               locator_id TEXT NOT NULL REFERENCES contact_locators(id) ON DELETE RESTRICT,
               source_id TEXT NOT NULL REFERENCES observation_sources(id) ON DELETE RESTRICT,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               platform_account_id TEXT REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               observed_value TEXT NOT NULL,
               observed_at INTEGER NOT NULL,
               CHECK(creator_id IS NOT NULL OR platform_account_id IS NOT NULL)
             );
             CREATE INDEX locator_observations_by_locator_recency
               ON locator_observations(locator_id, observed_at DESC);
             CREATE INDEX locator_observations_by_creator_source
               ON locator_observations(creator_id, source_id);
             CREATE INDEX locator_observations_by_platform_account_source
               ON locator_observations(platform_account_id, source_id);
             CREATE TABLE resolved_targets (
               id TEXT PRIMARY KEY,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
               target_kind TEXT NOT NULL CHECK(target_kind IN ('user','bot','channel','supergroup','group','creator','business')),
               external_target_id TEXT NOT NULL,
               platform_account_id TEXT REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               metadata_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(account_ref_id, platform, target_kind, external_target_id)
             );
             CREATE TABLE locator_resolutions (
               id TEXT PRIMARY KEY,
               locator_id TEXT NOT NULL REFERENCES contact_locators(id) ON DELETE RESTRICT,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               resolved_target_id TEXT NOT NULL REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               state TEXT NOT NULL CHECK(state IN ('active','invalidated')),
               resolved_at INTEGER NOT NULL,
               invalidated_at INTEGER
             );
             CREATE UNIQUE INDEX one_active_resolution_per_context
               ON locator_resolutions(locator_id, account_ref_id) WHERE state = 'active';
             CREATE TABLE identity_conflicts (
               id TEXT PRIMARY KEY,
               conflict_key TEXT NOT NULL UNIQUE,
               kind TEXT NOT NULL,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               platform_account_id TEXT REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               locator_id TEXT REFERENCES contact_locators(id) ON DELETE RESTRICT,
               resolved_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               evidence_json TEXT NOT NULL DEFAULT '{}',
               state TEXT NOT NULL CHECK(state IN ('open','resolved','dismissed')),
               created_at INTEGER NOT NULL,
               resolved_at INTEGER
             );
             CREATE TABLE account_relationships (
               id TEXT PRIMARY KEY,
               actor_account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               target_platform_account_id TEXT NOT NULL REFERENCES platform_accounts(id) ON DELETE RESTRICT,
               relationship_kind TEXT NOT NULL CHECK(relationship_kind IN ('following','followed_by','blocked','can_message')),
               relationship_value TEXT NOT NULL CHECK(relationship_value IN ('yes','no','unknown')),
               checked_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(actor_account_ref_id, target_platform_account_id, relationship_kind)
             );
             CREATE TABLE creator_origins (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('platform_search','community_administrator','manual_entry','creator_import')),
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               source_id TEXT NOT NULL REFERENCES observation_sources(id) ON DELETE RESTRICT,
               discovery_id TEXT CHECK(discovery_id IS NULL OR length(trim(discovery_id)) > 0),
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
                 (kind = 'platform_search'
                   AND discovery_id IS NOT NULL
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
                   AND ((source_creator_id IS NULL
                     AND source_contact_route_id IS NULL
                     AND source_resolved_target_id IS NULL)
                   OR (source_creator_id IS NOT NULL
                     AND source_resolved_target_id IS NOT NULL))
                   AND discovery_id IS NOT NULL)
                 OR
                 (kind = 'manual_entry'
                   AND discovery_id IS NULL
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
                 (kind = 'creator_import'
                   AND discovery_id IS NULL
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
             CREATE UNIQUE INDEX unique_community_administrator_origin
               ON creator_origins(
                 source_creator_id, source_contact_route_id, operator_account_ref_id,
                 source_resolved_target_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator';
             CREATE UNIQUE INDEX unique_operator_community_administrator_origin
               ON creator_origins(
                 operator_account_ref_id, community_external_id, discovery_id, creator_id
               )
               WHERE kind = 'community_administrator' AND source_creator_id IS NULL;
             CREATE UNIQUE INDEX unique_manual_creator_origin
               ON creator_origins(creator_id) WHERE kind = 'manual_entry';
             CREATE UNIQUE INDEX unique_platform_search_origin
               ON creator_origins(operator_account_ref_id, discovery_id, creator_id)
               WHERE kind = 'platform_search';
             CREATE UNIQUE INDEX unique_creator_import_origin
               ON creator_origins(import_content_sha256, imported_creator_id)
               WHERE kind = 'creator_import';
             CREATE TABLE conversations (
               id TEXT PRIMARY KEY,
               account_ref_id TEXT NOT NULL REFERENCES account_refs(id) ON DELETE RESTRICT,
               resolved_target_id TEXT NOT NULL REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','tiktok','website','link_hub','wechat')),
               provider_thread_id TEXT NOT NULL,
               cursor TEXT,
               unread_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(account_ref_id, resolved_target_id, channel, provider_thread_id)
             );
             CREATE TABLE conversation_contact_routes (
               conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
               contact_route_id TEXT NOT NULL REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               created_at INTEGER NOT NULL,
               PRIMARY KEY(conversation_id, contact_route_id)
             );
             CREATE INDEX conversation_contact_routes_by_route
               ON conversation_contact_routes(contact_route_id);
             CREATE TABLE mailbox_refs (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL CHECK(provider IN ('resend','smtp','imap','gmail')),
               label TEXT NOT NULL,
               email_address TEXT NOT NULL,
               display_name TEXT,
               capabilities_json TEXT NOT NULL,
               available INTEGER NOT NULL,
               last_error TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE email_auth_connections (
               id TEXT PRIMARY KEY,
               provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
               secret_record_id TEXT NOT NULL CHECK(length(trim(secret_record_id)) > 0),
               label TEXT NOT NULL CHECK(length(trim(label)) > 0),
               available INTEGER NOT NULL CHECK(available IN (0,1)),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(id, provider_kind),
               UNIQUE(secret_record_id)
             );
             CREATE TABLE mailbox_identity_mappings (
               mailbox_ref_id TEXT PRIMARY KEY REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               auth_connection_id TEXT NOT NULL,
               provider_kind TEXT NOT NULL CHECK(provider_kind IN ('resend','smtp','imap','remote')),
               projection_kind TEXT NOT NULL CHECK(projection_kind IN ('mailbox','send_as')),
               capability_projection_revision INTEGER NOT NULL CHECK(capability_projection_revision >= 1),
               checkpoint_namespace TEXT NOT NULL CHECK(length(trim(checkpoint_namespace)) BETWEEN 1 AND 256),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(auth_connection_id, provider_kind)
                 REFERENCES email_auth_connections(id, provider_kind) ON DELETE RESTRICT
             );
             CREATE INDEX mailbox_identity_mappings_by_connection
               ON mailbox_identity_mappings(auth_connection_id, mailbox_ref_id);
             CREATE TABLE mailbox_credential_deletion_guards (
               mailbox_ref_id TEXT PRIMARY KEY REFERENCES mailbox_refs(id) ON DELETE CASCADE,
               created_at INTEGER NOT NULL
             );
             CREATE TABLE email_templates (
               id TEXT PRIMARY KEY,
               name TEXT NOT NULL,
               subject_template TEXT NOT NULL,
               text_template TEXT NOT NULL,
               html_template TEXT,
               variables_json TEXT NOT NULL DEFAULT '[]',
               revision INTEGER NOT NULL CHECK(revision >= 1),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE email_threads (
               id TEXT PRIMARY KEY,
               mailbox_ref_id TEXT NOT NULL REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               provider_thread_key TEXT NOT NULL,
               subject TEXT NOT NULL,
               unread_count INTEGER NOT NULL DEFAULT 0,
               active_folder TEXT NOT NULL CHECK(active_folder IN ('inbox','sent')),
               archived_at INTEGER,
               deleted_at INTEGER,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(mailbox_ref_id, provider_thread_key)
             );
             CREATE INDEX email_threads_folder_lookup
               ON email_threads(mailbox_ref_id, deleted_at, archived_at, active_folder, updated_at);
             CREATE INDEX email_threads_by_creator
               ON email_threads(creator_id, id);
             CREATE TABLE messages (
               id TEXT PRIMARY KEY,
               conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE RESTRICT,
               external_message_id TEXT,
               sender_target_id TEXT REFERENCES resolved_targets(id) ON DELETE RESTRICT,
               direction TEXT NOT NULL,
               body TEXT NOT NULL,
               status TEXT NOT NULL,
               public_json TEXT NOT NULL DEFAULT '{}',
               sent_at INTEGER NOT NULL,
               UNIQUE(conversation_id, external_message_id)
             );
             CREATE TABLE emails (
               id TEXT PRIMARY KEY,
               thread_id TEXT NOT NULL REFERENCES email_threads(id) ON DELETE RESTRICT,
               provider_message_id TEXT NOT NULL,
               internet_message_id TEXT,
               direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
               subject TEXT NOT NULL,
               body_text TEXT,
               body_html TEXT,
               in_reply_to TEXT,
               references_json TEXT NOT NULL DEFAULT '[]',
               delivery_status TEXT NOT NULL,
               headers_json TEXT NOT NULL DEFAULT '{}',
               occurred_at INTEGER NOT NULL,
               origin TEXT NOT NULL CHECK(origin IN ('confirmed_send','provider_import')),
               UNIQUE(thread_id, provider_message_id)
             );
             CREATE TABLE email_recipients (
               email_id TEXT NOT NULL REFERENCES emails(id) ON DELETE RESTRICT,
               recipient_kind TEXT NOT NULL CHECK(recipient_kind IN ('from','to','cc','bcc','received_for','reply_to')),
               address TEXT NOT NULL,
               locator_id TEXT REFERENCES contact_locators(id) ON DELETE RESTRICT,
               PRIMARY KEY(email_id, recipient_kind, address)
             );
             CREATE INDEX emails_internet_message_lookup
               ON emails(internet_message_id) WHERE internet_message_id IS NOT NULL;
             CREATE TABLE email_attachments (
               id TEXT PRIMARY KEY,
               email_id TEXT NOT NULL REFERENCES emails(id) ON DELETE CASCADE,
               provider_attachment_id TEXT NOT NULL,
               filename TEXT NOT NULL,
               content_type TEXT NOT NULL,
               content_id TEXT,
               content_disposition TEXT,
               size_bytes INTEGER,
               direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
               available INTEGER NOT NULL CHECK(available IN (0,1)),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(email_id, provider_attachment_id)
             );
             CREATE TABLE email_provider_identities (
               mailbox_ref_id TEXT NOT NULL REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               provider_message_id TEXT NOT NULL,
               email_id TEXT NOT NULL REFERENCES emails(id) ON DELETE CASCADE,
               direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
               detail_complete INTEGER NOT NULL CHECK(detail_complete IN (0,1)),
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(mailbox_ref_id, provider_message_id)
             );
             CREATE INDEX email_provider_identities_by_email
               ON email_provider_identities(email_id);
             CREATE TABLE mailbox_sync_state (
               mailbox_ref_id TEXT NOT NULL REFERENCES mailbox_refs(id) ON DELETE CASCADE,
               direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
               newest_provider_message_id TEXT,
               newest_occurred_at INTEGER,
               oldest_backfill_cursor TEXT,
               backfill_complete INTEGER NOT NULL CHECK(backfill_complete IN (0,1)),
               last_page_at INTEGER,
               last_full_sync_at INTEGER,
               failure_json TEXT CHECK(failure_json IS NULL OR (json_valid(failure_json) AND json_type(failure_json) = 'object')),
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(mailbox_ref_id, direction)
             );
             CREATE TABLE mailbox_sync_incremental_continuations (
               mailbox_ref_id TEXT NOT NULL,
               direction TEXT NOT NULL CHECK(direction IN ('inbound','outbound')),
               next_cursor TEXT NOT NULL CHECK(length(trim(
                 next_cursor, char(9) || char(10) || char(11) || char(12) || char(13) || ' '
               )) > 0),
               newest_provider_message_id TEXT NOT NULL CHECK(length(trim(
                 newest_provider_message_id,
                 char(9) || char(10) || char(11) || char(12) || char(13) || ' '
               )) > 0),
               newest_occurred_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(mailbox_ref_id, direction),
               FOREIGN KEY(mailbox_ref_id, direction)
                 REFERENCES mailbox_sync_state(mailbox_ref_id, direction) ON DELETE CASCADE
             );
             CREATE TABLE email_drafts (
               id TEXT PRIMARY KEY,
               mailbox_ref_id TEXT NOT NULL REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               to_address TEXT NOT NULL,
               creator_id TEXT REFERENCES creators(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               reply_to_email_id TEXT REFERENCES emails(id) ON DELETE RESTRICT,
               subject TEXT NOT NULL,
               body_text TEXT NOT NULL,
               body_html TEXT,
               template_id TEXT REFERENCES email_templates(id) ON DELETE SET NULL,
               template_variables_json TEXT NOT NULL DEFAULT '{}',
               state TEXT NOT NULL CHECK(state IN ('draft','trashed')),
               sending_idempotency_key TEXT,
               send_reserved_at INTEGER,
               revision INTEGER NOT NULL CHECK(revision >= 1),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               CHECK((creator_id IS NULL AND contact_route_id IS NULL)
                  OR (creator_id IS NOT NULL AND contact_route_id IS NOT NULL))
             );
             CREATE INDEX email_drafts_mailbox_state
               ON email_drafts(mailbox_ref_id, state, updated_at);
             CREATE UNIQUE INDEX email_drafts_sending_key
               ON email_drafts(sending_idempotency_key)
               WHERE sending_idempotency_key IS NOT NULL;
             CREATE TABLE touchpoints (
               id TEXT PRIMARY KEY,
               creator_id TEXT NOT NULL REFERENCES creators(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               kind TEXT NOT NULL,
               message_id TEXT REFERENCES messages(id) ON DELETE RESTRICT,
               email_id TEXT REFERENCES emails(id) ON DELETE RESTRICT,
               account_relationship_id TEXT REFERENCES account_relationships(id) ON DELETE RESTRICT,
               occurred_at INTEGER NOT NULL,
               metadata_json TEXT NOT NULL DEFAULT '{}'
             );
             CREATE INDEX touchpoints_by_creator_recency
               ON touchpoints(creator_id, occurred_at DESC);
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
               payload_json TEXT NOT NULL DEFAULT '{}',
               created_at INTEGER NOT NULL
             );
             CREATE TABLE idempotency_records (
               key TEXT PRIMARY KEY,
               action TEXT NOT NULL,
               mailbox_ref_id TEXT REFERENCES mailbox_refs(id) ON DELETE RESTRICT,
               contact_route_id TEXT REFERENCES creator_contact_routes(id) ON DELETE RESTRICT,
               request_fingerprint TEXT,
               state TEXT NOT NULL CHECK(state IN ('in_progress','remote_started','remote_accepted','remote_verified','completed','failed')),
               result_json TEXT,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE TRIGGER prevent_email_claim_during_mailbox_credential_deletion
             BEFORE INSERT ON idempotency_records
             WHEN NEW.mailbox_ref_id IS NOT NULL
               AND NEW.action IN ('email_send','manual_email_send','email_draft_send')
               AND EXISTS (
                 SELECT 1 FROM mailbox_credential_deletion_guards
                 WHERE mailbox_ref_id = NEW.mailbox_ref_id
               )
             BEGIN
               SELECT RAISE(ABORT, 'mailbox credential deletion is in progress');
             END;
             CREATE TABLE reply_triage (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','dm')),
               email_id TEXT REFERENCES emails(id) ON DELETE CASCADE,
               message_id TEXT REFERENCES messages(id) ON DELETE CASCADE,
               intent TEXT NOT NULL CHECK(intent IN ('interested','question','negotiating','declined','irrelevant','other')),
               summary TEXT NOT NULL,
               suggested_reply TEXT,
               created_at INTEGER NOT NULL,
               acknowledged_at INTEGER,
               CHECK((channel = 'email' AND email_id IS NOT NULL AND message_id IS NULL)
                  OR (channel = 'dm' AND message_id IS NOT NULL AND email_id IS NULL))
             );
             CREATE UNIQUE INDEX reply_triage_one_per_email
               ON reply_triage(email_id) WHERE email_id IS NOT NULL;
             CREATE UNIQUE INDEX reply_triage_one_per_message
               ON reply_triage(message_id) WHERE message_id IS NOT NULL;
             CREATE INDEX reply_triage_open
               ON reply_triage(created_at DESC) WHERE acknowledged_at IS NULL;
             CREATE TABLE discovery_campaigns (
               id TEXT PRIMARY KEY,
               phase TEXT NOT NULL CHECK(phase IN ('draft','active','paused','completed','archived')),
               current_revision INTEGER NOT NULL CHECK(current_revision >= 1),
               target_count INTEGER NOT NULL CHECK(target_count >= 1),
               max_calls INTEGER CHECK(max_calls IS NULL OR max_calls >= 1),
               used_calls INTEGER NOT NULL DEFAULT 0 CHECK(used_calls >= 0 AND (max_calls IS NULL OR used_calls <= max_calls)),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL
             );
             CREATE INDEX discovery_campaigns_phase_updated
               ON discovery_campaigns(phase, updated_at DESC);
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
             CREATE INDEX discovery_candidates_by_verdict
               ON discovery_candidates(discovery_campaign_id, verdict, updated_at DESC);
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
             CREATE INDEX discovery_search_hits_by_campaign
               ON discovery_search_hits(discovery_campaign_id, platform, profile_ref, id);
             CREATE TABLE discovery_search_authors (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               author_id TEXT NOT NULL CHECK(length(author_id) BETWEEN 1 AND 100),
               author_url TEXT NOT NULL CHECK(length(author_url) BETWEEN 1 AND 2048),
               observed_at INTEGER NOT NULL
             );
             CREATE TABLE discovery_video_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_video_completions(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
               created_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_library_source_versions (
               discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
               snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64 AND snapshot_hash NOT GLOB '*[^0-9a-f]*'),
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_library_identity_versions (
               discovery_campaign_id TEXT PRIMARY KEY REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               source_revision INTEGER NOT NULL CHECK(source_revision BETWEEN 1 AND 9007199254740991),
               source_hash TEXT NOT NULL CHECK(length(source_hash)=64 AND source_hash NOT GLOB '*[^0-9a-f]*'),
               identity_revision INTEGER NOT NULL CHECK(identity_revision BETWEEN 1 AND 9007199254740991),
               identity_hash TEXT NOT NULL CHECK(length(identity_hash)=64 AND identity_hash NOT GLOB '*[^0-9a-f]*'),
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_evidence_reviews (
               id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
               operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
               discovery_campaign_id TEXT NOT NULL,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               native_author_id TEXT NOT NULL CHECK(length(native_author_id) BETWEEN 1 AND 256),
               profile_ref TEXT NOT NULL CHECK(length(profile_ref) BETWEEN 1 AND 256),
               request_id TEXT NOT NULL CHECK(length(trim(request_id)) BETWEEN 1 AND 128),
               request_hash TEXT NOT NULL CHECK(length(request_hash)=64 AND request_hash NOT GLOB '*[^0-9a-f]*'),
               material_hash TEXT NOT NULL CHECK(length(material_hash)=64 AND material_hash NOT GLOB '*[^0-9a-f]*'),
               material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object'
                 AND coalesce(json_extract(material_json,'$.format_version'),0) IN (1,2) AND length(CAST(material_json AS BLOB)) <= 1048576),
               policy_version TEXT NOT NULL CHECK(length(trim(policy_version)) BETWEEN 1 AND 64),
               reviewer_json TEXT NOT NULL CHECK(json_valid(reviewer_json) AND json_type(reviewer_json)='object' AND length(CAST(reviewer_json AS BLOB)) <= 2048),
               findings_json TEXT NOT NULL CHECK(json_valid(findings_json) AND json_type(findings_json)='array' AND length(CAST(findings_json AS BLOB)) <= 65536),
               verdict TEXT NOT NULL CHECK(verdict IN ('pending','qualified','rejected')),
               assessment_json TEXT NOT NULL CHECK(json_valid(assessment_json) AND json_type(assessment_json)='object' AND length(CAST(assessment_json AS BLOB)) <= 16384),
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               UNIQUE(operator_account_id, discovery_campaign_id, request_id),
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE INDEX discovery_evidence_reviews_by_candidate
               ON discovery_evidence_reviews(discovery_campaign_id, candidate_id, created_at, id);
             CREATE TABLE discovery_budget_increases (
               id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
               operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               previous_max_calls INTEGER NOT NULL CHECK(previous_max_calls BETWEEN 1 AND 10000),
               new_max_calls INTEGER NOT NULL CHECK(new_max_calls BETWEEN 1 AND 10000 AND new_max_calls > previous_max_calls),
               used_calls_at_decision INTEGER NOT NULL CHECK(used_calls_at_decision >= 0),
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_target_increases (
               id TEXT PRIMARY KEY NOT NULL CHECK(length(id) BETWEEN 1 AND 128),
               operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               previous_target_count INTEGER NOT NULL CHECK(previous_target_count BETWEEN 1 AND 10000),
               new_target_count INTEGER NOT NULL CHECK(new_target_count BETWEEN 1 AND 10000 AND new_target_count != previous_target_count),
               used_calls_at_decision INTEGER NOT NULL CHECK(used_calls_at_decision >= 0),
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE
             );
             CREATE TABLE discovery_profile_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','tiktok','twitter','reddit','facebook')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_profile_json TEXT CHECK(native_profile_json IS NULL OR (json_valid(native_profile_json) AND json_type(native_profile_json)='object' AND length(CAST(native_profile_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               UNIQUE(discovery_campaign_id, platform, profile_ref, author_id),
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_profile_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_profile_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_profile_json IS NULL AND stop_code IS NOT NULL))
             );
             CREATE TABLE discovery_content_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform IN ('bilibili','twitter','reddit','facebook')),
               profile_ref TEXT NOT NULL,
               author_id TEXT NOT NULL,
               source_url TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               native_content_json TEXT CHECK(native_content_json IS NULL OR (json_valid(native_content_json) AND json_type(native_content_json)='object' AND length(CAST(native_content_json AS BLOB)) <= 16000)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND native_content_json IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND native_content_json IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND native_content_json IS NULL AND stop_code IS NOT NULL))
             );
             CREATE TABLE discovery_identity_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_authors(search_hit_id) ON DELETE CASCADE,
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               platform TEXT NOT NULL CHECK(platform='twitter'),
               profile_ref TEXT NOT NULL,
               author_handle TEXT NOT NULL,
               source_url TEXT NOT NULL,
               post_id TEXT NOT NULL,
               expected_revision INTEGER NOT NULL CHECK(expected_revision >= 1),
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               account_id TEXT,
               observed_source_url TEXT,
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE,
               CHECK((status='reading' AND account_id IS NULL AND observed_source_url IS NULL AND stop_code IS NULL)
                 OR (status='completed' AND account_id IS NOT NULL AND observed_source_url IS NOT NULL AND stop_code IS NULL)
                 OR (status='failed' AND account_id IS NULL AND observed_source_url IS NULL AND stop_code IS NOT NULL))
             );
             CREATE TABLE discovery_profile_identity_matches (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_profile_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               updated_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id)
             );
             CREATE TABLE discovery_content_identity_bindings (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_content_reads(search_hit_id) ON DELETE CASCADE,
               identity_search_hit_id TEXT NOT NULL REFERENCES discovery_identity_reads(search_hit_id) ON DELETE CASCADE,
               profile_search_hit_id TEXT NOT NULL REFERENCES discovery_profile_identity_matches(search_hit_id) ON DELETE CASCADE,
               account_id TEXT NOT NULL CHECK(length(account_id) BETWEEN 1 AND 20 AND account_id NOT GLOB '*[^0-9]*' AND substr(account_id,1,1)!='0'),
               created_at INTEGER NOT NULL,
               CHECK(search_hit_id=identity_search_hit_id AND search_hit_id=profile_search_hit_id)
             );
             CREATE TABLE discovery_search_completions (
               discovery_campaign_id TEXT NOT NULL,
               platform TEXT NOT NULL,
               profile_ref TEXT NOT NULL,
               search_keyword TEXT NOT NULL,
               completed_at INTEGER NOT NULL,
               PRIMARY KEY(discovery_campaign_id, platform, profile_ref, search_keyword),
               FOREIGN KEY(discovery_campaign_id, platform, profile_ref)
                 REFERENCES discovery_platform_runs(discovery_campaign_id, platform, profile_ref) ON DELETE CASCADE
             );
             CREATE TABLE discovery_video_completions (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               completed_at INTEGER NOT NULL
             );
             CREATE TABLE discovery_video_read_failures (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               stop_code TEXT NOT NULL CHECK(length(stop_code) BETWEEN 1 AND 128),
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE discovery_transcript_reads (
               search_hit_id TEXT PRIMARY KEY REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','unavailable','failed')),
               segments_json TEXT NOT NULL CHECK(json_valid(segments_json) AND length(CAST(segments_json AS BLOB)) <= 24000),
               truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL
             );
             CREATE TABLE discovery_public_page_reads (
               candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               requested_url TEXT NOT NULL,
               search_hit_id TEXT NOT NULL REFERENCES discovery_search_hits(id) ON DELETE CASCADE,
               call_number INTEGER NOT NULL CHECK(call_number > 0),
               status TEXT NOT NULL CHECK(status IN ('reading','completed','failed')),
               source_excerpt TEXT NOT NULL CHECK(length(CAST(source_excerpt AS BLOB)) <= 16000),
               document_url TEXT,
               title TEXT NOT NULL,
               markdown_excerpt TEXT NOT NULL CHECK(length(CAST(markdown_excerpt AS BLOB)) <= 20000),
               truncated INTEGER NOT NULL CHECK(truncated IN (0,1)),
               stop_code TEXT,
               updated_at INTEGER NOT NULL,
               PRIMARY KEY(candidate_id, requested_url)
             );
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
             CREATE INDEX discovery_pending_decisions_open
               ON discovery_pending_decisions(discovery_campaign_id, created_at)
               WHERE state = 'pending';
             CREATE TABLE discovery_identity_reviews (
               discovery_campaign_id TEXT NOT NULL REFERENCES discovery_campaigns(id) ON DELETE CASCADE,
               decision_id TEXT NOT NULL CHECK(length(decision_id) BETWEEN 1 AND 128),
               operator_account_id TEXT NOT NULL CHECK(length(trim(operator_account_id)) BETWEEN 1 AND 256),
               criteria_revision INTEGER NOT NULL CHECK(criteria_revision >= 1),
               kind TEXT NOT NULL CHECK(kind IN ('same_author','different_authors','insufficient','revoke')),
               left_candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               right_candidate_id TEXT NOT NULL REFERENCES discovery_candidates(id) ON DELETE CASCADE,
               material_hash TEXT NOT NULL CHECK(length(material_hash)=64),
               request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
               input_json TEXT NOT NULL CHECK(json_valid(input_json) AND json_type(input_json)='object' AND length(CAST(input_json AS BLOB)) <= 65536),
               material_json TEXT NOT NULL CHECK(json_valid(material_json) AND json_type(material_json)='object' AND length(CAST(material_json AS BLOB)) <= 3145728),
               receipt_json TEXT NOT NULL CHECK(json_valid(receipt_json) AND json_type(receipt_json)='object' AND length(CAST(receipt_json AS BLOB)) <= 65536),
               revokes_decision_id TEXT,
               created_at INTEGER NOT NULL CHECK(created_at >= 0),
               PRIMARY KEY(discovery_campaign_id, decision_id),
               CHECK(left_candidate_id <> right_candidate_id),
               CHECK((kind='revoke' AND revokes_decision_id IS NOT NULL AND length(revokes_decision_id) BETWEEN 1 AND 128 AND revokes_decision_id<>decision_id)
                 OR (kind<>'revoke' AND revokes_decision_id IS NULL)),
               FOREIGN KEY(discovery_campaign_id, criteria_revision)
                 REFERENCES discovery_campaign_revisions(discovery_campaign_id, revision) ON DELETE CASCADE,
               FOREIGN KEY(discovery_campaign_id, revokes_decision_id)
                 REFERENCES discovery_identity_reviews(discovery_campaign_id, decision_id)
             );
             CREATE UNIQUE INDEX discovery_identity_reviews_one_revocation
               ON discovery_identity_reviews(discovery_campaign_id, revokes_decision_id)
               WHERE kind='revoke';
             CREATE INDEX discovery_identity_reviews_campaign_revision
               ON discovery_identity_reviews(discovery_campaign_id, criteria_revision, created_at);
             CREATE TRIGGER discovery_identity_reviews_no_update
               BEFORE UPDATE ON discovery_identity_reviews
               BEGIN SELECT RAISE(ABORT, 'identity_review_is_immutable'); END;
             ALTER TABLE creators ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]'
               CHECK(json_valid(tags_json) AND json_type(tags_json) = 'array' AND json_array_length(tags_json) <= 3);
             COMMIT;",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(super::migrations::v58_to_v59::SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(super::migrations::v59_to_v60::SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(super::migrations::v60_to_v61::SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(super::migrations::v63_to_v64::SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(super::migrations::v64_to_v65::SQL)
        .map_err(|e| e.to_string())?;
    connection
        .execute_batch(super::migrations::v65_to_v66::SQL)
        .map_err(|e| e.to_string())?;
    super::migrations::v66_to_v67::migrate(connection)?;
    super::migrations::v67_to_v68::migrate(connection)?;
    super::migrations::v68_to_v69::migrate(connection)?;
    super::migrations::v69_to_v70::migrate(connection)?;
    super::migrations::v70_to_v71::migrate(connection)?;
    super::migrations::v71_to_v72::migrate(connection)?;
    super::migrations::v72_to_v73::migrate(connection)?;
    super::migrations::v73_to_v74::migrate(connection)?;
    super::migrations::v74_to_v75::migrate(connection)?;
    super::migrations::v75_to_v76::migrate(connection)?;
    super::migrations::v76_to_v77::migrate(connection)?;
    super::migrations::v77_to_v78::migrate(connection)?;
    super::migrations::v78_to_v79::migrate(connection)?;
    super::migrations::v79_to_v80::migrate(connection)?;
    super::migrations::v80_to_v81::migrate(connection)?;
    super::migrations::v81_to_v82::migrate(connection)?;
    super::migrations::v82_to_v83::migrate(connection)?;
    super::migrations::v83_to_v84::migrate(connection)?;
    super::migrations::v84_to_v85::migrate(connection)?;
    super::migrations::v86_to_v87::migrate(connection)?;
    // The version stamp comes from the single source of truth so a schema
    super::migrations::v89_to_v90::migrate(connection)?;
    // bump can never leave a stale literal behind.
    connection
        .pragma_update(None, "user_version", super::CURRENT_SCHEMA_VERSION)
        .map_err(|error| error.to_string())
}
