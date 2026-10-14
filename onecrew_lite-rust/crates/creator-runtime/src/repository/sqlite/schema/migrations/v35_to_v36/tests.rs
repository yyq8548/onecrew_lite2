use super::*;
use rusqlite::Connection;
use serde_json::json;

use crate::repository::sqlite::schema::normalize_sql_for_test;

/// A v35 reference database with one campaign search snapshot, the two retired
/// Campaign create receipts, and one unrelated receipt; the frozen DDL is the
/// fixture, never the current schema.
fn v35_reference() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    crate::repository::sqlite::schema::current::create_current_schema_unchecked(&connection)
        .unwrap();
    crate::repository::sqlite::schema::downgrade_reference_to_v35(&connection).unwrap();
    connection
        .execute_batch(
            "INSERT INTO account_refs(id, platform, kind, label, created_at, updated_at)
               VALUES ('operator-x', 'x', 'profile', 'X', 1, 1), ('operator-y', 'x', 'profile', 'Y', 1, 1);
             INSERT INTO campaigns(id, name, status, goal, created_at, updated_at)
               VALUES ('campaign-a', 'A', 'running', 'goal', 1, 1);
             INSERT INTO creator_candidate_snapshots
               (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                account_platform, account_target_kind, account_external_id, account_handle,
                account_display_name, source_kind, campaign_id, account_ref_id, created_at, updated_at)
               VALUES ('discovery-1', 'x:user:ext-1', 'campaign_discovery',
                       '{\"kind\":\"campaign_discovery\",\"discovery_id\":\"discovery-1\",\"campaign_id\":\"campaign-a\",\"account_ref_id\":\"operator-x\",\"query\":\"q\",\"search_kind\":\"users\"}',
                       '{\"candidate_id\":\"x:user:ext-1\",\"origin\":{\"kind\":\"campaign_discovery\",\"discovery_id\":\"discovery-1\",\"campaign_id\":\"campaign-a\",\"account_ref_id\":\"operator-x\",\"query\":\"q\",\"search_kind\":\"users\"},\"account\":{\"platform\":\"x\",\"external_id\":\"ext-1\",\"target_kind\":\"user\",\"handle\":\"h\",\"display_name\":\"D\",\"profile_url\":null},\"public_data\":{},\"contact_discovery\":{\"routes\":[],\"enrichments\":[]},\"identity_resolution\":{\"status\":\"new\"},\"allowed_actions\":[]}',
                       'x', 'user', 'ext-1', 'h', 'D', 'campaign_discovery', 'campaign-a', 'operator-x', 1, 1);
             INSERT INTO idempotency_records
               (key, action, request_fingerprint, state, result_json, created_at, updated_at) VALUES
               ('campaign-receipt', 'campaign_create', 'fp', 'completed', '{\"campaign_id\":\"campaign-a\"}', 1, 1),
               ('template-receipt', 'campaign_template_create', 'fp', 'completed', '{\"template_id\":\"template-a\"}', 1, 1),
               ('email-receipt', 'email_send', 'fp', 'completed', '{\"email_id\":\"email-1\"}', 1, 1);",
        )
        .unwrap();
    connection
}

fn receipts(connection: &Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT key, action FROM idempotency_records ORDER BY key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn account(external_id: &str) -> AccountKey {
    AccountKey {
        platform: "x".into(),
        target_kind: "user".into(),
        external_id: external_id.into(),
    }
}

#[test]
fn v36_wire_shapes_match_the_owner_model_exactly() {
    let community = CommunityReference {
        external_id: "community-1".into(),
        title: "Title".into(),
        handle: None,
        target_kind: CommunityTargetKind::Channel,
    };
    let model_community = crate::CommunityReference {
        external_id: "community-1".into(),
        title: "Title".into(),
        handle: None,
        target_kind: crate::CommunityTargetKind::Channel,
    };
    let pairs: Vec<(String, String)> = vec![
        (
            to_json_string(&V36CandidateOrigin::PlatformSearch {
                discovery_id: "d".into(),
                account_ref_id: "a".into(),
                query: "q".into(),
                search_kind: "users".into(),
            })
            .unwrap(),
            serde_json::to_string(&crate::CreatorCandidateOrigin::PlatformSearch {
                discovery_id: crate::DiscoveryId::new("d"),
                account_ref_id: crate::AccountRefId::new("a"),
                query: "q".into(),
                search_kind: "users".into(),
            })
            .unwrap(),
        ),
        (
            to_json_string(&V36CandidateOrigin::CommunityAdministrator {
                discovery_id: "d".into(),
                role: AdministratorRole::Owner,
                community: community.clone(),
                source: V36CommunitySource::OperatorAccount {
                    operator_account_ref_id: "tg".into(),
                },
            })
            .unwrap(),
            serde_json::to_string(&crate::CreatorCandidateOrigin::CommunityAdministrator {
                discovery_id: crate::DiscoveryId::new("d"),
                role: crate::CommunityAdministratorRole::Owner,
                community: model_community.clone(),
                source: crate::CommunityAdministratorSource::OperatorAccount {
                    operator_account_ref_id: crate::AccountRefId::new("tg"),
                },
            })
            .unwrap(),
        ),
        (
            to_json_string(&V36CandidateOrigin::CommunityAdministrator {
                discovery_id: "d".into(),
                role: AdministratorRole::Administrator,
                community,
                source: V36CommunitySource::CreatorContactRoute {
                    source_creator_id: "c".into(),
                    source_contact_route_id: Some("r".into()),
                    operator_account_ref_id: "tg".into(),
                    source_resolved_target_id: "t".into(),
                },
            })
            .unwrap(),
            serde_json::to_string(&crate::CreatorCandidateOrigin::CommunityAdministrator {
                discovery_id: crate::DiscoveryId::new("d"),
                role: crate::CommunityAdministratorRole::Administrator,
                community: model_community,
                source: crate::CommunityAdministratorSource::CreatorContactRoute {
                    source_creator_id: crate::CreatorId::new("c"),
                    source_contact_route_id: Some(crate::CreatorContactRouteId::new("r")),
                    operator_account_ref_id: crate::AccountRefId::new("tg"),
                    source_resolved_target_id: crate::ResolvedTargetId::new("t"),
                },
            })
            .unwrap(),
        ),
        (
            to_json_string(&V36CreatorSearchOrigin::PlatformSearch {
                id: "o".into(),
                discovery_id: "d".into(),
                account_ref_id: "a".into(),
                creator_id: "c".into(),
                source_id: "s".into(),
                discovered_at: 7,
            })
            .unwrap(),
            serde_json::to_string(&crate::CreatorOrigin::PlatformSearch {
                id: crate::CreatorOriginId::new("o"),
                discovery_id: crate::DiscoveryId::new("d"),
                account_ref_id: crate::AccountRefId::new("a"),
                creator_id: crate::CreatorId::new("c"),
                source_id: crate::ObservationSourceId::new("s"),
                discovered_at: 7,
            })
            .unwrap(),
        ),
    ];
    for (edge, model) in pairs {
        assert_eq!(edge, model);
    }
}

#[test]
fn owner_model_no_longer_decodes_the_released_v35_kinds() {
    for kind in ["campaign_discovery", "ad_hoc_search"] {
        assert!(
            serde_json::from_value::<crate::CreatorCandidateOrigin>(json!({
                "kind": kind, "discovery_id": "d", "account_ref_id": "a",
                "query": "q", "search_kind": "users"
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<crate::CreatorOrigin>(json!({
                "kind": kind, "id": "o", "discovery_id": "d", "account_ref_id": "a",
                "creator_id": "c", "source_id": "s", "discovered_at": 1
            }))
            .is_err()
        );
    }
    assert!(
        serde_json::from_value::<crate::CommunityAdministratorSource>(json!({
            "kind": "campaign_community", "campaign_id": "campaign-a",
            "operator_account_ref_id": "tg"
        }))
        .is_err()
    );
}

#[test]
fn released_v35_shapes_are_strict() {
    let complete = json!({
        "kind": "campaign_discovery", "discovery_id": "d", "campaign_id": "campaign-a",
        "account_ref_id": "a", "query": "q", "search_kind": "users"
    });
    serde_json::from_value::<V35CandidateOrigin>(complete.clone()).unwrap();
    let mut extra = complete.clone();
    extra["plan"] = json!("x");
    assert!(serde_json::from_value::<V35CandidateOrigin>(extra).is_err());
    let mut missing = complete;
    missing.as_object_mut().unwrap().remove("campaign_id");
    assert!(serde_json::from_value::<V35CandidateOrigin>(missing).is_err());
    assert!(
        serde_json::from_value::<V35CreatorSearchOrigin>(json!({
            "kind": "campaign_discovery", "id": "o", "discovery_id": "d",
            "campaign_id": "campaign-a", "creator_id": "c", "source_id": "s"
        }))
        .is_err(),
        "discovered_at is required"
    );
}

#[test]
fn replay_walk_rewrites_only_retired_objects_and_removes_unattributable_origins() {
    let connection = v35_reference();
    let mut payload = json!({
        "result": {
            "creator": {
                "platform_accounts": [
                    {"id": "pa", "platform": "x", "target_kind": "user", "external_id": "ext-1",
                     "created_at": 1, "updated_at": 1}
                ],
                "origins": [
                    {"kind": "campaign_discovery", "id": "o1", "discovery_id": "discovery-1",
                     "campaign_id": "campaign-a", "creator_id": "c", "source_id": "s", "discovered_at": 1},
                    {"kind": "campaign_discovery", "id": "o2", "discovery_id": "discovery-unknown",
                     "campaign_id": "campaign-a", "creator_id": "c", "source_id": "s", "discovered_at": 2},
                    {"kind": "ad_hoc_search", "id": "o3", "discovery_id": "discovery-2",
                     "account_ref_id": "operator-y", "creator_id": "c", "source_id": "s", "discovered_at": 3},
                    {"kind": "manual_entry", "id": "o4", "creator_id": "c", "source_id": "s", "created_at": 4},
                    {"kind": "community_administrator", "id": "o5", "discovery_id": "d5", "creator_id": "c",
                     "source_id": "s", "role": "owner",
                     "community": {"external_id": "e", "title": "t", "handle": null, "target_kind": "channel"},
                     "source": {"kind": "campaign_community", "campaign_id": "campaign-a",
                                "operator_account_ref_id": "tg"},
                     "discovered_at": 5}
                ],
                "observation_sources": [{"id": "s", "kind": "community_administrator", "metadata": {"kind": "campaign_discovery_note"}}]
            },
            "candidate": {"origin": {"kind": "ad_hoc_search", "discovery_id": "discovery-2",
                                     "account_ref_id": "operator-y", "query": "q", "search_kind": "users"}}
        },
        "note": "campaign_discovery mentioned in prose stays"
    });
    assert!(rewrite_replay_value(&connection, &mut payload, &[]).unwrap());
    let origins = payload["result"]["creator"]["origins"].as_array().unwrap();
    assert_eq!(
        origins.len(),
        4,
        "the unattributable campaign origin is removed"
    );
    assert_eq!(
        origins[0],
        json!({"kind": "platform_search", "id": "o1", "discovery_id": "discovery-1",
               "account_ref_id": "operator-x", "creator_id": "c", "source_id": "s", "discovered_at": 1})
    );
    assert_eq!(
        origins[1],
        json!({"kind": "platform_search", "id": "o3", "discovery_id": "discovery-2",
               "account_ref_id": "operator-y", "creator_id": "c", "source_id": "s", "discovered_at": 3})
    );
    assert_eq!(origins[2]["kind"], "manual_entry");
    assert_eq!(
        origins[3]["source"],
        json!({"kind": "operator_account", "operator_account_ref_id": "tg"})
    );
    assert_eq!(origins[3]["community"]["title"], "t");
    assert_eq!(
        payload["result"]["creator"]["observation_sources"][0]["metadata"]["kind"],
        "campaign_discovery_note",
        "an observation source keeps its own kind and metadata"
    );
    assert_eq!(
        payload["result"]["candidate"]["origin"],
        json!({"kind": "platform_search", "discovery_id": "discovery-2",
               "account_ref_id": "operator-y", "query": "q", "search_kind": "users"})
    );
    assert_eq!(
        payload["note"],
        "campaign_discovery mentioned in prose stays"
    );

    let mut untouched = json!({"kind": "community_administrator", "id": "x"});
    assert!(!rewrite_replay_value(&connection, &mut untouched, &[]).unwrap());
}

#[test]
fn replay_walk_blocks_on_malformed_or_ambiguous_retired_objects() {
    let connection = v35_reference();
    let mut malformed =
        json!({"origin": {"kind": "campaign_discovery", "discovery_id": "discovery-1"}});
    let error = rewrite_replay_value(&connection, &mut malformed, &[]).unwrap_err();
    assert!(error.contains("released v35 shape"), "{error}");
    let mut lone = json!({"origin": {"kind": "campaign_discovery", "id": "o", "discovery_id": "discovery-none",
        "campaign_id": "campaign-a", "creator_id": "c", "source_id": "s", "discovered_at": 1}});
    let error = rewrite_replay_value(&connection, &mut lone, &[]).unwrap_err();
    assert!(error.contains("outside a list"), "{error}");
    connection
        .execute_batch(
            "INSERT INTO creator_candidate_snapshots
               (discovery_id, candidate_id, origin_kind, origin_json, candidate_json,
                account_platform, account_target_kind, account_external_id, account_handle,
                account_display_name, source_kind, campaign_id, account_ref_id, created_at, updated_at)
             SELECT discovery_id, 'x:user:ext-1:dup', origin_kind, origin_json,
                    json_set(candidate_json, '$.candidate_id', 'x:user:ext-1:dup'),
                    account_platform, account_target_kind, account_external_id, account_handle,
                    account_display_name, source_kind, campaign_id, 'operator-y', 1, 1
             FROM creator_candidate_snapshots WHERE discovery_id = 'discovery-1';",
        )
        .unwrap();
    let error = recover_operator(&connection, "discovery-1", &[account("ext-1")]).unwrap_err();
    assert!(error.contains("more than one operator"), "{error}");
}

#[test]
fn plan_is_deterministic_and_read_only() {
    let connection = v35_reference();
    let first = plan(&connection).unwrap();
    let second = plan(&connection).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.impact.removed_campaigns, 1);
    assert_eq!(first.impact.converted_campaign_search_snapshots, 1);
    assert_eq!(first.impact.removed_campaign_idempotency_records, 2);
    assert!(
        first.replay_rewrites.is_empty(),
        "receipts that will be removed are never planned for a rewrite"
    );
    assert_eq!(first.snapshots.len(), 1);
    assert_eq!(first.snapshots[0].origin_kind, "platform_search");
    assert_eq!(
        first.snapshots[0].origin_json,
        r#"{"kind":"platform_search","discovery_id":"discovery-1","account_ref_id":"operator-x","query":"q","search_kind":"users"}"#
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT origin_kind FROM creator_candidate_snapshots",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "campaign_discovery",
        "planning never writes"
    );
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        35
    );
    assert_eq!(receipts(&connection).len(), 3, "planning never writes");
}

#[test]
fn migrate_removes_the_retired_campaign_receipts_and_keeps_every_other_receipt() {
    let connection = v35_reference();
    migrate(&connection).unwrap();
    assert_eq!(
        receipts(&connection),
        vec![("email-receipt".to_string(), "email_send".to_string())]
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT result_json FROM idempotency_records WHERE key = 'email-receipt'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        r#"{"email_id":"email-1"}"#,
        "an unrelated receipt keeps its stored bytes"
    );
}

#[test]
fn v35_reference_rebuilds_the_frozen_ddl_verbatim() {
    let connection = v35_reference();
    let expected = [
        ("table:campaigns", "CREATE TABLE campaigns ("),
        (
            "table:creator_candidate_snapshots",
            "CREATE TABLE creator_candidate_snapshots (",
        ),
        ("table:creator_origins", "CREATE TABLE creator_origins ("),
        ("table:audit_events", "CREATE TABLE audit_events ("),
        (
            "index:unique_campaign_creator_origin",
            "CREATE UNIQUE INDEX unique_campaign_creator_origin",
        ),
        (
            "index:creator_candidate_snapshots_by_campaign",
            "CREATE INDEX creator_candidate_snapshots_by_campaign",
        ),
    ];
    let frozen = [
        super::super::reference_v35::V35_CAMPAIGN_OBJECTS_SQL,
        super::super::reference_v35::V35_CREATOR_CANDIDATE_SNAPSHOTS_SQL,
        super::super::reference_v35::V35_CREATOR_ORIGINS_SQL,
        super::super::reference_v35::V35_AUDIT_EVENTS_SQL,
    ]
    .concat();
    let frozen_statements = frozen
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(normalize_sql_for_test)
        .collect::<Vec<_>>();
    for (object, prefix) in expected {
        let (kind, name) = object.split_once(':').unwrap();
        let stored: String = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = ?1 AND name = ?2",
                [kind, name],
                |row| row.get(0),
            )
            .unwrap();
        assert!(stored.starts_with(prefix), "{object}: {stored}");
        assert!(
            frozen_statements.contains(&normalize_sql_for_test(&stored)),
            "{object} must be reconstructed from the frozen v35 DDL"
        );
    }
}
