//! Secret-free toolkit navigation derived from the compiled registry.

use onecrew_local_runtime::ToolkitId;
use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::descriptor::{EntryId, RegistryEntry, SurfaceId, ToolkitGuideEntry};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolkitOperationSummary {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolkitDirectory {
    pub id: String,
    pub toolkit: ToolkitId,
    pub title: String,
    pub summary: String,
    pub purpose: String,
    pub connection_model: String,
    pub identifier_semantics: Vec<String>,
    pub capability_categories: Vec<String>,
    pub gotchas: Vec<String>,
    pub operations: Vec<ToolkitOperationSummary>,
}

pub(super) fn project_toolkit_directory(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    surface: SurfaceId,
) -> Vec<ToolkitDirectory> {
    entries
        .values()
        .filter_map(|entry| match entry {
            RegistryEntry::ToolkitGuide(guide) if guide.metadata.surfaces.allows(surface) => {
                Some(project_guide(entries, guide, surface))
            }
            _ => None,
        })
        .collect()
}

pub(super) fn project_toolkit_operations(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    toolkit: ToolkitId,
    surface: SurfaceId,
) -> Vec<ToolkitOperationSummary> {
    entries
        .values()
        .filter_map(|entry| match entry {
            RegistryEntry::Operation(operation)
                if operation.metadata.toolkit == toolkit
                    && operation.metadata.surfaces.allows(surface) =>
            {
                Some(ToolkitOperationSummary {
                    id: operation.metadata.id.as_str().to_owned(),
                    title: operation.metadata.title.clone(),
                    summary: operation.metadata.summary.clone(),
                })
            }
            _ => None,
        })
        .collect()
}

fn project_guide(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    guide: &ToolkitGuideEntry,
    surface: SurfaceId,
) -> ToolkitDirectory {
    ToolkitDirectory {
        id: guide.metadata.id.as_str().to_owned(),
        toolkit: guide.metadata.toolkit,
        title: guide.metadata.title.clone(),
        summary: guide.metadata.summary.clone(),
        purpose: guide.purpose.clone(),
        connection_model: guide.connection_model.clone(),
        identifier_semantics: guide.identifier_semantics.clone(),
        capability_categories: guide.capability_categories.clone(),
        gotchas: guide.gotchas.clone(),
        operations: project_toolkit_operations(entries, guide.metadata.toolkit, surface),
    }
}
