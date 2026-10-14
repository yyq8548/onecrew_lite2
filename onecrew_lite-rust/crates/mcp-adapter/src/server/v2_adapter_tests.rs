use std::sync::Arc;

use onecrew_creator_runtime::{CreateCreatorGroupInput, ListCreatorGroupsInput};
use onecrew_local_runtime::LocalPlatform;

use crate::operation_registry::OperationRegistryHandle;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::{CallToolRequestParams, ProtocolVersion},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};

use super::health::health_http_client;
use super::*;

pub(super) fn test_options(access_token: &str) -> McpAdapterOptions {
    McpAdapterOptions {
        port: 0,
        access_token: access_token.to_owned(),
    }
}

fn test_platform() -> (Arc<LocalPlatform>, tempfile::TempDir) {
    let directory = tempfile::tempdir().expect("temporary platform directory");
    let platform = LocalPlatform::open(directory.path()).expect("local platform");
    (Arc::new(platform), directory)
}

fn registry(platform: &Arc<LocalPlatform>) -> OperationRegistryHandle {
    OperationRegistryHandle::build(
        Arc::clone(platform),
        Arc::new(onecrew_local_runtime::CapabilityCatalog::build().unwrap()),
    )
    .expect("operation registry")
}

async fn connect(
    handle: &McpAdapterHandle,
    bearer: &str,
) -> rmcp::service::RunningService<rmcp::service::RoleClient, ()> {
    let transport = StreamableHttpClientTransport::with_client(
        health_http_client().expect("health client"),
        StreamableHttpClientTransportConfig::with_uri(handle.endpoint.clone()).auth_header(bearer),
    );
    ().serve_with_lifecycle(
        transport,
        ClientLifecycleMode::Discover {
            preferred_versions: vec![ProtocolVersion::V_2026_07_28],
        },
    )
    .await
    .expect("MCP lifecycle")
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
    tool: &str,
    input: serde_json::Value,
) -> rmcp::model::CallToolResult {
    let input = if tool == "onecrew_result_present" {
        let mut input = input;
        if let Some(object) = input.as_object_mut() {
            object
                .entry("locale")
                .or_insert_with(|| serde_json::json!("en-US"));
        }
        input
    } else {
        input
    };
    client
        .call_tool(
            CallToolRequestParams::new(tool.to_owned())
                .with_arguments(input.as_object().expect("tool input object").clone()),
        )
        .await
        .unwrap_or_else(|error| panic!("{tool} transport failure: {error}"))
}

async fn contracts(
    client: &rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
    operation_ids: &[&str],
) -> serde_json::Value {
    let result = call(
        client,
        "onecrew_tool_get",
        serde_json::json!({
            "operation_ids": operation_ids,
            "include_output_schema": false
        }),
    )
    .await;
    assert_ne!(result.is_error, Some(true));
    result.structured_content.expect("tool contracts")
}

fn revision(contracts: &serde_json::Value, index: usize) -> &str {
    contracts["operations"][index]["schema_revision"]
        .as_str()
        .expect("schema revision")
}

#[tokio::test]
async fn native_update_pause_keeps_an_idle_running_listener_and_restores_tool_calls() {
    let bearer = "update-pause-local-fixture";
    let (platform, _directory) = test_platform();
    let handle = McpAdapter::start(platform.clone(), &registry(&platform), test_options(bearer))
        .await
        .unwrap();
    let client = connect(&handle, bearer).await;
    contracts(&client, &["onecrew.ask_user_question"]).await;
    let pause = handle
        .update_control()
        .pause(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(
        handle.completion().current(),
        McpAdapterTaskCompletion::Running
    );
    let rejected = client.call_tool(CallToolRequestParams::new("onecrew_tool_get").with_arguments(serde_json::json!({"operation_ids":["onecrew.ask_user_question"],"include_output_schema":false}).as_object().unwrap().clone())).await;
    assert!(rejected.is_err());
    drop(pause);
    contracts(&client, &["onecrew.ask_user_question"]).await;
    client.cancel().await.unwrap();
    handle.stop().await.unwrap();
}

#[tokio::test]
async fn v2_search_get_choice_present_state_and_remount_are_one_closed_flow() {
    let bearer = format!("v2-choice-{}", rand::random::<u64>());
    let (platform, _directory) = test_platform();
    let handle = McpAdapter::start(
        platform.clone(),
        &registry(&platform),
        test_options(&bearer),
    )
    .await
    .expect("start adapter");
    let client = connect(&handle, &bearer).await;

    let tools = client.list_all_tools().await.expect("tools/list");
    for current in [
        "onecrew_tool_search",
        "onecrew_tool_get",
        "onecrew_tool_execute",
        "onecrew_connection_list",
        "onecrew_result_present",
    ] {
        assert!(tools.iter().any(|tool| tool.name == current), "{current}");
    }
    let state = tools
        .iter()
        .find(|tool| tool.name == "onecrew_result_state")
        .expect("App-only state tool");
    assert_eq!(
        state
            .meta
            .as_ref()
            .and_then(|meta| meta.0.get("ui"))
            .and_then(|ui| ui.get("visibility")),
        Some(&serde_json::json!(["app"]))
    );
    let state_input = serde_json::Value::Object((*state.input_schema).clone());
    assert_eq!(state_input["type"], "object");
    assert_eq!(state_input["additionalProperties"], false);
    assert!(state_input.get("oneOf").is_none());
    assert!(state_input.get("anyOf").is_none());
    assert_eq!(state_input["required"], serde_json::json!(["action"]));
    assert_eq!(
        state_input["properties"]
            .as_object()
            .expect("flat state input properties")
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
    let state_output = serde_json::Value::Object(
        (**state.output_schema.as_ref().expect("state output schema")).clone(),
    );
    assert_eq!(state_output["type"], "object");
    assert_eq!(state_output["additionalProperties"], false);
    assert!(state_output.get("oneOf").is_none());
    assert!(state_output.get("anyOf").is_none());

    let search = call(
        &client,
        "onecrew_tool_search",
        serde_json::json!({
            "query": "ask the user a question",
            "toolkits": ["onecrew"],
            "effects": [],
            "limit": 8
        }),
    )
    .await;
    assert_ne!(search.is_error, Some(true));
    let search = search.structured_content.expect("search result");
    assert!(
        search["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|operation| { operation["id"] == "onecrew.ask_user_question" })
    );
    assert!(
        search["toolkit_guides"][0]["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|operation| operation["id"] == "onecrew.ask_user_question")
    );

    let contracts = contracts(
        &client,
        &["onecrew.ask_user_question", "onecrew.creator.list"],
    )
    .await;
    assert_eq!(
        contracts["operations"][0]["id"],
        "onecrew.ask_user_question"
    );
    assert_eq!(contracts["operations"][1]["id"], "onecrew.creator.list");

    let execute = call(
        &client,
        "onecrew_tool_execute",
        serde_json::json!({
            "calls": [{
                "call_id": "choice.integration",
                "operation_id": "onecrew.ask_user_question",
                "schema_revision": revision(&contracts, 0),
                "authority": {"kind":"direct"},
                "connection": null,
                "arguments": {
                    "interaction_id": "choice.integration",
                    "title": "测试选择卡",
                    "questions": [
                        {
                            "id": "route",
                            "header": "联系路由",
                            "prompt": "请选择一种路由",
                            "options": [
                                {"id":"email","label":"Email","description":"保存公开邮箱"},
                                {"id":"youtube","label":"YouTube","description":null}
                            ]
                        },
                        {
                            "id": "notify",
                            "header": "继续任务",
                            "prompt": "是否继续？",
                            "options": [
                                {"id":"yes","label":"继续","description":null},
                                {"id":"no","label":"停止","description":null}
                            ]
                        }
                    ]
                }
            }],
            "presentation": "requested"
        }),
    )
    .await;
    assert_ne!(execute.is_error, Some(true));
    let execute = execute.structured_content.expect("execute result");
    assert_eq!(execute["outcomes"][0]["status"], "input_required");
    assert_eq!(execute["outcomes"][0]["input_kind"], "choice");
    let result_ref = execute["result_ref"].as_str().expect("result ref");

    let presented = call(
        &client,
        "onecrew_result_present",
        serde_json::json!({"result_ref": result_ref, "locale":"zh-CN"}),
    )
    .await;
    assert_ne!(presented.is_error, Some(true));
    assert_eq!(
        presented.structured_content.as_ref().unwrap()["status"],
        "available"
    );
    let meta = presented.meta.expect("present metadata");
    let snapshot = meta
        .0
        .get("onecrew/result")
        .expect("direct result snapshot");
    assert_eq!(meta.0["viewUUID"], snapshot["binding"]["view"]["identity"]);
    assert_eq!(snapshot["locale"], "zh-CN");
    let hydrated = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({
            "action":"read_current", "binding":snapshot["binding"]
        }),
    )
    .await;
    assert_eq!(
        hydrated.structured_content.unwrap()["snapshot"]["locale"],
        "zh-CN"
    );

    assert_eq!(
        snapshot["batch"]["outcomes"][0]["operation_id"],
        "onecrew.ask_user_question"
    );

    let selected = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({
            "action": "select",
            "binding": snapshot["binding"],
            "answers": [
                {"question_id":"route","option_id":"email"},
                {"question_id":"notify","option_id":"yes"}
            ]
        }),
    )
    .await;
    assert_ne!(selected.is_error, Some(true));
    let selected = selected.structured_content.expect("selected snapshot");
    assert_eq!(selected["kind"], "snapshot");
    assert_eq!(selected["snapshot"]["locale"], "zh-CN");
    assert!(selected["snapshot"]["pending_action"].is_null());

    let remounted = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({
            "action":"read_current",
            "binding": selected["snapshot"]["binding"]
        }),
    )
    .await;
    assert_ne!(remounted.is_error, Some(true));
    assert_eq!(
        remounted.structured_content.as_ref().unwrap()["snapshot"]["locale"],
        "zh-CN"
    );
    assert_eq!(
        remounted.structured_content.as_ref().unwrap()["snapshot"]["binding"],
        selected["snapshot"]["binding"]
    );

    let replay = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({
            "action": "select",
            "binding": selected["snapshot"]["binding"],
            "answers": [
                {"question_id":"route","option_id":"email"},
                {"question_id":"notify","option_id":"yes"}
            ]
        }),
    )
    .await;
    assert_eq!(replay.is_error, Some(true));
    assert_eq!(replay.structured_content.unwrap()["code"], "replay");

    client.cancel().await.expect("close client");
    handle.stop().await.expect("stop adapter");
}

#[tokio::test]
async fn v2_attended_confirmation_invokes_the_owner_exactly_once() {
    let bearer = format!("v2-confirm-{}", rand::random::<u64>());
    let (platform, _directory) = test_platform();
    let creators = platform.creators().unwrap();
    let handle = McpAdapter::start(
        platform.clone(),
        &registry(&platform),
        test_options(&bearer),
    )
    .await
    .expect("start adapter");
    let client = connect(&handle, &bearer).await;
    let contracts = contracts(&client, &["onecrew.creator.group.delete"]).await;
    let name = format!("V2 confirmation {}", rand::random::<u64>());
    let group = creators
        .create_creator_group(CreateCreatorGroupInput { name: name.clone() })
        .unwrap();
    let groups = || {
        creators
            .list_creator_groups(ListCreatorGroupsInput {
                limit: None,
                cursor: None,
                query: Some(name.clone()),
                include_archived: true,
            })
            .unwrap()
            .items
    };
    assert_eq!(groups().len(), 1);
    let execute = call(
        &client,
        "onecrew_tool_execute",
        serde_json::json!({
            "calls": [{
                "call_id": "group.delete",
                "operation_id": "onecrew.creator.group.delete",
                "schema_revision": revision(&contracts, 0),
                "authority": {"kind":"direct"},
                "connection": null,
                "arguments": {
                    "group_id": group.id,
                    "expected_revision": group.revision
                }
            }],
            "presentation": "requested"
        }),
    )
    .await;
    assert_eq!(groups().len(), 1, "owner ran before result confirmation");
    let execute = execute.structured_content.expect("execute result");
    let presented = call(
        &client,
        "onecrew_result_present",
        serde_json::json!({"result_ref": execute["result_ref"]}),
    )
    .await;
    let snapshot = presented.meta.unwrap().0["onecrew/result"].clone();
    let confirmed = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({"action":"confirm","binding":snapshot["binding"]}),
    )
    .await;
    assert_ne!(confirmed.is_error, Some(true));
    assert!(groups().is_empty(), "confirmed owner did not run once");
    let confirmed = confirmed.structured_content.unwrap();
    let replay = call(
        &client,
        "onecrew_result_state",
        serde_json::json!({"action":"confirm","binding":confirmed["snapshot"]["binding"]}),
    )
    .await;
    assert_eq!(replay.is_error, Some(true));
    assert!(groups().is_empty(), "replay invoked owner twice");

    client.cancel().await.expect("close client");
    handle.stop().await.expect("stop adapter");
}

#[tokio::test]
async fn v2_requested_secret_presentation_fails_before_owner_and_never_echoes_the_secret() {
    let bearer = format!("v2-secret-{}", rand::random::<u64>());
    let (platform, _directory) = test_platform();
    let handle = McpAdapter::start(
        platform.clone(),
        &registry(&platform),
        test_options(&bearer),
    )
    .await
    .expect("start adapter");
    let client = connect(&handle, &bearer).await;
    let contracts = contracts(&client, &["x.chat.passcode.set"]).await;
    let sentinel = "SECRET_SENTINEL_DO_NOT_ECHO";
    let rejected = call(
        &client,
        "onecrew_tool_execute",
        serde_json::json!({
            "calls": [{
                "call_id":"secret.passcode",
                "operation_id":"x.chat.passcode.set",
                "schema_revision":revision(&contracts, 0),
                "authority":{"kind":"direct"},
                "connection":null,
                "arguments":{
                    "operator_account_id":"missing-account",
                    "x_chat_passcode":sentinel
                }
            }],
            "presentation":"requested"
        }),
    )
    .await;
    assert_eq!(rejected.is_error, Some(true));
    let wire = serde_json::to_string(&rejected).unwrap();
    assert!(wire.contains("presentation_unavailable"));
    assert!(!wire.contains(sentinel));

    client.cancel().await.expect("close client");
    handle.stop().await.expect("stop adapter");
}

#[tokio::test]
async fn v2_process_restart_returns_a_strict_absent_result_instead_of_reopening_the_card() {
    let bearer = format!("v2-restart-{}", rand::random::<u64>());
    let directory = tempfile::tempdir().expect("temporary platform directory");
    let first_platform = Arc::new(LocalPlatform::open(directory.path()).expect("first platform"));
    let first_handle = McpAdapter::start(
        first_platform.clone(),
        &registry(&first_platform),
        test_options(&bearer),
    )
    .await
    .expect("start first adapter");
    let first_client = connect(&first_handle, &bearer).await;
    let contract = contracts(&first_client, &["onecrew.ask_user_question"]).await;
    let execute = call(
        &first_client,
        "onecrew_tool_execute",
        serde_json::json!({
            "calls": [{
                "call_id":"choice.restart",
                "operation_id":"onecrew.ask_user_question",
                "schema_revision":revision(&contract, 0),
                "authority":{"kind":"direct"},
                "connection":null,
                "arguments":{
                    "interaction_id":"choice.restart",
                    "title":"Restart probe",
                    "questions":[{
                        "id":"continue",
                        "header":"Continue",
                        "prompt":"Continue after restart?",
                        "options":[
                            {"id":"yes","label":"Yes","description":null},
                            {"id":"no","label":"No","description":null}
                        ]
                    }]
                }
            }],
            "presentation":"requested"
        }),
    )
    .await
    .structured_content
    .expect("execute result");
    let presented = call(
        &first_client,
        "onecrew_result_present",
        serde_json::json!({"result_ref":execute["result_ref"]}),
    )
    .await;
    let binding = presented.meta.unwrap().0["onecrew/result"]["binding"].clone();
    first_client.cancel().await.expect("close first client");
    first_handle.stop().await.expect("stop first adapter");

    let second_platform = Arc::new(LocalPlatform::open(directory.path()).expect("second platform"));
    let second_handle = McpAdapter::start(
        second_platform.clone(),
        &registry(&second_platform),
        test_options(&bearer),
    )
    .await
    .expect("start second adapter");
    let second_client = connect(&second_handle, &bearer).await;
    let expired = call(
        &second_client,
        "onecrew_result_state",
        serde_json::json!({"action":"read_current","binding":binding}),
    )
    .await;
    assert_eq!(expired.is_error, Some(true));
    assert_eq!(expired.structured_content.unwrap()["code"], "absent");

    second_client.cancel().await.expect("close second client");
    second_handle.stop().await.expect("stop second adapter");
}

#[tokio::test]
async fn v2_framework_and_extractor_failures_never_echo_rejected_secret_values() {
    let bearer = format!("v2-framework-{}", rand::random::<u64>());
    let (platform, _directory) = test_platform();
    let handle = McpAdapter::start(
        platform.clone(),
        &registry(&platform),
        test_options(&bearer),
    )
    .await
    .expect("start adapter");
    let client = connect(&handle, &bearer).await;
    let sentinel = "FRAMEWORK_SECRET_SENTINEL";

    for (tool, input) in [
        (
            "onecrew_tool_execute",
            serde_json::json!({
                "calls":[{
                    "call_id":"malformed.call",
                    "operation_id":"onecrew.creator.list",
                    "schema_revision":"invalid",
                    "authority":{"kind":"direct"},
                    "connection":null,
                    "arguments":sentinel
                }],
                "presentation":"never"
            }),
        ),
        (
            "onecrew_tool_get",
            serde_json::json!({
                "operation_ids":["onecrew.creator.list"],
                "include_output_schema":false,
                "rejected_secret":sentinel
            }),
        ),
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new(tool).with_arguments(input.as_object().unwrap().clone()),
            )
            .await;
        let wire = match result {
            Ok(result) => {
                assert_eq!(result.is_error, Some(true));
                serde_json::to_string(&result).unwrap()
            }
            Err(error) => format!("{error:?}"),
        };
        assert!(!wire.contains(sentinel), "{tool} leaked rejected input");
    }

    client.cancel().await.expect("close client");
    handle.stop().await.expect("stop adapter");
}

#[tokio::test]
async fn initialization_lists_fully_inlined_creator_batch_state_schema() {
    let bearer = "creator-batch-initialize-fixture";
    let (platform, _directory) = test_platform();
    let handle = McpAdapter::start(platform.clone(), &registry(&platform), test_options(bearer))
        .await
        .expect("start adapter with batch schema");
    let client = connect(&handle, bearer).await;
    let tools = client
        .list_all_tools()
        .await
        .expect("tools after initialize");
    let state = tools
        .iter()
        .find(|tool| tool.name == "onecrew_result_state")
        .expect("App state tool");
    let output =
        serde_json::to_string(state.output_schema.as_ref().expect("state schema")).unwrap();
    assert!(output.contains("creator_save_batch"));
    assert!(output.contains("creator_save_rows"));
    assert!(!output.contains("\"$ref\""));
    client.cancel().await.unwrap();
    handle.stop().await.unwrap();
}
