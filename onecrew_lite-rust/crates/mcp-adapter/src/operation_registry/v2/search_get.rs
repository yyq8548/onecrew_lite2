use onecrew_local_runtime::ToolkitId;
use std::collections::{BTreeMap, BTreeSet};

use crate::operation_registry::{
    descriptor::{
        ApprovalClass, ConfirmationMode, ConnectionRequirement, EntryId, OperationSafety,
        RegistryEntry, SecretClass, SurfaceId, schema_json,
    },
    directory::{ToolkitDirectory, project_toolkit_directory},
    search::{SearchIndex, SearchQuery, search_v2},
};

use super::contract::{
    MAX_BATCH_OPERATIONS, MAX_SEARCH_GUIDES, MAX_SEARCH_OPERATIONS, MAX_WORKFLOW_HINTS,
    OperationExecutionContract, ToolGetFailure, ToolGetInput, ToolGetResult, ToolSearchInput,
    ToolSearchLimit, ToolSearchOperation, ToolSearchResult, ToolkitSearchGuide, WorkflowHint,
};

pub(in crate::operation_registry) fn project_tool_search(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    index: &SearchIndex,
    surface: SurfaceId,
    input: &ToolSearchInput,
) -> ToolSearchResult {
    let query = SearchQuery {
        text: input.query.clone(),
        toolkits: input.toolkits.clone(),
        domain: None,
        effects: input.effects.clone(),
        limit: None,
    };
    let (operations, playbooks) = search_v2(
        entries,
        index,
        surface,
        &query,
        input
            .limit
            .map_or(MAX_SEARCH_OPERATIONS, ToolSearchLimit::get),
        MAX_WORKFLOW_HINTS,
    );

    let operations = operations
        .into_iter()
        .map(|operation| ToolSearchOperation {
            id: operation.id,
            toolkit: operation.toolkit,
            title: operation.title,
            summary: operation.summary,
            effects: operation.effects,
            connection: operation
                .connection_requirements
                .into_iter()
                .next()
                .unwrap_or(ConnectionRequirement::None),
        })
        .collect::<Vec<_>>();

    let mut guide_toolkits = input.toolkits.iter().copied().collect::<BTreeSet<_>>();
    guide_toolkits.extend(infer_toolkits(input.query.as_str()));
    guide_toolkits.extend(operations.iter().map(|operation| operation.toolkit));
    for playbook in &playbooks {
        guide_toolkits.extend(playbook.toolkit_ids.iter().copied());
    }

    let toolkit_guides = project_toolkit_directory(entries, surface)
        .into_iter()
        .filter(|guide| guide_toolkits.contains(&guide.toolkit))
        .map(project_search_guide)
        .take(MAX_SEARCH_GUIDES)
        .collect();
    let workflow_hints = playbooks
        .into_iter()
        .filter_map(|playbook| {
            let mut seen = BTreeSet::new();
            let operation_ids = playbook
                .primary_operation_ids
                .iter()
                .chain(&playbook.supporting_operation_ids)
                .filter(|id| seen.insert((*id).clone()))
                .filter(|id| {
                    matches!(
                        entries.get(*id),
                        Some(RegistryEntry::Operation(operation))
                            if operation.metadata.surfaces.allows(surface)
                    )
                })
                .take(MAX_BATCH_OPERATIONS)
                .cloned()
                .collect::<Vec<_>>();
            let steps = playbook
                .recommended_steps
                .into_iter()
                .map(|step| step.instruction)
                .take(MAX_BATCH_OPERATIONS)
                .collect::<Vec<_>>();
            (!operation_ids.is_empty() && !steps.is_empty()).then_some(WorkflowHint {
                id: playbook.id,
                title: playbook.title,
                summary: playbook.summary,
                operation_ids,
                steps,
                pitfalls: playbook
                    .known_pitfalls
                    .into_iter()
                    .take(MAX_BATCH_OPERATIONS)
                    .collect(),
            })
        })
        .take(MAX_WORKFLOW_HINTS)
        .collect();

    ToolSearchResult {
        operations,
        toolkit_guides,
        workflow_hints,
    }
}

pub(in crate::operation_registry) fn project_tool_get(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    surface: SurfaceId,
    input: &ToolGetInput,
) -> Result<ToolGetResult, ToolGetFailure> {
    if !input.valid() {
        return Err(ToolGetFailure::InvalidBatch);
    }
    let operations = input
        .operation_ids
        .iter()
        .map(|id| match entries.get(id) {
            Some(RegistryEntry::Operation(operation))
                if operation.metadata.surfaces.allows(surface) =>
            {
                Ok(OperationExecutionContract {
                    id: operation.metadata.id.clone(),
                    toolkit: operation.metadata.toolkit,
                    title: operation.metadata.title.clone(),
                    summary: operation.metadata.summary.clone(),
                    schema_revision: operation.input.revision().to_owned(),
                    effect: operation.effect,
                    secret: (operation.secret != SecretClass::None).then_some(operation.secret),
                    approval: operation.approval,
                    connection: operation.connection.clone(),
                    input_schema: schema_json(&operation.input),
                    output_schema: input
                        .include_output_schema
                        .then(|| schema_json(&operation.output)),
                    safety: (operation.safety != implied_safety(operation.approval))
                        .then(|| operation.safety.clone()),
                })
            }
            Some(
                RegistryEntry::Operation(_)
                | RegistryEntry::Playbook(_)
                | RegistryEntry::ToolkitGuide(_),
            )
            | None => Err(ToolGetFailure::OperationUnavailable),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ToolGetResult { operations })
}

/// The safety an approval class implies: read-only, plus the attended prompt
/// that registry validation requires exactly for attended_prompt approval.
fn implied_safety(approval: ApprovalClass) -> OperationSafety {
    let mut safety = OperationSafety::read_only();
    if approval == ApprovalClass::AttendedPrompt {
        safety.confirmation.mode = ConfirmationMode::AttendedPrompt;
    }
    safety
}

fn infer_toolkits(query: &str) -> BTreeSet<ToolkitId> {
    let terms = query
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<BTreeSet<_>>();
    [
        (ToolkitId::X, &["x", "twitter"][..]),
        (ToolkitId::Instagram, &["instagram"][..]),
        (ToolkitId::Telegram, &["telegram"][..]),
        (ToolkitId::Youtube, &["youtube"][..]),
        (ToolkitId::OneCrew, &["onecrew"][..]),
        (ToolkitId::Email, &["email", "mail", "resend"][..]),
    ]
    .into_iter()
    .filter_map(|(toolkit, aliases)| {
        aliases
            .iter()
            .any(|alias| terms.contains(*alias))
            .then_some(toolkit)
    })
    .collect()
}

fn project_search_guide(guide: ToolkitDirectory) -> ToolkitSearchGuide {
    ToolkitSearchGuide {
        toolkit: guide.toolkit,
        title: guide.title,
        summary: guide.summary,
        purpose: guide.purpose,
        connection_model: guide.connection_model,
        identifier_semantics: guide.identifier_semantics,
        capability_categories: guide.capability_categories,
        gotchas: guide.gotchas,
        operations: guide.operations,
    }
}
