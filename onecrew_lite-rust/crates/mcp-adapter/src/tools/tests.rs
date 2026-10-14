use super::*;
use crate::server::McpSurface;

const ORDINARY_META_TOOLS: [&str; 5] = [
    "onecrew_connection_list",
    "onecrew_result_present",
    "onecrew_tool_execute",
    "onecrew_tool_get",
    "onecrew_tool_search",
];

fn names(surface: McpSurface) -> Vec<String> {
    let mut names = McpAdapterServer::surface_tool_router(surface)
        .list_all()
        .into_iter()
        .filter(|tool| {
            tool.meta
                .as_ref()
                .and_then(|meta| meta.0.get("ui"))
                .and_then(|ui| ui.get("visibility"))
                != Some(&serde_json::json!(["app"]))
        })
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn ordinary_surfaces_have_exactly_five_model_visible_meta_tools() {
    for surface in McpSurface::ALL {
        if surface != McpSurface::Skills {
            assert_eq!(names(surface), ORDINARY_META_TOOLS, "{surface:?}");
        }
    }
}

#[test]
fn skills_surface_is_strictly_search_and_get_for_the_model() {
    assert_eq!(
        names(McpSurface::Skills),
        ["onecrew_tool_get", "onecrew_tool_search"]
    );
}

#[test]
fn only_present_bears_model_app_metadata_and_state_is_app_only() {
    let router = McpAdapterServer::surface_tool_router(McpSurface::Full);
    for tool in router.list_all() {
        let has_template = tool
            .meta
            .as_ref()
            .is_some_and(|meta| meta.0.contains_key("openai/outputTemplate"));
        assert_eq!(
            has_template,
            tool.name == "onecrew_result_present",
            "{}",
            tool.name
        );
        let visibility = tool
            .meta
            .as_ref()
            .and_then(|meta| meta.0.get("ui"))
            .and_then(|ui| ui.get("visibility"));
        if tool.name == "onecrew_result_state" {
            assert_eq!(visibility, Some(&serde_json::json!(["app"])));
            assert!(tool.meta.as_ref().is_none_or(|meta| {
                meta.0
                    .get("ui")
                    .is_none_or(|ui| ui.get("resourceUri").is_none())
            }));
        } else if tool.name == "onecrew_result_present" {
            assert_eq!(visibility, Some(&serde_json::json!(["model", "app"])));
        } else {
            assert!(
                visibility.is_none(),
                "{} unexpectedly bears App metadata",
                tool.name
            );
        }
        if tool.name == "onecrew_tool_execute" {
            let annotations = tool.annotations.as_ref().expect("execute annotations");
            assert_eq!(annotations.read_only_hint, Some(false));
            assert_eq!(annotations.destructive_hint, Some(false));
            assert_eq!(annotations.idempotent_hint, Some(false));
            assert_eq!(annotations.open_world_hint, Some(false));
        }
        if tool.name == "onecrew_tool_get" {
            let annotations = tool.annotations.as_ref().expect("get annotations");
            assert_eq!(annotations.read_only_hint, Some(true));
            assert_eq!(annotations.idempotent_hint, Some(true));
        }
        if tool.name == "onecrew_result_present" {
            let annotations = tool.annotations.as_ref().expect("present annotations");
            assert_eq!(annotations.read_only_hint, Some(false));
            assert_eq!(annotations.idempotent_hint, Some(true));
        }
    }
}

#[test]
fn tools_list_payloads_are_fixed_and_small() {
    for surface in McpSurface::ALL {
        let tools = McpAdapterServer::surface_tool_router(surface).list_all();
        let model_tools = tools
            .iter()
            .filter(|tool| {
                tool.meta
                    .as_ref()
                    .and_then(|meta| meta.0.get("ui"))
                    .and_then(|ui| ui.get("visibility"))
                    != Some(&serde_json::json!(["app"]))
            })
            .collect::<Vec<_>>();
        let model_bytes = serde_json::to_vec(&model_tools).unwrap().len();
        let wire_bytes = serde_json::to_vec(&tools).unwrap().len();
        assert!(
            model_bytes < 48 * 1024,
            "{surface:?} model tools/list is {model_bytes} bytes"
        );
        assert!(
            wire_bytes < 72 * 1024,
            "{surface:?} complete tools/list is {wire_bytes} bytes"
        );
    }
}

#[test]
fn no_legacy_public_name_survives() {
    for surface in McpSurface::ALL {
        for name in names(surface) {
            assert!(
                matches!(
                    name.as_str(),
                    "onecrew_tool_search"
                        | "onecrew_tool_get"
                        | "onecrew_tool_execute"
                        | "onecrew_result_present"
                        | "onecrew_connection_list"
                        | "onecrew_result_state"
                ),
                "legacy route survived: {name}"
            );
        }
    }
}

#[test]
fn every_v2_tool_schema_is_closed_and_ref_free() {
    fn walk(value: &serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                assert!(!object.contains_key("$ref"));
                object.values().for_each(walk);
            }
            serde_json::Value::Array(items) => items.iter().for_each(walk),
            _ => {}
        }
    }

    for tool in McpAdapterServer::surface_tool_router(McpSurface::Full).list_all() {
        let input = serde_json::Value::Object((*tool.input_schema).clone());
        assert_eq!(input["type"], "object", "{} input", tool.name);
        assert_eq!(input["additionalProperties"], false, "{} input", tool.name);
        if tool.name == "onecrew_result_state" {
            assert!(input.get("oneOf").is_none());
            assert!(input.get("anyOf").is_none());
            assert_eq!(input["required"], serde_json::json!(["action"]));
            assert_eq!(
                input["properties"]
                    .as_object()
                    .expect("state properties")
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                [
                    "action",
                    "answers",
                    "binding",
                    "input",
                    "patch",
                    "selections"
                ]
            );
        }
        walk(&input);
        let output =
            serde_json::Value::Object((*tool.output_schema.expect("V2 output schema")).clone());
        assert_eq!(output["type"], "object", "{} output", tool.name);
        if tool.name == "onecrew_result_state" {
            assert_eq!(output["additionalProperties"], false);
            assert!(output.get("oneOf").is_none());
            assert!(output.get("anyOf").is_none());
        }
        walk(&output);
    }
}

#[test]
fn creator_sync_control_plane_has_no_mcp_tool_schema() {
    let forbidden = [
        "activation",
        "pairing",
        "enrollment",
        "creator_sync",
        "creator-sync",
        "snapshot_sync",
        "snapshot-sync",
        "creator-snapshots",
    ];
    for surface in McpSurface::ALL {
        for tool in McpAdapterServer::surface_tool_router(surface).list_all() {
            let wire = serde_json::to_string(&tool).unwrap().to_ascii_lowercase();
            for token in &forbidden {
                assert!(!wire.contains(token), "{} exposes {token}", tool.name);
            }
        }
    }
}
