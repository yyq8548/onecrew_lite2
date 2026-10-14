mod contract;
mod index;
mod rerank;
mod retrieval;

#[cfg(test)]
mod operation_eval_cases;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use crate::operation_registry::descriptor::{
    CatalogEntrySummary, EntryId, PlaybookSummary, RegistryEntry, SurfaceId,
};

pub(crate) use contract::SearchQuery;
pub(super) use contract::validate_query_text;
pub(crate) use index::SearchIndex;

pub(super) fn search_v2(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    index: &SearchIndex,
    surface: SurfaceId,
    query: &SearchQuery,
    operation_limit: usize,
    playbook_limit: usize,
) -> (Vec<CatalogEntrySummary>, Vec<PlaybookSummary>) {
    rerank::search_v2(
        entries,
        index,
        surface,
        query,
        operation_limit,
        playbook_limit,
    )
}

#[cfg(test)]
pub(super) use rerank::summary;
