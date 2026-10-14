use super::*;
use onecrew_local_runtime::ToolkitId;

#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "the field is consumed by Serialize and JsonSchema in registry contract tests"
)]
struct StringOutput {
    name: String,
}

#[derive(JsonSchema)]
#[allow(
    dead_code,
    reason = "the field is consumed by JsonSchema in registry contract tests"
)]
struct ValueOutput {
    name: Value,
}

#[derive(JsonSchema)]
#[allow(
    dead_code,
    reason = "the field is consumed by JsonSchema in registry contract tests"
)]
struct OptionalStringOutput {
    name: Option<String>,
}

#[derive(JsonSchema)]
#[allow(
    dead_code,
    reason = "the field is consumed by JsonSchema in registry contract tests"
)]
struct BoundedStringInput {
    #[schemars(length(max = 5))]
    name: String,
}

#[test]
fn generated_input_schema_matches_serde_for_nested_tagged_inputs() {
    let schema = CompiledSchema::for_input::<StrictInput>().unwrap();
    let cases = [
        json!({"name":"a","action":{"kind":"read"}}),
        json!({"name":"a","action":{"kind":"write","count":2}}),
        json!({"name":"a","action":{"kind":"write"}}),
        json!({"name":"a","action":{"kind":"unknown"}}),
        json!({"name":"a","action":{"kind":"read","count":2}}),
        json!({"name":"a","action":{"kind":"read"},"extra":true}),
        json!({"name":null,"action":{"kind":"read"}}),
    ];
    for candidate in cases {
        assert_eq!(
            schema.validate(&candidate),
            serde_json::from_value::<StrictInput>(candidate.clone()).is_ok(),
            "schema and Serde disagree for {candidate}"
        );
    }
    let encoded = serde_json::to_string(schema.document()).unwrap();
    assert!(!encoded.contains("$ref"));
    assert!(!encoded.contains("$defs"));
    assert!(schema.revision().starts_with("sha256:"));
}

#[test]
fn parameter_source_assignability_fails_closed_for_unproven_schema_subsets() {
    let string_target = CompiledSchema::for_input::<StrictInput>().unwrap();
    for source in [
        CompiledSchema::for_output::<ValueOutput>().unwrap(),
        CompiledSchema::for_output::<OptionalStringOutput>().unwrap(),
    ] {
        assert!(!string_target.accepts_pointer_value_from("/name", &source, "/name"));
    }

    let bounded_target = CompiledSchema::for_input::<BoundedStringInput>().unwrap();
    let unbounded_source = CompiledSchema::for_output::<StringOutput>().unwrap();
    assert!(!bounded_target.accepts_pointer_value_from("/name", &unbounded_source, "/name"));
}

#[test]
fn entry_id_schema_and_serde_enforce_the_same_invariant() {
    let schema =
        CompiledSchema::for_input::<crate::operation_registry::v2::ToolGetInput>().unwrap();
    for candidate in [
        json!({"operation_ids":["onecrew.valid.operation-1"],"include_output_schema":false}),
        json!({"operation_ids":[""],"include_output_schema":false}),
        json!({"operation_ids":["Uppercase"],"include_output_schema":false}),
        json!({"operation_ids":["contains/slash"],"include_output_schema":false}),
        json!({"operation_ids":["a".repeat(129)],"include_output_schema":false}),
    ] {
        assert_eq!(
            schema.validate(&candidate),
            serde_json::from_value::<crate::operation_registry::v2::ToolGetInput>(
                candidate.clone()
            )
            .is_ok(),
            "EntryId schema and Serde disagree for {candidate}"
        );
    }
}

#[test]
fn toolkit_filters_are_closed_bounded_and_unique_at_decode() {
    use crate::operation_registry::{descriptor::ConnectionListInput, v2::ToolSearchInput};

    let decoded: ToolSearchInput = serde_json::from_value(json!({
        "query": "search accounts",
        "toolkits": ["x", "youtube"],
        "effects": ["read_open_world", "sync_open_world"],
        "limit": 8
    }))
    .unwrap();
    assert_eq!(decoded.toolkits, vec![ToolkitId::X, ToolkitId::Youtube]);
    assert!(decoded.limit.is_some());

    for invalid in [
        json!({"query": "x", "toolkits": ["platform"]}),
        json!({"query": "x", "toolkits": ["x", "x"]}),
        json!({"query": "x", "effects": ["read_local", "read_local"]}),
        json!({"query": "x", "limit": 0}),
        json!({"query": "x", "limit": 9}),
        json!({"query": ""}),
    ] {
        assert!(serde_json::from_value::<ToolSearchInput>(invalid).is_err());
    }
    assert!(
        serde_json::from_value::<ConnectionListInput>(json!({
            "toolkits": ["x", "x"]
        }))
        .is_err()
    );
}
