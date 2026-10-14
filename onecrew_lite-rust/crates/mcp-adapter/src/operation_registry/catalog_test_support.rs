//! Explicit catalog fixtures for isolated MCP registry contracts.
use onecrew_local_runtime::{
    CapabilityCatalog, CatalogSurface, OperationId, OperationSummary, ToolkitId,
};
use std::sync::Arc;

pub(crate) fn operation_id(id: &str) -> OperationId {
    OperationId::from_static(Box::leak(id.to_owned().into_boxed_str()))
}

pub(crate) fn catalog(ids: &[&str]) -> Arc<CapabilityCatalog> {
    let production = CapabilityCatalog::build().expect("production catalog");
    let toolkits = production.toolkits().cloned().collect::<Vec<_>>();
    let operations = ids
        .iter()
        .map(|id| {
            if let Some(summary) = production.operation(id) {
                return *summary;
            }
            OperationSummary {
                id: operation_id(id),
                toolkit_id: ToolkitId::OneCrew,
                title: Box::leak(format!("Title {id}").into_boxed_str()),
                summary: Box::leak(format!("Summary {id}").into_boxed_str()),
                surfaces: &[CatalogSurface::McpBinding],
            }
        })
        .collect::<Vec<_>>();
    Arc::new(
        CapabilityCatalog::from_static(
            Box::leak(toolkits.into_boxed_slice()),
            Box::leak(operations.into_boxed_slice()),
        )
        .expect("explicit fixture catalog"),
    )
}
