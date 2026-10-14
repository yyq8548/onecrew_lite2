//! Immutable capability identity and navigation, independent of adapters and
//! execution. Static declarations are the authority for all consumers.

mod inventory;
mod models;
mod validation;

use std::collections::BTreeMap;

pub use models::{CatalogSurface, OperationId, OperationSummary, ToolkitId, ToolkitSummary};
pub use validation::CatalogBuildError;

/// A validated process-local catalog with no execution or mutation API.
#[derive(Debug)]
pub struct CapabilityCatalog {
    toolkits: BTreeMap<&'static str, &'static ToolkitSummary>,
    operations: BTreeMap<&'static str, &'static OperationSummary>,
}

impl CapabilityCatalog {
    /// Builds the production catalog from the one static declaration source.
    pub fn build() -> Result<Self, CatalogBuildError> {
        Self::from_static(inventory::TOOLKITS, inventory::OPERATIONS)
    }

    /// Builds an explicit isolated catalog, including for adapter fixtures.
    ///
    /// No production entries are added implicitly. All descriptions must pass
    /// the same invariants as the production catalog.
    pub fn from_static(
        toolkits: &'static [ToolkitSummary],
        operations: &'static [OperationSummary],
    ) -> Result<Self, CatalogBuildError> {
        validation::build(toolkits, operations)
    }

    /// Returns toolkit navigation in stable, lexical navigation-ID order.
    pub fn toolkits(&self) -> impl ExactSizeIterator<Item = &'static ToolkitSummary> + '_ {
        self.toolkits.values().copied()
    }

    /// Returns operation descriptions in stable, lexical operation-ID order.
    pub fn operations(&self) -> impl ExactSizeIterator<Item = &'static OperationSummary> + '_ {
        self.operations.values().copied()
    }

    pub fn toolkit(&self, toolkit: ToolkitId) -> Option<&'static ToolkitSummary> {
        self.toolkits().find(|summary| summary.toolkit == toolkit)
    }

    pub fn operation(&self, id: &str) -> Option<&'static OperationSummary> {
        self.operations.get(id).copied()
    }
}

#[cfg(test)]
mod tests;
