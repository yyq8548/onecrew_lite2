mod approval;
mod descriptor;
mod directory;
mod domains;
mod execution;
mod handle;
mod registration;
mod schema;
mod search;
mod v2;

pub(crate) use registration::OperationRegistry;
use registration::{RegistryBuildError, RegistryBuilder};

pub(crate) use descriptor::SurfaceId;
pub use directory::{ToolkitDirectory, ToolkitOperationSummary};
pub(crate) use execution::{BlockingTaskFailure, run_blocking};
pub use handle::OperationRegistryHandle;
pub(crate) use v2::apply_v2_tool_metadata;
pub use v2::{app_presentation_kinds, result_contract_schema_manifest};

pub(crate) use domains::RegistryOwners;

pub(crate) fn build_registry(
    owners: RegistryOwners,
    catalog: std::sync::Arc<onecrew_local_runtime::CapabilityCatalog>,
) -> Result<OperationRegistry, RegistryBuildError> {
    let mut builder = RegistryBuilder::new(catalog);
    domains::register_all(&mut builder, owners)?;
    builder.build()
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod contract_snapshot_tests;

#[cfg(test)]
pub(crate) mod catalog_test_support;
