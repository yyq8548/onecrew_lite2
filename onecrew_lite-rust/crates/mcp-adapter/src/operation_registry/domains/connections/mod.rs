mod check;
mod operator;
mod operator_lifecycle;
mod shared;
mod youtube;

pub(super) use check::CheckedToolkit;

use super::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    operator::register(builder, owners.clone())?;
    operator_lifecycle::register(builder, owners.clone())?;
    youtube::register(builder, owners.clone())?;
    check::register(builder, owners)?;
    Ok(())
}
