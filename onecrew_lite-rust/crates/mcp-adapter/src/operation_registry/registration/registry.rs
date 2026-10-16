//! Sealed entries and their search index provide surface-filtered queries.
use super::error::RegistryBuildError;
use crate::operation_registry::{
    descriptor::{EntryId, RegistryEntry, SurfaceId, SurfaceSet},
    search::SearchIndex,
    v2::{
        ToolGetFailure, ToolGetInput, ToolGetResult, ToolSearchInput, ToolSearchResult,
        project_tool_get, project_tool_search,
    },
};
use onecrew_local_runtime::CapabilityCatalog;
use onecrew_local_runtime::ToolkitId;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) struct OperationRegistry {
    catalog: Arc<CapabilityCatalog>,
    entries: BTreeMap<EntryId, RegistryEntry>,
    search_index: SearchIndex,
}

impl OperationRegistry {
    pub(super) fn from_entries(
        mut entries: BTreeMap<EntryId, RegistryEntry>,
        catalog: Arc<CapabilityCatalog>,
    ) -> Result<Self, RegistryBuildError> {
        entries.retain(|id, entry| {
            if onecrew_local_runtime::is_retired_operation(id.as_str()) {
                return false;
            }
            match entry {
                RegistryEntry::Playbook(playbook) => playbook.primary_operation_ids.iter()
                    .chain(&playbook.supporting_operation_ids)
                    .chain(playbook.recommended_steps.iter().flat_map(|step| &step.operation_ids))
                    .all(|id| !onecrew_local_runtime::is_retired_operation(id.as_str())),
                RegistryEntry::Operation(_) | RegistryEntry::ToolkitGuide(_) => true,
            }
        });
        let active_toolkits = entries.values().filter_map(|entry| match entry {
            RegistryEntry::Operation(operation) => Some(operation),
            _ => None,
        }).flat_map(|operation| operation.metadata.surfaces.iter()
            .map(|surface| (operation.metadata.toolkit, surface)))
            .collect::<BTreeSet<_>>();
        entries.retain(|_, entry| {
            let RegistryEntry::ToolkitGuide(guide) = entry else { return true; };
            let surfaces = guide.metadata.surfaces.iter()
                .filter(|surface| active_toolkits.contains(&(guide.metadata.toolkit, *surface)))
                .collect::<Vec<_>>();
            if surfaces.is_empty() {
                return false;
            }
            guide.metadata.surfaces = SurfaceSet::new(surfaces).expect("nonempty active toolkit surfaces");
            true
        });
        let search_index =
            SearchIndex::build(&entries).map_err(|_| RegistryBuildError::SearchIndexBuild)?;
        Ok(Self {
            catalog,
            entries,
            search_index,
        })
    }

    pub(crate) fn catalog(&self) -> &Arc<CapabilityCatalog> {
        &self.catalog
    }

    pub(crate) fn entry(&self, id: &EntryId, surface: SurfaceId) -> Option<&RegistryEntry> {
        self.entries
            .get(id)
            .filter(|entry| entry.metadata().surfaces.allows(surface))
    }

    pub(in crate::operation_registry) fn tool_search_v2(
        &self,
        surface: SurfaceId,
        input: &ToolSearchInput,
    ) -> ToolSearchResult {
        project_tool_search(&self.entries, &self.search_index, surface, input)
    }

    pub(in crate::operation_registry) fn tool_get_v2(
        &self,
        surface: SurfaceId,
        input: &ToolGetInput,
    ) -> Result<ToolGetResult, ToolGetFailure> {
        project_tool_get(&self.entries, surface, input)
    }

    /// Closed toolkit set reachable from executable operations on one fixed
    /// surface. The connection inventory uses this projection so a narrow
    /// endpoint never advertises credentials or accounts it cannot consume.
    pub(crate) fn operation_toolkits(&self, surface: SurfaceId) -> BTreeSet<ToolkitId> {
        self.entries
            .values()
            .filter_map(|entry| match entry {
                RegistryEntry::Operation(operation)
                    if operation.metadata.surfaces.allows(surface) =>
                {
                    Some(operation.metadata.toolkit)
                }
                _ => None,
            })
            .collect()
    }

    pub(crate) fn toolkit_directory(
        &self,
        surface: SurfaceId,
    ) -> Vec<crate::operation_registry::directory::ToolkitDirectory> {
        crate::operation_registry::directory::project_toolkit_directory(&self.entries, surface)
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn search_for_test(
        &self,
        surface: SurfaceId,
        query: &crate::operation_registry::search::SearchQuery,
    ) -> (
        Vec<crate::operation_registry::descriptor::CatalogEntrySummary>,
        Vec<crate::operation_registry::descriptor::PlaybookSummary>,
    ) {
        crate::operation_registry::search::search_v2(
            &self.entries,
            &self.search_index,
            surface,
            query,
            8,
            3,
        )
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn playbook_ids(&self) -> Vec<EntryId> {
        self.entries
            .iter()
            .filter(|(_, entry)| matches!(entry, RegistryEntry::Playbook(_)))
            .map(|(id, _)| id.clone())
            .collect()
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn toolkit_guide_ids(&self) -> Vec<EntryId> {
        self.entries
            .iter()
            .filter(|(_, entry)| matches!(entry, RegistryEntry::ToolkitGuide(_)))
            .map(|(id, _)| id.clone())
            .collect()
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn operation_ids_and_surfaces(
        &self,
    ) -> Vec<(EntryId, Vec<SurfaceId>)> {
        self.entries
            .iter()
            .filter_map(|(id, entry)| match entry {
                RegistryEntry::Operation(operation) => {
                    Some((id.clone(), operation.metadata.surfaces.iter().collect()))
                }
                RegistryEntry::Playbook(_) => None,
                RegistryEntry::ToolkitGuide(_) => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub(in crate::operation_registry) fn catalog_summaries(
        &self,
        surface: SurfaceId,
    ) -> Vec<crate::operation_registry::descriptor::CatalogEntrySummary> {
        self.entries
            .values()
            .filter(|entry| entry.metadata().surfaces.allows(surface))
            .filter_map(crate::operation_registry::search::summary)
            .collect()
    }
}
