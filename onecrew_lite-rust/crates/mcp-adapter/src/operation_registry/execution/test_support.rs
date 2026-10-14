use crate::operation_registry::{
    descriptor::{
        EntryId, OperationEntry, OperationRegistration, RegistryEntry, SurfaceId, SurfaceSet,
    },
    registration::OperationRegistry,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TestInput {
    pub(super) name: String,
}

#[derive(Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct TestOutput {
    pub(super) accepted: bool,
}

#[derive(Clone, Copy)]
pub(super) struct TestOwnerError;

pub(super) fn metadata(id: &str) -> OperationRegistration {
    OperationRegistration {
        id: crate::operation_registry::catalog_test_support::operation_id(id),
        domain_id: "test".to_owned(),
        aliases: Vec::new(),
        surfaces: SurfaceSet::new([SurfaceId::Full]).expect("surface set is non-empty"),
    }
}

pub(super) fn operation<'a>(registry: &'a OperationRegistry, id: &EntryId) -> &'a OperationEntry {
    match registry
        .entry(id, SurfaceId::Full)
        .expect("registered operation is visible")
    {
        RegistryEntry::Operation(operation) => operation,
        RegistryEntry::Playbook(_) | RegistryEntry::ToolkitGuide(_) => {
            panic!("test entry must be executable")
        }
    }
}
