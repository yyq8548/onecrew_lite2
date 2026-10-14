//! Transport-independent service for a local executable host. Clones share the
//! registry and operation ownership; the host supplies admission and transport.
use std::sync::Arc;

use onecrew_local_runtime::LocalPlatform;
use rmcp::{ErrorData, RoleServer, ServerHandler, model::*, service::RequestContext};

use super::surface::McpAdapterServer;
use crate::operation_registry::OperationRegistryHandle;

/// The existing MCP business surface without an HTTP listener or bearer setup.
#[derive(Clone)]
pub struct LocalMcpService {
    server: McpAdapterServer,
}

impl LocalMcpService {
    /// Static App resources do not require login or an application graph.
    pub fn status_resource_uri() -> &'static str {
        crate::apps::app_resource_uri("operation-inspector")
    }

    pub fn app_resources() -> ListResourcesResult {
        ListResourcesResult::with_all_items(crate::apps::list_app_resources())
    }

    pub fn app_resource(uri: &str) -> Result<ReadResourceResponse, ErrorData> {
        crate::apps::read_app_resource(uri)
            .map(ReadResourceResponse::from)
            .ok_or_else(|| ErrorData::resource_not_found("unknown App resource", None))
    }
    /// The fixed protocol manifest can be listed without opening user data.
    /// Execution still requires the host's logged-in application admission.
    pub fn tools() -> Vec<Tool> {
        McpAdapterServer::surface_tool_router(super::surface::McpSurface::Full).list_all()
    }

    /// Stable discovery instructions and MCP Apps capabilities, available even
    /// before the local host admits a logged-in application graph.
    pub fn info() -> ServerInfo {
        super::surface::server_info(super::surface::McpSurface::Full)
    }

    /// Construct once per application graph over the registry the host
    /// compiled for it, then clone for client sessions.
    pub fn new(platform: Arc<LocalPlatform>, registry: &OperationRegistryHandle) -> Self {
        crate::qr_images::cleanup_stale_files();
        Self {
            server: McpAdapterServer::new(platform, registry),
        }
    }

    /// The registry build this service answers from.
    pub fn registry(&self) -> OperationRegistryHandle {
        self.server.registry()
    }
}

impl ServerHandler for LocalMcpService {
    fn get_info(&self) -> ServerInfo {
        self.server.get_info()
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        self.server.list_tools(request, context).await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        self.server.call_tool(request, context).await
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        self.server.list_resources(request, context).await
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        self.server.read_resource(request, context).await
    }
}
