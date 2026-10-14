//! Data-evolution matrix rows for the v35 -> v36 Campaign retirement edge:
//! fresh == migrated, the v35 predecessor with every origin/source kind, typed
//! impact equality between preflight and the locked revalidation, typed
//! blockers, rollback, and the retained recovery point.

use std::collections::BTreeMap;

use super::*;
use crate::{CreatorContactRouteId, ResolvedTargetId};

fn backup_count(directory: &std::path::Path) -> usize {
    std::fs::read_dir(directory)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".backup-"))
        .count()
}

fn user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap()
}

fn count(connection: &Connection, sql: &str) -> u64 {
    connection.query_row(sql, [], |row| row.get(0)).unwrap()
}

fn table_exists(connection: &Connection, table: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .unwrap()
}

fn schema_objects(connection: &Connection) -> BTreeMap<String, String> {
    connection
        .prepare(
            "SELECT type || ':' || name, sql FROM sqlite_schema
             WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                schema::normalize_sql_for_test(&row.get::<_, String>(1)?),
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

/// The released v35 wire shape of a campaign search candidate origin.
fn v35_campaign_search_origin(discovery_id: &str, campaign_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "campaign_discovery",
        "discovery_id": discovery_id,
        "campaign_id": campaign_id,
        "account_ref_id": "operator-x",
        "query": "crypto",
        "search_kind": "users"
    })
}

fn v35_ad_hoc_origin(discovery_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "ad_hoc_search",
        "discovery_id": discovery_id,
        "account_ref_id": "operator-x",
        "query": "crypto",
        "search_kind": "users"
    })
}

fn community() -> serde_json::Value {
    serde_json::json!({
        "external_id": "supergroup-42",
        "title": "Source Community",
        "handle": "sourcecommunity",
        "target_kind": "supergroup"
    })
}

fn v35_campaign_community_origin(discovery_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "community_administrator",
        "discovery_id": discovery_id,
        "role": "owner",
        "community": community(),
        "source": {
            "kind": "campaign_community",
            "campaign_id": "campaign-a",
            "operator_account_ref_id": "operator-tg"
        }
    })
}

/// The exact bytes a released build wrote for a contact-route community
/// origin: the shape is unchanged in v36, so the edge must leave them as is.
fn v35_route_origin_json() -> String {
    serde_json::to_string(&CreatorCandidateOrigin::CommunityAdministrator {
        discovery_id: DiscoveryId::new("discovery-admin-route"),
        role: CommunityAdministratorRole::Administrator,
        community: CommunityReference {
            external_id: "supergroup-42".into(),
            title: "Source Community".into(),
            handle: Some("sourcecommunity".into()),
            target_kind: CommunityTargetKind::Supergroup,
        },
        source: CommunityAdministratorSource::CreatorContactRoute {
            source_creator_id: CreatorId::new("creator-source"),
            source_contact_route_id: Some(CreatorContactRouteId::new("route-source")),
            operator_account_ref_id: AccountRefId::new("operator-tg"),
            source_resolved_target_id: ResolvedTargetId::new("target-source"),
        },
    })
    .unwrap()
}

fn v35_route_community_origin(discovery_id: &str) -> serde_json::Value {
    serde_json::json!({
        "kind": "community_administrator",
        "discovery_id": discovery_id,
        "role": "administrator",
        "community": community(),
        "source": {
            "kind": "creator_contact_route",
            "source_creator_id": "creator-source",
            "source_contact_route_id": "route-source",
            "operator_account_ref_id": "operator-tg",
            "source_resolved_target_id": "target-source"
        }
    })
}

/// A complete candidate payload whose `origin` member is the released v35
/// shape; every other member is the current wire shape (unchanged in v36).
fn v35_candidate_json(
    candidate_id: &str,
    origin: serde_json::Value,
    platform: Platform,
    target_kind: TargetKind,
    external_id: &str,
    handle: &str,
) -> String {
    // Frozen v35 envelope. Only the scenario IDs and its released origin are
    // substituted; this does not call a retired runtime candidate constructor.
    serde_json::json!({
        "candidate_id": candidate_id, "origin": origin,
        "account": { "platform": platform.as_str(), "external_id": external_id,
            "target_kind": target_kind.as_str(), "handle": handle,
            "display_name": format!("Creator {handle}"), "profile_url": null },
        "public_data": { "followers": 7 },
        "contact_discovery": { "routes": [], "enrichments": [] },
        "identity_resolution": { "status": "new" }, "allowed_actions": []
    })
    .to_string()
}

/// A Creator record in the current wire shape whose `origins` list is replaced
/// by released v35 origin objects.
fn v35_creator_record_json(
    creator_id: &str,
    account: (&str, &str),
    origins: Vec<serde_json::Value>,
) -> serde_json::Value {
    let record = crate::CreatorRecord {
        creator: crate::Creator {
            id: CreatorId::new(creator_id),
            display_name: "Replayed Creator".into(),
            tags: Vec::new(),
            notes: None,
            score: None,
            assessment: None,
            follower_counts: Default::default(),
            created_at: 1,
            updated_at: 1,
            archived_at: None,
            pinned_at: None,
        },
        group: None,
        relationship: crate::CreatorRelationship {
            creator_id: CreatorId::new(creator_id),
            stage: crate::CreatorStage::Prospect,
            last_touch_at: None,
            created_at: 1,
            updated_at: 1,
        },
        platform_accounts: vec![crate::PlatformAccount {
            id: crate::PlatformAccountId::new(account.0),
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: account.1.into(),
            created_at: 1,
            updated_at: 1,
        }],
        account_links: Vec::new(),
        account_observations: Vec::new(),
        observation_sources: Vec::new(),
        origins: Vec::new(),
        contact_routes: Vec::new(),
    };
    let mut value = serde_json::to_value(record).unwrap();
    value["origins"] = serde_json::Value::Array(origins);
    value
}

fn v35_creator_search_origin(
    id: &str,
    discovery_id: &str,
    campaign_id: &str,
    creator_id: &str,
    source_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "campaign_discovery",
        "id": id,
        "discovery_id": discovery_id,
        "campaign_id": campaign_id,
        "creator_id": creator_id,
        "source_id": source_id,
        "discovered_at": 1
    })
}

fn v35_creator_ad_hoc_origin(
    id: &str,
    discovery_id: &str,
    creator_id: &str,
    source_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "kind": "ad_hoc_search",
        "id": id,
        "discovery_id": discovery_id,
        "account_ref_id": "operator-x",
        "creator_id": creator_id,
        "source_id": source_id,
        "discovered_at": 1
    })
}

fn insert_snapshot(
    connection: &Connection,
    discovery_id: &str,
    candidate_id: &str,
    origin: &serde_json::Value,
    origin_json: &str,
    columns: &str,
    values: &str,
) {
    let (platform, target_kind, external_id, handle) =
        if origin["kind"] == "community_administrator" {
            (
                Platform::Telegram,
                TargetKind::User,
                candidate_id.rsplit(':').next().unwrap(),
                "admin",
            )
        } else {
            (
                Platform::X,
                TargetKind::User,
                candidate_id.rsplit(':').next().unwrap(),
                "creator",
            )
        };
    let candidate_json = v35_candidate_json(
        candidate_id,
        origin.clone(),
        platform,
        target_kind,
        external_id,
        handle,
    );
    connection
        .execute(
            &format!(
                "INSERT INTO creator_candidate_snapshots
                   (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                    account_platform, account_target_kind, account_external_id, account_handle,
                    account_display_name, {columns}, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, {values}, 1, 1)"
            ),
            rusqlite::params![
                discovery_id,
                candidate_id,
                origin["kind"].as_str().unwrap(),
                origin_json,
                candidate_json,
                platform.as_str(),
                target_kind.as_str(),
                external_id,
                handle,
                format!("Creator {handle}"),
            ],
        )
        .unwrap();
}

const UNTOUCHED_REPLAY: &str =
    r#"{"outcomes":[{"kind":"accepted","input_index":0}],"replayed":false}"#;
const RELATIONSHIP_REPLAY: &str = r#"{"creator_id":"creator-adhoc","stage":"engaged"}"#;
const RETIRED_RECEIPTS_SQL: &str = "SELECT COUNT(*) FROM idempotency_records \
     WHERE action IN ('campaign_create','campaign_template_create')";
const PENDING_FINGERPRINT_REPLAY: &str = r#"{"candidate_fingerprint":"{\"origin\":{\"kind\":\"campaign_discovery\"}}","discovery_id":"discovery-campaign","candidate_id":"x:user:x-campaign-1","notes":null,"quality":{},"routes":[],"expires_at":9}"#;

/// The v35 predecessor with every origin and source kind: a recoverable
/// campaign search, an unattributable campaign search, an ad-hoc search, a
/// campaign community administrator, a contact-route community administrator,
/// a manual entry, an import, campaign-attributed audit, replay payloads that
/// embed the same provenance, the two retired Campaign create receipts, and an
/// unrelated receipt that must survive.
fn seeded_v35_campaign_database() -> (tempfile::TempDir, PathBuf) {
    let (directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v35(&connection).unwrap();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             INSERT INTO account_refs(id, platform, kind, label, created_at, updated_at) VALUES
               ('operator-x', 'x', 'profile', 'X operator', 1, 1),
               ('operator-tg', 'telegram', 'profile', 'Telegram operator', 1, 1);
             INSERT INTO campaigns(id, name, status, goal, created_at, updated_at) VALUES
               ('campaign-a', 'Retired A', 'running', 'goal', 1, 1),
               ('campaign-b', 'Retired B', 'archived', 'goal', 1, 1);
             INSERT INTO campaign_templates(id, name, goal, created_at, updated_at)
               VALUES ('template-a', 'Template', 'goal', 1, 1);
             INSERT INTO campaign_account_bindings(id, campaign_id, account_ref_id, purpose, created_at) VALUES
               ('binding-x', 'campaign-a', 'operator-x', 'discovery', 1),
               ('binding-tg', 'campaign-a', 'operator-tg', 'discovery', 1);
             INSERT INTO campaign_delete_operations
               (id, campaign_id, plan_fingerprint, impact_json, status, expires_at, created_at, updated_at)
               VALUES ('delete-op', 'campaign-b', 'fp', '{}', 'prepared', 9, 1, 1);
             INSERT INTO campaign_template_delete_operations
               (id, template_id, plan_fingerprint, impact_json, status, expires_at, created_at, updated_at)
               VALUES ('template-delete-op', 'template-a', 'fp', '{}', 'prepared', 9, 1, 1);
             INSERT INTO creators(id, display_name, created_at, updated_at) VALUES
               ('creator-campaign', 'Campaign Creator', 1, 1),
               ('creator-lost', 'Lost Creator', 1, 1),
               ('creator-adhoc', 'Ad Hoc Creator', 1, 1),
               ('creator-admin-campaign', 'Admin Campaign', 1, 1),
               ('creator-admin-route', 'Admin Route', 1, 1),
               ('creator-source', 'Source Community', 1, 1),
               ('creator-manual', 'Manual', 1, 1),
               ('creator-import', 'Imported', 1, 1);
             INSERT INTO creator_relationships(creator_id, stage, created_at, updated_at)
               SELECT id, 'prospect', 1, 1 FROM creators;
             INSERT INTO platform_accounts(id, platform, target_kind, external_id, created_at, updated_at) VALUES
               ('account-campaign', 'x', 'user', 'x-campaign-1', 1, 1),
               ('account-lost', 'x', 'user', 'x-lost-1', 1, 1),
               ('account-adhoc', 'x', 'user', 'x-adhoc-1', 1, 1),
               ('account-admin-campaign', 'telegram', 'user', 'admin-1', 1, 1),
               ('account-admin-route', 'telegram', 'user', 'admin-2', 1, 1);
             INSERT INTO creator_account_links
               (id, creator_id, platform_account_id, state, confidence, created_at, updated_at) VALUES
               ('link-campaign', 'creator-campaign', 'account-campaign', 'confirmed', 'exact', 1, 1),
               ('link-lost', 'creator-lost', 'account-lost', 'confirmed', 'exact', 1, 1),
               ('link-adhoc', 'creator-adhoc', 'account-adhoc', 'confirmed', 'exact', 1, 1),
               ('link-admin-campaign', 'creator-admin-campaign', 'account-admin-campaign', 'confirmed', 'exact', 1, 1),
               ('link-admin-route', 'creator-admin-route', 'account-admin-route', 'confirmed', 'exact', 1, 1);
             INSERT INTO observation_sources(id, kind, platform, external_ref, account_ref_id, observed_at) VALUES
               ('source-campaign', 'initial_platform_account', 'x', 'candidate:discovery-campaign:x:user:x-campaign-1', 'operator-x', 1),
               ('source-lost', 'initial_platform_account', 'x', 'candidate:discovery-lost:x:user:x-lost-1', 'operator-x', 1),
               ('source-adhoc', 'initial_platform_account', 'x', 'candidate:discovery-adhoc:x:user:x-adhoc-1', 'operator-x', 1),
               ('source-admin-campaign', 'community_administrator', 'telegram', 'candidate:discovery-admin-campaign:telegram:user:admin-1', 'operator-tg', 1),
               ('source-admin-route', 'community_administrator', 'telegram', 'candidate:discovery-admin-route:telegram:user:admin-2', 'operator-tg', 1),
               ('source-manual', 'manual_entry', NULL, NULL, NULL, 1),
               ('source-import', 'creator_import', NULL, 'import-digest', NULL, 1);
             INSERT INTO creator_import_receipts
               (content_sha256, format, format_version, profile, snapshot_kind, creator_count,
                impact_json, source_id, imported_at)
               VALUES ('digest-1', 'reacher.creator_bundle', 3, 'portable_public', 'full', 1, '{}', 'source-import', 1);
             INSERT INTO contact_locators(id, channel, kind, normalized_value, display_value, created_at, updated_at)
               VALUES ('locator-source', 'telegram', 'handle', 'sourcecommunity', '@sourcecommunity', 1, 1);
             INSERT INTO creator_contact_routes(id, creator_id, locator_id, confidence, created_at, updated_at)
               VALUES ('route-source', 'creator-source', 'locator-source', 'exact', 1, 1);
             INSERT INTO resolved_targets
               (id, account_ref_id, platform, target_kind, external_target_id, created_at, updated_at)
               VALUES ('target-source', 'operator-tg', 'telegram', 'supergroup', 'supergroup-42', 1, 1);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, campaign_id, operator_account_ref_id, discovered_at) VALUES
               ('origin-campaign', 'campaign_discovery', 'creator-campaign', 'source-campaign', 'discovery-campaign', 'campaign-a', NULL, 1),
               ('origin-lost', 'campaign_discovery', 'creator-lost', 'source-lost', 'discovery-lost', 'campaign-b', NULL, 1),
               ('origin-adhoc', 'ad_hoc_search', 'creator-adhoc', 'source-adhoc', 'discovery-adhoc', NULL, 'operator-x', 1);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, campaign_id, operator_account_ref_id,
                administrator_role, community_external_id, community_title, community_handle,
                community_target_kind, discovered_at)
               VALUES ('origin-admin-campaign', 'community_administrator', 'creator-admin-campaign',
                       'source-admin-campaign', 'discovery-admin-campaign', 'campaign-a', 'operator-tg',
                       'owner', 'supergroup-42', 'Source Community', 'sourcecommunity', 'supergroup', 1);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, discovery_id, source_creator_id, source_contact_route_id,
                operator_account_ref_id, source_resolved_target_id, administrator_role,
                community_external_id, community_title, community_handle, community_target_kind, discovered_at)
               VALUES ('origin-admin-route', 'community_administrator', 'creator-admin-route',
                       'source-admin-route', 'discovery-admin-route', 'creator-source', 'route-source',
                       'operator-tg', 'target-source', 'administrator', 'supergroup-42',
                       'Source Community', 'sourcecommunity', 'supergroup', 1);
             INSERT INTO creator_origins(id, kind, creator_id, source_id, discovered_at)
               VALUES ('origin-manual', 'manual_entry', 'creator-manual', 'source-manual', 1);
             INSERT INTO creator_origins
               (id, kind, creator_id, source_id, import_content_sha256, imported_creator_id, discovered_at)
               VALUES ('origin-import', 'creator_import', 'creator-import', 'source-import', 'digest-1', 'imported-1', 1);
             INSERT INTO audit_events(id, kind, creator_id, account_ref_id, campaign_id, payload_json, created_at) VALUES
               ('audit-campaign', 'creator_saved', 'creator-campaign', 'operator-x', 'campaign-a', '{}', 1),
               ('audit-plain', 'creator_saved', 'creator-adhoc', 'operator-x', NULL, '{}', 1);",
        )
        .unwrap();
    insert_snapshot(
        &connection,
        "discovery-campaign",
        "x:user:x-campaign-1",
        &v35_campaign_search_origin("discovery-campaign", "campaign-a"),
        &v35_campaign_search_origin("discovery-campaign", "campaign-a").to_string(),
        "source_kind, campaign_id, account_ref_id",
        "'campaign_discovery', 'campaign-a', 'operator-x'",
    );
    insert_snapshot(
        &connection,
        "discovery-adhoc",
        "x:user:x-adhoc-1",
        &v35_ad_hoc_origin("discovery-adhoc"),
        &v35_ad_hoc_origin("discovery-adhoc").to_string(),
        "source_kind, account_ref_id",
        "'ad_hoc_search', 'operator-x'",
    );
    insert_snapshot(
        &connection,
        "discovery-admin-campaign",
        "telegram:user:admin-1",
        &v35_campaign_community_origin("discovery-admin-campaign"),
        &v35_campaign_community_origin("discovery-admin-campaign").to_string(),
        "administrator_role, community_external_id, community_title, community_handle, \
         community_target_kind, source_kind, campaign_id, account_ref_id",
        "'owner', 'supergroup-42', 'Source Community', 'sourcecommunity', 'supergroup', \
         'campaign_community', 'campaign-a', 'operator-tg'",
    );
    insert_snapshot(
        &connection,
        "discovery-admin-route",
        "telegram:user:admin-2",
        &v35_route_community_origin("discovery-admin-route"),
        &v35_route_origin_json(),
        "administrator_role, community_external_id, community_title, community_handle, \
         community_target_kind, source_kind, account_ref_id, source_creator_id, \
         source_contact_route_snapshot_id, source_contact_route_id, source_resolved_target_id",
        "'administrator', 'supergroup-42', 'Source Community', 'sourcecommunity', 'supergroup', \
         'creator_contact_route', 'operator-tg', 'creator-source', 'route-source', 'route-source', \
         'target-source'",
    );

    let saved_creator = v35_creator_record_json(
        "creator-campaign",
        ("account-campaign", "x-campaign-1"),
        vec![v35_creator_search_origin(
            "origin-campaign",
            "discovery-campaign",
            "campaign-a",
            "creator-campaign",
            "source-campaign",
        )],
    );
    let mut saved_candidate: serde_json::Value = serde_json::from_str(&v35_candidate_json(
        "x:user:x-campaign-1",
        v35_campaign_search_origin("discovery-campaign", "campaign-a"),
        Platform::X,
        TargetKind::User,
        "x-campaign-1",
        "creator",
    ))
    .unwrap();
    saved_candidate["identity_resolution"] =
        serde_json::json!({"status": "exact_existing_creator", "creator_id": "creator-campaign"});
    let save_completed = serde_json::json!({
        "confirmation_fingerprint": "keep-confirmation",
        "result": {"creator": saved_creator, "candidate": saved_candidate, "created": true}
    });
    let email_completed = serde_json::json!({
        "preparation": {
            "creator": v35_creator_record_json(
                "creator-lost",
                ("account-lost", "x-lost-1"),
                vec![
                    v35_creator_search_origin("origin-lost", "discovery-lost", "campaign-b", "creator-lost", "source-lost"),
                    v35_creator_ad_hoc_origin("origin-adhoc-embedded", "discovery-adhoc", "creator-lost", "source-adhoc"),
                ],
            ),
            "confirmation_required": true
        },
        "preserved": "email receipt"
    });
    let admin_completed = serde_json::json!({
        "candidate": {"origin": v35_campaign_community_origin("discovery-admin-campaign")},
        "note": "community source"
    });
    for (key, action, state, payload) in [
        (
            "save-completed",
            "creator_candidate_save_confirmation",
            "completed",
            save_completed.to_string(),
        ),
        (
            "email-completed",
            "email_send",
            "completed",
            email_completed.to_string(),
        ),
        (
            "admin-completed",
            "creator_candidate_save_confirmation",
            "completed",
            admin_completed.to_string(),
        ),
        (
            "untouched",
            "creator_batch_create",
            "completed",
            UNTOUCHED_REPLAY.to_string(),
        ),
        (
            "pending-save",
            "creator_candidate_save_confirmation",
            "in_progress",
            PENDING_FINGERPRINT_REPLAY.to_string(),
        ),
        (
            "campaign-receipt",
            "campaign_create",
            "completed",
            r#"{"campaign_id":"campaign-a"}"#.to_string(),
        ),
        (
            "template-receipt",
            "campaign_template_create",
            "completed",
            r#"{"template_id":"template-a"}"#.to_string(),
        ),
        (
            "relationship-kept",
            "relationship_update",
            "completed",
            RELATIONSHIP_REPLAY.to_string(),
        ),
    ] {
        connection
            .execute(
                "INSERT INTO idempotency_records
                   (key, action, request_fingerprint, state, result_json, created_at, updated_at)
                 VALUES (?1, ?2, 'request', ?3, ?4, 1, 1)",
                rusqlite::params![key, action, state, payload],
            )
            .unwrap();
    }
    drop(connection);
    (directory, path)
}

fn seeded_v35_impact() -> crate::CreatorDatabaseUpgradeImpact {
    crate::CreatorDatabaseUpgradeImpact {
        removed_campaigns: 2,
        removed_campaign_templates: 1,
        removed_campaign_account_bindings: 2,
        removed_campaign_delete_operations: 1,
        removed_campaign_template_delete_operations: 1,
        converted_campaign_search_snapshots: 1,
        removed_candidate_snapshots: 4,
        removed_creator_save_confirmations: 3,
        converted_campaign_search_origins: 1,
        removed_unattributable_campaign_origins: 1,
        converted_campaign_community_sources: 2,
        cleared_audit_campaign_refs: 1,
        removed_campaign_idempotency_records: 2,
        ..Default::default()
    }
}

fn expected_upgrade_required() -> crate::CreatorDatabaseStatus {
    crate::CreatorDatabaseStatus::UpgradeRequired {
        from_version: 35,
        to_version: schema::CURRENT_SCHEMA_VERSION,
        impact: seeded_v35_impact(),
    }
}

fn snapshot_row(
    connection: &Connection,
    discovery_id: &str,
) -> (String, String, String, serde_json::Value) {
    connection
        .query_row(
            "SELECT origin_kind, source_kind, origin_json, candidate_json
             FROM creator_candidate_snapshots WHERE discovery_id = ?1",
            [discovery_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    serde_json::from_str(&row.get::<_, String>(3)?).unwrap(),
                ))
            },
        )
        .unwrap()
}

fn replay_payload(connection: &Connection, key: &str) -> String {
    connection
        .query_row(
            "SELECT result_json FROM idempotency_records WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn v35_campaign_retirement_converts_provenance_and_reports_typed_impact() {
    let (directory, path) = seeded_v35_campaign_database();
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        expected_upgrade_required()
    );
    assert_eq!(
        backup_count(directory.path()),
        0,
        "preflight must not write"
    );

    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(upgraded.from_version, 35);
    assert_eq!(upgraded.to_version, schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(upgraded.impact, seeded_v35_impact());
    assert_eq!(backup_count(directory.path()), 1);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );

    // Retain the immediate v36 snapshot-conversion assertions against the
    // frozen successor of the independently readable v35 recovery point.
    let predecessor = Connection::open(&upgraded.backup_path).unwrap();
    let mut v36_reference = Connection::open_in_memory().unwrap();
    {
        let backup = rusqlite::backup::Backup::new(&predecessor, &mut v36_reference).unwrap();
        backup
            .run_to_completion(100, std::time::Duration::from_millis(1), None)
            .unwrap();
    }
    v36_reference
        .execute_batch("PRAGMA foreign_keys=OFF; BEGIN IMMEDIATE;")
        .unwrap();
    schema::apply_forward_migrations(&v36_reference, 35, 36).unwrap();
    v36_reference
        .execute_batch("COMMIT; PRAGMA foreign_keys=ON;")
        .unwrap();
    let connection = Connection::open(&path).unwrap();
    assert!(!table_exists(&connection, "creator_candidate_snapshots"));
    for table in [
        "campaigns",
        "campaign_templates",
        "campaign_account_bindings",
        "campaign_delete_operations",
        "campaign_template_delete_operations",
    ] {
        assert!(!table_exists(&connection, table), "{table} must be dropped");
    }
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM pragma_table_info('audit_events') WHERE name = 'campaign_id'"
        ),
        0
    );
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM audit_events"), 2);

    let (origin_kind, source_kind, origin_json, candidate_json) =
        snapshot_row(&v36_reference, "discovery-campaign");
    assert_eq!(
        (origin_kind.as_str(), source_kind.as_str()),
        ("platform_search", "platform_search")
    );
    let expected_origin = serde_json::json!({
        "kind": "platform_search",
        "discovery_id": "discovery-campaign",
        "account_ref_id": "operator-x",
        "query": "crypto",
        "search_kind": "users"
    });
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&origin_json).unwrap(),
        expected_origin
    );
    assert_eq!(candidate_json["origin"], expected_origin);
    assert_eq!(
        candidate_json["public_data"],
        serde_json::json!({ "followers": 7 })
    );
    let (origin_kind, source_kind, origin_json, candidate_json) =
        snapshot_row(&v36_reference, "discovery-adhoc");
    assert_eq!(
        (origin_kind.as_str(), source_kind.as_str()),
        ("platform_search", "platform_search")
    );
    assert_eq!(candidate_json["origin"]["kind"], "platform_search");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&origin_json).unwrap(),
        candidate_json["origin"],
        "origin_json and the payload origin stay the same object"
    );
    let (origin_kind, source_kind, origin_json, candidate_json) =
        snapshot_row(&v36_reference, "discovery-admin-campaign");
    assert_eq!(
        (origin_kind.as_str(), source_kind.as_str()),
        ("community_administrator", "operator_community")
    );
    let origin: serde_json::Value = serde_json::from_str(&origin_json).unwrap();
    assert_eq!(
        origin["source"],
        serde_json::json!({"kind": "operator_account", "operator_account_ref_id": "operator-tg"})
    );
    assert_eq!(origin["community"], community());
    assert_eq!(candidate_json["origin"], origin);
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM pragma_table_info('creator_candidate_snapshots') WHERE name = 'campaign_id'"
        ) + count(
            &connection,
            "SELECT COUNT(*) FROM pragma_table_info('creator_origins') WHERE name = 'campaign_id'"
        ),
        0,
        "campaign_id columns must be gone"
    );
    let (_, source_kind, origin_json, _) = snapshot_row(&v36_reference, "discovery-admin-route");
    assert_eq!(source_kind, "creator_contact_route");
    assert_eq!(
        origin_json,
        v35_route_origin_json(),
        "an unchanged kind keeps its stored bytes"
    );

    let origin_rows = connection
        .prepare(
            "SELECT id, kind, operator_account_ref_id, source_creator_id FROM creator_origins ORDER BY id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        origin_rows,
        vec![
            (
                "origin-adhoc".into(),
                "platform_search".into(),
                Some("operator-x".into()),
                None
            ),
            (
                "origin-admin-campaign".into(),
                "community_administrator".into(),
                Some("operator-tg".into()),
                None
            ),
            (
                "origin-admin-route".into(),
                "community_administrator".into(),
                Some("operator-tg".into()),
                Some("creator-source".into())
            ),
            (
                "origin-campaign".into(),
                "platform_search".into(),
                Some("operator-x".into()),
                None
            ),
            ("origin-import".into(), "creator_import".into(), None, None),
            ("origin-manual".into(), "manual_entry".into(), None, None),
        ]
    );
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM creators WHERE id = 'creator-lost'"
        ),
        1
    );
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM observation_sources WHERE id = 'source-lost'"
        ),
        1,
        "the unattributable origin's Creator and observation source remain"
    );

    let save: serde_json::Value =
        serde_json::from_str(&replay_payload(&v36_reference, "save-completed")).unwrap();
    assert_eq!(save["confirmation_fingerprint"], "keep-confirmation");
    let creator: crate::CreatorRecord =
        serde_json::from_value(save["result"]["creator"].clone()).unwrap();
    assert!(
        matches!(&creator.origins[..], [CreatorOrigin::PlatformSearch { id, discovery_id, account_ref_id, .. }] if id.as_str() == "origin-campaign" && discovery_id.as_str() == "discovery-campaign" && account_ref_id.as_str() == "operator-x")
    );
    let candidate = &save["result"]["candidate"];
    let origin: CreatorCandidateOrigin =
        serde_json::from_value(candidate["origin"].clone()).unwrap();
    assert!(
        matches!(origin, CreatorCandidateOrigin::PlatformSearch { account_ref_id, query, .. } if account_ref_id.as_str() == "operator-x" && query == "crypto")
    );
    assert_eq!(
        candidate["identity_resolution"],
        serde_json::json!({"status":"exact_existing_creator", "creator_id":"creator-campaign"})
    );
    let email: serde_json::Value =
        serde_json::from_str(&replay_payload(&connection, "email-completed")).unwrap();
    assert_eq!(email["preserved"], "email receipt");
    let embedded: crate::CreatorRecord =
        serde_json::from_value(email["preparation"]["creator"].clone()).unwrap();
    assert!(
        matches!(
            &embedded.origins[..],
            [CreatorOrigin::PlatformSearch { id, discovery_id, account_ref_id, .. }]
                if id.as_str() == "origin-adhoc-embedded" && discovery_id.as_str() == "discovery-adhoc"
                    && account_ref_id.as_str() == "operator-x"
        ),
        "the unattributable embedded campaign origin is removed, the ad-hoc one converted"
    );
    let admin: serde_json::Value =
        serde_json::from_str(&replay_payload(&v36_reference, "admin-completed")).unwrap();
    assert_eq!(admin["note"], "community source");
    let admin_origin: CreatorCandidateOrigin =
        serde_json::from_value(admin["candidate"]["origin"].clone()).unwrap();
    assert!(matches!(
        admin_origin,
        CreatorCandidateOrigin::CommunityAdministrator {
            source: CommunityAdministratorSource::OperatorAccount { operator_account_ref_id },
            ..
        } if operator_account_ref_id.as_str() == "operator-tg"
    ));
    assert_eq!(replay_payload(&connection, "untouched"), UNTOUCHED_REPLAY);
    assert_eq!(
        replay_payload(&v36_reference, "pending-save"),
        PENDING_FINGERPRINT_REPLAY,
        "retired kinds inside string values are data, not provenance objects"
    );
    assert_eq!(
        count(&connection, RETIRED_RECEIPTS_SQL),
        0,
        "the retired Campaign create receipts are gone"
    );
    assert_eq!(
        count(&connection, "SELECT COUNT(*) FROM idempotency_records"),
        3,
        "Email, completed relationship and unrelated batch receipts survive; retired save confirmations are removed"
    );
    assert_eq!(
        replay_payload(&connection, "relationship-kept"),
        RELATIONSHIP_REPLAY
    );
    drop(connection);

    let repository = SqliteRepository::open(&path).unwrap();
    let reopened = repository
        .get_creator(&CreatorId::new("creator-campaign"))
        .unwrap();
    assert!(matches!(
        &reopened.origins[..],
        [CreatorOrigin::PlatformSearch { account_ref_id, discovery_id, .. }]
            if account_ref_id.as_str() == "operator-x" && discovery_id.as_str() == "discovery-campaign"
    ));
    assert!(
        repository
            .get_creator(&CreatorId::new("creator-lost"))
            .unwrap()
            .origins
            .is_empty()
    );
    assert!(matches!(
        &repository.get_creator(&CreatorId::new("creator-admin-campaign")).unwrap().origins[..],
        [CreatorOrigin::CommunityAdministrator {
            source: CommunityAdministratorSource::OperatorAccount { operator_account_ref_id },
            ..
        }] if operator_account_ref_id.as_str() == "operator-tg"
    ));
    assert!(matches!(
        &repository.get_creator(&CreatorId::new("creator-admin-route")).unwrap().origins[..],
        [CreatorOrigin::CommunityAdministrator {
            source: CommunityAdministratorSource::CreatorContactRoute { source_contact_route_id, .. },
            ..
        }] if source_contact_route_id.as_ref().map(CreatorContactRouteId::as_str) == Some("route-source")
    ));
    for (discovery_id, candidate_id) in [
        ("discovery-campaign", "x:user:x-campaign-1"),
        ("discovery-adhoc", "x:user:x-adhoc-1"),
        ("discovery-admin-campaign", "telegram:user:admin-1"),
        ("discovery-admin-route", "telegram:user:admin-2"),
    ] {
        let connection = &v36_reference;
        let (payload, origin_json, platform, external_id): (String, String, String, Option<String>) = connection.query_row(
            "SELECT candidate_json, origin_json, account_platform, account_external_id FROM creator_candidate_snapshots WHERE discovery_id = ?1 AND candidate_id = ?2",
            rusqlite::params![discovery_id, candidate_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        ).unwrap_or_else(|error| panic!("{discovery_id} historical snapshot must remain stored: {error}"));
        let snapshot: serde_json::Value =
            serde_json::from_str(&payload).expect("frozen historical snapshot remains readable");
        assert_eq!(snapshot["candidate_id"], candidate_id);
        let origin: CreatorCandidateOrigin =
            serde_json::from_value(snapshot["origin"].clone()).unwrap();
        assert_eq!(origin.discovery_id().as_str(), discovery_id);
        assert_eq!(
            snapshot["origin"],
            serde_json::from_str::<serde_json::Value>(&origin_json).unwrap()
        );
        assert_eq!(snapshot["account"]["platform"], platform);
        assert_eq!(
            snapshot["account"]["external_id"]
                .as_str()
                .map(str::to_owned),
            external_id
        );
    }
    drop(repository);

    let backup = Connection::open(&upgraded.backup_path).unwrap();
    assert_eq!(user_version(&backup), 35);
    assert_eq!(count(&backup, "SELECT COUNT(*) FROM campaigns"), 2);
    assert_eq!(count(&backup, RETIRED_RECEIPTS_SQL), 2);
    assert_eq!(
        count(
            &backup,
            "SELECT COUNT(*) FROM creator_origins WHERE kind = 'campaign_discovery'"
        ),
        2
    );
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&upgraded.backup_path),
        expected_upgrade_required()
    );
}

#[test]
fn v36_fresh_schema_equals_the_migrated_v35_schema() {
    let (_fresh_directory, fresh_path) = current_database();
    let (_directory, path) = seeded_v35_campaign_database();
    crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let fresh = schema_objects(&Connection::open(&fresh_path).unwrap());
    let migrated = schema_objects(&Connection::open(&path).unwrap());
    assert_eq!(migrated, fresh);
    assert!(fresh.contains_key("index:unique_platform_search_origin"));
    assert!(fresh.contains_key("index:unique_operator_community_administrator_origin"));
    assert!(
        !fresh
            .keys()
            .any(|name| name.contains("campaign") && !name.contains("discovery_"))
    );
}

#[test]
fn v35_impact_is_recomputed_under_the_lock_and_rejects_a_changed_source() {
    let (directory, path) = seeded_v35_campaign_database();
    let concurrent_path = path.clone();
    let error = schema::upgrade_with_before_lock_for_test(&path, move || {
        Connection::open(concurrent_path)
            .unwrap()
            .execute_batch(
                "INSERT INTO campaigns(id, name, status, goal, created_at, updated_at)
                 VALUES ('campaign-late', 'Late', 'running', 'goal', 5, 5);",
            )
            .unwrap();
    })
    .unwrap_err();
    assert_eq!(error.support_code, "LOCAL_DATA_SOURCE_CHANGED");
    assert!(!error.storage_upgraded);
    assert_eq!(backup_count(directory.path()), 0);
    let source = Connection::open(&path).unwrap();
    assert_eq!(user_version(&source), 35);
    assert_eq!(count(&source, "SELECT COUNT(*) FROM campaigns"), 3);
    drop(source);
    let mut late_impact = seeded_v35_impact();
    late_impact.removed_campaigns = 3;
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 35,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: late_impact.clone(),
        }
    );
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(
        upgraded.impact, late_impact,
        "locked revalidation and preflight agree"
    );
}

#[test]
fn v35_snapshot_origin_that_cannot_be_rewritten_blocks_before_backup_or_write() {
    for (label, origin_json) in [
        (
            "missing fields",
            r#"{"kind":"campaign_discovery","discovery_id":"discovery-campaign"}"#.to_string(),
        ),
        (
            "unknown field",
            r#"{"kind":"campaign_discovery","discovery_id":"discovery-campaign","campaign_id":"campaign-a","account_ref_id":"operator-x","query":"crypto","search_kind":"users","plan":"x"}"#.to_string(),
        ),
        (
            "disagrees with the payload",
            v35_campaign_search_origin("discovery-campaign", "campaign-other").to_string(),
        ),
    ] {
        let (directory, path) = seeded_v35_campaign_database();
        Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE creator_candidate_snapshots SET origin_json = ?1
                 WHERE discovery_id = 'discovery-campaign'",
                [&origin_json],
            )
            .unwrap();
        let status = crate::CreatorRuntime::inspect_database(&path);
        let crate::CreatorDatabaseStatus::Unsupported { version, reason } = status else {
            panic!("{label}: malformed snapshot origin must block, got {status:?}");
        };
        assert_eq!(version, 35);
        assert!(reason.contains("candidate snapshot origin"), "{label}: {reason}");
        assert!(!reason.contains("operator-x"), "{label}: reason must stay identifier-free");
        let error = crate::CreatorRuntime::upgrade_database(&path).unwrap_err();
        assert_eq!(error.kind, crate::CreatorDatabaseUpgradeFailureKind::UnsafeData);
        assert!(!error.storage_upgraded);
        assert_eq!(backup_count(directory.path()), 0, "{label}");
        let source = Connection::open(&path).unwrap();
        assert_eq!(user_version(&source), 35);
        assert_eq!(count(&source, "SELECT COUNT(*) FROM campaigns"), 2);
    }
}

#[test]
fn v35_ambiguous_operator_attribution_blocks_instead_of_guessing() {
    let (directory, path) = seeded_v35_campaign_database();
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "INSERT INTO account_refs(id, platform, kind, label, created_at, updated_at)
             VALUES ('operator-x-2', 'x', 'profile', 'Second X operator', 1, 1);",
        )
        .unwrap();
    // A second campaign snapshot for the same discovery and candidate account,
    // run by another operator: two plausible owners, no guess.
    insert_snapshot(
        &connection,
        "discovery-campaign",
        "x:user:x-campaign-1-duplicate:x-campaign-1",
        &{
            let mut origin = v35_campaign_search_origin("discovery-campaign", "campaign-a");
            origin["account_ref_id"] = serde_json::json!("operator-x-2");
            origin
        },
        &{
            let mut origin = v35_campaign_search_origin("discovery-campaign", "campaign-a");
            origin["account_ref_id"] = serde_json::json!("operator-x-2");
            origin.to_string()
        },
        "source_kind, campaign_id, account_ref_id",
        "'campaign_discovery', 'campaign-a', 'operator-x-2'",
    );
    drop(connection);
    let status = crate::CreatorRuntime::inspect_database(&path);
    let crate::CreatorDatabaseStatus::Unsupported { version, reason } = status else {
        panic!("ambiguous attribution must block, got {status:?}");
    };
    assert_eq!(version, 35);
    assert!(reason.contains("more than one operator"), "{reason}");
    assert!(crate::CreatorRuntime::upgrade_database(&path).is_err());
    assert_eq!(backup_count(directory.path()), 0);
    assert_eq!(user_version(&Connection::open(&path).unwrap()), 35);
}

#[test]
fn v35_retirement_validation_failure_rolls_back_every_conversion_and_keeps_the_backup() {
    let (directory, path) = seeded_v35_campaign_database();
    let error = schema::upgrade_with_post_migration_mutation_for_test(&path, |transaction| {
        transaction
            .execute_batch("CREATE TABLE unexpected_retirement_drift(id TEXT)")
            .unwrap();
    })
    .unwrap_err();
    assert_eq!(error.support_code, "LOCAL_DATA_VALIDATION_FAILED");
    assert!(!error.storage_upgraded);
    let source = Connection::open(&path).unwrap();
    assert_eq!(user_version(&source), 35);
    assert_eq!(count(&source, "SELECT COUNT(*) FROM campaigns"), 2);
    assert_eq!(
        count(&source, "SELECT COUNT(*) FROM campaign_account_bindings"),
        2
    );
    assert_eq!(
        count(
            &source,
            "SELECT COUNT(*) FROM creator_origins WHERE id = 'origin-lost' AND kind = 'campaign_discovery'"
        ),
        1
    );
    assert_eq!(
        count(
            &source,
            "SELECT COUNT(*) FROM creator_candidate_snapshots WHERE origin_kind = 'campaign_discovery'"
        ),
        1
    );
    assert_eq!(
        count(
            &source,
            "SELECT COUNT(*) FROM audit_events WHERE campaign_id = 'campaign-a'"
        ),
        1
    );
    let save: serde_json::Value =
        serde_json::from_str(&replay_payload(&source, "save-completed")).unwrap();
    assert_eq!(
        save["result"]["creator"]["origins"][0]["kind"],
        "campaign_discovery"
    );
    drop(source);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        expected_upgrade_required()
    );
    assert_eq!(
        backup_count(directory.path()),
        1,
        "verified recovery point is retained"
    );
}

#[test]
fn v35_full_chain_rolls_back_to_the_reopenable_predecessor_inside_one_transaction() {
    let (_directory, path) = seeded_v35_campaign_database();
    let mut connection = Connection::open(&path).unwrap();
    // Match the production upgrader: table rebuilds run with FK checks disabled,
    // followed by explicit schema/data validation before commit.
    connection.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    schema::apply_forward_migrations(&transaction, 35, schema::CURRENT_SCHEMA_VERSION).unwrap();
    assert!(!table_exists(&transaction, "campaigns"));
    assert_eq!(
        count(
            &transaction,
            "SELECT COUNT(*) FROM creator_origins WHERE kind = 'platform_search'"
        ),
        2
    );
    transaction.rollback().unwrap();
    drop(connection);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        expected_upgrade_required()
    );
    let source = Connection::open(&path).unwrap();
    assert!(table_exists(&source, "campaigns"));
    assert_eq!(
        count(
            &source,
            "SELECT COUNT(*) FROM creator_origins WHERE kind = 'campaign_discovery'"
        ),
        2
    );
}

#[test]
fn v27_chain_reaches_v36_with_zero_campaign_impact_when_nothing_was_persisted() {
    let (_directory, path) = current_database();
    let connection = Connection::open(&path).unwrap();
    schema::downgrade_reference_to_v27(&connection).unwrap();
    drop(connection);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::UpgradeRequired {
            from_version: 27,
            to_version: schema::CURRENT_SCHEMA_VERSION,
            impact: Default::default(),
        }
    );
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    assert_eq!(
        upgraded.impact,
        crate::CreatorDatabaseUpgradeImpact::default()
    );
    let connection = Connection::open(&path).unwrap();
    assert!(!table_exists(&connection, "campaigns"));
    assert_eq!(user_version(&connection), schema::CURRENT_SCHEMA_VERSION);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );
}

#[test]
fn v35_reference_is_rejected_for_v36_only_provenance_before_any_write() {
    let (_directory, repository) = repository();
    let origin = platform_search_origin(&repository);
    persist_entry(
        &repository,
        save_input(entry(origin, "x-reference-guard", "reference_guard"), None),
    )
    .unwrap();
    let connection = repository.lock().unwrap();
    let error = schema::downgrade_reference_to_v35(&connection).unwrap_err();
    assert!(error.contains("v36-only provenance"), "{error}");
    assert_eq!(user_version(&connection), schema::CURRENT_SCHEMA_VERSION);
    assert!(!table_exists(&connection, "campaigns"));
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM creator_origins WHERE kind = 'platform_search'"
        ),
        1
    );
}

#[test]
fn v35_recovery_point_is_private_standalone_and_opens_independently() {
    let (directory, path) = seeded_v35_campaign_database();
    let upgraded = crate::CreatorRuntime::upgrade_database(&path).unwrap();
    let backup_path = std::path::Path::new(&upgraded.backup_path);
    assert_eq!(
        backup_path.parent(),
        Some(directory.path()),
        "the recovery point is kept beside its source"
    );
    assert!(
        backup_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("crm.db.backup-")
    );
    for suffix in ["-wal", "-shm"] {
        assert!(
            !std::path::Path::new(&format!("{}{suffix}", upgraded.backup_path)).exists(),
            "a checkpointed recovery point leaves no {suffix} sidecar"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(backup_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the recovery point is private to the owner");
    }
    let backup =
        Connection::open_with_flags(backup_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(user_version(&backup), 35);
    assert_eq!(
        backup
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(count(&backup, "SELECT COUNT(*) FROM campaigns"), 2);
    assert_eq!(
        count(
            &backup,
            "SELECT COUNT(*) FROM creator_candidate_snapshots WHERE origin_kind = 'campaign_discovery'"
        ),
        1
    );
    drop(backup);
    assert_eq!(
        crate::CreatorRuntime::inspect_database(backup_path),
        expected_upgrade_required()
    );
    assert_eq!(
        crate::CreatorRuntime::inspect_database(&path),
        crate::CreatorDatabaseStatus::Ready
    );
}
