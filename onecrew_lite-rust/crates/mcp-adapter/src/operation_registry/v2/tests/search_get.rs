use onecrew_local_runtime::ToolkitId;
use std::collections::BTreeSet;

use serde_json::Value;

use super::fixtures::{id, production_registry, search_input};
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, ConfirmationMode, OperationSafety, RegistryEntry, SecretClass, SurfaceId,
        schema_json,
    },
    v2::{
        contract::{MAX_SEARCH_OPERATIONS, MAX_WORKFLOW_HINTS, ToolGetFailure, ToolGetInput},
        mcp::structured_tool_get,
    },
};

fn get(operation_ids: &[&str], include_output_schema: bool) -> Value {
    let result = production_registry()
        .tool_get_v2(
            SurfaceId::Full,
            &ToolGetInput {
                operation_ids: operation_ids.iter().copied().map(id).collect(),
                include_output_schema,
            },
        )
        .expect("surface-visible operations");
    serde_json::to_value(result).expect("tool_get result")
}

#[test]
fn inlined_enum_fields_keep_their_description_and_default() {
    let contract = get(&["x.posts.search.advanced"], false);
    let properties = &contract["operations"][0]["input_schema"]["properties"];
    for (field, default) in [
        ("search_plan_policy", "require_confirmation"),
        ("replies", "any"),
        ("links", "any"),
        ("result_mode", "top"),
    ] {
        assert_eq!(properties[field]["default"], default, "{field}");
        assert!(properties[field]["description"].is_string(), "{field}");
        assert!(properties[field]["enum"].is_array(), "{field}");
    }
    for field in ["search_plan_policy", "confirmed_plan_ref"] {
        assert!(
            properties[field]["description"]
                .as_str()
                .unwrap()
                .starts_with("Execute-only; omit from plan scope arguments."),
            "{field}"
        );
    }
    assert_eq!(properties["count"]["minimum"], 1);
    assert_eq!(properties["count"]["maximum"], 100);
}

#[test]
fn plan_scopes_and_search_counts_carry_model_facing_guidance() {
    let contract = get(
        &["onecrew.search.plan.prepare", "youtube.channels.search"],
        false,
    );
    let scope = &contract["operations"][0]["input_schema"]["properties"]["scopes"]["items"];
    for field in ["arguments", "schema_revision", "connection"] {
        assert!(
            scope["properties"][field]["description"].is_string(),
            "{field}"
        );
    }
    assert!(
        scope["properties"]["arguments"]["description"]
            .as_str()
            .unwrap()
            .contains("Omit search_plan_policy, confirmed_plan_ref and cursor")
    );
    let count = &contract["operations"][1]["input_schema"]["properties"]["count"];
    assert_eq!(count["minimum"], 1);
    assert_eq!(count["maximum"], 50);
    assert_eq!(
        count["description"],
        "YouTube page size. Omitted means 20; a confirmed plan freezes it."
    );
    assert!(count.get("format").is_none());
}

/// Whether the model view drops only `$schema` and `title` at the root,
/// integer formats, integer-type bounds, and `default: null`.
fn assert_only_listed_keys_removed(canonical: &Value, view: &Value, root: bool, path: &str) {
    match (canonical, view) {
        (Value::Object(canonical), Value::Object(view)) => {
            let integer_format = canonical
                .get("format")
                .and_then(Value::as_str)
                .is_some_and(|format| format.starts_with("uint") || format.starts_with("int"));
            for (key, original) in canonical {
                let Some(projected) = view.get(key) else {
                    let listed = match key.as_str() {
                        "$schema" | "title" => root,
                        "default" => original.is_null(),
                        "format" => integer_format,
                        "minimum" => integer_format && original.as_i64().is_some_and(|min| min < 0),
                        "maximum" => {
                            integer_format && original.as_u64().is_some_and(|max| max >= 127)
                        }
                        _ => false,
                    };
                    assert!(listed, "{path}/{key} was removed");
                    continue;
                };
                if matches!(key.as_str(), "const" | "default" | "enum" | "examples") {
                    assert_eq!(original, projected, "{path}/{key}");
                } else {
                    assert_only_listed_keys_removed(
                        original,
                        projected,
                        false,
                        &format!("{path}/{key}"),
                    );
                }
            }
            for key in view.keys() {
                assert!(canonical.contains_key(key), "{path}/{key} was added");
            }
        }
        (Value::Array(canonical), Value::Array(view)) => {
            assert_eq!(canonical.len(), view.len(), "{path}");
            for (index, (original, projected)) in canonical.iter().zip(view).enumerate() {
                assert_only_listed_keys_removed(
                    original,
                    projected,
                    false,
                    &format!("{path}/{index}"),
                );
            }
        }
        _ => assert_eq!(canonical, view, "{path}"),
    }
}

fn has_model_noise(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("default") == Some(&Value::Null)
                || object
                    .get("format")
                    .and_then(Value::as_str)
                    .is_some_and(|format| format.starts_with("uint") || format.starts_with("int"))
                || object.values().any(has_model_noise)
        }
        Value::Array(items) => items.iter().any(has_model_noise),
        _ => false,
    }
}

#[test]
fn model_schema_view_removes_only_listed_keys_from_every_production_schema() {
    let registry = production_registry();
    for (operation_id, surfaces) in registry.operation_ids_and_surfaces() {
        let Some(RegistryEntry::Operation(operation)) = registry.entry(&operation_id, surfaces[0])
        else {
            panic!("{operation_id} is not a visible operation");
        };
        for (direction, schema) in [("input", &operation.input), ("output", &operation.output)] {
            let view = schema_json(schema);
            let path = format!("{operation_id}/{direction}");
            assert_only_listed_keys_removed(schema.document(), &view, true, &path);
            assert!(view.get("$schema").is_none(), "{path}");
            assert!(view.get("title").is_none(), "{path}");
            assert!(!has_model_noise(&view), "{path}");
        }
    }
}

#[test]
fn tool_get_omits_implied_safety_and_absent_secret_and_matches_its_output_schema() {
    let registry = production_registry();
    let tools =
        crate::server::McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full)
            .list_all();
    let tool_get = tools
        .iter()
        .find(|tool| tool.name == "onecrew_tool_get")
        .expect("onecrew_tool_get");
    let output_schema = Value::Object((**tool_get.output_schema.as_ref().unwrap()).clone());
    let validator = jsonschema::validator_for(&output_schema).expect("tool_get output schema");
    let operation_ids = registry
        .operation_ids_and_surfaces()
        .into_iter()
        .filter(|(_, surfaces)| surfaces.contains(&SurfaceId::Full))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let (mut omitted, mut kept, mut secret) = (0, 0, 0);
    for chunk in operation_ids.chunks(8) {
        let result = registry
            .tool_get_v2(
                SurfaceId::Full,
                &ToolGetInput {
                    operation_ids: chunk.to_vec(),
                    include_output_schema: true,
                },
            )
            .unwrap();
        let structured = serde_json::to_value(&result).unwrap();
        validator.validate(&structured).unwrap();
        for (contract, value) in result
            .operations
            .iter()
            .zip(structured["operations"].as_array().unwrap())
        {
            let Some(RegistryEntry::Operation(operation)) =
                registry.entry(&contract.id, SurfaceId::Full)
            else {
                panic!("{} is not a visible operation", contract.id);
            };
            let mut implied = OperationSafety::read_only();
            if operation.approval == ApprovalClass::AttendedPrompt {
                implied.confirmation.mode = ConfirmationMode::AttendedPrompt;
            }
            if operation.safety == implied {
                omitted += 1;
                assert!(value.get("safety").is_none(), "{}", contract.id);
            } else {
                kept += 1;
                assert_eq!(
                    contract.safety.as_ref(),
                    Some(&operation.safety),
                    "{}",
                    contract.id
                );
            }
            if operation.secret == SecretClass::None {
                assert!(value.get("secret").is_none(), "{}", contract.id);
            } else {
                secret += 1;
                assert_eq!(contract.secret, Some(operation.secret), "{}", contract.id);
            }
        }
    }
    assert!(
        omitted > 0 && kept > 0 && secret > 0,
        "{omitted} {kept} {secret}"
    );
    let plan = get(&["onecrew.search.plan.prepare", "x.profile.get"], false);
    for contract in plan["operations"].as_array().unwrap() {
        assert!(contract.get("safety").is_none(), "{}", contract["id"]);
        assert!(contract.get("secret").is_none(), "{}", contract["id"]);
    }
    let write = get(&["onecrew.creator.batch.create"], false);
    assert!(write["operations"][0]["safety"]["confirmation"].is_object());
}

#[test]
fn exact_get_omits_output_schema_unless_requested() {
    let registry = production_registry();
    let mut input: ToolGetInput = serde_json::from_value(serde_json::json!({
        "operation_ids": ["youtube.channels.search"]
    }))
    .unwrap();
    assert!(!input.include_output_schema);
    let result = registry.tool_get_v2(SurfaceId::Full, &input).unwrap();
    assert_eq!(result.operations.len(), 1);
    assert!(result.operations[0].output_schema.is_none());
    input.include_output_schema = true;
    assert!(
        registry
            .tool_get_v2(SurfaceId::Full, &input)
            .unwrap()
            .operations[0]
            .output_schema
            .is_some()
    );
}

#[test]
fn v2_search_order_is_deterministic_and_bounded() {
    let registry = production_registry();
    let input = search_input(
        "Find X creator accounts and inspect public profiles",
        vec![ToolkitId::X],
    );
    let first = registry.tool_search_v2(SurfaceId::Full, &input);
    let second = registry.tool_search_v2(SurfaceId::Full, &input);
    assert_eq!(first, second);
    assert!(!first.operations.is_empty());
    assert!(first.operations.len() <= MAX_SEARCH_OPERATIONS);
    assert!(first.workflow_hints.len() <= MAX_WORKFLOW_HINTS);
    assert_eq!(
        first.workflow_hints[0].id.as_str(),
        "playbook.x.creator-discovery"
    );
    let operation_ids = first
        .operations
        .iter()
        .map(|operation| operation.id.as_str())
        .collect::<Vec<_>>();
    assert!(operation_ids.contains(&"x.posts.search.advanced"));
    assert!(operation_ids.contains(&"x.profile.get"));
}

#[test]
fn find_and_save_search_uses_the_live_catalog_guides_and_workflow_hints() {
    let registry = production_registry();
    let x = registry.tool_search_v2(
        SurfaceId::Full,
        &search_input(
            "Find X creators and save selected candidates as Creators",
            vec![ToolkitId::X, ToolkitId::OneCrew],
        ),
    );
    let x_guide = x
        .toolkit_guides
        .iter()
        .find(|guide| guide.toolkit == ToolkitId::X)
        .expect("X guide from the catalog");
    assert!(
        x_guide
            .gotchas
            .iter()
            .any(|text| text.contains("only when") && text.contains("evidence"))
    );
    let x_hint = x
        .workflow_hints
        .iter()
        .find(|hint| hint.id.as_str() == "playbook.recipe.find-x-creators")
        .expect("X find-and-save workflow hint");
    assert!(
        x_hint
            .steps
            .iter()
            .any(|step| step.contains("without an extra profile read"))
    );
    assert!(
        x_hint
            .steps
            .iter()
            .any(|step| step.contains("real provenance"))
    );
    let onecrew_guide = x
        .toolkit_guides
        .iter()
        .find(|guide| guide.toolkit == ToolkitId::OneCrew)
        .expect("OneCrew guide from the catalog");
    assert!(
        onecrew_guide
            .identifier_semantics
            .iter()
            .any(|text| text.contains("explicit intent") && text.contains("save_or_update"))
    );
    assert!(
        onecrew_guide
            .gotchas
            .iter()
            .any(|text| text.contains("Owner statuses") && text.contains("eligible_new"))
    );
    let save_operation = onecrew_guide
        .operations
        .iter()
        .find(|operation| operation.id.as_str() == "onecrew.creator.candidate.save")
        .expect("save operation in the shared OneCrew guide");
    assert!(
        save_operation
            .summary
            .contains("explicit intent (add_new or save_or_update)")
    );
    assert!(
        save_operation
            .summary
            .contains("owner obtains fresh platform data")
    );

    let youtube = registry.tool_search_v2(
        SurfaceId::Full,
        &search_input(
            "Find YouTube creators and save selected channels as Creators",
            vec![ToolkitId::Youtube, ToolkitId::OneCrew],
        ),
    );
    let youtube_guide = youtube
        .toolkit_guides
        .iter()
        .find(|guide| guide.toolkit == ToolkitId::Youtube)
        .expect("YouTube guide from the catalog");
    assert!(
        youtube_guide
            .gotchas
            .iter()
            .any(|text| text.contains("stable channel ID")
                && text.contains("across connections and read depths"))
    );
    assert!(
        youtube_guide
            .gotchas
            .iter()
            .any(|text| text.contains("depth: light or full") && text.contains("omits comments"))
    );
    let youtube_hint = youtube
        .workflow_hints
        .iter()
        .find(|hint| hint.id.as_str() == "playbook.recipe.find-youtube-creators")
        .expect("YouTube find-and-save workflow hint");
    assert!(
        youtube_hint
            .steps
            .iter()
            .any(|step| step.contains("channel ID") && step.contains("duplicate"))
    );
    assert!(
        youtube_hint
            .steps
            .iter()
            .any(|step| step.contains("without an extra channel read"))
    );
    assert!(
        youtube_hint
            .steps
            .iter()
            .any(|step| step.contains("depth: light or full"))
    );
}

#[test]
fn discovery_playbook_hints_only_read_for_decision_changing_evidence_gaps() {
    let registry = production_registry();
    let find_hint = |query: &str, toolkit: ToolkitId, id: &str| {
        registry
            .tool_search_v2(SurfaceId::Full, &search_input(query, vec![toolkit]))
            .workflow_hints
            .into_iter()
            .find(|hint| hint.id.as_str() == id)
            .expect("discovery playbook hint")
    };

    let x = find_hint(
        "Search X posts with advanced filters, inspect an author profile, and review recent posts",
        ToolkitId::X,
        "playbook.x.creator-discovery",
    );
    assert!(
        x.steps
            .iter()
            .any(|step| step.contains("one decision-changing gap"))
    );
    assert!(x.steps.iter().all(|step| {
        !step.contains("Review recent posts from each selected author before evaluation")
    }));

    let instagram = find_hint(
        "Discover Instagram creator accounts and inspect one profile",
        ToolkitId::Instagram,
        "playbook.instagram.creator-discovery",
    );
    assert!(
        instagram
            .steps
            .iter()
            .any(|step| step.contains("one decision-changing gap"))
    );
    assert!(
        instagram
            .steps
            .iter()
            .all(|step| !step.eq_ignore_ascii_case("Inspect the selected Instagram profile."))
    );

    let youtube = find_hint(
        "Discover YouTube channels and inspect one channel",
        ToolkitId::Youtube,
        "playbook.youtube.channel-discovery",
    );
    assert!(youtube.steps.iter().any(|step| {
        step.contains("stable channel ID") && step.contains("not connection or depth")
    }));
    assert!(youtube.steps.iter().all(|step| {
        !step.contains("stable channel ID for the same connection and read depth")
    }));
}

#[test]
fn v2_search_closes_guides_over_explicit_inferred_and_represented_toolkits() {
    let registry = production_registry();
    let input = search_input(
        "Find X creator accounts and save candidates as Creators",
        vec![ToolkitId::X, ToolkitId::OneCrew],
    );
    let result = registry.tool_search_v2(SurfaceId::Full, &input);
    let guide_toolkits = result
        .toolkit_guides
        .iter()
        .map(|guide| guide.toolkit)
        .collect::<BTreeSet<_>>();
    assert!(guide_toolkits.contains(&ToolkitId::X));
    assert!(guide_toolkits.contains(&ToolkitId::OneCrew));
    assert!(
        result
            .operations
            .iter()
            .all(|operation| guide_toolkits.contains(&operation.toolkit))
    );

    let directory = registry.toolkit_directory(SurfaceId::Full);
    for guide in &result.toolkit_guides {
        let expected = directory
            .iter()
            .find(|entry| entry.toolkit == guide.toolkit)
            .expect("represented toolkit has a visible guide");
        assert_eq!(guide.operations, expected.operations);
        assert!(!guide.operations.is_empty());
    }
}

#[test]
fn v2_search_infers_the_onecrew_guide_from_its_name_only() {
    let registry = production_registry();
    let inferred = registry.tool_search_v2(SurfaceId::Full, &search_input("onecrew", Vec::new()));
    assert!(
        inferred
            .toolkit_guides
            .iter()
            .any(|guide| guide.toolkit == ToolkitId::OneCrew)
    );
    let retired = registry.tool_search_v2(SurfaceId::Full, &search_input("crm", Vec::new()));
    assert!(
        retired
            .toolkit_guides
            .iter()
            .all(|guide| guide.toolkit != ToolkitId::OneCrew),
        "{:?}",
        retired.toolkit_guides
    );
}

#[test]
fn v2_toolkit_filter_returns_complete_guide_when_lexical_recall_is_empty() {
    let registry = production_registry();
    let result = registry.tool_search_v2(
        SurfaceId::Full,
        &search_input("quantum observability lattice", vec![ToolkitId::X]),
    );
    assert!(result.operations.is_empty());
    assert_eq!(result.toolkit_guides.len(), 1);
    assert_eq!(result.toolkit_guides[0].toolkit, ToolkitId::X);
    let expected = registry
        .toolkit_directory(SurfaceId::Full)
        .into_iter()
        .find(|guide| guide.toolkit == ToolkitId::X)
        .expect("X guide");
    assert_eq!(result.toolkit_guides[0].operations, expected.operations);

    let inferred = registry.tool_search_v2(
        SurfaceId::Full,
        &search_input("Telegram quantum observability lattice", Vec::new()),
    );
    assert!(inferred.operations.is_empty());
    assert_eq!(inferred.toolkit_guides.len(), 1);
    assert_eq!(inferred.toolkit_guides[0].toolkit, ToolkitId::Telegram);
}

#[test]
fn v2_batch_get_preserves_order_and_fails_atomically_for_unavailable_entries() {
    let registry = production_registry();
    let ordered = ToolGetInput {
        operation_ids: vec![id("x.profile.get"), id("x.posts.search.advanced")],
        include_output_schema: true,
    };
    let result = registry
        .tool_get_v2(SurfaceId::Full, &ordered)
        .expect("surface-visible operations");
    assert_eq!(
        result
            .operations
            .iter()
            .map(|operation| operation.id.as_str())
            .collect::<Vec<_>>(),
        ["x.profile.get", "x.posts.search.advanced"]
    );

    for unavailable in [
        vec![id("x.profile.get"), id("unknown.operation")],
        vec![id("toolkit.x")],
        vec![id("playbook.x.creator-discovery")],
    ] {
        assert_eq!(
            registry.tool_get_v2(
                SurfaceId::Full,
                &ToolGetInput {
                    operation_ids: unavailable,
                    include_output_schema: true,
                },
            ),
            Err(ToolGetFailure::OperationUnavailable)
        );
    }

    let hidden = registry
        .operation_ids_and_surfaces()
        .into_iter()
        .find(|(_, surfaces)| !surfaces.contains(&SurfaceId::Skills))
        .map(|(id, _)| id)
        .expect("at least one operation is hidden from Skills");
    assert_eq!(
        registry.tool_get_v2(
            SurfaceId::Skills,
            &ToolGetInput {
                operation_ids: vec![hidden],
                include_output_schema: true,
            },
        ),
        Err(ToolGetFailure::OperationUnavailable)
    );
}

/// Asserts that the `onecrew_tool_get` content is a human outline followed by
/// the serialized JSON block, and that each outline line states only the ID,
/// title, connection, effect, and short revision of its contract in
/// `structuredContent`. Returns the outline's line lengths after the header.
fn assert_derived_outline(response: &rmcp::model::CallToolResult) -> Vec<usize> {
    let structured = response.structured_content.as_ref().expect("contracts");
    let [outline, json] = response.content.as_slice() else {
        panic!("an outline and the serialized JSON block");
    };
    let json = &json.as_text().expect("JSON text block").text;
    assert_eq!(serde_json::from_str::<Value>(json).unwrap(), *structured);
    let outline = &outline.as_text().expect("outline text block").text;
    let contracts = structured["operations"].as_array().unwrap();
    let lines = outline.lines().collect::<Vec<_>>();
    assert_eq!(
        lines[0],
        format!(
            "{} operation contracts; the JSON contract follows.",
            contracts.len()
        )
    );
    assert_eq!(lines.len(), 1 + contracts.len(), "{outline}");
    for (line, contract) in lines[1..].iter().zip(contracts) {
        let text = |value: &Value| value.as_str().unwrap().to_owned();
        let words = |value: &Value| text(value).replace('_', " ");
        let connection = &contract["connection"];
        let connection = match connection.get("toolkit") {
            Some(toolkit) => format!("{} {}", text(toolkit), words(&connection["kind"])),
            None => "no connection".to_owned(),
        };
        let revision = text(&contract["schema_revision"]);
        let short_revision = &revision.strip_prefix("sha256:").unwrap()[..8];
        assert_eq!(
            *line,
            format!(
                "- {} — {} · {connection} · {} · rev {short_revision}…",
                text(&contract["id"]),
                text(&contract["title"]),
                words(&contract["effect"]),
            )
        );
    }
    lines[1..].iter().map(|line| line.len()).collect()
}

/// Budgets on the compact structured content the model receives for the two
/// captured four-operation `onecrew_tool_get` sets, set just above their
/// measured sizes. Run with `--nocapture` to print the measurements; raise a
/// budget only for a deliberate contract change. The prepare contract states
/// the plan TTL once in each set.
#[test]
fn captured_tool_get_sets_fit_their_budgets_behind_a_human_outline() {
    let registry = production_registry();
    for (name, operation_ids, without_output_schemas, with_output_schemas) in [
        (
            "x",
            [
                "onecrew.search.plan.prepare",
                "x.posts.search.advanced",
                "x.profile.get",
                "x.user_posts.list",
            ],
            8_300,
            13_300,
        ),
        (
            "youtube",
            [
                "onecrew.search.plan.prepare",
                "youtube.channels.search",
                "youtube.videos.search",
                "youtube.channel.get",
            ],
            // Both YouTube searches share one input, so a word added to its
            // `count`, `order` or `published_after` is paid twice here.
            // Measured 6_799 B and 7_510 B: the first budget sits just above
            // its own measurement and rose from 6_200 to pay for `order` and
            // `published_after`, which are what let one search page carry its
            // own recency evidence instead of a channel read per hit. The
            // second is unchanged and still well clear. Raise a budget only
            // for a deliberate contract change, never to quiet this test.
            6_900,
            13_200,
        ),
    ] {
        for include_output_schema in [false, true] {
            let result = registry
                .tool_get_v2(
                    SurfaceId::Full,
                    &ToolGetInput {
                        operation_ids: operation_ids.into_iter().map(id).collect(),
                        include_output_schema,
                    },
                )
                .expect("captured operation set");
            let response = structured_tool_get(result).expect("tool_get response");
            let structured = response.structured_content.as_ref().expect("contracts");
            let bytes = serde_json::to_vec(structured).expect("structured").len();
            let wire = serde_json::to_vec(&response).expect("response").len();
            let outline = response.content[0].as_text().expect("outline").text.len();
            println!(
                "tool_get {name} include_output_schema={include_output_schema}: {bytes} B structured, {outline} B outline, {wire} B wire"
            );
            let budget = if include_output_schema {
                with_output_schemas
            } else {
                without_output_schemas
            };
            assert!(bytes <= budget, "{name} {include_output_schema}: {bytes} B");
            assert_derived_outline(&response);
        }
    }
    let tools =
        crate::server::McpAdapterServer::surface_tool_router(crate::server::McpSurface::Full)
            .list_all();
    let bytes = serde_json::to_vec(&tools).expect("tools/list").len();
    println!("tools/list full: {bytes} B");
}

/// Every production contract has an outline line, and the eight longest lines
/// together stay near 1 KB.
#[test]
fn tool_get_outline_is_derived_for_every_production_contract() {
    let registry = production_registry();
    let operation_ids = registry
        .operation_ids_and_surfaces()
        .into_iter()
        .filter(|(_, surfaces)| surfaces.contains(&SurfaceId::Full))
        .map(|(id, _)| id)
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    for chunk in operation_ids.chunks(8) {
        let result = registry
            .tool_get_v2(
                SurfaceId::Full,
                &ToolGetInput {
                    operation_ids: chunk.to_vec(),
                    include_output_schema: false,
                },
            )
            .unwrap();
        lines.extend(assert_derived_outline(
            &structured_tool_get(result).unwrap(),
        ));
    }
    assert_eq!(lines.len(), operation_ids.len());
    lines.sort_unstable_by(|left, right| right.cmp(left));
    let longest: usize = lines.iter().take(8).map(|length| length + 1).sum();
    println!("longest eight-operation outline body: {longest} B");
    assert!(longest <= 1_200, "{longest} B");
}

#[test]
fn v2_real_registry_search_and_get_instances_fit_context_budgets() {
    let registry = production_registry();
    let search = registry.tool_search_v2(
        SurfaceId::Full,
        &search_input(
            "Find creator accounts inspect profiles manage creator groups and send email",
            vec![
                ToolkitId::X,
                ToolkitId::Instagram,
                ToolkitId::Telegram,
                ToolkitId::Youtube,
                ToolkitId::OneCrew,
                ToolkitId::Email,
            ],
        ),
    );
    assert_eq!(search.toolkit_guides.len(), 6);
    let search_bytes = serde_json::to_vec(&search).expect("search instance").len();
    assert!(
        search_bytes < 64 * 1024,
        "V2 production search instance uses {search_bytes} bytes"
    );

    let get = registry
        .tool_get_v2(
            SurfaceId::Full,
            &ToolGetInput {
                operation_ids: [
                    "x.posts.search.advanced",
                    "x.profile.get",
                    "instagram.users.search",
                    "instagram.profile.get",
                    "youtube.channels.search",
                    "youtube.channel.get",
                    "onecrew.creator.candidate.save",
                    "onecrew.creator.batch.create",
                ]
                .into_iter()
                .map(id)
                .collect(),
                include_output_schema: true,
            },
        )
        .expect("representative eight-operation batch");
    let get_bytes = serde_json::to_vec(&get).expect("get instance").len();
    assert!(
        get_bytes < 128 * 1024,
        "V2 production get instance uses {get_bytes} bytes"
    );
}
