use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use super::{CapabilityCatalog, OperationSummary, ToolkitSummary};

/// A static declaration failure. It never contains provider or session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogBuildError {
    InvalidId,
    DuplicateToolkitId,
    DuplicateToolkitIdentity,
    DuplicateOperationId,
    DanglingToolkit,
    InvalidStaticDescription,
    EmptySurfaces,
    DuplicateSurface,
}

impl fmt::Display for CatalogBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidId => "invalid static catalog ID",
            Self::DuplicateToolkitId => "duplicate toolkit navigation ID",
            Self::DuplicateToolkitIdentity => "duplicate toolkit identity",
            Self::DuplicateOperationId => "duplicate operation ID",
            Self::DanglingToolkit => "operation references an undeclared toolkit",
            Self::InvalidStaticDescription => "invalid static catalog description",
            Self::EmptySurfaces => "operation declares no supported surface",
            Self::DuplicateSurface => "operation declares a surface more than once",
        })
    }
}

impl std::error::Error for CatalogBuildError {}

fn validate_id(value: &str) -> Result<(), CatalogBuildError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return Err(CatalogBuildError::InvalidId);
    }
    Ok(())
}

fn validate_description(value: &str) -> Result<(), CatalogBuildError> {
    if value.trim().is_empty() {
        return Err(CatalogBuildError::InvalidStaticDescription);
    }
    Ok(())
}

pub(super) fn build(
    toolkits: &'static [ToolkitSummary],
    operations: &'static [OperationSummary],
) -> Result<CapabilityCatalog, CatalogBuildError> {
    let mut toolkit_index = BTreeMap::new();
    let mut toolkit_identities = BTreeSet::new();
    for toolkit in toolkits {
        validate_id(toolkit.id)?;
        for description in [
            toolkit.title,
            toolkit.summary,
            toolkit.purpose,
            toolkit.connection_model,
        ] {
            validate_description(description)?;
        }
        for description in toolkit
            .identifier_semantics
            .iter()
            .chain(toolkit.capability_categories)
            .chain(toolkit.gotchas)
        {
            validate_description(description)?;
        }
        if toolkit_index.insert(toolkit.id, toolkit).is_some() {
            return Err(CatalogBuildError::DuplicateToolkitId);
        }
        if !toolkit_identities.insert(toolkit.toolkit) {
            return Err(CatalogBuildError::DuplicateToolkitIdentity);
        }
    }
    let mut operation_index = BTreeMap::new();
    for operation in operations {
        validate_id(operation.id.as_str())?;
        validate_description(operation.title)?;
        validate_description(operation.summary)?;
        if !toolkit_identities.contains(&operation.toolkit_id) {
            return Err(CatalogBuildError::DanglingToolkit);
        }
        if operation.surfaces.is_empty() {
            return Err(CatalogBuildError::EmptySurfaces);
        }
        if operation
            .surfaces
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != operation.surfaces.len()
        {
            return Err(CatalogBuildError::DuplicateSurface);
        }
        if operation_index
            .insert(operation.id.as_str(), operation)
            .is_some()
        {
            return Err(CatalogBuildError::DuplicateOperationId);
        }
    }
    Ok(CapabilityCatalog {
        toolkits: toolkit_index,
        operations: operation_index,
    })
}
