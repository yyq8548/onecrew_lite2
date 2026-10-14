//! Plan scopes as models write them. Two live Codex sessions copied the search
//! call's arguments into each scope, with the neutral plan fields
//! `confirmed_plan_ref: null` and `search_plan_policy: "require_confirmation"`.
//! The two requests below are their prepare payloads; only the connection IDs
//! are synthetic and the schema revisions are read from the registry. Each
//! also runs as written by a model that follows the tool_get contracts, which
//! mark the plan fields and `cursor` execute-only.
use super::authorization::{confirm_prepared_plan, scope_call};
use super::*;
use crate::operation_registry::v2::{
    PresentationRequest, ToolExecuteInput, ToolExecuteOrchestrationFailure, ToolExecutionOutcome,
};
use onecrew_local_runtime::ResultOrigin;

const X_OPERATOR: &str = "10000000-0000-4000-8000-000000000002";
const YOUTUBE_CREDENTIAL: &str = "10000000-0000-4000-8000-000000000003";

fn x_transcript_request() -> Value {
    json!({
        "presentation": "requested",
        "calls": [{
            "call_id": "prepare_x_turkey_rwa_5_20260911",
            "operation_id": "onecrew.search.plan.prepare",
            "schema_revision": "from-registry",
            "authority": {"kind": "direct"},
            "connection": null,
            "arguments": {
                "description": "在 X 上发现并核验恰好 5 个与土耳其有明确关联的个人账号：主题聚焦 RWA（Real World Assets）及其 crypto/blockchain/tokenization 相关内容；近 30 天内有持续活动；至少有近期原创、具体、有信息量的内容；排除品牌/机构号、机器人、转发占主导、重复推广、抽奖、空投、引流或其他水文账号。仅做公开资料发现和证据整理，不保存到 OneCrew Creator，不联系、不发帖。最终不足 5 个时如实报告缺口，不用弱匹配补数。",
                "keywords": [
                    "土耳其/Türkiye 关联",
                    "RWA/Real World Assets",
                    "crypto/blockchain/tokenization",
                    "近30天活跃",
                    "原创实质内容",
                    "排除水文和转发主导",
                    "恰好5个",
                    "只读不写入CRM"
                ],
                "scopes": [{
                    "operation_id": "x.posts.search.advanced",
                    "schema_revision": "from-registry",
                    "connection": {
                        "kind": "operator_account",
                        "operator_account_id": X_OPERATOR,
                        "toolkit": "x"
                    },
                    "arguments": {
                        "all_words": [],
                        "any_words": [
                            "RWA",
                            "tokenization",
                            "tokenisation",
                            "tokenizasyon",
                            "tokenlaştırma",
                            "real world assets",
                            "security token",
                            "gerçek dünya varlıkları",
                            "varlık tokenizasyonu"
                        ],
                        "confirmed_plan_ref": null,
                        "count": 20,
                        "cursor": null,
                        "exact_phrase": null,
                        "exclude_words": ["giveaway", "airdrop", "sponsored", "referral", "casino"],
                        "from_accounts": [],
                        "hashtags": [],
                        "language": "tr",
                        "links": "any",
                        "mentioning_accounts": [],
                        "min_likes": null,
                        "min_replies": null,
                        "min_reposts": null,
                        "replies": "any",
                        "result_mode": "latest",
                        "search_plan_policy": "require_confirmation",
                        "since": "2026-08-12",
                        "to_accounts": [],
                        "until": null
                    }
                }]
            }
        }]
    })
}

fn youtube_transcript_request() -> Value {
    let connection = json!({
        "kind": "service_credential",
        "toolkit": "youtube",
        "credential_id": YOUTUBE_CREDENTIAL
    });
    let query = "Turkey Türkiye Turkish Türkçe RWA crypto real world assets tokenization";
    json!({
        "presentation": "requested",
        "calls": [{
            "call_id": "prepare-youtube-rwa-turkey-20260911",
            "operation_id": "onecrew.search.plan.prepare",
            "schema_revision": "from-registry",
            "authority": {"kind": "direct"},
            "connection": null,
            "arguments": {
                "description": "在 YouTube 发现并筛选恰好 5 个与土耳其区相关、围绕 RWA crypto/现实世界资产代币化持续创作的频道。硬性条件：近期有公开发布（优先近 90 天），内容需是原创且有实质分析、访谈、研究、教育或项目解读；排除纯价格喊单、重复转载、抽奖、泛泛营销和明显水文。只做公开资料发现与结果展示，不保存到 CRM，不联系创作者。",
                "keywords": [
                    "YouTube",
                    "Turkey",
                    "Türkiye",
                    "Turkish",
                    "Türkçe",
                    "RWA",
                    "real world assets",
                    "tokenization",
                    "crypto",
                    "recent activity",
                    "original substantive content",
                    "exclude spam",
                    "exactly 5"
                ],
                "scopes": [
                    {
                        "operation_id": "youtube.channels.search",
                        "schema_revision": "from-registry",
                        "connection": connection,
                        "arguments": {
                            "query": query,
                            "count": 20,
                            "search_plan_policy": "require_confirmation"
                        }
                    },
                    {
                        "operation_id": "youtube.videos.search",
                        "schema_revision": "from-registry",
                        "connection": connection,
                        "arguments": {
                            "query": query,
                            "count": 30,
                            "search_plan_policy": "require_confirmation"
                        }
                    }
                ]
            }
        }]
    })
}

/// The execute envelope of a plan-governed search; a plan scope omits it.
const EXECUTE_ONLY: [&str; 3] = ["confirmed_plan_ref", "cursor", "search_plan_policy"];

/// The prepare request as a model writes it when it follows the tool_get
/// contracts: each scope leaves out the arguments marked execute-only, and
/// those marks name exactly the execute envelope.
fn follow_contracts(
    registry: &crate::operation_registry::OperationRegistry,
    mut request: Value,
) -> Value {
    for scope in request["calls"][0]["arguments"]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        let contracts = registry
            .tool_get_v2(
                SurfaceId::Full,
                &crate::operation_registry::v2::ToolGetInput {
                    operation_ids: vec![
                        EntryId::parse(scope["operation_id"].as_str().unwrap()).unwrap(),
                    ],
                    include_output_schema: false,
                },
            )
            .unwrap();
        let properties = &contracts.operations[0].input_schema["properties"];
        scope["arguments"]
            .as_object_mut()
            .unwrap()
            .retain(|name, _| {
                let execute_only = properties[name]["description"]
                    .as_str()
                    .is_some_and(|text| text.starts_with("Execute-only"));
                assert_eq!(
                    execute_only,
                    EXECUTE_ONLY.contains(&name.as_str()),
                    "{name}"
                );
                !execute_only
            });
    }
    request
}

/// Decode a transcript request through the MCP wire type after replacing the
/// prepare and scope schema revisions with the registry's current values.
fn with_registry_revisions(
    registry: &crate::operation_registry::OperationRegistry,
    mut request: Value,
) -> ToolExecuteInput {
    let call = &mut request["calls"][0];
    call["schema_revision"] = json!(
        registry_operation(registry, "onecrew.search.plan.prepare")
            .input
            .revision()
    );
    for scope in call["arguments"]["scopes"].as_array_mut().unwrap() {
        let operation = scope["operation_id"].as_str().unwrap().to_owned();
        scope["schema_revision"] = json!(registry_operation(registry, &operation).input.revision());
    }
    serde_json::from_value(request).unwrap()
}

async fn prepare(
    registry: &crate::operation_registry::OperationRegistry,
    platform: &onecrew_local_runtime::LocalPlatform,
    origin: &ResultOrigin,
    label: &str,
    input: &ToolExecuteInput,
) -> Result<onecrew_local_runtime::ResultRef, ToolExecuteOrchestrationFailure> {
    let output = registry
        .tool_execute_v2(platform, origin, SurfaceId::Full, label, input)
        .await?;
    assert!(
        matches!(
            output.outcomes.as_slice(),
            [ToolExecutionOutcome::InputRequired { .. }]
        ),
        "{label}: {:?}",
        output.outcomes
    );
    Ok(output.result_ref.expect("a prepared plan mints a result"))
}

#[test]
fn every_plan_governed_search_contract_marks_exactly_its_execute_envelope() {
    let (_directory, _platform, registry) = production_registry();
    let operations = plan_governed_operations(&registry);
    assert!(!operations.is_empty());
    for (id, operation) in operations {
        let view = crate::operation_registry::descriptor::schema_json(&operation.input);
        let properties = view["properties"].as_object().unwrap();
        for name in ["confirmed_plan_ref", "search_plan_policy"] {
            assert!(properties.contains_key(name), "{id}/{name}");
        }
        for (name, property) in properties {
            let execute_only = property["description"]
                .as_str()
                .is_some_and(|text| text.starts_with("Execute-only"));
            assert_eq!(
                execute_only,
                EXECUTE_ONLY.contains(&name.as_str()),
                "{id}/{name}"
            );
        }
    }
}

#[tokio::test]
async fn transcript_prepare_requests_confirm_and_copied_scopes_reach_real_toolkit_admission() {
    let (_directory, platform, registry) = production_registry();
    let origin = ResultOrigin::new("mcp:full").unwrap();
    let youtube = vec!["youtube.channels.search", "youtube.videos.search"];
    for (label, request, expected) in [
        (
            "x-transcript",
            x_transcript_request(),
            vec!["x.posts.search.advanced"],
        ),
        (
            "x-contract",
            follow_contracts(&registry, x_transcript_request()),
            vec!["x.posts.search.advanced"],
        ),
        (
            "youtube-transcript",
            youtube_transcript_request(),
            youtube.clone(),
        ),
        (
            "youtube-contract",
            follow_contracts(&registry, youtube_transcript_request()),
            youtube,
        ),
    ] {
        let input = with_registry_revisions(&registry, request);
        let submitted = input.calls[0].arguments["scopes"].clone();
        let result_ref = prepare(&registry, &platform, &origin, label, &input)
            .await
            .unwrap_or_else(|failure| panic!("{label}: prepare failed: {failure:?}"));
        let (confirmed_plan_ref, scopes) =
            confirm_prepared_plan(&registry, &platform, &origin, &result_ref).await;
        assert_eq!(serde_json::to_value(&scopes).unwrap(), submitted, "{label}");
        assert_eq!(scopes.len(), expected.len(), "{label}");
        for (scope, operation) in scopes.iter().zip(expected) {
            assert_eq!(scope.operation_id, operation);
            let search = scope_call(&registry, scope, &confirmed_plan_ref);
            let output = registry
                .tool_execute_v2(
                    &platform,
                    &origin,
                    SurfaceId::Full,
                    &format!("{label}-{operation}"),
                    &ToolExecuteInput {
                        calls: vec![search],
                        presentation: PresentationRequest::Never,
                    },
                )
                .await
                .unwrap_or_else(|failure| panic!("{operation}: {failure:?}"));
            assert_connection_admission_failure(&output.outcomes);
        }
    }
}
