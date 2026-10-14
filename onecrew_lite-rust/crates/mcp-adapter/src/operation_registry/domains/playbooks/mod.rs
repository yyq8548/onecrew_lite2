//! Bounded registry-native playbooks returned by catalog search. Exact schemas,
//! execution policy, and typed bindings remain on their registered operations.

mod connections;
mod descriptor;
mod discovery;
mod email;
mod onecrew;
mod recipes;

use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(builder: &mut RegistryBuilder) -> Result<(), RegistryBuildError> {
    discovery::register(builder)?;
    onecrew::register_creator_workflows(builder)?;
    email::register(builder)?;
    onecrew::register_direct_message(builder)?;
    recipes::register(builder)?;
    connections::register(builder)?;
    Ok(())
}
