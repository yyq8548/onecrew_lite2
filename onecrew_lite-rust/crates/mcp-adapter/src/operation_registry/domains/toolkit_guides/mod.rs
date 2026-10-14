//! MCP protocol guides project their neutral descriptions from the shared catalog.
use crate::operation_registry::{
    descriptor::{EntryId, EntryMetadata, SurfaceId, SurfaceSet, ToolkitGuideEntry},
    registration::{RegistryBuildError, RegistryBuilder},
};

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    let entries = builder.catalog().toolkits().map(entry).collect::<Vec<_>>();
    for guide in entries {
        builder.register_toolkit_guide(guide)?;
    }
    Ok(())
}

fn entry(definition: &onecrew_local_runtime::ToolkitSummary) -> ToolkitGuideEntry {
    ToolkitGuideEntry {
        metadata: EntryMetadata {
            id: EntryId::parse(definition.id).expect("static toolkit guide ID"),
            title: definition.title.to_owned(),
            summary: definition.summary.to_owned(),
            toolkit: definition.toolkit,
            domain_id: "toolkit-guide".to_owned(),
            aliases: Vec::new(),
            // The builder replaces this construction-only surface with the
            // union of the toolkit's registered operation surfaces.
            surfaces: SurfaceSet::new([SurfaceId::Full]).expect("construction surface"),
        },
        purpose: definition.purpose.to_owned(),
        connection_model: definition.connection_model.to_owned(),
        identifier_semantics: definition
            .identifier_semantics
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        capability_categories: definition
            .capability_categories
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        gotchas: definition
            .gotchas
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    }
}
