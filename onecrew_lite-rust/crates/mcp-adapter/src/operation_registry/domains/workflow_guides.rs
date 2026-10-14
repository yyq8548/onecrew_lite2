//! Read-only conversation guidance. No runtime owner, provider, or task is invoked.
mod contract;
mod definition;
mod enrich;
mod find;
mod outreach;
mod registry;

use contract::{GuideInput, GuideOutput};

use crate::operation_registry::{
    descriptor::{
        ConnectionRequirement, EffectClass, EntryId, OperationRegistration, SurfaceId, SurfaceSet,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};
use std::convert::Infallible;

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    let guides = registry::registered().map_err(|_| RegistryBuildError::InvalidWorkflowGuide)?;
    for definition in guides.groups().iter().flat_map(|group| group.definitions) {
        for id in definition.operation_ids {
            let reference = EntryId::parse(id.as_str())
                .map_err(|_| RegistryBuildError::InvalidCatalogReference)?;
            let Some(operation) = builder.catalog().operation(id.as_str()) else {
                return Err(RegistryBuildError::UnknownCatalogOperation(reference));
            };
            if !operation.supports(onecrew_local_runtime::CatalogSurface::McpBinding) {
                return Err(RegistryBuildError::OperationNotMcpSupported(reference));
            }
        }
    }
    builder.register_operation::<GuideInput, GuideOutput, Infallible, _, _, _>(
        OperationRegistration {
            id: onecrew_local_runtime::OperationId::ONECREW_WORKFLOW_GUIDE,
            domain_id: "workflow-guides".into(),
            aliases: vec![
                "workflow guidance".into(),
                "creator discovery enrichment outreach".into(),
            ],
            surfaces: SurfaceSet::new([SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators])
                .expect("guide surfaces"),
        },
        OperationPolicy::read_only(
            EffectClass::ReadLocal,
            ConnectionRequirement::None,
            crate::operation_registry::execution::ExecutionDeclaration::Sequential,
        ),
        move |_context, input| async move { Ok(contract::project(guides, input)) },
        |never| match never {},
    )
}

#[cfg(test)]
mod tests;
