use super::*;
use crate::operation_registry::{
    descriptor::{ApprovalClass, RegistryEntry},
    execution::{ExecutionContext, dispatch_prepared, prepare_invocation},
    v2::{ToolGetInput, ToolSearchInput},
};
use serde_json::json;

const TOPICS: [&str; 8] = [
    "find-batch",
    "find-one",
    "find-replies",
    "find-admins",
    "enrich-followers",
    "enrich-scores",
    "enrich-routes",
    "enrich-one",
];

#[test]
fn export_workflow_guides_node_oracle() {
    let Some(directory) = std::env::var_os("ONECREW_WORKFLOW_GUIDES_ORACLE_DIRECTORY") else { return; };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let guides = registry::registered().unwrap();
    let mut outputs = serde_json::Map::new();
    outputs.insert("directory".into(), serde_json::to_value(contract::project(guides, serde_json::from_value(json!({})).unwrap())).unwrap());
    for topic in TOPICS {
        outputs.insert(topic.into(), serde_json::to_value(contract::project(guides, serde_json::from_value(json!({"topic": topic})).unwrap())).unwrap());
    }
    std::fs::write(directory.join("workflow-guides.json"), serde_json::to_vec_pretty(&outputs).unwrap()).unwrap();
}

#[test]
fn workflow_operation_references_must_exist_in_the_supplied_catalog() {
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(&[
            onecrew_local_runtime::OperationId::ONECREW_WORKFLOW_GUIDE.as_str(),
        ]));
    assert!(matches!(
        register(&mut builder),
        Err(RegistryBuildError::UnknownCatalogOperation(_))
    ));
}

#[test]
fn registered_definitions_directory_lookup_and_topic_schema_share_one_source() {
    let guides = registry::registered().unwrap();
    let topics = guides
        .definitions()
        .map(|definition| definition.topic)
        .collect::<Vec<_>>();
    assert_eq!(topics, TOPICS);
    assert_eq!(
        topics
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        topics.len()
    );
    let schema = serde_json::to_value(schemars::schema_for!(contract::Topic)).unwrap();
    assert_eq!(schema["enum"], json!(topics));
    let directory = serde_json::to_value(contract::project(
        guides,
        serde_json::from_value(json!({})).unwrap(),
    ))
    .unwrap();
    let directory_topics = directory["categories"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| {
            group["topics"]
                .as_array()
                .unwrap()
                .iter()
                .map(|topic| topic["topic"].as_str().unwrap())
        })
        .collect::<Vec<_>>();
    assert_eq!(directory_topics, topics);
    for definition in guides.definitions() {
        assert!(std::ptr::eq(
            guides.lookup(definition.topic).unwrap(),
            definition
        ));
        let parsed: contract::Topic = serde_json::from_value(json!(definition.topic)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), definition.topic);
    }
    assert!(guides.lookup("unregistered-topic").is_none());
    assert!(serde_json::from_value::<contract::Topic>(json!("unregistered-topic")).is_err());
    for retired in ["outreach-send", "outreach-batch", "outreach-followup", "outreach-sync"] {
        assert!(guides.lookup(retired).is_none());
        assert!(serde_json::from_value::<contract::Topic>(json!(retired)).is_err());
    }
}

#[test]
fn agent_facing_workflow_payload_uses_fixed_english_copy() {
    let guides = registry::registered().unwrap();
    let directory = serde_json::to_value(contract::project(
        guides,
        serde_json::from_value(json!({})).unwrap(),
    ))
    .unwrap();
    let mut texts: Vec<String> = Vec::new();
    for category in directory["categories"].as_array().unwrap() {
        texts.push(category["title"].as_str().unwrap().to_owned());
        for topic in category["topics"].as_array().unwrap() {
            texts.push(topic["title"].as_str().unwrap().to_owned());
        }
    }
    for definition in guides.definitions() {
        let output = serde_json::to_value(contract::project(
            guides,
            serde_json::from_value(json!({"topic": definition.topic})).unwrap(),
        ))
        .unwrap();
        let guide = &output["guide"];
        texts.push(guide["title"].as_str().unwrap().to_owned());
        texts.push(guide["confirmation_boundary"].as_str().unwrap().to_owned());
        texts.push(guide["example"].as_str().unwrap().to_owned());
        texts.extend(
            guide["questions"]
                .as_array()
                .unwrap()
                .iter()
                .chain(guide["workflow"].as_array().unwrap())
                .chain(guide["tool_discovery_queries"].as_array().unwrap())
                .chain(guide["expected_output"].as_array().unwrap())
                .map(|value| value.as_str().unwrap().to_owned()),
        );
        if definition.topic == "find-batch" {
            let workflow = guide["workflow"]
                .as_array()
                .unwrap()
                .iter()
                .map(|step| step.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ");
            assert!(workflow.contains(
                "actual user-facing questions/plans/explanations in the user's language"
            ));
        }
    }
    for text in texts {
        assert!(
            !text
                .chars()
                .any(|character| ('\u{4e00}'..='\u{9fff}').contains(&character)),
            "agent-facing workflow copy is not fixed English: {text}"
        );
    }
}

#[test]
fn guide_registration_rejects_duplicate_topics_unknown_categories_and_missing_metadata() {
    use definition::{GuideDefinition, GuideGroup};
    use registry::{GuideRegistrationError, GuideRegistry};
    const BASE: GuideDefinition = find::GROUP.definitions[0];
    let duplicate = GuideRegistry::build(&[GuideGroup {
        category: "find",
        title: "Find creators",
        definitions: &[BASE, BASE],
    }])
    .unwrap_err();
    assert_eq!(
        duplicate,
        GuideRegistrationError::DuplicateTopic(BASE.topic)
    );
    let unknown = GuideRegistry::build(&[GuideGroup {
        category: "schedule",
        ..find::GROUP
    }])
    .unwrap_err();
    assert_eq!(unknown, GuideRegistrationError::UnknownCategory("schedule"));
    let duplicate_category = GuideRegistry::build(&[find::GROUP, find::GROUP]).unwrap_err();
    assert_eq!(
        duplicate_category,
        GuideRegistrationError::DuplicateCategory("find")
    );
    let invalid_topic = GuideRegistry::build(&[GuideGroup {
        definitions: &[GuideDefinition {
            topic: "bad topic",
            ..BASE
        }],
        ..find::GROUP
    }])
    .unwrap_err();
    assert_eq!(
        invalid_topic,
        GuideRegistrationError::InvalidTopic("bad topic")
    );
    for definitions in [
        &[GuideDefinition { title: "", ..BASE }][..],
        &[GuideDefinition {
            questions: &[],
            ..BASE
        }][..],
        &[GuideDefinition {
            workflow: " ",
            ..BASE
        }][..],
        &[GuideDefinition {
            queries: &[""],
            ..BASE
        }][..],
        &[GuideDefinition {
            expected: "",
            ..BASE
        }][..],
        &[GuideDefinition {
            example: "",
            ..BASE
        }][..],
    ] {
        assert_eq!(
            GuideRegistry::build(&[GuideGroup {
                definitions,
                ..find::GROUP
            }])
            .unwrap_err(),
            GuideRegistrationError::MissingMetadata(BASE.topic)
        );
    }
    assert!(
        GuideRegistry::build(&[GuideGroup {
            title: "",
            ..find::GROUP
        }])
        .is_err()
    );
    assert!(
        GuideRegistry::build(&[GuideGroup {
            definitions: &[],
            ..find::GROUP
        }])
        .is_err()
    );
    assert!(GuideRegistry::build(&[]).is_err());
}

#[test]
fn adding_a_definition_only_requires_group_registration() {
    use definition::{GuideDefinition, GuideGroup};
    const EXTRA: GuideDefinition = GuideDefinition {
        topic: "find-new-scenario",
        title: "New example",
        ..find::GROUP.definitions[0]
    };
    const EXTENDED_DEFINITIONS: &[GuideDefinition] = &[find::GROUP.definitions[0], EXTRA];
    let guides = registry::GuideRegistry::build(&[GuideGroup {
        definitions: EXTENDED_DEFINITIONS,
        ..find::GROUP
    }])
    .unwrap();
    assert_eq!(guides.lookup(EXTRA.topic).unwrap().title, EXTRA.title);
    assert_eq!(
        guides
            .definitions()
            .map(|definition| definition.topic)
            .collect::<Vec<_>>(),
        ["find-batch", EXTRA.topic]
    );
    assert_eq!(guides.groups()[0].definitions.len(), 2);
}

#[test]
fn update_guides_compose_platform_reads_creator_update_and_creator_save_without_refresh() {
    let guides = registry::registered().unwrap();
    let one = guides.lookup("enrich-one").unwrap();
    assert_eq!(
        one.operation_ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        [
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE.as_str(),
            onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
            onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT.as_str(),
        ]
    );
    // Three independent paths in order, then one readback: the platform read
    // returns native platform data, explicit fields go through Creator update,
    // and observations and new contact routes append through the Creator save.
    let position = |marker: &str| {
        one.workflow
            .find(marker)
            .unwrap_or_else(|| panic!("enrich-one workflow does not name {marker}"))
    };
    for [earlier, later] in [
        ["native platform data", "onecrew.creator.update"],
        ["onecrew.creator.update", "onecrew.creator.candidate.save"],
        ["onecrew.creator.candidate.save", "onecrew.link_hub.extract"],
        ["onecrew.link_hub.extract", "read back"],
    ] {
        assert!(
            position(earlier) < position(later),
            "{earlier} before {later}"
        );
    }
    for topic in ["enrich-one", "enrich-followers"] {
        let guide = guides.lookup(topic).unwrap();
        let texts = [guide.title, guide.workflow, guide.expected, guide.example]
            .into_iter()
            .chain(guide.questions.iter().copied())
            .chain(guide.queries.iter().copied());
        for text in texts {
            assert!(
                !text.to_ascii_lowercase().contains("refresh"),
                "{topic} still names refresh: {text}"
            );
        }
    }
}

#[test]
fn discovery_guidance_is_platform_neutral_and_bounded_before_candidate_review() {
    let guides = registry::registered().unwrap();
    let workflow = |topic: &str| guides.lookup(topic).unwrap().workflow;
    let find = workflow("find-batch");
    let score = workflow("enrich-scores");

    for text in [find, score] {
        for marker in [
            "content or account leads",
            "stable platform identity",
            "matched, unmatched, or unverified",
            "unknown does not mean unmatched",
            "one targeted supplementary read",
            "mark unknown and stop when the same gap remains unverifiable",
            "stop searching after reaching the matching target",
            "actual cursor",
        ] {
            assert!(
                text.to_ascii_lowercase()
                    .contains(&marker.to_ascii_lowercase()),
                "{marker} missing from discovery guidance"
            );
        }
    }

    for marker in [
        "excellent 95",
        "clear match with direct concrete evidence",
        "distinctive advantage",
        "good 85",
        "enough existing content evidence to support recommendation",
        "medium 70",
        "recommendation limitation",
        "low 40",
        "specific, citable, significant recommendation weakness",
        "Only when the user explicitly provides a minimum tier",
        "the tier become an eligibility gate",
        "deduplicate only by platform + stable ID",
        "The same connection and read depth are used only to reuse equivalent reads",
        "When delivering the candidate list, prepare one review using the existing candidate batch save flow",
    ] {
        assert!(
            find.to_ascii_lowercase()
                .contains(&marker.to_ascii_lowercase()),
            "{marker} missing from canonical find rubric"
        );
    }

    for marker in [
        "per-candidate",
        "concrete facts",
        "do not add",
        "do not re-verify candidates in the same tier",
    ] {
        assert!(
            find.to_ascii_lowercase()
                .contains(&marker.to_ascii_lowercase()),
            "{marker} missing from find guidance"
        );
        assert!(
            score
                .to_ascii_lowercase()
                .contains(&marker.to_ascii_lowercase()),
            "{marker} missing from score guidance"
        );
    }

    for marker in [
        "add_new",
        "save_or_update",
        "fresh platform data",
        "prepare",
        "present",
        "preparation does not write",
        "does not auto-confirm",
        "do not issue an independent profile/channel get just because of saving",
        "eligible_new",
        "existing_skipped",
        "identity_pending",
        "real provenance",
    ] {
        assert!(
            find.to_ascii_lowercase()
                .contains(&marker.to_ascii_lowercase()),
            "{marker} missing from candidate review guidance"
        );
    }

    assert!(!find.contains("YouTube-only"));
    assert!(!find.contains("video-only"));
}

#[test]
fn find_batch_guidance_explains_review_delivery_and_targeted_supplement_reads() {
    let workflow = registry::registered()
        .unwrap()
        .lookup("find-batch")
        .unwrap()
        .workflow;
    for marker in [
        "prepare one review using the existing candidate batch save flow",
        "<=50",
        "numeric score",
        "per-candidate assessment",
        "A pure Markdown list is not the default completion",
        "the user explicitly requests plain text",
        "no card",
        "actual capability is blocked",
        "do not silently trigger a write",
        "which fact is missing from the search results",
        "which value would change the match or tier",
        "recent publication or content evidence already exists",
        "get every channel again for review",
        "unrelated quality due diligence",
    ] {
        assert!(
            workflow
                .to_ascii_lowercase()
                .contains(&marker.to_ascii_lowercase()),
            "missing {marker}"
        );
    }
}

#[tokio::test]
async fn guide_discovery_contract_and_calls_need_no_runtime_owner() {
    // Construct and invoke without LocalPlatform, the Creator runtime, credentials, or a data directory.
    let mut builder =
        RegistryBuilder::new(crate::operation_registry::catalog_test_support::catalog(
            const {
                &[
                    onecrew_local_runtime::OperationId::ONECREW_WORKFLOW_GUIDE.as_str(),
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE.as_str(),
                    onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str(),
                    onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT.as_str(),
                ]
            },
        ));
    #[derive(serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    for id in [
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_UPDATE,
        onecrew_local_runtime::OperationId::ONECREW_CREATOR_CANDIDATE_SAVE,
        onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT,
    ] {
        builder
            .register_operation::<Empty, Empty, Infallible, _, _, _>(
                OperationRegistration {
                    id,
                    domain_id: "test".into(),
                    aliases: vec![],
                    surfaces: SurfaceSet::new([SurfaceId::Full]).unwrap(),
                },
                OperationPolicy::read_only(
                    EffectClass::ReadLocal,
                    ConnectionRequirement::None,
                    crate::operation_registry::execution::ExecutionDeclaration::Sequential,
                ),
                |_, _| async { Ok(Empty {}) },
                |never| match never {},
            )
            .unwrap();
    }
    register(&mut builder).unwrap();
    let registry = builder.build().unwrap();
    let id = EntryId::parse("onecrew.workflow.guide").unwrap();
    for surface in [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators] {
        for query in [
            "workflow guidance",
            "find creators guidance",
            "enrich profiles guidance",
        ] {
            let search: ToolSearchInput = serde_json::from_value(json!({"query":query})).unwrap();
            let result = registry.tool_search_v2(surface, &search);
            assert!(result.operations.iter().any(|operation| operation.id == id));
        }
        let get: ToolGetInput =
            serde_json::from_value(json!({"operation_ids":[id], "include_output_schema":true}))
                .unwrap();
        let contract = registry.tool_get_v2(surface, &get).unwrap();
        assert_eq!(contract.operations.len(), 1);
        assert!(contract.operations[0].output_schema.is_some());
        let RegistryEntry::Operation(operation) = registry.entry(&id, surface).unwrap() else {
            panic!("operation")
        };
        assert_eq!(operation.effect, EffectClass::ReadLocal);
        assert_eq!(operation.connection, ConnectionRequirement::None);
        assert_eq!(operation.approval, ApprovalClass::None);
        for topic in std::iter::once(None).chain(TOPICS.into_iter().map(Some)) {
            let args = topic.map_or_else(|| json!({}), |topic| json!({"topic":topic}));
            let context = ExecutionContext {
                surface,
                request_id: "guide-read".into(),
                connection: None,
                confirmed: false,
            };
            let invocation =
                prepare_invocation(operation, context, operation.input.revision(), args).unwrap();
            let result = dispatch_prepared(invocation).await.unwrap();
            if let Some(topic) = topic {
                assert_eq!(result["kind"], "guide");
                assert_eq!(result["guide"]["topic"], topic);
                for key in [
                    "questions",
                    "workflow",
                    "tool_discovery_queries",
                    "expected_output",
                ] {
                    assert!(!result["guide"][key].as_array().unwrap().is_empty());
                }
                let workflow = result["guide"]["workflow"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|step| step.as_str().unwrap())
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(workflow.contains("onecrew_tool_search"));
                assert!(workflow.contains("onecrew_tool_get"));
                if topic == "find-batch" {
                    for marker in [
                        "quality or geographic evidence is missing",
                        "do not pre-read profile solely for a later save",
                        "stable channel ID",
                        "same connection",
                        "depth: light or full",
                        "light intentionally omitting comments",
                        "real provenance",
                        "intent",
                        "eligible_new",
                        "existing_skipped",
                        "originally approved scope",
                        "do not accept Agent profiles or caches",
                    ] {
                        assert!(
                            workflow
                                .to_ascii_lowercase()
                                .contains(&marker.to_ascii_lowercase()),
                            "find-batch guide misses {marker}"
                        );
                    }
                }
                assert!(!result["guide"]["example"].as_str().unwrap().is_empty());
                assert!(
                    result["guide"]["confirmation_boundary"]
                        .as_str()
                        .unwrap()
                        .contains("GmailBulk on the Mail page")
                );
            } else {
                assert_eq!(result["kind"], "directory");
                let categories = result["categories"].as_array().unwrap();
                assert_eq!(categories.len(), 2);
                let actual = categories
                    .iter()
                    .flat_map(|category| {
                        category["topics"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|entry| entry["topic"].as_str().unwrap())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, TOPICS);
            }
        }
        for invalid in [
            json!({"topic":"outreach-send"}),
            json!({"topic":"outreach-batch"}),
            json!({"topic":"outreach-followup"}),
            json!({"topic":"outreach-sync"}),
            json!({"topic":"schedule-find"}),
            json!({"topic":"unknown"}),
            json!({"send":true}),
        ] {
            let context = ExecutionContext {
                surface,
                request_id: "invalid-guide".into(),
                connection: None,
                confirmed: false,
            };
            assert!(
                prepare_invocation(operation, context, operation.input.revision(), invalid)
                    .is_err()
            );
        }
    }
    for surface in [
        SurfaceId::Accounts,
        SurfaceId::Mail,
        SurfaceId::Outreach,
        SurfaceId::Skills,
    ] {
        assert!(registry.entry(&id, surface).is_none());
    }
}
