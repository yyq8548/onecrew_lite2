use rusqlite::Connection;

// v32 -> v33 (Desktop app 0.5.17) adds explicitly scoped TikTok One identities
// and non-secret cloud bindings. All existing rows and foreign keys survive.
// The orchestrator owns the transaction and disables foreign keys before it.
pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    connection.execute_batch(r#"CREATE TABLE platform_accounts_v33_copy AS SELECT * FROM platform_accounts;
DROP TABLE platform_accounts;
CREATE TABLE platform_accounts (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
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
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
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
               platform TEXT CHECK(platform IS NULL OR platform IN ('x','instagram','telegram','youtube','tiktok')),
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
               platform TEXT NOT NULL CHECK(platform IN ('x','instagram','telegram','youtube','tiktok')),
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
INSERT INTO creator_candidate_snapshots SELECT * FROM creator_candidate_snapshots_v33_copy;
DROP TABLE creator_candidate_snapshots_v33_copy;
CREATE INDEX creator_candidate_snapshots_by_campaign
               ON creator_candidate_snapshots(campaign_id, updated_at DESC);
CREATE TABLE contact_locators_v33_copy AS SELECT * FROM contact_locators;
DROP TABLE contact_locators;
CREATE TABLE contact_locators (
               id TEXT PRIMARY KEY,
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','tiktok','website','link_hub')),
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
               channel TEXT NOT NULL CHECK(channel IN ('email','x','instagram','telegram','youtube','tiktok','website','link_hub')),
               provider_thread_id TEXT NOT NULL,
               cursor TEXT,
               unread_count INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(account_ref_id, resolved_target_id, channel, provider_thread_id)
             );
INSERT INTO conversations SELECT * FROM conversations_v33_copy;
DROP TABLE conversations_v33_copy;
CREATE TABLE tiktok_one_account_bindings (
               account_ref_id TEXT PRIMARY KEY REFERENCES account_refs(id) ON DELETE CASCADE,
               connection_id TEXT NOT NULL CHECK(length(trim(connection_id)) > 0),
               cloud_account_id TEXT NOT NULL CHECK(length(trim(cloud_account_id)) > 0),
               installation_id TEXT NOT NULL CHECK(length(trim(installation_id)) > 0),
               access_user_id TEXT NOT NULL CHECK(length(trim(access_user_id)) > 0),
               created_at INTEGER NOT NULL,
               updated_at INTEGER NOT NULL,
               UNIQUE(installation_id, access_user_id, connection_id, cloud_account_id)
             );"#).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::sqlite::schema::{
        current::create_current_schema_unchecked, downgrade_reference_to_v32,
        validation::validate_database_for_version,
    };

    type GraphSnapshot = std::collections::BTreeMap<&'static str, Vec<Vec<rusqlite::types::Value>>>;

    fn graph_snapshot(connection: &Connection) -> GraphSnapshot {
        [
            // Every rebuilt table, followed by the old facts that reference it.
            "platform_accounts",
            "account_refs",
            "observation_sources",
            "resolved_targets",
            "creator_candidate_snapshots",
            "contact_locators",
            "conversations",
            "creators",
            "creator_relationships",
            "creator_account_links",
            "platform_account_observations",
            "creator_contact_routes",
            "locator_observations",
            "campaigns",
            "campaign_account_bindings",
            "conversation_contact_routes",
            "audit_events",
        ]
        .into_iter()
        .map(|table| {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
                .unwrap();
            let columns = statement.column_count();
            let rows = statement
                .query_map([], |row| {
                    (0..columns)
                        .map(|column| row.get(column))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (table, rows)
        })
        .collect()
    }

    fn seed_v32_reference_graph(connection: &Connection) {
        // Insert with foreign keys enabled into the frozen v32 reference, not
        // into v33 and then downgrade potentially new-format user facts.
        connection.execute_batch(r#"
            INSERT INTO creators(id,display_name,notes,created_at,updated_at,follower_counts_json)
              VALUES ('creator-a','Alice','old notes',11,17,'{"x":77}'),
                     ('creator-b','Bob',NULL,12,18,'{}');
            INSERT INTO creator_relationships VALUES ('creator-a','engaged',16,11,17);
            INSERT INTO platform_accounts VALUES ('platform','x','user','42',11,17);
            INSERT INTO creator_account_links VALUES ('account-link','creator-a','platform','confirmed','exact',11,17);
            INSERT INTO account_refs VALUES ('operator','x','profile','Old operator',1,'available',11,17);
            INSERT INTO observation_sources VALUES
              ('profile-source','initial_platform_account','x','https://x.com/alice','42','operator','{"capture":"v32"}',13),
              ('import-source','creator_import',NULL,NULL,'old-import-digest',NULL,'{"format_version":1}',14);
            INSERT INTO platform_account_observations VALUES
              ('profile-observation','platform','profile-source','alice','Alice','https://x.com/alice','{"followers":77}',13);
            INSERT INTO contact_locators VALUES
              ('locator-a','x','handle','alice','@alice',11,17),
              ('locator-b','email','email_address','bob@example.com','bob@example.com',12,18);
            INSERT INTO creator_contact_routes VALUES
              ('route-a','creator-a','locator-a','exact',11,17),
              ('route-b','creator-b','locator-b','asserted',12,18);
            INSERT INTO locator_observations VALUES
              ('locator-observation','locator-a','profile-source','creator-a','platform','@alice',13);
            INSERT INTO resolved_targets VALUES
              ('target','operator','x','user','42','platform','{"resolved":"v32"}',14,17);
            INSERT INTO campaigns(id,name,status,goal,platforms_json,created_at,updated_at)
              VALUES ('campaign','Old campaign','reviewing','Preserve old discovery','["x"]',11,17);
            INSERT INTO campaign_account_bindings VALUES ('campaign-binding','campaign','operator','discovery','{}',11);
            INSERT INTO conversations VALUES ('conversation','operator','target','x','provider-thread','old-cursor',3,15,17);
            INSERT INTO conversation_contact_routes VALUES ('conversation','route-a',15);
            INSERT INTO audit_events(id,kind,creator_id,account_ref_id,resolved_target_id,contact_locator_id,contact_route_id,campaign_id,payload_json,created_at)
              VALUES ('audit','discovery','creator-a','operator','target','locator-a','route-a','campaign','{"old_fact":true}',17);
        "#).unwrap();
        let origin = serde_json::json!({
            "kind": "campaign_discovery", "discovery_id": "discovery",
            "campaign_id": "campaign", "account_ref_id": "operator",
            "query": "creator", "search_kind": "users"
        });
        let candidate = serde_json::json!({
            "candidate_id": "candidate", "origin": origin.clone(),
            "account": {"platform":"x", "target_kind":"user", "external_id":"42",
              "handle":"alice", "display_name":"Alice", "profile_url":"https://x.com/alice"},
            "public_data":{"followers":77}, "contact_discovery":{"routes":[], "enrichments":[]},
            "identity_resolution":{"status":"exact_existing_creator", "creator_id":"creator-a"},
            "allowed_actions":["open_profile", "view_creator"]
        });
        // The v32 candidate wire shape is frozen data; the v33 CHECK
        // constraints below validate it, not the current owner model.
        connection
            .execute(
                "INSERT INTO creator_candidate_snapshots
             (discovery_id,candidate_id,origin_kind,origin_json,candidate_json,account_platform,
              account_target_kind,account_external_id,account_handle,account_display_name,
              source_kind,campaign_id,account_ref_id,created_at,updated_at)
             VALUES ('discovery','candidate','campaign_discovery',?1,?2,'x','user','42','alice',
                     'Alice','campaign_discovery','campaign','operator',11,17)",
                rusqlite::params![origin.to_string(), candidate.to_string()],
            )
            .unwrap();
        validate_database_for_version(connection, 32).unwrap();
    }

    fn assert_reference_graph_constraints(connection: &Connection) {
        for table in [
            "platform_accounts",
            "account_refs",
            "observation_sources",
            "resolved_targets",
            "contact_locators",
        ] {
            let error = connection
                .execute(&format!("DELETE FROM {table}"), [])
                .unwrap_err();
            assert_eq!(
                error.sqlite_error_code(),
                Some(rusqlite::ErrorCode::ConstraintViolation)
            );
            assert!(
                error.to_string().contains("FOREIGN KEY constraint failed"),
                "{table}: {error}"
            );
        }
        let duplicate_source = connection.execute(
            "INSERT INTO observation_sources
             SELECT 'duplicate-source',kind,platform,source_url,external_ref,account_ref_id,metadata_json,observed_at
             FROM observation_sources WHERE id='import-source'", [],
        ).unwrap_err();
        assert_eq!(
            duplicate_source.sqlite_error().unwrap().extended_code,
            rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        );
        let indexed_candidate: String = connection.query_row(
            "SELECT candidate_id FROM creator_candidate_snapshots INDEXED BY creator_candidate_snapshots_by_campaign
             WHERE campaign_id='campaign' ORDER BY updated_at DESC", [], |row| row.get(0),
        ).unwrap();
        assert_eq!(indexed_candidate, "candidate");
        for sql in [
            "INSERT INTO creator_contact_routes VALUES ('shared-route','creator-b','locator-a','exact',18,18)",
            "UPDATE creator_contact_routes SET locator_id='locator-a' WHERE id='route-b'",
        ] {
            let error = connection.execute(sql, []).unwrap_err();
            // Require the existing owner trigger, rather than accepting the
            // duplicate-locator index as an accidental substitute.
            assert_eq!(
                error.sqlite_error().unwrap().extended_code,
                rusqlite::ffi::SQLITE_CONSTRAINT_TRIGGER
            );
            assert!(
                error
                    .to_string()
                    .contains("contact route already belongs to another Creator")
            );
        }
        connection
            .execute_batch(
                "SAVEPOINT relationship_actions;
            DELETE FROM conversations WHERE id='conversation';
            DELETE FROM campaigns WHERE id='campaign';",
            )
            .unwrap();
        for table in [
            "conversation_contact_routes",
            "creator_candidate_snapshots",
            "campaign_account_bindings",
        ] {
            let count: u64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table} must retain its cascade action");
        }
        let audit_campaign: Option<String> = connection
            .query_row(
                "SELECT campaign_id FROM audit_events WHERE id='audit'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_campaign, None, "the audit survives campaign deletion");
        connection
            .execute_batch("ROLLBACK TO relationship_actions; RELEASE relationship_actions;")
            .unwrap();
    }

    #[test]
    fn v32_upgrade_preserves_reference_graph_and_constraint_behavior() {
        let connection = Connection::open_in_memory().unwrap();
        create_current_schema_unchecked(&connection).unwrap();
        downgrade_reference_to_v32(&connection).unwrap();
        seed_v32_reference_graph(&connection);
        let before = graph_snapshot(&connection);
        assert!(before.values().all(|rows| !rows.is_empty()));
        assert_reference_graph_constraints(&connection);
        assert_eq!(graph_snapshot(&connection), before);

        connection
            .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
            .unwrap();
        migrate(&connection).unwrap();
        connection.pragma_update(None, "user_version", 33).unwrap();
        connection
            .execute_batch("COMMIT; PRAGMA foreign_keys=ON;")
            .unwrap();
        validate_database_for_version(&connection, 33).unwrap();
        assert_eq!(
            graph_snapshot(&connection),
            before,
            "all old IDs, JSON, timestamps, projections and foreign-key references must survive"
        );
        assert_reference_graph_constraints(&connection);
        assert_eq!(graph_snapshot(&connection), before);
        let violations: u64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(violations, 0);
    }

    #[test]
    fn v32_upgrade_preserves_account_rows_and_matches_fresh_schema() {
        let connection = Connection::open_in_memory().unwrap();
        create_current_schema_unchecked(&connection).unwrap();
        downgrade_reference_to_v32(&connection).unwrap();
        validate_database_for_version(&connection, 32).unwrap();
        connection.execute_batch("INSERT INTO platform_accounts (id,platform,target_kind,external_id,created_at,updated_at) VALUES ('old','x','user','42',1,2);").unwrap();
        connection
            .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
            .unwrap();
        migrate(&connection).unwrap();
        connection.pragma_update(None, "user_version", 33).unwrap();
        connection
            .execute_batch("COMMIT; PRAGMA foreign_keys=ON;")
            .unwrap();
        validate_database_for_version(&connection, 33).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT external_id FROM platform_accounts WHERE id='old'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "42"
        );
        connection.execute_batch("INSERT INTO platform_accounts (id,platform,target_kind,external_id,created_at,updated_at) VALUES ('new','tiktok','creator','tto:123',3,4);").unwrap();
    }

    #[test]
    fn failed_v33_transaction_rolls_back_to_recognizable_v32() {
        let connection = Connection::open_in_memory().unwrap();
        create_current_schema_unchecked(&connection).unwrap();
        downgrade_reference_to_v32(&connection).unwrap();
        seed_v32_reference_graph(&connection);
        let before = graph_snapshot(&connection);
        connection
            .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
            .unwrap();
        migrate(&connection).unwrap();
        connection
            .execute_batch("ROLLBACK; PRAGMA foreign_keys=ON;")
            .unwrap();
        validate_database_for_version(&connection, 32).unwrap();
        assert_eq!(graph_snapshot(&connection), before);
        assert_reference_graph_constraints(&connection);
        assert_eq!(graph_snapshot(&connection), before);
    }
}
