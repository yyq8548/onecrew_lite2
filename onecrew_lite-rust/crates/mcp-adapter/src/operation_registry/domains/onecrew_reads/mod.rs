mod creators;
mod discovery;
mod link_hub;
mod mail;
mod outreach;
mod shared;

use std::sync::Arc;

use super::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    creators::register(builder, owners.clone())?;
    discovery::register(builder, owners.clone())?;
    link_hub::register(builder, Arc::clone(&owners.platform))?;
    outreach::register(builder, owners.clone())?;
    mail::register(builder, owners)?;
    Ok(())
}
