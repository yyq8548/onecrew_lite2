//! Cross-entry references and safety pointers are checked before registry sealing.
use super::{
    error::{ParameterPointerRole, RegistryBuildError},
    policy::OperationPolicy,
};
use crate::operation_registry::{
    descriptor::{
        ApprovalClass, ConfirmationMode, EntryId, IdempotencyMode, OperationEntry, PlaybookEntry,
        RegistryEntry, ToolkitGuideEntry,
    },
    schema::CompiledSchema,
};
use std::collections::{BTreeMap, BTreeSet};
const MAX_SAFETY_POINTERS: usize = 12;
const MAX_READBACK_OPERATIONS: usize = 12;

pub(super) fn validate_readback_references(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    operation: &OperationEntry,
) -> Result<(), RegistryBuildError> {
    for referenced_id in &operation.safety.readback.operation_ids {
        let referenced = match entries.get(referenced_id) {
            Some(RegistryEntry::Operation(referenced)) => referenced,
            Some(RegistryEntry::Playbook(_)) => {
                return Err(RegistryBuildError::ReadbackReferenceKindMismatch {
                    operation_id: operation.metadata.id.clone(),
                    referenced_id: referenced_id.clone(),
                });
            }
            Some(RegistryEntry::ToolkitGuide(_)) => {
                return Err(RegistryBuildError::ReadbackReferenceKindMismatch {
                    operation_id: operation.metadata.id.clone(),
                    referenced_id: referenced_id.clone(),
                });
            }
            None => {
                return Err(RegistryBuildError::UnknownReadbackOperation {
                    operation_id: operation.metadata.id.clone(),
                    referenced_id: referenced_id.clone(),
                });
            }
        };
        if operation
            .metadata
            .surfaces
            .iter()
            .any(|surface| !referenced.metadata.surfaces.allows(surface))
        {
            return Err(RegistryBuildError::ReadbackSurfaceMismatch {
                operation_id: operation.metadata.id.clone(),
                referenced_id: referenced_id.clone(),
            });
        }
    }
    Ok(())
}

pub(super) fn validate_playbook(
    entries: &BTreeMap<EntryId, RegistryEntry>,
    playbook: &PlaybookEntry,
) -> Result<(), RegistryBuildError> {
    if !playbook.metadata.id.as_str().starts_with("playbook.")
        || !(1..=3).contains(&playbook.toolkit_ids.len())
        || !(1..=4).contains(&playbook.primary_operation_ids.len())
        || playbook.supporting_operation_ids.len() > 8
        || playbook.primary_operation_ids.len() + playbook.supporting_operation_ids.len() > 12
        || playbook.recommended_steps.len() > 6
        || playbook.known_pitfalls.len() > 5
        || playbook.use_case.is_empty()
        || playbook.use_case.len() > 240
        || playbook.metadata.title.is_empty()
        || playbook.metadata.title.len() > 120
        || playbook.metadata.summary.is_empty()
        || playbook.metadata.summary.len() > 240
        || playbook.metadata.aliases.len() > 8
        || playbook
            .metadata
            .aliases
            .iter()
            .any(|alias| alias.is_empty() || alias.len() > 80)
        || playbook
            .known_pitfalls
            .iter()
            .any(|pitfall| pitfall.is_empty() || pitfall.len() > 200)
        || has_duplicate(&playbook.toolkit_ids)
        || !playbook.toolkit_ids.contains(&playbook.metadata.toolkit)
        || has_duplicate(&playbook.primary_operation_ids)
        || has_duplicate(&playbook.supporting_operation_ids)
        || playbook
            .primary_operation_ids
            .iter()
            .any(|id| playbook.supporting_operation_ids.contains(id))
    {
        return Err(RegistryBuildError::InvalidPlaybook(
            playbook.metadata.id.clone(),
        ));
    }
    let refs = playbook
        .primary_operation_ids
        .iter()
        .chain(&playbook.supporting_operation_ids)
        .collect::<BTreeSet<_>>();
    for id in &refs {
        let Some(RegistryEntry::Operation(operation)) = entries.get(*id) else {
            return Err(RegistryBuildError::UnknownPlaybookOperation((*id).clone()));
        };
        if !playbook.toolkit_ids.contains(&operation.metadata.toolkit)
            || playbook
                .metadata
                .surfaces
                .iter()
                .any(|surface| !operation.metadata.surfaces.allows(surface))
        {
            return Err(RegistryBuildError::PlaybookOperationMismatch {
                playbook_id: playbook.metadata.id.clone(),
                operation_id: (*id).clone(),
            });
        }
    }
    for step in &playbook.recommended_steps {
        if step.instruction.is_empty()
            || step.instruction.len() > 240
            || step.operation_ids.len() > 3
            || has_duplicate(&step.operation_ids)
            || step.operation_ids.iter().any(|id| !refs.contains(id))
        {
            return Err(RegistryBuildError::InvalidPlaybookStep(
                playbook.metadata.id.clone(),
            ));
        }
    }
    Ok(())
}
pub(super) fn validate_toolkit_guide(guide: &ToolkitGuideEntry) -> Result<(), RegistryBuildError> {
    let valid = guide.metadata.id.as_str()
        == format!("toolkit.{}", guide.metadata.toolkit.as_str())
        && guide.metadata.aliases.is_empty()
        && guide.metadata.domain_id == "toolkit-guide"
        && (1..=80).contains(&guide.metadata.title.len())
        && (1..=240).contains(&guide.metadata.summary.len())
        && (1..=480).contains(&guide.purpose.len())
        && (1..=240).contains(&guide.connection_model.len())
        && (1..=8).contains(&guide.capability_categories.len())
        && guide
            .capability_categories
            .iter()
            .all(|value| (1..=120).contains(&value.len()))
        && guide.identifier_semantics.len() <= 6
        && guide
            .identifier_semantics
            .iter()
            .all(|value| (1..=160).contains(&value.len()))
        && (1..=6).contains(&guide.gotchas.len())
        && guide
            .gotchas
            .iter()
            .all(|value| (1..=180).contains(&value.len()))
        && !has_duplicate(&guide.capability_categories)
        && !has_duplicate(&guide.identifier_semantics)
        && !has_duplicate(&guide.gotchas);
    if valid {
        Ok(())
    } else {
        Err(RegistryBuildError::InvalidToolkitGuide(
            guide.metadata.id.clone(),
        ))
    }
}

pub(super) fn validate_operation_safety(
    policy: &OperationPolicy,
    input: &CompiledSchema,
    output: &CompiledSchema,
) -> Result<(), RegistryBuildError> {
    let confirmation_parameters = &policy.safety.confirmation.parameter_pointers;
    if policy.safety.confirmation.mode.has_attended_prompt()
        != matches!(policy.approval, ApprovalClass::AttendedPrompt)
        || policy.safety.confirmation.mode.has_owner_parameters()
            != !confirmation_parameters.is_empty()
        || (matches!(policy.approval, ApprovalClass::None)
            && !matches!(policy.safety.confirmation.mode, ConfirmationMode::None))
    {
        return Err(RegistryBuildError::InvalidSafetyPolicy);
    }
    validate_safety_pointers(
        confirmation_parameters,
        "confirmation.parameter_pointers",
        input,
        ParameterPointerRole::Input,
    )?;

    let identity_pointers = &policy.safety.idempotency.identity_pointers;
    if (!matches!(policy.safety.idempotency.mode, IdempotencyMode::None))
        != !identity_pointers.is_empty()
    {
        return Err(RegistryBuildError::InvalidSafetyPolicy);
    }
    validate_safety_pointers(
        identity_pointers,
        "idempotency.identity_pointers",
        input,
        ParameterPointerRole::Input,
    )?;

    let readback_operations = &policy.safety.readback.operation_ids;
    if policy.safety.readback.mode.has_follow_up_operations() != !readback_operations.is_empty()
        || readback_operations.len() > MAX_READBACK_OPERATIONS
        || has_duplicate(readback_operations)
    {
        return Err(RegistryBuildError::InvalidSafetyPolicy);
    }
    if policy.safety.warnings_present != !policy.safety.warning_output_pointers.is_empty()
        || policy.safety.recovery_present != !policy.safety.recovery_output_pointers.is_empty()
    {
        return Err(RegistryBuildError::InvalidSafetyPolicy);
    }

    validate_safety_pointers(
        &policy.safety.warning_output_pointers,
        "warning_output_pointers",
        output,
        ParameterPointerRole::Output,
    )?;
    validate_safety_pointers(
        &policy.safety.recovery_output_pointers,
        "recovery_output_pointers",
        output,
        ParameterPointerRole::Output,
    )?;
    Ok(())
}

fn validate_safety_pointers(
    pointers: &[crate::operation_registry::descriptor::InstancePointer],
    field: &'static str,
    schema: &CompiledSchema,
    role: ParameterPointerRole,
) -> Result<(), RegistryBuildError> {
    if pointers.len() > MAX_SAFETY_POINTERS {
        return Err(RegistryBuildError::SafetyPointerOutOfBounds { field });
    }
    if has_duplicate(pointers) {
        return Err(RegistryBuildError::DuplicateSafetyPointer { field });
    }
    if let Some(pointer) = pointers
        .iter()
        .find(|pointer| !schema.validates_instance_pointer(pointer.as_str()))
    {
        return Err(RegistryBuildError::SafetyPointerMismatch {
            pointer: pointer.as_str().to_owned(),
            role,
        });
    }
    Ok(())
}

fn has_duplicate<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}
