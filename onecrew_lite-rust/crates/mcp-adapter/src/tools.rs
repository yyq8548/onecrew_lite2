use crate::server::McpAdapterServer;
use rmcp::handler::server::router::tool::ToolRouter;

impl McpAdapterServer {
    /// Every ordinary endpoint exposes the exact V2 model surface plus one
    /// App-only result-state capability. The Skills endpoint is strictly the
    /// read-only Search/Get directory and has no mounted result capability.
    pub(crate) fn surface_tool_router(surface: crate::server::McpSurface) -> ToolRouter<Self> {
        let mut router = Self::v2_router();
        if surface == crate::server::McpSurface::Skills {
            for route in [
                "onecrew_tool_execute",
                "onecrew_result_present",
                "onecrew_result_state",
                "onecrew_connection_list",
            ] {
                router.remove_route(route);
            }
        }
        crate::operation_registry::apply_v2_tool_metadata(&mut router);
        router
    }
}

#[cfg(test)]
mod tests;
