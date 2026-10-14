//! The one compiled registry an application graph shares between its inbound
//! adapters.

use std::sync::Arc;

use onecrew_local_runtime::LocalPlatform;

use crate::accounts::{OperatorAccountWorkflow, PlatformRuntimeOperatorAccountWorkflow};

use super::{
    descriptor::SurfaceId, directory::ToolkitDirectory, domains::RegistryOwners,
    registration::OperationRegistry,
};

/// An opaque handle over the compiled operation registry and its search index.
///
/// The host builds it once per application graph and hands the same handle to
/// in-process MCP service. Its immutable catalog is also shared with the
/// independent HTTP navigation projection; Web does not read MCP registrations.
#[derive(Clone)]
pub struct OperationRegistryHandle {
    registry: Arc<OperationRegistry>,
}

impl OperationRegistryHandle {
    /// Compiles the registry from the runtime owners behind `platform`.
    ///
    /// # Errors
    ///
    /// Returns an error when the Creator runtime is unavailable or when the
    /// registry cannot be constructed from the supplied owners.
    pub fn build(
        platform: Arc<LocalPlatform>,
        catalog: Arc<onecrew_local_runtime::CapabilityCatalog>,
    ) -> Result<Self, String> {
        let creators = platform.creators()?;
        let operator_accounts: Arc<dyn OperatorAccountWorkflow> = Arc::new(
            PlatformRuntimeOperatorAccountWorkflow::new(Arc::clone(&platform)),
        );
        let registry = super::build_registry(
            RegistryOwners {
                platform,
                creators,
                operator_accounts,
            },
            catalog,
        )
        .map_err(|_| "failed to construct MCP operation registry".to_owned())?;
        Ok(Self::from_shared(Arc::new(registry)))
    }

    pub(crate) fn from_shared(registry: Arc<OperationRegistry>) -> Self {
        Self { registry }
    }

    /// Secret-free full-surface toolkit navigation projected from this build.
    pub fn toolkit_directory(&self) -> Vec<ToolkitDirectory> {
        self.registry.toolkit_directory(SurfaceId::Full)
    }

    /// Whether both handles point at the same compiled registry.
    pub fn shares_build_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.registry, &other.registry)
    }

    /// The immutable catalog shared with the application graph.
    pub fn catalog(&self) -> &Arc<onecrew_local_runtime::CapabilityCatalog> {
        self.registry.catalog()
    }

    pub(crate) fn registry(&self) -> Arc<OperationRegistry> {
        Arc::clone(&self.registry)
    }
}
