//! Optional deterministic export for behavior-preserving registry moves.
//! The fixture builds metadata only; no operation or provider is invoked.

use std::{collections::BTreeSet, sync::Arc};

use onecrew_local_runtime::LocalPlatform;
use serde_json::{Value, json};

use crate::{
    accounts::{OperatorAccountWorkflow, PlatformRuntimeOperatorAccountWorkflow},
    server::{McpAdapterServer, McpSurface},
};

use super::{
    RegistryOwners, build_registry,
    descriptor::{EntryMetadata, RegistryEntry, SurfaceId},
    result_contract_schema_manifest,
    v2::ToolGetInput,
};

const SURFACES: [SurfaceId; 7] = [
    SurfaceId::Full,
    SurfaceId::Compact,
    SurfaceId::Accounts,
    SurfaceId::Creators,
    SurfaceId::Outreach,
    SurfaceId::Mail,
    SurfaceId::Skills,
];

#[test]
fn export_native_search_node_contracts() {
    let Some(path) = std::env::var_os("ONECREW_NATIVE_SEARCH_NODE_CONTRACTS") else { return; };
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(LocalPlatform::open(directory.path()).unwrap());
    let creators = platform.creators().unwrap();
    let operator_accounts: Arc<dyn OperatorAccountWorkflow> = Arc::new(PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)));
    let registry = build_registry(RegistryOwners { platform, creators, operator_accounts }, Arc::new(onecrew_local_runtime::CapabilityCatalog::build().unwrap())).unwrap();
    let rows: Vec<_> = ["youtube.channels.search", "youtube.videos.search", "instagram.users.search", "telegram.communities.search", "x.posts.search.advanced"].into_iter().map(|id| {
        let parsed = super::descriptor::EntryId::parse(id).unwrap();
        let RegistryEntry::Operation(operation) = registry.entry(&parsed, SurfaceId::Full).unwrap() else { panic!("search operation missing") };
        assert!(operation.binding.supports_search_plan());
        let connection_value = if id.starts_with("youtube.") { json!({"kind":"service_credential","toolkit":"youtube","credential_id":"selected"}) } else { json!({"kind":"operator_account","toolkit":if id.starts_with("instagram.") {"instagram"} else if id.starts_with("telegram.") {"telegram"} else {"x"},"operator_account_id":"selected"}) };
        let connection: super::descriptor::ConnectionRef = serde_json::from_value(connection_value.clone()).unwrap();
        let arguments = if id.starts_with("x.") { json!({"all_words":["headphone review"]}) } else { json!({"query":"headphone review"}) };
        let prepared = super::execution::prepare_invocation(operation, super::execution::ExecutionContext { surface:SurfaceId::Full, request_id:"native-search-oracle".into(), connection:Some(&connection), confirmed:false }, operation.input.revision(), arguments.clone()).unwrap();
        let token = prepared.search_scope_token(super::execution::SearchScopePurpose::InitialPlan).unwrap().unwrap();
        json!({"operation_id":id,"connection":operation.connection,"input_schema":operation.input.document(),"schema_revision":operation.input.revision(),"initial_scope":{"operation_id":id,"schema_revision":operation.input.revision(),"connection":connection_value,"arguments":arguments},"initial_token":token})
    }).collect();
    std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
}

#[test]
fn retired_operations_are_hidden_from_real_registry_and_contract_reads() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(LocalPlatform::open(directory.path()).unwrap());
    let catalog = Arc::new(onecrew_local_runtime::CapabilityCatalog::build().unwrap());
    let retired = catalog.operations().filter(|operation|
        onecrew_local_runtime::is_retired_operation(operation.id.as_str()))
        .map(|operation| operation.id.as_str()).collect::<Vec<_>>();
    assert_eq!(retired.len(), 30);
    let registry = build_registry(RegistryOwners {
        creators: platform.creators().unwrap(),
        operator_accounts: Arc::new(PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform))),
        platform,
    }, catalog).unwrap();
    for surface in SURFACES {
        assert!(registry.toolkit_directory(surface).iter()
            .flat_map(|toolkit| &toolkit.operations)
            .all(|operation| !onecrew_local_runtime::is_retired_operation(&operation.id)));
        for id in &retired {
            assert!(registry.tool_get_v2(surface, &ToolGetInput {
                operation_ids: vec![super::descriptor::EntryId::parse(*id).unwrap()],
                include_output_schema: true,
            }).is_err());
        }
    }
    for id in ["onecrew.creator.candidate.batch.save", "onecrew.search.plan.prepare", "onecrew.ask_user_question", "email.thread.get", "x.profile.get"] {
        let id = super::descriptor::EntryId::parse(id).unwrap();
        assert!(registry.entry(&id, SurfaceId::Full).is_some());
    }
}

fn metadata(entry: &EntryMetadata) -> Value {
    json!({
        "id": entry.id,
        "title": entry.title,
        "summary": entry.summary,
        "toolkit": entry.toolkit,
        "domain_id": entry.domain_id,
        "aliases": entry.aliases,
        "surfaces": entry.surfaces.iter().collect::<Vec<_>>(),
    })
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

fn assert_search_plan_guidance(value: &Value, expected: &str) -> usize {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| assert_search_plan_guidance(value, expected))
            .sum(),
        Value::Object(fields) => {
            if fields.get("id").and_then(Value::as_str) == Some("onecrew.search.plan.prepare") {
                assert_eq!(
                    fields.get("summary").and_then(Value::as_str),
                    Some(expected)
                );
                1
            } else {
                fields
                    .values()
                    .map(|value| assert_search_plan_guidance(value, expected))
                    .sum()
            }
        }
        _ => 0,
    }
}

fn first_difference(actual: &Value, expected: &Value, path: &str) -> Option<String> {
    if actual == expected {
        return None;
    }
    match (actual, expected) {
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                let next = format!("{path}/{index}");
                match (left.get(index), right.get(index)) {
                    (Some(left), Some(right)) => {
                        if let Some(difference) = first_difference(left, right, &next) {
                            return Some(difference);
                        }
                    }
                    _ => return Some(next),
                }
            }
            Some(path.to_owned())
        }
        (Value::Object(left), Value::Object(right)) => {
            for key in left.keys().chain(right.keys()) {
                let next = format!("{path}/{key}");
                match (left.get(key), right.get(key)) {
                    (Some(left), Some(right)) => {
                        if let Some(difference) = first_difference(left, right, &next) {
                            return Some(difference);
                        }
                    }
                    _ => return Some(next),
                }
            }
            Some(path.to_owned())
        }
        _ => Some(path.to_owned()),
    }
}

#[test]
fn registry_contract_snapshot_is_exportable() {
    let directory = tempfile::tempdir().unwrap();
    let platform = Arc::new(LocalPlatform::open(directory.path()).unwrap());
    let creators = platform.creators().unwrap();
    let operator_accounts: Arc<dyn OperatorAccountWorkflow> = Arc::new(
        PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
    );
    let catalog =
        Arc::new(onecrew_local_runtime::CapabilityCatalog::build().expect("production catalog"));
    let search_plan_summary = catalog
        .operation("onecrew.search.plan.prepare")
        .expect("search plan operation")
        .summary
        .to_owned();
    let registry = build_registry(
        RegistryOwners {
            platform,
            creators,
            operator_accounts,
        },
        Arc::clone(&catalog),
    )
    .unwrap();
    let operations = registry.operation_ids_and_surfaces();
    let mut ids = operations
        .iter()
        .map(|(id, _)| id.clone())
        .collect::<BTreeSet<_>>();
    ids.extend(registry.playbook_ids());
    ids.extend(registry.toolkit_guide_ids());
    let entries = ids
        .iter()
        .map(|id| {
            let entry = SURFACES.iter().find_map(|surface| registry.entry(id, *surface)).unwrap();
            match entry {
                RegistryEntry::Operation(operation) => json!({
                    "kind": "operation",
                    "metadata": metadata(&operation.metadata),
                    "effect": operation.effect,
                    "secret": operation.secret,
                    "approval": operation.approval,
                    "connection": operation.connection,
                    "sensitive_input_paths": operation.sensitive_input_paths,
                    "view_kind": operation.view_kind,
                    "input_schema": operation.input.document(),
                    "input_revision": operation.input.revision(),
                    "supports_search_plan": operation.binding.supports_search_plan(),
                    "output_schema": operation.output.document(),
                    "output_revision": operation.output.revision(),
                    "approval_request": operation.approval_request.as_ref().map(|value| &value.message),
                    "safety": operation.safety,
                }),
                RegistryEntry::Playbook(playbook) => json!({
                    "kind": "playbook",
                    "metadata": metadata(&playbook.metadata),
                    "use_case": playbook.use_case,
                    "toolkit_ids": playbook.toolkit_ids,
                    "primary_operation_ids": playbook.primary_operation_ids,
                    "supporting_operation_ids": playbook.supporting_operation_ids,
                    "recommended_steps": playbook.recommended_steps,
                    "known_pitfalls": playbook.known_pitfalls,
                    "difficulty": playbook.difficulty,
                }),
                RegistryEntry::ToolkitGuide(guide) => json!({
                    "kind": "toolkit_guide",
                    "metadata": metadata(&guide.metadata),
                    "purpose": guide.purpose,
                    "connection_model": guide.connection_model,
                    "identifier_semantics": guide.identifier_semantics,
                    "capability_categories": guide.capability_categories,
                    "gotchas": guide.gotchas,
                }),
            }
        })
        .collect::<Vec<_>>();
    let surfaces = SURFACES
        .iter()
        .map(|surface| {
            let operation_ids = operations.iter()
                .filter(|(_, allowed)| allowed.contains(surface))
                .map(|(id, _)| id.clone()).collect::<Vec<_>>();
            let gets = operation_ids.chunks(8).map(|chunk| {
                registry.tool_get_v2(*surface, &ToolGetInput {
                    operation_ids: chunk.to_vec(),
                    include_output_schema: true,
                }).unwrap()
            }).collect::<Vec<_>>();
            json!({"surface": surface, "get": gets, "directory": registry.toolkit_directory(*surface)})
        })
        .collect::<Vec<_>>();
    let expected_directories: Value = serde_json::from_str(include_str!(
        "../../../platform-runtime/src/catalog/fixtures/directory-surfaces.json"
    ))
    .unwrap();
    let directories = Value::Array(
        surfaces
            .iter()
            .map(
                |surface| json!({"surface": surface["surface"], "directory": surface["directory"]}),
            )
            .collect(),
    );
    assert_eq!(
        assert_search_plan_guidance(&directories, &search_plan_summary),
        3,
        "current search-plan guidance must reach every supported directory surface"
    );
    let mut actual_directories = mask_search_plan_guidance(directories);
    // Keep the historical directory baseline, while independently requiring
    // every newly exposed identity operation on exactly its supported surfaces.
    let identity_ids = [
        "onecrew.discovery.identity.material.get",
        "onecrew.discovery.identity.groups.get",
        "onecrew.discovery.identity.review.get",
        "onecrew.discovery.identity.review.submit",
        "onecrew.discovery.identity.review.revoke",
    ];
    let mut identity_surface_count = 0;
    for surface in actual_directories.as_array_mut().unwrap() {
        let supported = matches!(
            surface["surface"].as_str(),
            Some("full" | "compact" | "creators")
        );
        let mut found = Vec::new();
        for toolkit in surface["directory"].as_array_mut().unwrap() {
            toolkit["operations"]
                .as_array_mut()
                .unwrap()
                .retain(|operation| {
                    let id = operation["id"].as_str().unwrap();
                    if identity_ids.contains(&id) {
                        found.push(id.to_owned());
                        false
                    } else {
                        true
                    }
                });
        }
        found.sort();
        if supported {
            let mut expected = identity_ids.map(str::to_owned).to_vec();
            expected.sort();
            assert_eq!(
                found, expected,
                "identity review directory surface coverage"
            );
            identity_surface_count += 1;
        } else {
            assert!(
                found.is_empty(),
                "identity reviews leaked to unsupported surface"
            );
        }
    }
    assert_eq!(identity_surface_count, 3);
    let expected_directories = mask_search_plan_guidance(expected_directories);
    assert!(
        actual_directories == expected_directories,
        "MCP directory differs at {}",
        first_difference(&actual_directories, &expected_directories, "").unwrap_or_default()
    );
    let tools = McpSurface::ALL.into_iter().map(|surface| {
        json!({"surface": format!("{surface:?}"), "tools": McpAdapterServer::surface_tool_router(surface).list_all()})
    }).collect::<Vec<_>>();
    let snapshot = json!({
        "entries": entries,
        "surfaces": surfaces,
        "tools": tools,
        "mcp_app_contract": result_contract_schema_manifest().unwrap(),
    });
    assert!(!operations.is_empty());
    // An explicit export is test instrumentation, never an ordinary build write.
    if let Some(output) = std::env::var_os("MCP_REGISTRY_SNAPSHOT_OUT") {
        std::fs::write(output, serde_json::to_vec_pretty(&snapshot).unwrap()).unwrap();
    }
}
