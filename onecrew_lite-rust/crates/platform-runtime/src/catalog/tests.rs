use serde_json::{Value, json};

use super::{
    CapabilityCatalog, CatalogBuildError, CatalogSurface, OperationId, OperationSummary, ToolkitId,
    ToolkitSummary,
};

const TOOLKIT: ToolkitSummary = ToolkitSummary {
    id: "toolkit.x",
    toolkit: ToolkitId::X,
    title: "Fixture platform",
    summary: "Explicit fixture platform description.",
    purpose: "Exercise an isolated static catalog.",
    connection_model: "No provider connection is used by this fixture.",
    identifier_semantics: &[],
    capability_categories: &[],
    gotchas: &[],
};
const OPERATION: OperationSummary = OperationSummary {
    id: OperationId::from_static("fixture.read"),
    toolkit_id: ToolkitId::X,
    title: "Fixture read",
    summary: "Describe one fixture read.",
    surfaces: &[CatalogSurface::McpBinding],
};

fn navigation(catalog: &CapabilityCatalog) -> Value {
    Value::Array(
        catalog
            .toolkits()
            .filter_map(|toolkit| {
                let operations = catalog
                    .operations()
                    .filter(|operation| {
                        operation.toolkit_id == toolkit.toolkit
                            && operation.supports(CatalogSurface::WebNavigation)
                            && !crate::is_retired_operation(operation.id.as_str())
                    })
                    .map(|operation| {
                        json!({
                            "id": operation.id.as_str(),
                            "title": operation.title,
                            "summary": operation.summary,
                        })
                    })
                    .collect::<Vec<_>>();
                if operations.is_empty() {
                    return None;
                }
                Some(json!({
                    "id": toolkit.id,
                    "toolkit": toolkit.toolkit,
                    "title": toolkit.title,
                    "summary": toolkit.summary,
                    "purpose": toolkit.purpose,
                    "connection_model": toolkit.connection_model,
                    "identifier_semantics": toolkit.identifier_semantics,
                    "capability_categories": toolkit.capability_categories,
                    "gotchas": toolkit.gotchas,
                    "operations": operations,
                }))
            })
            .collect(),
    )
}

fn mask_search_plan_guidance(value: Value) -> Value {
    match value {
        Value::Array(values) => {
            Value::Array(values.into_iter().map(mask_search_plan_guidance).collect())
        }
        Value::Object(fields) => {
            let is_search_plan =
                fields.get("id").and_then(Value::as_str) == Some("onecrew.search.plan.prepare");
            Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| {
                        if is_search_plan && key == "summary" {
                            (
                                key,
                                Value::String("<current search-plan guidance>".to_owned()),
                            )
                        } else {
                            (key, mask_search_plan_guidance(value))
                        }
                    })
                    .collect(),
            )
        }
        value => value,
    }
}

#[test]
fn production_navigation_matches_frozen_directory_content_and_order() {
    let catalog = CapabilityCatalog::build().unwrap();
    let expected: Value = serde_json::from_str(include_str!("fixtures/directory-full.json"))
        .expect("pre-migration directory fixture");
    let mut current = navigation(&catalog);
    let operations = current
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|toolkit| toolkit["id"] == "toolkit.onecrew")
        .unwrap()["operations"]
        .as_array_mut()
        .unwrap();
    assert_eq!(
        operations
            .iter()
            .filter(|operation| operation["id"] == "onecrew.discovery.draft.prepare")
            .collect::<Vec<_>>(),
        vec![
            &json!({"id":"onecrew.discovery.draft.prepare", "title":"Prepare a product search draft with local Codex",
            "summary":"Web-only model bridge: use locally signed-in Codex for bounded tool-free planning and validate an unapproved Creator draft. No plugin transport, Campaign approval, provider search or outreach. Lost responses read back the stable draft ID. MCP agents use their own model and campaign.create."})
        ]
    );
    operations.retain(|operation| operation["id"] != "onecrew.discovery.draft.prepare");
    // Target adjustment now supports both directions. Pin that intentional
    // contract change explicitly, then compare the remaining historical export.
    let target = operations
        .iter_mut()
        .find(|operation| operation["id"] == "onecrew.discovery.target.increase")
        .unwrap();
    assert_eq!(
        *target,
        json!({"id":"onecrew.discovery.target.increase","title":"Adjust discovery Campaign target","summary":"Record an authenticated, idempotent decision to raise or lower the approved Campaign target while preserving frozen criteria, sources and read budget."})
    );
    let identity_ids = [
        "onecrew.discovery.identity.material.get",
        "onecrew.discovery.identity.groups.get",
        "onecrew.discovery.identity.review.get",
        "onecrew.discovery.identity.review.submit",
        "onecrew.discovery.identity.review.revoke",
    ];
    assert_eq!(
        operations
            .iter()
            .filter(|operation| identity_ids.contains(&operation["id"].as_str().unwrap()))
            .cloned()
            .collect::<Vec<_>>(),
        vec![
            json!({"id":"onecrew.discovery.identity.groups.get","title":"Read Campaign author groups","summary":"Read current evidence-validated account groups and independent member qualifications. Native-only groups do not prove distinct people."}),
            json!({"id":"onecrew.discovery.identity.material.get","title":"Read saved identity comparison material","summary":"Read source-bound native account evidence for two saved hits. No provider call or identity decision."}),
            json!({"id":"onecrew.discovery.identity.review.get","title":"Read saved identity review","summary":"Read an account-scoped immutable identity decision receipt before retrying an uncertain write."}),
            json!({"id":"onecrew.discovery.identity.review.revoke","title":"Revoke an identity review","summary":"Append an authenticated correction to an existing identity decision. Preserve original evidence and independent member qualifications."}),
            json!({"id":"onecrew.discovery.identity.review.submit","title":"Submit source-bound identity review","summary":"Record a citation-backed account relationship judgment using saved material and authenticated actor. Never infers identity from names or transfers qualification."})
        ]
    );
    // The current fixture already includes these five identity operations.
    // Compare them in place so content and ordering remain covered.
    assert_eq!(
        catalog
            .operation("onecrew.discovery.draft.prepare")
            .unwrap()
            .surfaces,
        &[CatalogSurface::WebNavigation]
    );
    // This fixture remains a historical baseline: only the approved navigation
    // copy is updated, while unrelated entries and array ordering stay frozen.
    assert_eq!(
        mask_search_plan_guidance(current),
        mask_search_plan_guidance(expected)
    );
    assert_eq!(catalog.toolkits().len(), 6);
    assert_eq!(catalog.operations().len(), 160);
    assert!(catalog.operation("onecrew.discovery.search_once").is_some());
    assert_eq!(
        catalog
            .operation(OperationId::ONECREW_CREATOR_LIST.as_str())
            .unwrap()
            .id,
        OperationId::ONECREW_CREATOR_LIST,
    );
}

#[test]
fn current_search_plan_guidance_uses_user_facing_direct_search_language() {
    let catalog = CapabilityCatalog::build().unwrap();
    let summary = catalog
        .operation(OperationId::ONECREW_SEARCH_PLAN_PREPARE.as_str())
        .unwrap()
        .summary;

    assert!(!summary.contains("user_requested_direct"));
    assert_eq!(
        summary,
        "Confirm external creator/content search criteria and keywords; no provider call or persisted plan. Creator discovery ends in a candidate review card, not a write. confirmed_plan_ref expires 60 minutes after confirmation: page freely with next_cursor; changed criteria need a new confirmed plan. Skip only if the current user explicitly requests direct search."
    );
}

#[test]
fn discovery_catalog_requires_quality_reads_but_not_save_only_prefetch() {
    let catalog = CapabilityCatalog::build().unwrap();
    let x = catalog.toolkit(ToolkitId::X).unwrap();
    let youtube = catalog.toolkit(ToolkitId::Youtube).unwrap();
    let onecrew = catalog.toolkit(ToolkitId::OneCrew).unwrap();

    assert!(
        x.gotchas
            .iter()
            .any(|text| text.contains("only when") && text.contains("evidence"))
    );
    assert!(
        x.gotchas
            .iter()
            .any(|text| text.contains("provenance") && text.contains("save"))
    );
    assert!(youtube.gotchas.iter().any(|text| {
        text.contains("stable channel ID") && text.contains("across connections and read depths")
    }));
    assert!(
        youtube
            .gotchas
            .iter()
            .any(|text| text.contains("depth: light or full") && text.contains("omits comments"))
    );
    assert!(
        onecrew
            .identifier_semantics
            .iter()
            .any(|text| text.contains("explicit intent") && text.contains("save_or_update"))
    );
    assert!(
        onecrew
            .gotchas
            .iter()
            .any(|text| text.contains("Owner statuses") && text.contains("eligible_new"))
    );
    assert!(
        youtube
            .gotchas
            .iter()
            .any(|text| text.contains("only when") && text.contains("quality"))
    );

    let x_search = catalog
        .operation(OperationId::X_POSTS_SEARCH_ADVANCED.as_str())
        .unwrap()
        .summary;
    let youtube_search = catalog
        .operation(OperationId::YOUTUBE_VIDEOS_SEARCH.as_str())
        .unwrap()
        .summary;
    let youtube_channels_search = catalog
        .operation(OperationId::YOUTUBE_CHANNELS_SEARCH.as_str())
        .unwrap()
        .summary;
    let save = catalog
        .operation(OperationId::ONECREW_CREATOR_CANDIDATE_SAVE.as_str())
        .unwrap()
        .summary;
    let batch_save = catalog
        .operation(OperationId::ONECREW_CREATOR_CANDIDATE_BATCH_SAVE.as_str())
        .unwrap()
        .summary;
    assert!(!x_search.contains("Verify authors using x.profile.get"));
    assert!(
        youtube
            .gotchas
            .iter()
            .all(|text| !text.contains("Use youtube.channel.get before Creator save"))
    );
    assert!(x_search.contains("without an extra profile read"));
    assert!(youtube_search.contains("without an extra channel read"));
    assert!(youtube_search.contains("Dedupe IDs"));
    assert!(youtube_channels_search.contains("Dedupe IDs"));
    assert!(save.contains("explicit intent (add_new or save_or_update)"));
    assert!(save.contains("owner obtains fresh platform data"));
    assert!(save.contains("need not prefetch"));
    assert!(batch_save.contains("each candidate's explicit intent"));
    assert!(batch_save.contains("owner obtains fresh platform data"));
}

#[test]
fn fixture_catalog_is_explicit_and_navigation_is_not_mcp_support() {
    static OPERATIONS: &[OperationSummary] = &[
        OPERATION,
        OperationSummary {
            id: OperationId::from_static("fixture.navigation"),
            surfaces: &[CatalogSurface::WebNavigation],
            ..OPERATION
        },
    ];
    let empty = CapabilityCatalog::from_static(&[], &[]).unwrap();
    assert_eq!(empty.toolkits().len(), 0);
    assert_eq!(empty.operations().len(), 0);
    let catalog = CapabilityCatalog::from_static(&[TOOLKIT], OPERATIONS).unwrap();
    assert_eq!(catalog.operations().len(), 2);
    assert!(catalog.operation("onecrew.creator.list").is_none());
    let navigation_only = catalog.operation("fixture.navigation").unwrap();
    assert!(!navigation_only.supports(CatalogSurface::McpBinding));
    assert!(navigation_only.supports(CatalogSurface::WebNavigation));
    assert_eq!(
        navigation(&catalog)[0]["operations"],
        json!([{
            "id": "fixture.navigation", "title": OPERATION.title, "summary": OPERATION.summary,
        }])
    );
}

#[test]
fn duplicate_identity_and_invalid_references_are_rejected() {
    assert_eq!(
        CapabilityCatalog::from_static(&[TOOLKIT, TOOLKIT], &[]).unwrap_err(),
        CatalogBuildError::DuplicateToolkitId,
    );
    assert_eq!(
        CapabilityCatalog::from_static(
            &[
                TOOLKIT,
                ToolkitSummary {
                    id: "toolkit.other",
                    ..TOOLKIT
                },
            ],
            &[]
        )
        .unwrap_err(),
        CatalogBuildError::DuplicateToolkitIdentity,
    );
    assert_eq!(
        CapabilityCatalog::from_static(&[TOOLKIT], &[OPERATION, OPERATION]).unwrap_err(),
        CatalogBuildError::DuplicateOperationId,
    );
    assert_eq!(
        CapabilityCatalog::from_static(&[], &[OPERATION]).unwrap_err(),
        CatalogBuildError::DanglingToolkit,
    );
}

#[test]
fn missing_descriptions_and_invalid_surfaces_fail_closed() {
    assert_eq!(
        CapabilityCatalog::from_static(
            &[ToolkitSummary {
                purpose: " ",
                ..TOOLKIT
            }],
            &[]
        )
        .unwrap_err(),
        CatalogBuildError::InvalidStaticDescription,
    );
    assert_eq!(
        CapabilityCatalog::from_static(
            &[ToolkitSummary {
                identifier_semantics: &[""],
                ..TOOLKIT
            }],
            &[]
        )
        .unwrap_err(),
        CatalogBuildError::InvalidStaticDescription,
    );
    assert_eq!(
        CapabilityCatalog::from_static(
            &[TOOLKIT],
            &[OperationSummary {
                title: "",
                ..OPERATION
            }]
        )
        .unwrap_err(),
        CatalogBuildError::InvalidStaticDescription,
    );
    assert_eq!(
        CapabilityCatalog::from_static(
            &[TOOLKIT],
            &[OperationSummary {
                surfaces: &[],
                ..OPERATION
            }]
        )
        .unwrap_err(),
        CatalogBuildError::EmptySurfaces,
    );
    assert_eq!(
        CapabilityCatalog::from_static(
            &[TOOLKIT],
            &[OperationSummary {
                surfaces: &[CatalogSurface::McpBinding, CatalogSurface::McpBinding],
                ..OPERATION
            }]
        )
        .unwrap_err(),
        CatalogBuildError::DuplicateSurface,
    );
    assert!(serde_json::from_str::<CatalogSurface>("\"full\"").is_err());
    assert!(serde_json::from_str::<CatalogSurface>("\"future_surface\"").is_err());
}

#[test]
fn identity_validation_rejects_malformed_static_declarations() {
    static INVALID_OPERATIONS: &[OperationSummary] = &[OperationSummary {
        id: OperationId::from_static("fixture/invalid"),
        ..OPERATION
    }];
    assert_eq!(
        CapabilityCatalog::from_static(
            &[ToolkitSummary {
                id: "INVALID ID",
                ..TOOLKIT
            }],
            &[]
        )
        .unwrap_err(),
        CatalogBuildError::InvalidId,
    );
    assert_eq!(
        CapabilityCatalog::from_static(&[TOOLKIT], INVALID_OPERATIONS).unwrap_err(),
        CatalogBuildError::InvalidId,
    );
}

#[test]
fn toolkit_serialization_and_schema_identity_are_preserved() {
    let expected = json!(["x", "instagram", "telegram", "youtube", "onecrew", "email"]);
    assert_eq!(
        serde_json::to_value([
            ToolkitId::X,
            ToolkitId::Instagram,
            ToolkitId::Telegram,
            ToolkitId::Youtube,
            ToolkitId::OneCrew,
            ToolkitId::Email,
        ])
        .unwrap(),
        expected
    );
    let schema = serde_json::to_value(schemars::schema_for!(ToolkitId)).unwrap();
    assert_eq!(schema["title"], "ToolkitId");
    assert_eq!(schema["enum"], expected);
}
