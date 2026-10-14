use crate::operation_registry::descriptor::{
    EntryId, EntryMetadata, PlaybookDifficulty, PlaybookEntry, PlaybookStep, SurfaceId, SurfaceSet,
};
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};
use onecrew_local_runtime::ToolkitId;

#[expect(
    clippy::too_many_arguments,
    reason = "playbook recipes keep each bounded registry descriptor field explicit"
)]
pub(super) fn recipe(
    builder: &mut RegistryBuilder,
    id: &str,
    title: &str,
    toolkits: &[ToolkitId],
    primary: &[&str],
    supporting: &[&str],
    steps: &[PlaybookStep],
    use_case: &str,
) -> Result<(), RegistryBuildError> {
    add(
        builder,
        id,
        title,
        use_case,
        use_case,
        toolkits,
        primary,
        supporting,
        steps,
        &[],
        PlaybookDifficulty::Advanced,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "playbook registration keeps each bounded registry descriptor field explicit"
)]
pub(super) fn add(
    builder: &mut RegistryBuilder,
    id: &str,
    title: &str,
    summary: &str,
    use_case: &str,
    toolkits: &[ToolkitId],
    primary: &[&str],
    supporting: &[&str],
    steps: &[PlaybookStep],
    pitfalls: &[&str],
    difficulty: PlaybookDifficulty,
) -> Result<(), RegistryBuildError> {
    builder.register_playbook(PlaybookEntry {
        metadata: EntryMetadata {
            id: eid(id),
            title: title.into(),
            summary: summary.into(),
            toolkit: toolkits[0],
            domain_id: "playbooks".into(),
            aliases: Vec::new(),
            surfaces: SurfaceSet::new([SurfaceId::Full]).expect("playbook surface"),
        },
        use_case: use_case.into(),
        toolkit_ids: toolkits.to_vec(),
        primary_operation_ids: ids(primary),
        supporting_operation_ids: ids(supporting),
        recommended_steps: steps.to_vec(),
        known_pitfalls: pitfalls.iter().map(|v| (*v).into()).collect(),
        difficulty,
    })
}

pub(super) fn step(instruction: &str, operations: &[&str]) -> PlaybookStep {
    PlaybookStep {
        instruction: instruction.into(),
        operation_ids: ids(operations),
    }
}
fn ids(values: &[&str]) -> Vec<EntryId> {
    values.iter().map(|value| eid(value)).collect()
}
fn eid(value: &str) -> EntryId {
    EntryId::parse(value).expect("static registry ID")
}
