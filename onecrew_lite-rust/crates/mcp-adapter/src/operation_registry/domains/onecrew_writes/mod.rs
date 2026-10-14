mod creators;
mod outreach_mail;

use super::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    creators::register(builder, owners.clone())?;
    outreach_mail::register(builder, owners)?;
    Ok(())
}
