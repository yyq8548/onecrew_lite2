//! Wire-shape golden test for the Creator owner models. The literals were
//! captured from the serializers while these types still lived under
//! `models::discovery`; the move must not change one byte of any of them.
use std::collections::BTreeMap;

use crate::*;

fn source(id: &str) -> ObservationSource {
    ObservationSource {
        id: ObservationSourceId::new(id),
        kind: SourceKind::InitialPlatformAccount,
        platform: Some(Platform::X),
        source_url: Some("https://x.com/creator_one".into()),
        external_ref: Some("candidate:discovery-1:x:user:x-1".into()),
        account_ref_id: Some(AccountRefId::new("account-1")),
        metadata: serde_json::json!({"query": "crypto", "search_kind": "users"}),
        observed_at: 5,
    }
}

fn contact_route_record() -> CreatorContactRouteRecord {
    CreatorContactRouteRecord {
        route: CreatorContactRoute {
            id: CreatorContactRouteId::new("route-1"),
            creator_id: CreatorId::new("creator-1"),
            locator_id: ContactLocatorId::new("locator-1"),
            confidence: Confidence::Exact,
            created_at: 1,
            updated_at: 2,
        },
        locator: ContactLocator {
            id: ContactLocatorId::new("locator-1"),
            channel: ContactChannel::X,
            kind: LocatorKind::Handle,
            normalized_value: "creator_one".into(),
            display_value: "@creator_one".into(),
            created_at: 1,
            updated_at: 2,
        },
        observations: vec![ContactRouteObservationRecord {
            observation: LocatorObservation {
                id: LocatorObservationId::new("locator-observation-1"),
                locator_id: ContactLocatorId::new("locator-1"),
                source_id: ObservationSourceId::new("source-1"),
                creator_id: Some(CreatorId::new("creator-1")),
                platform_account_id: Some(PlatformAccountId::new("account-x")),
                observed_value: "creator_one".into(),
                observed_at: 5,
            },
            source: source("source-1"),
        }],
        allowed_actions: vec![
            CreatorContactRouteAction::Update,
            CreatorContactRouteAction::Delete,
        ],
    }
}

fn origins() -> Vec<CreatorOrigin> {
    vec![
        CreatorOrigin::PlatformSearch {
            id: CreatorOriginId::new("origin-search"),
            discovery_id: DiscoveryId::new("discovery-1"),
            account_ref_id: AccountRefId::new("account-1"),
            creator_id: CreatorId::new("creator-1"),
            source_id: ObservationSourceId::new("source-1"),
            discovered_at: 1_700_000_000,
        },
        CreatorOrigin::CommunityAdministrator {
            id: CreatorOriginId::new("origin-admin"),
            discovery_id: DiscoveryId::new("discovery-2"),
            creator_id: CreatorId::new("creator-1"),
            source_id: ObservationSourceId::new("source-2"),
            role: CommunityAdministratorRole::Owner,
            community: CommunityReference {
                external_id: "community-42".into(),
                title: "Community".into(),
                handle: Some("community".into()),
                target_kind: CommunityTargetKind::Supergroup,
            },
            source: CommunityAdministratorSource::CreatorContactRoute {
                source_creator_id: CreatorId::new("creator-0"),
                source_contact_route_id: Some(CreatorContactRouteId::new("route-0")),
                operator_account_ref_id: AccountRefId::new("account-1"),
                source_resolved_target_id: ResolvedTargetId::new("target-0"),
            },
            discovered_at: 1_700_000_001,
        },
        CreatorOrigin::CommunityAdministrator {
            id: CreatorOriginId::new("origin-admin-operator"),
            discovery_id: DiscoveryId::new("discovery-3"),
            creator_id: CreatorId::new("creator-1"),
            source_id: ObservationSourceId::new("source-5"),
            role: CommunityAdministratorRole::Administrator,
            community: CommunityReference {
                external_id: "community-43".into(),
                title: "Channel".into(),
                handle: None,
                target_kind: CommunityTargetKind::Channel,
            },
            source: CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: AccountRefId::new("account-2"),
            },
            discovered_at: 1_700_000_004,
        },
        CreatorOrigin::ManualEntry {
            id: CreatorOriginId::new("origin-manual"),
            creator_id: CreatorId::new("creator-1"),
            source_id: ObservationSourceId::new("source-3"),
            created_at: 1_700_000_002,
        },
        CreatorOrigin::CreatorImport {
            id: CreatorOriginId::new("origin-import"),
            creator_id: CreatorId::new("creator-1"),
            source_id: ObservationSourceId::new("source-4"),
            content_sha256: "abc123".into(),
            imported_creator_id: CreatorId::new("imported-1"),
            imported_at: 1_700_000_003,
        },
    ]
}

fn creator_record() -> CreatorRecord {
    CreatorRecord {
        creator: Creator {
            id: CreatorId::new("creator-1"),
            display_name: "Creator One".into(),
            tags: vec!["美妆".into(), "Beauty".into()],
            notes: Some("met at conference".into()),
            score: Some(88),
            assessment: Some("fits the brief".into()),
            follower_counts: BTreeMap::from([(Platform::X, 42_u64), (Platform::YouTube, 7)]),
            created_at: 1,
            updated_at: 2,
            archived_at: None,
            pinned_at: Some(3),
        },
        group: Some(CreatorGroupRef {
            id: CreatorGroupId::new("group-1"),
            name: "Launch".into(),
        }),
        relationship: CreatorRelationship {
            creator_id: CreatorId::new("creator-1"),
            stage: CreatorStage::Prospect,
            last_touch_at: Some(4),
            created_at: 1,
            updated_at: 2,
        },
        platform_accounts: vec![PlatformAccount {
            id: PlatformAccountId::new("account-x"),
            platform: Platform::X,
            target_kind: TargetKind::User,
            external_id: "x-1".into(),
            created_at: 1,
            updated_at: 2,
        }],
        account_links: vec![CreatorAccountLink {
            id: CreatorAccountLinkId::new("link-1"),
            creator_id: CreatorId::new("creator-1"),
            platform_account_id: PlatformAccountId::new("account-x"),
            state: LinkState::Confirmed,
            confidence: Confidence::Exact,
            created_at: 1,
            updated_at: 2,
        }],
        account_observations: vec![PlatformAccountObservation {
            id: PlatformAccountObservationId::new("observation-1"),
            platform_account_id: PlatformAccountId::new("account-x"),
            source_id: ObservationSourceId::new("source-1"),
            handle: Some("creator_one".into()),
            display_name: Some("Creator One".into()),
            profile_url: Some("https://x.com/creator_one".into()),
            public_data: serde_json::json!({"followers": 42}),
            observed_at: 5,
        }],
        observation_sources: vec![source("source-1")],
        origins: origins(),
        contact_routes: vec![contact_route_record()],
    }
}

fn group() -> CreatorGroup {
    CreatorGroup {
        id: CreatorGroupId::new("group-1"),
        name: "Launch".into(),
        revision: 3,
        position: 1,
        created_at: 1,
        updated_at: 2,
    }
}

const CREATOR_RECORD: &str = r#"{"creator":{"id":"creator-1","display_name":"Creator One","tags":["美妆","Beauty"],"notes":"met at conference","score":88,"assessment":"fits the brief","follower_counts":{"x":42,"youtube":7},"created_at":1,"updated_at":2,"archived_at":null,"pinned_at":3},"group":{"id":"group-1","name":"Launch"},"relationship":{"creator_id":"creator-1","stage":"prospect","last_touch_at":4,"created_at":1,"updated_at":2},"platform_accounts":[{"id":"account-x","platform":"x","target_kind":"user","external_id":"x-1","created_at":1,"updated_at":2}],"account_links":[{"id":"link-1","creator_id":"creator-1","platform_account_id":"account-x","state":"confirmed","confidence":"exact","created_at":1,"updated_at":2}],"account_observations":[{"id":"observation-1","platform_account_id":"account-x","source_id":"source-1","handle":"creator_one","display_name":"Creator One","profile_url":"https://x.com/creator_one","public_data":{"followers":42},"observed_at":5}],"observation_sources":[{"id":"source-1","kind":"initial_platform_account","platform":"x","source_url":"https://x.com/creator_one","external_ref":"candidate:discovery-1:x:user:x-1","account_ref_id":"account-1","metadata":{"query":"crypto","search_kind":"users"},"observed_at":5}],"origins":[{"kind":"platform_search","id":"origin-search","discovery_id":"discovery-1","account_ref_id":"account-1","creator_id":"creator-1","source_id":"source-1","discovered_at":1700000000},{"kind":"community_administrator","id":"origin-admin","discovery_id":"discovery-2","creator_id":"creator-1","source_id":"source-2","role":"owner","community":{"external_id":"community-42","title":"Community","handle":"community","target_kind":"supergroup"},"source":{"kind":"creator_contact_route","source_creator_id":"creator-0","source_contact_route_id":"route-0","operator_account_ref_id":"account-1","source_resolved_target_id":"target-0"},"discovered_at":1700000001},{"kind":"community_administrator","id":"origin-admin-operator","discovery_id":"discovery-3","creator_id":"creator-1","source_id":"source-5","role":"administrator","community":{"external_id":"community-43","title":"Channel","handle":null,"target_kind":"channel"},"source":{"kind":"operator_account","operator_account_ref_id":"account-2"},"discovered_at":1700000004},{"kind":"manual_entry","id":"origin-manual","creator_id":"creator-1","source_id":"source-3","created_at":1700000002},{"kind":"creator_import","id":"origin-import","creator_id":"creator-1","source_id":"source-4","content_sha256":"abc123","imported_creator_id":"imported-1","imported_at":1700000003}],"contact_routes":[{"route":{"id":"route-1","creator_id":"creator-1","locator_id":"locator-1","confidence":"exact","created_at":1,"updated_at":2},"locator":{"id":"locator-1","channel":"x","kind":"handle","normalized_value":"creator_one","display_value":"@creator_one","created_at":1,"updated_at":2},"observations":[{"observation":{"id":"locator-observation-1","locator_id":"locator-1","source_id":"source-1","creator_id":"creator-1","platform_account_id":"account-x","observed_value":"creator_one","observed_at":5},"source":{"id":"source-1","kind":"initial_platform_account","platform":"x","source_url":"https://x.com/creator_one","external_ref":"candidate:discovery-1:x:user:x-1","account_ref_id":"account-1","metadata":{"query":"crypto","search_kind":"users"},"observed_at":5}}],"allowed_actions":["update","delete"]}]}"#;
const ORIGINS: &str = r#"[{"kind":"platform_search","id":"origin-search","discovery_id":"discovery-1","account_ref_id":"account-1","creator_id":"creator-1","source_id":"source-1","discovered_at":1700000000},{"kind":"community_administrator","id":"origin-admin","discovery_id":"discovery-2","creator_id":"creator-1","source_id":"source-2","role":"owner","community":{"external_id":"community-42","title":"Community","handle":"community","target_kind":"supergroup"},"source":{"kind":"creator_contact_route","source_creator_id":"creator-0","source_contact_route_id":"route-0","operator_account_ref_id":"account-1","source_resolved_target_id":"target-0"},"discovered_at":1700000001},{"kind":"community_administrator","id":"origin-admin-operator","discovery_id":"discovery-3","creator_id":"creator-1","source_id":"source-5","role":"administrator","community":{"external_id":"community-43","title":"Channel","handle":null,"target_kind":"channel"},"source":{"kind":"operator_account","operator_account_ref_id":"account-2"},"discovered_at":1700000004},{"kind":"manual_entry","id":"origin-manual","creator_id":"creator-1","source_id":"source-3","created_at":1700000002},{"kind":"creator_import","id":"origin-import","creator_id":"creator-1","source_id":"source-4","content_sha256":"abc123","imported_creator_id":"imported-1","imported_at":1700000003}]"#;
const CREATOR_SUMMARY: &str = r#"{"id":"creator-1","display_name":"Creator One","tags":["美妆"],"stage":"prospect","group":{"id":"group-1","name":"Launch"},"primary_platform":"x","primary_handle":"creator_one","avatar_url":"https://img/new.png","platforms":["x","telegram"],"channels":["x","email"],"has_manual_origin":false,"score":88,"assessment":"fits the brief","follower_counts":{"x":42},"last_touch_at":4,"pinned_at":null,"platform_account_count":2,"contact_route_count":3,"archived_at":null,"created_at":1,"updated_at":2}"#;
const LIST_CREATORS_INPUT: &str = r#"{"limit":25,"cursor":"cursor","query":"harry","stages":["prospect"],"primary_platforms":["x"],"last_touch_range":{"from":1,"to_exclusive":2},"updated_range":null,"sorting":[{"field":"last_touch_at","direction":"desc"}],"include_facets":true,"group":{"kind":"uncategorized"},"include_archived":false}"#;
const MANUAL_BATCH_RESULT: &str = r#"{"outcomes":[{"kind":"accepted","input_index":0,"creator":{"creator_id":"creator-1","display_name":"Creator One","tags":[],"stage":"prospect","created_at":1,"contact_routes":[{"route":{"id":"route-1","creator_id":"creator-1","locator_id":"locator-1","confidence":"exact","created_at":1,"updated_at":2},"locator":{"id":"locator-1","channel":"x","kind":"handle","normalized_value":"creator_one","display_value":"@creator_one","created_at":1,"updated_at":2},"observations":[{"observation":{"id":"locator-observation-1","locator_id":"locator-1","source_id":"source-1","creator_id":"creator-1","platform_account_id":"account-x","observed_value":"creator_one","observed_at":5},"source":{"id":"source-1","kind":"initial_platform_account","platform":"x","source_url":"https://x.com/creator_one","external_ref":"candidate:discovery-1:x:user:x-1","account_ref_id":"account-1","metadata":{"query":"crypto","search_kind":"users"},"observed_at":5}}],"allowed_actions":["update","delete"]}]}},{"kind":"rejected","input_index":1,"message":"duplicate"}],"replayed":false}"#;
const GROUP: &str =
    r#"{"id":"group-1","name":"Launch","revision":3,"position":1,"created_at":1,"updated_at":2}"#;
const GROUP_LIST: &str = r#"{"items":[{"group":{"id":"group-1","name":"Launch","revision":3,"position":1,"created_at":1,"updated_at":2},"creator_count":4}],"uncategorized_count":2,"next_cursor":"next"}"#;
const GROUP_MOVE: &str = r#"{"assignments":[{"creator_id":"creator-1","group":{"id":"group-1","name":"Launch"}},{"creator_id":"creator-2","group":null}],"moved_count":1,"unchanged_count":1}"#;
const GROUP_DESTINATIONS: &str =
    r#"[{"kind":"uncategorized"},{"kind":"group","group_id":"group-1"}]"#;
const GROUP_FILTERS: &str = r#"[{"kind":"uncategorized"},{"kind":"group","group_id":"group-1"}]"#;
const CONTACT_ROUTE_ADD: &str = r#"{"status":"added","creator_id":"creator-1","route_id":"route-1","channel":"email","locator_kind":"email_address","normalized_value":"person@example.com","display_value":"person@example.com","confidence":"asserted","created_at":1}"#;
const CONTACT_ROUTE_UPDATE: &str = r#"{"status":"replacement_required","creator_id":"creator-1","route_id":"route-1","route":{"route":{"id":"route-1","creator_id":"creator-1","locator_id":"locator-1","confidence":"exact","created_at":1,"updated_at":2},"locator":{"id":"locator-1","channel":"x","kind":"handle","normalized_value":"creator_one","display_value":"@creator_one","created_at":1,"updated_at":2},"observations":[{"observation":{"id":"locator-observation-1","locator_id":"locator-1","source_id":"source-1","creator_id":"creator-1","platform_account_id":"account-x","observed_value":"creator_one","observed_at":5},"source":{"id":"source-1","kind":"initial_platform_account","platform":"x","source_url":"https://x.com/creator_one","external_ref":"candidate:discovery-1:x:user:x-1","account_ref_id":"account-1","metadata":{"query":"crypto","search_kind":"users"},"observed_at":5}}],"allowed_actions":["update","delete"]}}"#;
const CONTACT_ROUTE_DELETE_PREPARE: &str = r#"[{"status":"blocked","creator_id":"creator-1","contact_route_id":"route-1","reason":"has_derived_contact_routes","children":[{"creator_id":"creator-2","contact_route_id":"route-2","dependency_kind":"link_hub","channel":"telegram","display_value":"@child"}],"blocking_artifacts":[{"kind":"in_flight_email","count":2}]},{"status":"confirmation_required","operation_id":"operation-1","creator_id":"creator-1","contact_route_id":"route-1","impact":{"contact_routes":1,"dependencies":0,"email_threads":2,"email_messages":0,"email_recipients":0,"email_drafts":0,"direct_message_conversations":0,"direct_messages":0,"outreach_touchpoints":0,"audit_events":0,"idempotency_records":0,"target_resolutions":0,"identity_conflicts":0,"observations":0,"contact_locators":0,"observation_sources":0,"resolved_targets":0,"creator_origins_detached":0,"local_records_total":3},"remote_not_deleted":true,"expires_at":99,"next_tool":"onecrew.creator.contact_route.delete.confirm"}]"#;
const CONTACT_ROUTE_DELETE_CONFIRM: &str = r#"[{"status":"deleted","operation_id":"operation-1","creator_id":"creator-1","contact_route_id":"route-1","deleted":{"contact_routes":0,"dependencies":0,"email_threads":0,"email_messages":0,"email_recipients":0,"email_drafts":0,"direct_message_conversations":0,"direct_messages":0,"outreach_touchpoints":0,"audit_events":0,"idempotency_records":0,"target_resolutions":0,"identity_conflicts":0,"observations":0,"contact_locators":0,"observation_sources":0,"resolved_targets":0,"creator_origins_detached":0,"local_records_total":0},"remote_not_deleted":false},{"status":"stale","operation_id":"operation-1","reason":"plan_changed","next_tool":"onecrew.creator.contact_route.delete.prepare"},{"status":"expired","operation_id":"operation-1","next_tool":"onecrew.creator.contact_route.delete.prepare"}]"#;
const RESOLVE_CONTACT_TARGET_INPUT: &str = r#"{"creator_id":"creator-1","contact_route_id":"route-1","operator_account_ref_id":"account-1","refresh":true}"#;
const OBSERVATION_INPUTS: &str = r#"[{"platform":"telegram","target_kind":"user","external_id":"tg-200"},{"platform_account_id":"account-x","source_id":"source-1","handle":"creator_one","display_name":null,"profile_url":"https://x.com/creator_one","public_data":{"profile_image_url":"https://img/new.png"}},{"kind":"profile_bio","platform":"x","source_url":null,"external_ref":"conflict-source","account_ref_id":"account-1","metadata":{}},{"channel":"email","kind":"email_address","normalized_value":"person@example.com","display_value":"Person@Example.com"}]"#;

macro_rules! assert_golden {
    ($golden:expr, $value:expr) => {
        assert_eq!(serde_json::to_string(&$value).unwrap(), $golden);
    };
}

#[test]
fn creator_models_serialize_exactly_as_before_the_move() {
    assert_golden!(CREATOR_RECORD, creator_record());
    assert_golden!(ORIGINS, origins());
    assert_golden!(
        CREATOR_SUMMARY,
        CreatorSummary {
            id: CreatorId::new("creator-1"),
            display_name: "Creator One".into(),
            tags: vec!["美妆".into()],
            stage: CreatorStage::Prospect,
            group: Some(CreatorGroupRef {
                id: CreatorGroupId::new("group-1"),
                name: "Launch".into(),
            }),
            primary_platform: Some(Platform::X),
            primary_handle: Some("creator_one".into()),
            avatar_url: Some("https://img/new.png".into()),
            platforms: vec![Platform::X, Platform::Telegram],
            channels: vec![ContactChannel::X, ContactChannel::Email],
            has_manual_origin: false,
            score: Some(88),
            assessment: Some("fits the brief".into()),
            follower_counts: BTreeMap::from([(Platform::X, 42_u64)]),
            last_touch_at: Some(4),
            pinned_at: None,
            platform_account_count: 2,
            contact_route_count: 3,
            archived_at: None,
            created_at: 1,
            updated_at: 2,
        }
    );
    assert_golden!(
        LIST_CREATORS_INPUT,
        ListCreatorsInput {
            limit: Some(25),
            cursor: Some("cursor".into()),
            query: Some("harry".into()),
            stages: vec![CreatorStage::Prospect],
            primary_platforms: vec![Platform::X],
            last_touch_range: Some(CreatorDateRange {
                from: Some(1),
                to_exclusive: Some(2),
            }),
            updated_range: None,
            sorting: vec![CreatorListSort {
                field: CreatorSortField::LastTouchAt,
                direction: CreatorSortDirection::Desc,
            }],
            include_facets: true,
            group: Some(CreatorGroupFilter::Uncategorized),
            include_archived: false,
        }
    );
    assert_golden!(
        MANUAL_BATCH_RESULT,
        ManualCreatorBatchCreateResult {
            outcomes: vec![
                ManualCreatorBatchItemOutcome::Accepted {
                    input_index: 0,
                    creator: ManualCreatorCreateResult {
                        creator_id: CreatorId::new("creator-1"),
                        display_name: "Creator One".into(),
                        tags: Vec::new(),
                        stage: CreatorStage::Prospect,
                        created_at: 1,
                        contact_routes: vec![contact_route_record()],
                    },
                },
                ManualCreatorBatchItemOutcome::Rejected {
                    input_index: 1,
                    message: "duplicate".into(),
                },
            ],
            replayed: false,
        }
    );
    assert_golden!(GROUP, group());
    assert_golden!(
        GROUP_LIST,
        CreatorGroupListResult {
            items: vec![CreatorGroupSummary {
                group: group(),
                creator_count: 4,
            }],
            uncategorized_count: 2,
            next_cursor: Some("next".into()),
        }
    );
    assert_golden!(
        GROUP_MOVE,
        MoveCreatorsToGroupResult {
            assignments: vec![
                CreatorGroupAssignment {
                    creator_id: CreatorId::new("creator-1"),
                    group: Some(CreatorGroupRef {
                        id: CreatorGroupId::new("group-1"),
                        name: "Launch".into(),
                    }),
                },
                CreatorGroupAssignment {
                    creator_id: CreatorId::new("creator-2"),
                    group: None,
                },
            ],
            moved_count: 1,
            unchanged_count: 1,
        }
    );
    assert_golden!(
        GROUP_DESTINATIONS,
        vec![
            CreatorGroupDestination::Uncategorized,
            CreatorGroupDestination::Group {
                group_id: CreatorGroupId::new("group-1"),
            },
        ]
    );
    assert_golden!(
        GROUP_FILTERS,
        vec![
            CreatorGroupFilter::Uncategorized,
            CreatorGroupFilter::Group {
                group_id: CreatorGroupId::new("group-1"),
            },
        ]
    );
    assert_golden!(
        CONTACT_ROUTE_ADD,
        ManualCreatorContactRouteAddResult {
            status: ManualCreatorContactRouteAddStatus::Added,
            creator_id: CreatorId::new("creator-1"),
            route_id: CreatorContactRouteId::new("route-1"),
            channel: ContactChannel::Email,
            locator_kind: LocatorKind::EmailAddress,
            normalized_value: "person@example.com".into(),
            display_value: "person@example.com".into(),
            confidence: Confidence::Asserted,
            created_at: 1,
        }
    );
    assert_golden!(
        CONTACT_ROUTE_UPDATE,
        ManualCreatorContactRouteUpdateResult {
            status: ManualCreatorContactRouteUpdateStatus::ReplacementRequired,
            creator_id: CreatorId::new("creator-1"),
            route_id: CreatorContactRouteId::new("route-1"),
            route: contact_route_record(),
        }
    );
    assert_golden!(
        CONTACT_ROUTE_DELETE_PREPARE,
        vec![
            PrepareCreatorContactRouteDeleteResult::Blocked {
                creator_id: CreatorId::new("creator-1"),
                contact_route_id: CreatorContactRouteId::new("route-1"),
                reason: CreatorContactRouteDeleteBlockReason::HasDerivedContactRoutes,
                children: vec![CreatorContactRouteDeleteChildSummary {
                    creator_id: CreatorId::new("creator-2"),
                    contact_route_id: CreatorContactRouteId::new("route-2"),
                    dependency_kind: CreatorContactRouteDependencyKind::LinkHub,
                    channel: ContactChannel::Telegram,
                    display_value: "@child".into(),
                }],
                blocking_artifacts: vec![CreatorContactRouteDeleteBlockingArtifact {
                    kind: CreatorContactRouteDeleteBlockingArtifactKind::InFlightEmail,
                    count: 2,
                }],
            },
            PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
                operation_id: CreatorContactRouteDeleteOperationId::new("operation-1"),
                creator_id: CreatorId::new("creator-1"),
                contact_route_id: CreatorContactRouteId::new("route-1"),
                impact: CreatorContactRouteDeleteImpact {
                    contact_routes: 1,
                    email_threads: 2,
                    local_records_total: 3,
                    ..CreatorContactRouteDeleteImpact::default()
                },
                remote_not_deleted: true,
                expires_at: 99,
                next_tool: "onecrew.creator.contact_route.delete.confirm".into(),
            },
        ]
    );
    assert_golden!(
        CONTACT_ROUTE_DELETE_CONFIRM,
        vec![
            ConfirmCreatorContactRouteDeleteResult::Deleted {
                operation_id: CreatorContactRouteDeleteOperationId::new("operation-1"),
                creator_id: CreatorId::new("creator-1"),
                contact_route_id: CreatorContactRouteId::new("route-1"),
                deleted: CreatorContactRouteDeleteImpact::default(),
                remote_not_deleted: false,
            },
            ConfirmCreatorContactRouteDeleteResult::Stale {
                operation_id: CreatorContactRouteDeleteOperationId::new("operation-1"),
                reason: CreatorContactRouteDeleteStaleReason::PlanChanged,
                next_tool: "onecrew.creator.contact_route.delete.prepare".into(),
            },
            ConfirmCreatorContactRouteDeleteResult::Expired {
                operation_id: CreatorContactRouteDeleteOperationId::new("operation-1"),
                next_tool: "onecrew.creator.contact_route.delete.prepare".into(),
            },
        ]
    );
    assert_golden!(
        RESOLVE_CONTACT_TARGET_INPUT,
        ResolveContactTargetInput {
            creator_id: CreatorId::new("creator-1"),
            contact_route_id: CreatorContactRouteId::new("route-1"),
            operator_account_ref_id: AccountRefId::new("account-1"),
            refresh: true,
        }
    );
    assert_golden!(
        OBSERVATION_INPUTS,
        (
            UpsertPlatformAccountInput {
                platform: Platform::Telegram,
                target_kind: TargetKind::User,
                external_id: "tg-200".into(),
            },
            PlatformAccountObservationInput {
                platform_account_id: PlatformAccountId::new("account-x"),
                source_id: ObservationSourceId::new("source-1"),
                handle: Some("creator_one".into()),
                display_name: None,
                profile_url: Some("https://x.com/creator_one".into()),
                public_data: serde_json::json!({"profile_image_url": "https://img/new.png"}),
            },
            ObservationSourceInput {
                kind: SourceKind::ProfileBio,
                platform: Some(Platform::X),
                source_url: None,
                external_ref: Some("conflict-source".into()),
                account_ref_id: Some(AccountRefId::new("account-1")),
                metadata: serde_json::json!({}),
            },
            ContactLocatorInput {
                channel: ContactChannel::Email,
                kind: LocatorKind::EmailAddress,
                normalized_value: "person@example.com".into(),
                display_value: "Person@Example.com".into(),
            },
        )
    );
}

#[test]
fn manual_contact_route_inputs_reject_legacy_confirmed_field() {
    let add = serde_json::json!({
        "creator_id": "creator-manual",
        "channel": "email",
        "value": "person@example.com",
        "source_url": null,
        "confirmed": true,
    });
    assert!(serde_json::from_value::<ManualCreatorContactRouteAddInput>(add).is_err());

    let update = serde_json::json!({
        "creator_id": "creator-manual",
        "contact_route_id": "route-manual",
        "channel": "email",
        "value": "person@example.com",
        "source_url": null,
        "confirmed": false,
    });
    assert!(serde_json::from_value::<ManualCreatorContactRouteUpdateInput>(update).is_err());
}
