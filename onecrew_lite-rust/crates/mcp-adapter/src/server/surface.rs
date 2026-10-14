use std::sync::Arc;

use onecrew_local_runtime::LocalPlatform;
use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{ExtensionCapabilities, JsonObject, ServerCapabilities, ServerInfo},
    tool_handler,
};

/// Fixed MCP surfaces; each endpoint pins one immutable tool manifest for
/// the connection while sharing the runtime graph and bearer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum McpSurface {
    Full,
    Compact,
    Accounts,
    Creators,
    Outreach,
    Mail,
    /// Documentation-only endpoint: operator skills and the navigation
    /// catalog, no workflow tools. The same skills also ride every other
    /// surface; this exists so a human wiring up an agent can browse them
    /// without granting any workflow capability.
    Skills,
}

impl McpSurface {
    pub(crate) const ALL: [McpSurface; 7] = [
        McpSurface::Full,
        McpSurface::Compact,
        McpSurface::Accounts,
        McpSurface::Creators,
        McpSurface::Outreach,
        McpSurface::Mail,
        McpSurface::Skills,
    ];

    pub(crate) fn path(self) -> &'static str {
        match self {
            McpSurface::Full => "/mcp",
            McpSurface::Compact => "/mcp/compact",
            McpSurface::Accounts => "/mcp/domains/accounts",
            McpSurface::Creators => "/mcp/domains/creators",
            McpSurface::Outreach => "/mcp/domains/outreach",
            McpSurface::Mail => "/mcp/domains/mail",
            McpSurface::Skills => "/mcp/skills",
        }
    }

    fn tool_router(self) -> ToolRouter<McpAdapterServer> {
        McpAdapterServer::surface_tool_router(self)
    }

    pub(crate) const fn registry_id(self) -> crate::operation_registry::SurfaceId {
        use crate::operation_registry::SurfaceId;
        match self {
            Self::Full => SurfaceId::Full,
            Self::Compact => SurfaceId::Compact,
            Self::Accounts => SurfaceId::Accounts,
            Self::Creators => SurfaceId::Creators,
            Self::Outreach => SurfaceId::Outreach,
            Self::Mail => SurfaceId::Mail,
            Self::Skills => SurfaceId::Skills,
        }
    }
}

#[derive(Clone)]
pub(crate) struct McpAdapterServer {
    pub(crate) update_control: super::McpAdapterUpdateControl,
    pub(crate) platform: Arc<LocalPlatform>,
    pub(crate) surface: McpSurface,
    pub(crate) operation_registry: Arc<crate::operation_registry::OperationRegistry>,
    tool_router: ToolRouter<Self>,
}

impl McpAdapterServer {
    /// One server over the registry the host compiled for this platform.
    pub(crate) fn new(
        platform: Arc<LocalPlatform>,
        registry: &crate::operation_registry::OperationRegistryHandle,
    ) -> Self {
        Self {
            update_control: super::McpAdapterUpdateControl::default(),
            platform,
            surface: McpSurface::Full,
            operation_registry: registry.registry(),
            tool_router: Self::surface_tool_router(McpSurface::Full),
        }
    }

    pub(crate) fn registry(&self) -> crate::operation_registry::OperationRegistryHandle {
        crate::operation_registry::OperationRegistryHandle::from_shared(Arc::clone(
            &self.operation_registry,
        ))
    }

    async fn dispatch_owned_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        if request.name.as_ref() == "onecrew_tool_execute"
            && request
                .arguments
                .as_ref()
                .and_then(|arguments| arguments.get("calls"))
                .and_then(serde_json::Value::as_array)
                .is_some_and(|calls| {
                    calls.iter().any(|call| {
                        call.get("arguments")
                            .is_some_and(|arguments| !arguments.is_object())
                    })
                })
        {
            return Err(rmcp::ErrorData::invalid_params(
                "arguments must be object",
                Some(serde_json::json!({
                    "code": "operation_arguments_not_object",
                    "parameter": "calls[].arguments",
                    "nextAction": "Pass every call's arguments as a JSON object directly; do not stringify it."
                })),
            ));
        }
        let context = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router
            .call(context)
            .await
            .map_err(crate::failure::sanitize_error_data)
    }

    /// The same runtime graph behind a different fixed surface.
    pub(crate) fn for_surface(&self, surface: McpSurface) -> Self {
        let mut server = self.clone();
        server.surface = surface;
        server.tool_router = surface.tool_router();
        server
    }
}

/// Hosts may truncate instructions hard, so each surface starts with one
/// self-contained route that its first-512-byte test exercises before the
/// shared routing-priority and safety guidance.
const COMPACT_STATELESS_ROUTE: &str = "Use onecrew_connection_list for account status/selection. \
    Discover capabilities with onecrew_tool_search; get exact contracts with onecrew_tool_get; execute via \
    onecrew_tool_execute. Present result_ref once with onecrew_result_present(locale=zh-CN for \
    Chinese users, en-US otherwise). Cards show status only. Presentation is not execution.";

const USER_ONLY_DECISION_RULE: &str =
    "User decisions belong in the onecrew_lite workbench; never decide for the user.";

const SKILLS_CATALOG_ROUTE: &str = "Use this read-only directory route: call onecrew_tool_search with one \
    complete normalized-English outcome and pass every user-named toolkit as a filter. Search \
    returns complete compact toolkit guides and bounded workflow hints. Call onecrew_tool_get only for \
    exact current contracts of selected operation IDs; this surface cannot execute or present.";

const ROUTING_PRIMACY: &str = "Authoritative gateway for connected X, Instagram, Telegram, YouTube \
    and onecrew_lite (creators, outreach, mail, choices), covering people, posts, communities. \
    Never substitute web search, browsing or memory unless the user explicitly requests an outside source.";

const STATELESS_SAFETY_GUIDANCE: &str = "Normalize only capability-search queries to English. Write user-facing plan descriptions \
    in user's language. External creator/content searches require onecrew.search.plan.prepare + confirmed_plan_ref; policy cannot bypass. \
    After confirmation, copy exact search args; one dependency round; transport completion != provider success. \
    Split unrelated outcomes. Effect filters only explicit registry rules, not merely excluding writes. \
    Workflow hints aren't executable. Never echo secrets or invent IDs, schemas, refs or fallbacks.";

const CREATOR_DISCOVERY_REVIEW_GUIDANCE: &str = "Creator discovery: After the confirmed search plan, show candidates in the onecrew_lite workbench \
    after search/match/assessment; before user confirmation, do not write Creators or contact anyone. \
    Schema: one existing candidate batch-save review: onecrew.creator.candidate.batch.save, up to 50, explicit add_new; \
    save_or_update only for explicit updates; present result_ref and stop. Cards show status only. \
    Select and confirm in the workbench; stop if its owner-backed review is unavailable; \
    never trigger an unrequested write.";

pub(crate) fn surface_instructions(surface: McpSurface) -> String {
    let route = if surface == McpSurface::Skills {
        SKILLS_CATALOG_ROUTE
    } else {
        COMPACT_STATELESS_ROUTE
    };
    let mut instructions = String::from(route);
    if matches!(surface, McpSurface::Full | McpSurface::Compact) {
        instructions.push(' ');
        instructions.push_str(USER_ONLY_DECISION_RULE);
    }
    instructions.push(' ');
    instructions.push_str(ROUTING_PRIMACY);
    instructions.push(' ');
    instructions.push_str(STATELESS_SAFETY_GUIDANCE);
    if matches!(surface, McpSurface::Full | McpSurface::Creators) {
        instructions.push(' ');
        instructions.push_str(CREATOR_DISCOVERY_REVIEW_GUIDANCE);
        instructions.push(' ');
        instructions.push_str(&onecrew_creator_runtime::creator_tags_description());
    }
    instructions
}

const MCP_APPS_EXTENSION_ID: &str = "io.modelcontextprotocol/ui";

pub(super) fn server_capabilities() -> ServerCapabilities {
    let mut settings = JsonObject::new();
    settings.insert(
        "mimeTypes".into(),
        serde_json::json!([crate::apps::APPS_RESOURCE_MIME]),
    );
    let mut extensions = ExtensionCapabilities::new();
    extensions.insert(MCP_APPS_EXTENSION_ID.into(), settings);
    ServerCapabilities::builder()
        .enable_extensions_with(extensions)
        .enable_tools()
        .enable_resources()
        .build()
}

pub(super) fn server_info(surface: McpSurface) -> ServerInfo {
    ServerInfo::new(server_capabilities())
        .with_server_info(
            rmcp::model::Implementation::new("onecrew_lite", env!("CARGO_PKG_VERSION"))
                .with_title("onecrew_lite MCP"),
        )
        .with_instructions(surface_instructions(surface))
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpAdapterServer {
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, rmcp::ErrorData> {
        let server = self.clone();
        self.update_control
            .run_owned(async move { server.dispatch_owned_tool(request, context).await })
            .await
    }

    fn get_info(&self) -> ServerInfo {
        server_info(self.surface)
    }

    // The resources capability serves MCP Apps UI templates only (ADR
    // 0008): enumeration lists the current `ui://` component. Reads return
    // current bytes for its exact hash or the inert incompatibility App for a
    // valid retired inspector hash; malformed and foreign URIs fail closed.
    // Workflow discovery stays tools/list plus the catalog and skills.
    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        Ok(rmcp::model::ListResourcesResult::with_all_items(
            crate::apps::list_app_resources(),
        ))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
        crate::apps::read_app_resource(&request.uri)
            .map(rmcp::model::ReadResourceResponse::from)
            .ok_or_else(|| {
                rmcp::ErrorData::resource_not_found(
                    format!("unknown ui resource {}", request.uri),
                    None,
                )
            })
    }
}

#[cfg(test)]
mod native_dispatch_tests {
    use super::*;

    #[test]
    fn every_surface_publishes_onecrew_identity() {
        for surface in McpSurface::ALL {
            let info = server_info(surface);
            assert_eq!(info.server_info.name, "onecrew_lite");
            assert_eq!(info.server_info.title.as_deref(), Some("onecrew_lite MCP"));
        }
        let info = super::super::local::LocalMcpService::info();
        assert_eq!(info.server_info.name, "onecrew_lite");
        assert_eq!(info.server_info.title.as_deref(), Some("onecrew_lite MCP"));
    }

    const SOURCE: &str = include_str!("surface.rs");
    const HOST_PREFIX_BYTES: usize = 512;

    #[test]
    fn creator_write_instructions_share_the_owner_tag_requirements() {
        let requirements = onecrew_creator_runtime::creator_tags_description();
        for surface in [McpSurface::Full, McpSurface::Creators] {
            assert!(
                server_info(surface)
                    .instructions
                    .as_deref()
                    .is_some_and(|instructions| instructions.contains(&requirements))
            );
        }
        assert!(
            !server_info(McpSurface::Skills)
                .instructions
                .as_deref()
                .is_some_and(|instructions| instructions.contains(&requirements))
        );
    }

    fn host_prefix(instructions: &str) -> &str {
        assert!(instructions.is_ascii());
        &instructions[..instructions.len().min(HOST_PREFIX_BYTES)]
    }

    fn assert_in_order(text: &str, needles: &[&str]) {
        let mut cursor = 0;
        for needle in needles {
            let offset = text[cursor..]
                .find(needle)
                .unwrap_or_else(|| panic!("missing `{needle}` from `{text}`"));
            cursor += offset + needle.len();
        }
    }

    #[test]
    fn assembled_first_512_bytes_hold_the_complete_surface_valid_route() {
        for surface in McpSurface::ALL {
            let info = server_info(surface);
            let instructions = info.instructions.as_deref().expect("instructions");
            let prefix = host_prefix(instructions);
            if surface == McpSurface::Skills {
                assert!(
                    instructions.starts_with(&format!("{SKILLS_CATALOG_ROUTE} {ROUTING_PRIMACY}"))
                );
                assert!(prefix.contains(SKILLS_CATALOG_ROUTE));
                assert_in_order(
                    prefix,
                    &[
                        "onecrew_tool_search",
                        "workflow hints",
                        "onecrew_tool_get",
                        "cannot execute or present",
                    ],
                );
                for unavailable in [
                    "onecrew_tool_execute",
                    "onecrew_result_present",
                    "onecrew_connection_list",
                ] {
                    assert!(!instructions.contains(unavailable), "{unavailable}");
                }
            } else {
                let expected_lead = if matches!(surface, McpSurface::Full | McpSurface::Compact) {
                    format!("{COMPACT_STATELESS_ROUTE} {USER_ONLY_DECISION_RULE} {ROUTING_PRIMACY}")
                } else {
                    format!("{COMPACT_STATELESS_ROUTE} {ROUTING_PRIMACY}")
                };
                assert!(instructions.starts_with(&expected_lead));
                assert!(prefix.contains(COMPACT_STATELESS_ROUTE));
                assert!(prefix.contains("locale=zh-CN"));
                assert!(prefix.contains("en-US otherwise"));
                assert!(
                    instructions.contains("Normalize only capability-search queries to English")
                );
                assert!(instructions.contains("user-facing plan descriptions"));
                let route = vec![
                    "onecrew_connection_list",
                    "onecrew_tool_search",
                    "onecrew_tool_get",
                    "onecrew_tool_execute",
                    "onecrew_result_present",
                ];
                assert!(!instructions.contains("onecrew.ask_user_question"));
                assert_in_order(prefix, &route);
            }
        }
    }

    #[test]
    fn assembled_instructions_require_workbench_review_before_writes() {
        for surface in [McpSurface::Full, McpSurface::Creators] {
            let instructions = server_info(surface).instructions.expect("instructions");
            for marker in [
                "After the confirmed search plan, show candidates in the onecrew_lite workbench",
                "before user confirmation, do not write Creators or contact anyone",
                "one existing candidate batch-save review",
                "up to 50",
                "explicit add_new",
                "present result_ref and stop",
                "Cards show status only",
                "never trigger an unrequested write",
            ] {
                assert!(
                    instructions.contains(marker),
                    "{surface:?}: missing {marker}"
                );
            }
        }
    }

    #[test]
    fn final_http_server_info_has_the_user_decision_rule_and_byte_budget() {
        for surface in McpSurface::ALL {
            let info = server_info(surface);
            let instructions = info.instructions.as_deref().expect("instructions");
            assert!(
                instructions.len() <= 2048,
                "{surface:?} instructions exceed the host budget: {} bytes",
                instructions.len()
            );
            assert_eq!(
                instructions.contains(USER_ONLY_DECISION_RULE),
                matches!(surface, McpSurface::Full | McpSurface::Compact),
                "user-decision guidance does not match the operation surface"
            );
        }
    }

    #[test]
    fn project_server_delegates_dispatch_and_disabled_state_to_rmcp() {
        for forbidden in [
            [".has_", "route("].concat(),
            [".map.", "get("].concat(),
            ["(route.", "call)("].concat(),
        ] {
            assert!(
                !SOURCE.contains(&forbidden),
                "manual dispatch returned: {forbidden}"
            );
        }
        let native_call = [".call(", "context)"].concat();
        assert_eq!(SOURCE.matches(&native_call).count(), 1);

        let mut router = McpAdapterServer::surface_tool_router(McpSurface::Full);
        let name = "onecrew_connection_list";
        assert!(router.get(name).is_some());
        assert!(router.disable_route(name));
        assert!(router.get(name).is_none());
        assert!(!router.list_all().iter().any(|tool| tool.name == name));
    }

    #[test]
    fn every_app_bearing_surface_advertises_the_official_ui_extension() {
        let capabilities = serde_json::to_value(server_capabilities()).unwrap();
        assert_eq!(
            capabilities["extensions"][MCP_APPS_EXTENSION_ID]["mimeTypes"],
            serde_json::json!([crate::apps::APPS_RESOURCE_MIME])
        );
        assert!(capabilities["tools"].is_object());
        assert!(capabilities["resources"].is_object());

        for surface in McpSurface::ALL {
            let router = McpAdapterServer::surface_tool_router(surface);
            if surface == McpSurface::Skills {
                assert!(router.get("onecrew_result_present").is_none());
                continue;
            }
            assert!(
                router.get("onecrew_result_present").is_some(),
                "{surface:?}"
            );
            let get = router.get("onecrew_tool_get").expect("plain tool get");
            assert!(get.meta.as_ref().is_none_or(|meta| {
                !meta.0.contains_key("ui") && !meta.0.contains_key("openai/outputTemplate")
            }));
        }
        let resources = crate::apps::list_app_resources();
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources[0].mime_type.as_deref(),
            Some(crate::apps::APPS_RESOURCE_MIME)
        );
    }
}
