//! MCP session admission. Every client gets an independent transport and peer;
//! all clients share one application graph and its operation registry.
use onecrew_mcp_adapter::LocalMcpService;
use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct Session {
    state: Arc<crate::ServiceState>,
    #[cfg(test)]
    test_service: Option<LocalMcpService>,
    #[cfg(test)]
    test_gate: Option<Arc<TestCallGate>>,
}

#[cfg(test)]
struct TestCallGate {
    entered: tokio::sync::Notify,
    release: tokio::sync::Notify,
    finished: tokio::sync::Notify,
}

impl Session {
    pub(crate) fn new(state: Arc<crate::ServiceState>) -> Self {
        Self {
            state,
            #[cfg(test)]
            test_service: None,
            #[cfg(test)]
            test_gate: None,
        }
    }

    #[cfg(test)]
    fn with_test_service(
        state: Arc<crate::ServiceState>,
        service: LocalMcpService,
        test_gate: Option<Arc<TestCallGate>>,
    ) -> Self {
        Self {
            state,
            test_service: Some(service),
            test_gate,
        }
    }
}

fn host_error(message: String) -> ErrorData {
    ErrorData::internal_error(message, Some(json!({"source":"onecrew_local_service"})))
}

fn login_tools() -> Vec<Tool> {
    let schema = serde_json::Map::from_iter([
        ("type".into(), Value::from("object")),
        ("properties".into(), json!({})),
        ("additionalProperties".into(), Value::Bool(false)),
    ]);
    [
        ("onecrew_status", "Read secret-free onecrew_lite service/login status and next_actions. When cli_update is available, briefly tell the user current_version to latest_version and the returned optional update method; wait for explicit update authorization and do not repeat the same version in one conversation. Label cached release evidence using source/checked_at. Never infer an update from current/unavailable/not_checked. This only checks metadata when Ready and active; it never updates, installs plugins, or queries Codex."),
        ("onecrew_login", "Begin onecrew_lite Access browser login. Open the returned authorize_url in the user's browser; wait for the user to log in there, then check onecrew_status. Never ask for their password or capture callback secrets."),
        ("onecrew_login_cancel", "Cancel the pending local onecrew_lite login and join its callback owner. A callback already enrolling is allowed to finish before cancellation returns."),
        ("onecrew_logout", "Log out of onecrew_lite locally after draining accepted work. Preserves Creator data and provider accounts; does not revoke the installation remotely. Use only when the user asks to log out."),
        ("onecrew_web", "Start every onecrew_lite session, including open/setup requests: mount the read-only status card and return verified workbench and Library URLs. Open BOTH returned pages in the customer's chosen browser, reusing tabs; honor explicit no-browser requests. All customer choices stay in the workbench. Before a Campaign is selected, Library home is the results entry; once selected open its verified results table. A missing Library URL is an exception, not a successful launch. This tool never grants business authority or starts searches."),
    ].into_iter().map(|(name, description)| {
        let mut tool = Tool::new(name, description, schema.clone());
        if name == "onecrew_status" {
            tool.meta = Some(MetaObject(serde_json::Map::from_iter([
                ("ui".into(), json!({"visibility":["model","app"]})),
            ])));
        }
        if name == "onecrew_web" {
            let uri = LocalMcpService::status_resource_uri();
            tool.meta = Some(MetaObject(serde_json::Map::from_iter([
                ("ui".into(), json!({"resourceUri":uri,"visibility":["model","app"]})),
                ("openai/outputTemplate".into(), json!(uri)),
            ])));
            tool.output_schema = Some(Arc::new(serde_json::from_value(json!({
                "type":"object","additionalProperties":false,
                "required":["kind","url","library_url","library_error","locale","startup","workspace_activity","workspace_activity_error"],
                "properties":{
                    "kind":{"const":"workspace"},"url":{"type":"string"},
                    "library_url":{"type":["string","null"]},"library_error":{"type":["string","null"]},
                    "locale":{"enum":["en-US","zh-CN"]},
                    "startup":{"enum":["inspecting","upgrading","ready","failed"]},
                    "workspace_activity_error":{"type":["string","null"]},
                    "workspace_activity":{"anyOf":[{"type":"null"},{
                        "type":"object","additionalProperties":false,
                        "required":["campaign_id","brief","stage","qualified","pending","target","attempted","total","used","max","login_required","read_errors","restricted","stale"],
                        "properties":{
                            "campaign_id":{"type":"string"},"brief":{"type":"string"},
                            "updated_at":{"type":"integer","minimum":1},
                            "library_sync_pending":{"type":"boolean"},
                            "deferred_scores":{"type":"integer","minimum":0},
                            "failure_phase":{"enum":["read","score","admission","readback","execution"]},
                            "stop":{"type":"object","additionalProperties":false,"required":["reason","recorded_at"],
                                "properties":{"reason":{"enum":["user_paused","search_page_left","page_closed","request_failed","revision_changed","progress_unconfirmed","busy","needs_evidence","search_exhausted","target_reached","budget_exhausted","execution_interrupted"]},
                                    "recorded_at":{"type":"integer","minimum":1,"maximum":8640000000000000u64}}},
                            "stage":{"enum":["ready","searching","verifying","pausing","paused","finished","budget_exhausted","needs_evidence","error"]},
                            "qualified":{"type":"integer","minimum":0},"pending":{"type":"integer","minimum":0},"target":{"type":"integer","minimum":0},
                            "attempted":{"type":"integer","minimum":0},"total":{"type":"integer","minimum":0},
                            "used":{"type":"integer","minimum":0},"max":{"type":"integer","minimum":0},
                            "login_required":{"type":"integer","minimum":0},"read_errors":{"type":"integer","minimum":0},"restricted":{"type":"integer","minimum":0},"stale":{"type":"boolean"}
                        }
                    }]}
                }
            })).expect("fixed workspace schema")));
            let schema = Arc::make_mut(tool.output_schema.as_mut().unwrap());
            let fields = schema["properties"]["workspace_activity"]["anyOf"][1]["properties"]
                .as_object_mut().expect("fixed activity properties");
            fields.insert("current_read_stops".into(), json!({"type":"integer","minimum":0}));
            fields.insert("read_operation_stops".into(), json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["platform","profile_ref","operation","code"],"properties":{
                "platform":{"type":"string"},"profile_ref":{"type":"string"},"operation":{"type":"string"},"code":{"type":"string"}
            }}}));
            for name in ["failure_code", "library_sync_error"] {
                fields.insert(name.into(), json!({"type":"string","pattern":"^[a-z][a-z_]{0,95}$"}));
            }
            fields.insert("library_synced_at".into(), json!({"type":"integer","minimum":1,"maximum":8640000000000000u64}));
        }
        tool
    }).collect()
}

fn card_request_tool() -> Tool {
    let mut tool = Tool::new(
        "onecrew_workspace_card_request",
        "Claim a server-authored, once-only display notification for an existing status view. Grants no business authority.",
        serde_json::from_value::<serde_json::Map<String, Value>>(json!({
            "type":"object","additionalProperties":false,"required":["view_id"],
            "properties":{"view_id":{"type":"string","format":"uuid"},"manual":{"type":"boolean"},"baseline_campaign_id":{"type":["string","null"],"format":"uuid"}}
        }))
        .expect("fixed card request schema"),
    );
    tool.meta = Some(MetaObject(serde_json::Map::from_iter([(
        "ui".into(),
        json!({"visibility":["app"]}),
    )])));
    tool.output_schema = Some(Arc::new(serde_json::from_value(json!({"type":"object","oneOf":[
        {"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"none"}}},
        {"type":"object","additionalProperties":false,"required":["kind","message"],"properties":{"kind":{"const":"message"},"message":{"type":"string","minLength":1,"maxLength":2048}}}
    ]})).expect("fixed card request reply schema")));
    tool
}

impl ServerHandler for Session {
    fn get_info(&self) -> ServerInfo {
        let mut info = LocalMcpService::info();
        info.server_info.name = "onecrew_lite".into();
        info.server_info.title = Some("onecrew_lite MCP".into());
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        let business = info.instructions.take().unwrap_or_default();
        info.instructions = Some(format!(
            "Bootstrap: onecrew_status, onecrew_login, onecrew_login_cancel, onecrew_logout, onecrew_web. {business}"
        ));
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let mut tools = login_tools();
        tools.push(card_request_tool());
        // Keep a stable manifest across login/logout so a host that caches its
        // initial tool list can use the same connection after authentication.
        // Static schemas expose no user data; every business call is gated.
        tools.extend(LocalMcpService::tools());
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if request.name == "onecrew_workspace_card_request" {
            let input: crate::workspace_card_requests::Request =
                serde_json::from_value(Value::Object(request.arguments.unwrap_or_default()))
                    .map_err(|_| {
                        ErrorData::invalid_params("workspace_card_request_invalid", None)
                    })?;
            let state = self.state.clone();
            let reply = self
                .state
                .controls
                .run(async move {
                    let account = crate::discovery_execution::account(&state).await?;
                    let reply = match state.discovery_execution.snapshot(&account) {
                        Some(run) => state.workspace_card_requests.request(
                            input,
                            &account,
                            &run.campaign_id,
                            state.presentation_preferences.locale()
                                == onecrew_local_runtime::PresentationLocale::SimplifiedChinese,
                        ),
                        None => crate::workspace_card_requests::Reply::None,
                    };
                    serde_json::to_value(reply)
                        .map_err(|_| "workspace_card_request_unavailable".to_owned())
                })
                .await
                .map_err(host_error)?
                .map_err(host_error)?;
            let mut response = CallToolResult::success(vec![ContentBlock::text(reply.to_string())]);
            response.structured_content = Some(reply);
            return Ok(response.into());
        }
        if login_tools().iter().any(|tool| tool.name == request.name) {
            if request
                .arguments
                .as_ref()
                .is_some_and(|args| !args.is_empty())
            {
                return Err(ErrorData::invalid_params(
                    "This login command takes no arguments",
                    None,
                ));
            }
            let state = self.state.clone();
            let name = request.name.to_string();
            let result = self
                .state
                .controls
                .run(async move {
                    match name.as_str() {
                        "onecrew_status" => serde_json::to_value(
                            state
                                .service_status(crate::status::StatusContext::Mcp)
                                .await?,
                        )
                        .map_err(|_| "status_unavailable".to_owned()),
                        "onecrew_login" => state.begin_login().await,
                        "onecrew_login_cancel" => state.cancel_login().await,
                        "onecrew_logout" => state.logout().await,
                        "onecrew_web" => {
                            let url = state.browser.launch_url()?;
                            let executable = crate::UserInstallLayout::current()?.executable().to_path_buf();
                            let library = crate::library_workspace_url(&executable).await;
                            let status = state.service_status(crate::status::StatusContext::Mcp).await?;
                            let (library_url, library_error) = match library {
                                Ok(url) => (Some(url), None),
                                Err(error) => (None, Some(error.split(':').next().unwrap_or("workspace_library_unavailable").to_owned())),
                            };
                            Ok(json!({"kind":"workspace","url":url,"library_url":library_url,
                                "library_error":library_error,"locale":state.presentation_preferences.locale(),
                                "startup":status.startup.phase,"workspace_activity":status.workspace_activity,
                                "workspace_activity_error":status.workspace_activity_error}))
                        },
                        _ => Err("unknown_login_command".to_owned()),
                    }
                })
                .await
                .map_err(host_error)?
                .map_err(host_error)?;
            let mut response =
                CallToolResult::success(vec![ContentBlock::text(result.to_string())]);
            response.structured_content = Some(result.clone());
            if request.name == "onecrew_web" {
                let view = uuid::Uuid::new_v4();
                let account = crate::discovery_execution::account(&self.state).await.ok();
                let baseline = result["workspace_activity"]["campaign_id"]
                    .as_str()
                    .map(str::to_owned);
                self.state
                    .workspace_card_requests
                    .register(view, account, baseline);
                response.meta = Some(MetaObject(serde_json::Map::from_iter([
                    ("onecrew/workspace".into(), result),
                    ("viewUUID".into(), json!(view.to_string())),
                ])));
            }
            return Ok(response.into());
        }
        let state = self.state.clone();
        #[cfg(test)]
        let test_service = self.test_service.clone();
        #[cfg(test)]
        let test_gate = self.test_gate.clone();
        self.state
            .work
            .run(async move {
                #[cfg(test)]
                let service = match test_service {
                    Some(service) => service,
                    None => state.mcp().await.map_err(host_error)?,
                };
                #[cfg(not(test))]
                let service = state.mcp().await.map_err(host_error)?;
                #[cfg(test)]
                if request.name.as_ref() == "onecrew_tool_execute"
                    && let Some(gate) = test_gate.as_ref()
                {
                    gate.entered.notify_waiters();
                    gate.release.notified().await;
                }
                let result = service.call_tool(request, context).await;
                #[cfg(test)]
                if let Some(gate) = test_gate {
                    gate.finished.notify_waiters();
                }
                result
            })
            .await
            .map_err(host_error)?
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        Ok(LocalMcpService::app_resources())
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        LocalMcpService::app_resource(&request.uri)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn card_notification_is_app_only_and_closed() {
        let tool = super::card_request_tool();
        assert_eq!(
            tool.meta.unwrap().0["ui"]["visibility"],
            serde_json::json!(["app"])
        );
        assert_eq!(tool.input_schema["additionalProperties"], false);
        assert!(tool.output_schema.is_some());
        assert_eq!(tool.output_schema.as_ref().unwrap()["type"], "object");
        assert!(
            serde_json::from_value::<crate::workspace_card_requests::Request>(
                serde_json::json!({"view_id":"not-uuid"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<crate::workspace_card_requests::Request>(
                serde_json::json!({"view_id":uuid::Uuid::new_v4(),"message":"injected"})
            )
            .is_err()
        );
    }
    use super::*;

    #[test]
    fn workspace_output_declares_every_serialized_activity_field() {
        let tools = login_tools();
        let web = tools
            .iter()
            .find(|tool| tool.name == "onecrew_web")
            .unwrap();
        let declared = &web.output_schema.as_ref().unwrap()["properties"]["workspace_activity"]["anyOf"]
            [1]["properties"];
        let actual = serde_json::to_value(schemars::schema_for!(
            crate::discovery_workspace::WorkspaceActivity
        ))
        .unwrap();
        let fields = |value: &Value| {
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(fields(declared), fields(&actual["properties"]));
    }

    #[test]
    fn workspace_entry_advertises_status_card_and_library_receipt() {
        let tools = login_tools();
        let web = tools
            .iter()
            .find(|tool| tool.name == "onecrew_web")
            .unwrap();
        assert!(
            web.meta
                .as_ref()
                .is_some_and(|meta| meta.0.get("ui").is_some()),
            "opening onecrew_lite must mount a status card"
        );
        assert!(
            web.output_schema.as_ref().is_some_and(|schema| schema
                .get("properties")
                .and_then(|properties| properties.get("library_url"))
                .is_some()),
            "opening onecrew_lite must return the Library entry"
        );
    }
    use rmcp::ServiceExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

    async fn send(stream: &mut BufReader<DuplexStream>, value: Value) {
        stream
            .get_mut()
            .write_all(format!("{value}\n").as_bytes())
            .await
            .unwrap();
    }
    async fn response(stream: &mut BufReader<DuplexStream>, id: u64) -> Value {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let mut line = String::new();
                assert!(stream.read_line(&mut line).await.unwrap() > 0);
                let value: Value = serde_json::from_str(&line).unwrap();
                assert_eq!(value["jsonrpc"], "2.0");
                if value["id"] == id {
                    return value;
                }
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn initialize_and_stable_manifest_need_no_login_or_application_graph() {
        let data = tempfile::tempdir().unwrap();
        let state = crate::ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        let (server_io, client_io) = tokio::io::duplex(256 * 1024);
        let server = tokio::spawn(async move {
            Session::new(state)
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        let mut client = BufReader::new(client_io);
        send(&mut client, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"onecrew-test","version":"1"}}})).await;
        let initialized = response(&mut client, 1).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "onecrew_lite");
        assert_eq!(
            initialized["result"]["serverInfo"]["title"],
            "onecrew_lite MCP"
        );
        assert_eq!(
            initialized["result"]["serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
        let instructions = initialized["result"]["instructions"]
            .as_str()
            .expect("stdio initialize instructions");
        assert!(
            instructions.len() <= 2048,
            "stdio instructions exceed the host budget: {} bytes",
            instructions.len()
        );
        assert!(
            instructions.is_ascii(),
            "stdio initialize instructions must remain ASCII for deterministic host truncation"
        );
        let prefix = &instructions[..instructions.len().min(512)];
        for tool in login_tools() {
            assert!(
                prefix.contains(tool.name.as_ref()),
                "stdio bootstrap route is incomplete: {}",
                tool.name
            );
        }
        let mcp_info = LocalMcpService::info();
        let mcp_instructions = mcp_info.instructions.as_deref().expect("MCP instructions");
        for tool in LocalMcpService::tools() {
            if mcp_instructions.contains(tool.name.as_ref()) {
                assert!(
                    prefix.contains(tool.name.as_ref()),
                    "stdio MCP route is incomplete: {}",
                    tool.name
                );
            }
        }
        assert!(prefix.contains(
            "User decisions belong in the onecrew_lite workbench; never decide for the user."
        ));
        send(
            &mut client,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .await;
        let tools = response(&mut client, 2).await;
        let names: Vec<_> = tools["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"onecrew_login"));
        assert!(names.contains(&"onecrew_status"));
        assert!(names.contains(&"onecrew_tool_execute"));
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":4,"method":"resources/list"}),
        )
        .await;
        let resources = response(&mut client, 4).await;
        let uri = resources["result"]["resources"][0]["uri"].as_str().unwrap();
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":uri}}),
        )
        .await;
        let resource = response(&mut client, 5).await;
        assert!(
            resource["result"]["contents"][0]["text"]
                .as_str()
                .unwrap()
                .contains("<!doctype html>")
        );
        assert_eq!(
            std::fs::read_dir(data.path()).unwrap().count(),
            0,
            "tool discovery must not construct the Creator runtime or vault"
        );
        send(&mut client, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"onecrew_connection_list","arguments":{}}})).await;
        let denied = response(&mut client, 3).await;
        assert!(
            denied["error"]["message"]
                .as_str()
                .unwrap()
                .contains("login_required")
        );
        drop(client);
        tokio::time::timeout(std::time::Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }

    async fn initialize_client(stream: &mut BufReader<DuplexStream>) {
        send(
            stream,
            json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"initialize",
                "params":{
                    "protocolVersion":"2025-06-18",
                    "capabilities":{},
                    "clientInfo":{"name":"onecrew-lifecycle-test","version":"1"}
                }
            }),
        )
        .await;
        let initialized = response(stream, 1).await;
        assert_eq!(initialized["result"]["serverInfo"]["name"], "onecrew_lite");
        send(
            stream,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;
    }

    fn execute_local_group_reads(revision: &str) -> Value {
        json!({
            "calls": (0..4).map(|index| json!({
                "call_id": format!("group-read-{index}"),
                "operation_id": "onecrew.creator.group.list",
                "schema_revision": revision,
                "authority": {"kind": "direct"},
                "connection": null,
                "arguments": {}
            })).collect::<Vec<_>>(),
            "presentation": "never"
        })
    }

    fn session_pair(
        state: Arc<crate::ServiceState>,
        service: LocalMcpService,
        test_gate: Option<Arc<TestCallGate>>,
    ) -> (tokio::task::JoinHandle<()>, BufReader<DuplexStream>) {
        let (server_io, client_io) = tokio::io::duplex(256 * 1024);
        let server = tokio::spawn(async move {
            Session::with_test_service(state, service, test_gate)
                .serve(server_io)
                .await
                .unwrap()
                .waiting()
                .await
                .unwrap();
        });
        (server, BufReader::new(client_io))
    }

    #[tokio::test]
    async fn disconnected_mcp_execute_drains_before_service_stop() {
        use onecrew_local_runtime::{CapabilityCatalog, LocalPlatform};
        use onecrew_mcp_adapter::OperationRegistryHandle;
        use std::task::Poll;

        let data = tempfile::tempdir().unwrap();
        let state = crate::ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.startup.wait_ready().await.unwrap();

        let platform = Arc::new(LocalPlatform::open(data.path()).unwrap());
        let catalog = Arc::new(CapabilityCatalog::build().unwrap());
        let registry = OperationRegistryHandle::build(platform.clone(), catalog).unwrap();
        let service = LocalMcpService::new(platform, &registry);

        // The real production registry and adapter handle the second client;
        // this gate only holds the first Session after WorkOwner admission so
        // the test can close its caller without inventing a provider manager.
        let gate = Arc::new(TestCallGate {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            finished: tokio::sync::Notify::new(),
        });
        let (control_server, mut control_client) =
            session_pair(state.clone(), service.clone(), None);
        initialize_client(&mut control_client).await;
        send(
            &mut control_client,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"onecrew_tool_get",
                    "arguments":{
                        "operation_ids":["onecrew.creator.group.list"],
                        "include_output_schema":false
                    }
                }
            }),
        )
        .await;
        let contract = response(&mut control_client, 2).await;
        let revision = contract["result"]["structuredContent"]["operations"][0]["schema_revision"]
            .as_str()
            .unwrap()
            .to_owned();

        let (caller_server, mut caller_client) =
            session_pair(state.clone(), service, Some(gate.clone()));
        initialize_client(&mut caller_client).await;
        send(
            &mut caller_client,
            json!({
                "jsonrpc":"2.0",
                "id":2,
                "method":"tools/call",
                "params":{
                    "name":"onecrew_tool_execute",
                    "arguments":execute_local_group_reads(&revision)
                }
            }),
        )
        .await;
        gate.entered.notified().await;

        // A live request through the same production service proves that the
        // real v2 route remains usable while another caller is disconnected.
        send(
            &mut control_client,
            json!({
                "jsonrpc":"2.0",
                "id":3,
                "method":"tools/call",
                "params":{
                    "name":"onecrew_tool_execute",
                    "arguments":execute_local_group_reads(&revision)
                }
            }),
        )
        .await;
        let live = response(&mut control_client, 3).await;
        assert_eq!(
            live["result"]["structuredContent"]["outcomes"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        drop(control_client);
        tokio::time::timeout(std::time::Duration::from_secs(5), control_server)
            .await
            .unwrap()
            .unwrap();

        // Dropping the caller must not cancel the Session's owned task.
        drop(caller_client);
        let mut stopping = Box::pin(state.stop());
        futures::future::poll_fn(|cx| {
            assert!(stopping.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert!(state.work.run(async {}).await.is_err());

        gate.release.notify_waiters();
        gate.finished.notified().await;
        stopping.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(5), caller_server)
            .await
            .unwrap()
            .unwrap();
    }
}
