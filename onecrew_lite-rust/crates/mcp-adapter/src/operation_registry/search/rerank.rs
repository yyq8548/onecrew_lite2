use std::collections::{BTreeMap, BTreeSet};

use super::{contract::SearchQuery, index::SearchIndex, retrieval::LexicalHit};
use crate::operation_registry::descriptor::{
    CatalogEntrySummary, CatalogMatchRole, ConnectionRequirement, EntryId, EntryKind,
    EntryMetadata, OperationEntry, PlaybookEntry, PlaybookSummary, RegistryEntry, SurfaceId,
};

const PRIMARY_PLAYBOOK_BOOST: f32 = 8.0;
const SUPPORTING_PLAYBOOK_BOOST: f32 = 4.0;

struct RankedOperation {
    id: EntryId,
    score: f32,
}

struct RankedPlaybook {
    summary: PlaybookSummary,
    score: f32,
}

pub(super) fn search_v2(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    index: &SearchIndex,
    surface: SurfaceId,
    query: &SearchQuery,
    operation_limit: usize,
    playbook_limit: usize,
) -> (Vec<CatalogEntrySummary>, Vec<PlaybookSummary>) {
    search_with_bounds(
        entries,
        index,
        surface,
        query,
        operation_limit,
        playbook_limit,
    )
}

fn search_with_bounds(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    index: &SearchIndex,
    surface: SurfaceId,
    query: &SearchQuery,
    operation_limit: usize,
    playbook_limit: usize,
) -> (Vec<CatalogEntrySummary>, Vec<PlaybookSummary>) {
    let query_text = query.text.as_str().trim();
    if looks_like_legacy_slug(query_text) {
        return (Vec::new(), Vec::new());
    }
    if looks_like_dotted_id(query_text) {
        return EntryId::parse(query_text.to_owned())
            .ok()
            .and_then(|exact_id| entries.get(&exact_id))
            .map_or_else(
                || (Vec::new(), Vec::new()),
                |entry| exact_result(entries, entry, surface, query, operation_limit),
            );
    }

    let allowed_operation_ids = entries
        .iter()
        .filter_map(|(id, entry)| match entry {
            RegistryEntry::Operation(operation) if operation_allowed(operation, surface, query) => {
                Some(id.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let allowed_playbook_ids = entries
        .iter()
        .filter_map(|(id, entry)| match entry {
            RegistryEntry::Playbook(playbook)
                if playbook_search_allowed(entries, playbook, surface, query) =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let selected_playbooks = ranked_playbooks(
        entries,
        index.playbook_recall(query.text.as_str(), &allowed_playbook_ids),
        surface,
        query,
    );
    let selected_playbook = selected_playbooks.first();
    let primary_ids = selected_playbook
        .as_ref()
        .map(|hit| {
            hit.summary
                .primary_operation_ids
                .iter()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let supporting_ids = selected_playbook
        .as_ref()
        .map(|hit| {
            hit.summary
                .supporting_operation_ids
                .iter()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    let mut operation_scores = BTreeMap::<EntryId, f32>::new();
    for hit in index.operation_recall(query.text.as_str(), &allowed_operation_ids) {
        let Some(RegistryEntry::Operation(operation)) = entries.get(&hit.id) else {
            continue;
        };
        if operation_allowed(operation, surface, query) {
            operation_scores.insert(hit.id, hit.score);
        }
    }
    for id in &primary_ids {
        if let Some(RegistryEntry::Operation(operation)) = entries.get(*id)
            && operation_allowed(operation, surface, query)
        {
            *operation_scores.entry((*id).clone()).or_default() += PRIMARY_PLAYBOOK_BOOST;
        }
    }
    for id in &supporting_ids {
        if let Some(RegistryEntry::Operation(operation)) = entries.get(*id)
            && operation_allowed(operation, surface, query)
        {
            *operation_scores.entry((*id).clone()).or_default() += SUPPORTING_PLAYBOOK_BOOST;
        }
    }

    let mut operations = operation_scores
        .into_iter()
        .map(|(id, score)| RankedOperation { id, score })
        .collect::<Vec<_>>();
    operations.sort_by(|left, right| {
        let left_graph_rank = if selected_playbook.is_none() || primary_ids.contains(&left.id) {
            0
        } else if supporting_ids.contains(&left.id) {
            1
        } else {
            2
        };
        let right_graph_rank = if selected_playbook.is_none() || primary_ids.contains(&right.id) {
            0
        } else if supporting_ids.contains(&right.id) {
            1
        } else {
            2
        };
        left_graph_rank.cmp(&right_graph_rank).then_with(|| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        })
    });
    operations.truncate(operation_limit);
    let has_playbook = selected_playbook.is_some();
    let operations = operations
        .into_iter()
        .enumerate()
        .filter_map(|(rank, hit)| match entries.get(&hit.id) {
            Some(RegistryEntry::Operation(operation)) => Some(operation_summary(
                operation,
                if (has_playbook && primary_ids.contains(&hit.id)) || (!has_playbook && rank == 0) {
                    CatalogMatchRole::Primary
                } else {
                    CatalogMatchRole::Related
                },
            )),
            _ => None,
        })
        .collect();

    (
        operations,
        selected_playbooks
            .into_iter()
            .take(playbook_limit)
            .map(|hit| hit.summary)
            .collect(),
    )
}

fn looks_like_legacy_slug(value: &str) -> bool {
    value.contains('_')
        && !value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn looks_like_dotted_id(value: &str) -> bool {
    value.contains('.')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn exact_result(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    entry: &RegistryEntry,
    surface: SurfaceId,
    query: &SearchQuery,
    operation_limit: usize,
) -> (Vec<CatalogEntrySummary>, Vec<PlaybookSummary>) {
    match entry {
        RegistryEntry::Operation(operation) if operation_allowed(operation, surface, query) => (
            vec![operation_summary(operation, CatalogMatchRole::Primary)],
            Vec::new(),
        ),
        RegistryEntry::Playbook(playbook)
            if playbook_hard_allowed(entries, playbook, surface, query) =>
        {
            let primary_ids = playbook
                .primary_operation_ids
                .iter()
                .collect::<BTreeSet<_>>();
            let operations = playbook
                .primary_operation_ids
                .iter()
                .chain(&playbook.supporting_operation_ids)
                .filter_map(|id| match entries.get(id) {
                    Some(RegistryEntry::Operation(operation))
                        if operation_allowed(operation, surface, query) =>
                    {
                        Some(operation_summary(
                            operation,
                            if primary_ids.contains(id) {
                                CatalogMatchRole::Primary
                            } else {
                                CatalogMatchRole::Related
                            },
                        ))
                    }
                    _ => None,
                })
                .take(operation_limit)
                .collect();
            (operations, vec![playbook_summary(playbook)])
        }
        _ => (Vec::new(), Vec::new()),
    }
}

fn ranked_playbooks(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    recall: Vec<LexicalHit>,
    surface: SurfaceId,
    query: &SearchQuery,
) -> Vec<RankedPlaybook> {
    let mut playbooks = recall
        .into_iter()
        .filter_map(|hit| match entries.get(&hit.id) {
            Some(RegistryEntry::Playbook(playbook))
                if playbook_search_allowed(entries, playbook, surface, query) =>
            {
                Some(RankedPlaybook {
                    summary: playbook_summary(playbook),
                    score: hit.score,
                })
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    playbooks.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.summary.id.cmp(&right.summary.id))
    });
    playbooks
}

fn metadata_allowed(metadata: &EntryMetadata, surface: SurfaceId, query: &SearchQuery) -> bool {
    metadata.surfaces.allows(surface)
        && query
            .domain
            .as_ref()
            .is_none_or(|domain| metadata.domain_id == domain.as_str())
}

fn operation_allowed(operation: &OperationEntry, surface: SurfaceId, query: &SearchQuery) -> bool {
    metadata_allowed(&operation.metadata, surface, query)
        && (query.toolkits.is_empty() || query.toolkits.contains(&operation.metadata.toolkit))
        && (query.effects.is_empty() || query.effects.contains(&operation.effect))
}

fn playbook_search_allowed(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    playbook: &PlaybookEntry,
    surface: SurfaceId,
    query: &SearchQuery,
) -> bool {
    playbook_hard_allowed(entries, playbook, surface, query)
}

fn playbook_hard_allowed(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    playbook: &PlaybookEntry,
    surface: SurfaceId,
    query: &SearchQuery,
) -> bool {
    if !metadata_allowed(&playbook.metadata, surface, query)
        || (!query.toolkits.is_empty()
            && !playbook
                .toolkit_ids
                .iter()
                .all(|toolkit| query.toolkits.contains(toolkit)))
    {
        return false;
    }
    query.effects.is_empty()
        || playbook
            .primary_operation_ids
            .iter()
            .chain(&playbook.supporting_operation_ids)
            .all(|id| match entries.get(id) {
                Some(RegistryEntry::Operation(operation)) => {
                    query.effects.contains(&operation.effect)
                }
                _ => false,
            })
}

fn operation_summary(
    operation: &OperationEntry,
    match_role: CatalogMatchRole,
) -> CatalogEntrySummary {
    CatalogEntrySummary {
        id: operation.metadata.id.clone(),
        kind: EntryKind::Operation,
        title: operation.metadata.title.clone(),
        summary: operation.metadata.summary.clone(),
        toolkit: operation.metadata.toolkit,
        match_role,
        effects: vec![operation.effect],
        connection_requirements: match &operation.connection {
            ConnectionRequirement::None => Vec::new(),
            requirement => vec![requirement.clone()],
        },
        safety: Some(operation.safety.summary()),
    }
}

fn playbook_summary(playbook: &PlaybookEntry) -> PlaybookSummary {
    PlaybookSummary {
        id: playbook.metadata.id.clone(),
        title: playbook.metadata.title.clone(),
        summary: playbook.metadata.summary.clone(),
        use_case: playbook.use_case.clone(),
        toolkit_ids: playbook.toolkit_ids.clone(),
        primary_operation_ids: playbook.primary_operation_ids.clone(),
        supporting_operation_ids: playbook.supporting_operation_ids.clone(),
        recommended_steps: playbook.recommended_steps.clone(),
        known_pitfalls: playbook.known_pitfalls.clone(),
        difficulty: playbook.difficulty,
    }
}

#[cfg(test)]
pub(crate) fn summary(entry: &RegistryEntry) -> Option<CatalogEntrySummary> {
    match entry {
        RegistryEntry::Operation(operation) => {
            Some(operation_summary(operation, CatalogMatchRole::Primary))
        }
        RegistryEntry::Playbook(_) => None,
        RegistryEntry::ToolkitGuide(_) => None,
    }
}
