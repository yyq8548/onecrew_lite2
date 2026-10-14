use onecrew_local_runtime::ToolkitId;
use serde_json::{Map, Value};

use super::fixtures::{id, schema_value};
use crate::operation_registry::v2::contract::{
    APP_ONLY_TOOL_NAME, AskUserQuestionInput, MAX_SEARCH_GUIDES, MAX_SEARCH_OPERATIONS,
    MAX_WORKFLOW_HINTS, MODEL_TOOL_NAMES, PresentationRequest, ResultPresentInput,
    ResultPresentResult, ResultStateFailure, ResultStateInput, ResultStateOutput,
    ResultStateWireSchema, SKILLS_MODEL_TOOL_NAMES, ToolExecuteInput, ToolExecuteResult,
    ToolExecutionAuthority, ToolExecutionCall, ToolGetInput, ToolGetResult, ToolSearchInput,
    ToolSearchResult,
};

fn assert_closed_object(schema: &Value) {
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn v2_model_surface_is_exact_and_plan_free() {
    assert_eq!(MODEL_TOOL_NAMES.len(), 5);
    assert_eq!(
        MODEL_TOOL_NAMES,
        [
            "onecrew_connection_list",
            "onecrew_tool_search",
            "onecrew_tool_get",
            "onecrew_tool_execute",
            "onecrew_result_present",
        ]
    );
    assert_eq!(APP_ONLY_TOOL_NAME, "onecrew_result_state");
    assert_eq!(
        SKILLS_MODEL_TOOL_NAMES,
        ["onecrew_tool_search", "onecrew_tool_get"]
    );
    let schemas = [
        schema_value::<ToolSearchInput>(),
        schema_value::<ToolGetInput>(),
        schema_value::<ToolExecuteInput>(),
        schema_value::<ResultPresentInput>(),
    ];
    let wire = serde_json::to_string(&schemas).expect("schemas wire");
    for retired in [
        "entry_card",
        "entry_form",
        "entry_replacement",
        "playbook",
        "plan_confirmed",
        "business_intent_plan",
    ] {
        assert!(!wire.to_lowercase().contains(retired), "retired {retired}");
    }
}

#[test]
fn v2_input_schemas_are_closed_and_bounded() {
    let search = schema_value::<ToolSearchInput>();
    let get = schema_value::<ToolGetInput>();
    let execute = schema_value::<ToolExecuteInput>();
    let present = schema_value::<ResultPresentInput>();
    for schema in [&search, &get, &execute, &present] {
        assert_closed_object(schema);
    }
    assert_eq!(get["properties"]["operation_ids"]["minItems"], 1);
    assert_eq!(get["properties"]["operation_ids"]["maxItems"], 8);
    assert_eq!(execute["properties"]["calls"]["minItems"], 1);
    assert_eq!(execute["properties"]["calls"]["maxItems"], 8);
    assert_eq!(search["properties"]["toolkits"]["maxItems"], 8);
    assert_eq!(
        present["$defs"]["ResultRef"]["pattern"],
        "^rr_[0-9a-f]{32}$"
    );
    assert!(
        present["required"]
            .as_array()
            .is_some_and(|required| required.iter().any(|value| value == "locale"))
    );
}

#[test]
fn v2_result_state_descriptor_is_flat_closed_and_strict_decode_remains() {
    let input = schema_value::<ResultStateInput>();
    let output = schema_value::<ResultStateWireSchema>();
    let strict_output = schema_value::<ResultStateOutput>();
    let failure = schema_value::<ResultStateFailure>();
    let wire = serde_json::to_string(&(&input, &output, &strict_output, &failure)).unwrap();

    assert_closed_object(&input);
    assert_closed_object(&output);
    assert!(input.get("oneOf").is_none());
    assert!(input.get("anyOf").is_none());
    assert_eq!(input["required"], serde_json::json!(["action"]));
    assert_eq!(
        input["properties"]
            .as_object()
            .unwrap()
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
    assert_eq!(
        output["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "claimed",
            "code",
            "completed",
            "current_revision",
            "dm_failure",
            "kind",
            "snapshot"
        ]
    );
    assert!(wire.contains("read_current"));
    assert!(wire.contains("confirm"));
    assert!(!wire.contains("\"confirm_creator_save\""));
    assert!(wire.contains("confirm_creator_save_batch"));
    assert!(wire.contains("confirmed_route_ids"));
    assert!(wire.contains("select"));
    assert!(wire.contains("cancel"));
    assert!(wire.contains("request_edit"));
    assert!(wire.contains("claim_continuation"));
    assert!(wire.contains("complete_continuation"));
    assert!(wire.contains("claimed_continuation"));
    assert!(wire.contains("completed_continuation"));
    // Only operation-owned argument maps are open; the state envelope stays closed.
    assert_eq!(output["additionalProperties"], false);
    assert!(wire.contains("SearchPlanScope"));
    assert!(!wire.contains("SearchPlanSearch"));
    assert!(wire.contains("instruction"));
}

#[test]
fn v2_result_state_schema_accepts_every_real_action_and_rejects_unknown_fields() {
    let schema = schema_value::<ResultStateInput>();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(&schema)
        .expect("valid result-state schema");
    let binding = serde_json::json!({
        "result_ref": format!("rr_{}", "1".repeat(32)),
        "view": {
            "identity": format!("rv_{}", "2".repeat(32)),
            "kind": "choice"
        },
        "origin": "mcp:full",
        "fingerprint": format!("sha256:{}", "3".repeat(64)),
        "revision": 2
    });
    let actions = [
        serde_json::json!({"action":"read_current","binding":binding}),
        serde_json::json!({"action":"confirm","binding":binding}),
        serde_json::json!({"action":"confirm_creator_save_batch","binding":binding,"selections":[{"row_id":"row","confirmed_route_ids":["route"]}]}),
        serde_json::json!({
            "action":"select",
            "binding":binding,
            "answers":[{"question_id":"question","option_id":"option"}]
        }),
        serde_json::json!({"action":"cancel","binding":binding}),
        serde_json::json!({
            "action":"request_edit",
            "input":{"binding":binding,"instruction":"Use three candidates."}
        }),
        serde_json::json!({
            "action":"claim_continuation",
            "input":{
                "binding":binding,
                "expected_continuation_revision":1,
                "mode":"pending"
            }
        }),
        serde_json::json!({
            "action":"complete_continuation",
            "input":{
                "binding":binding,
                "expected_continuation_revision":2,
                "lease_id":format!("rcl_{}", "4".repeat(32)),
                "outcome":"delivered"
            }
        }),
    ];

    for action in actions {
        assert!(
            validator.is_valid(&action),
            "schema rejected valid action {action}"
        );
        assert!(
            serde_json::from_value::<ResultStateInput>(action.clone()).is_ok(),
            "Serde rejected valid action {action}"
        );
    }

    let invalid = serde_json::json!({
        "action":"confirm",
        "binding":binding,
        "unexpected":true
    });
    assert!(!validator.is_valid(&invalid));
    assert!(serde_json::from_value::<ResultStateInput>(invalid).is_err());

    let schema_permits_but_strict_decoder_rejects_wrong_action_shape = serde_json::json!({
        "action":"confirm",
        "input":{"binding":binding,"instruction":"Not valid for confirm."}
    });
    assert!(validator.is_valid(&schema_permits_but_strict_decoder_rejects_wrong_action_shape));
    assert!(
        serde_json::from_value::<ResultStateInput>(
            schema_permits_but_strict_decoder_rejects_wrong_action_shape
        )
        .is_err()
    );
}

#[test]
fn choice_request_schema_and_decode_are_closed_bounded_and_typed() {
    let valid = serde_json::json!({
        "interaction_id":"choice.multi",
        "title":"Choose routes",
        "questions":[{
            "id":"route",
            "header":"Route",
            "prompt":"Choose one route.",
            "options":[
                {"id":"email","label":"Email","description":null},
                {"id":"youtube","label":"YouTube","description":null}
            ]
        }]
    });
    assert!(serde_json::from_value::<AskUserQuestionInput>(valid.clone()).is_ok());
    let mut extra = valid.clone();
    extra
        .as_object_mut()
        .unwrap()
        .insert("raw".to_owned(), serde_json::json!({}));
    assert!(serde_json::from_value::<AskUserQuestionInput>(extra).is_err());
    let mut duplicate = valid.clone();
    duplicate["questions"] =
        serde_json::json!([valid["questions"][0].clone(), valid["questions"][0].clone()]);
    assert!(serde_json::from_value::<AskUserQuestionInput>(duplicate).is_err());
    let mut blank_title = valid.clone();
    blank_title["title"] = serde_json::json!("  ");
    assert!(serde_json::from_value::<AskUserQuestionInput>(blank_title).is_err());

    let schema = schema_value::<AskUserQuestionInput>();
    assert_closed_object(&schema);
    assert_eq!(schema["properties"]["questions"]["minItems"], 1);
    assert_eq!(schema["properties"]["questions"]["maxItems"], 4);
    let validator = jsonschema::options().build(&schema).unwrap();
    for question_count in [0, 1, 4, 5, 32] {
        for option_count in [0, 1, 2, 4, 5, 32] {
            let mut input = valid.clone();
            input["questions"] = serde_json::json!((0..question_count).map(|index| {
                let mut question = valid["questions"][0].clone();
                question["id"] = serde_json::json!(format!("q{index}"));
                question["options"] = serde_json::json!((0..option_count).map(|index| {
                    serde_json::json!({"id":format!("o{index}"),"label":"Option","description":null})
                }).collect::<Vec<_>>());
                question
            }).collect::<Vec<_>>());
            let allowed = (1..=4).contains(&question_count) && (2..=4).contains(&option_count);
            assert_eq!(
                validator.is_valid(&input),
                allowed,
                "schema: {question_count}/{option_count}"
            );
            let decoded = serde_json::from_value::<AskUserQuestionInput>(input);
            assert_eq!(
                decoded.is_ok(),
                allowed,
                "decode: {question_count}/{option_count}"
            );
            if let Ok(decoded) = decoded {
                assert!(decoded.valid());
            }
        }
    }
    assert!(
        !serde_json::to_string(&schema)
            .unwrap()
            .contains("serde_json::Value")
    );
}

#[test]
fn v2_batch_validation_rejects_empty_overflow_and_duplicates() {
    let one = id("x.posts.search.advanced");
    assert!(
        !ToolGetInput {
            operation_ids: vec![],
            include_output_schema: true,
        }
        .valid()
    );
    assert!(
        !ToolGetInput {
            operation_ids: vec![one.clone(), one.clone()],
            include_output_schema: true,
        }
        .valid()
    );
    assert!(
        !ToolGetInput {
            operation_ids: (0..9)
                .map(|index| id(&format!("test.operation.{index}")))
                .collect(),
            include_output_schema: true,
        }
        .valid()
    );

    let call = |call_id: &str| ToolExecutionCall {
        call_id: id(call_id),
        operation_id: one.clone(),
        schema_revision: "sha256:test".into(),
        authority: ToolExecutionAuthority::Direct {},
        connection: None,
        arguments: Map::new(),
    };
    assert!(
        ToolExecuteInput {
            calls: vec![call("call.one")],
            presentation: PresentationRequest::IfRequired,
        }
        .valid()
    );
    assert!(
        !ToolExecuteInput {
            calls: vec![call("call.same"), call("call.same")],
            presentation: PresentationRequest::IfRequired,
        }
        .valid()
    );
}

#[test]
fn v2_wire_decode_rejects_invalid_search_and_batches() {
    assert!(
        serde_json::from_value::<ToolSearchInput>(serde_json::json!({
            "query": "在 X 找人",
            "toolkits": [],
            "effects": [],
            "limit": 8
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ToolSearchInput>(serde_json::json!({
            "query": "search X accounts",
            "toolkits": ["x", "x"],
            "effects": [],
            "limit": 8
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ToolGetInput>(serde_json::json!({
            "operation_ids": ["x.posts.search.advanced", "x.posts.search.advanced"],
            "include_output_schema": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ToolExecuteInput>(serde_json::json!({
            "calls": [
                {
                    "call_id": "call.same",
                    "operation_id": "x.posts.search.advanced",
                    "schema_revision": "sha256:test",
                    "authority": {"kind": "direct"},
                    "connection": null,
                    "arguments": {}
                },
                {
                    "call_id": "call.same",
                    "operation_id": "x.profile.get",
                    "schema_revision": "sha256:test",
                    "authority": {"kind": "direct"},
                    "connection": null,
                    "arguments": {}
                }
            ],
            "presentation": "never"
        }))
        .is_err()
    );
}

#[test]
fn v2_response_schemas_fit_the_fixed_context_budget() {
    let schemas = [
        schema_value::<ToolSearchResult>(),
        schema_value::<ToolGetResult>(),
        schema_value::<ToolExecuteResult>(),
        schema_value::<ResultPresentResult>(),
    ];
    let bytes = serde_json::to_vec(&schemas).expect("schema bytes").len();
    assert!(bytes < 64 * 1024, "V2 response schemas use {bytes} bytes");
    let execute = schema_value::<ToolExecuteResult>();
    assert!(execute["properties"].get("presentation_failure").is_some());
    let wire = serde_json::to_string(&execute).unwrap();
    for code in [
        "projection_unavailable",
        "capacity",
        "store_rejected",
        "store_unavailable",
    ] {
        assert!(wire.contains(code), "missing presentation failure {code}");
    }
    assert_eq!(MAX_SEARCH_OPERATIONS, 8);
    assert_eq!(MAX_SEARCH_GUIDES, 8);
    assert_eq!(MAX_WORKFLOW_HINTS, 3);
}

#[test]
fn toolkit_wire_identity_is_onecrew() {
    assert_eq!(ToolkitId::OneCrew.as_str(), "onecrew");
    assert_eq!(serde_json::to_value(ToolkitId::OneCrew).unwrap(), "onecrew");
    assert_eq!(
        serde_json::from_str::<ToolkitId>("\"onecrew\"").unwrap(),
        ToolkitId::OneCrew
    );
    assert!(serde_json::from_str::<ToolkitId>("\"crm\"").is_err());
    assert!(
        serde_json::from_value::<ToolSearchInput>(serde_json::json!({
            "query": "list creators", "toolkits": ["crm"]
        }))
        .is_err()
    );
}
