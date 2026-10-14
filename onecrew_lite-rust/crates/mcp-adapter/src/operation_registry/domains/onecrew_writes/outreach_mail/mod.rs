mod direct_messages;
mod mail_delivery;
mod mail_state;
mod outreach;
mod owner_review;
mod shared;
mod triage;

pub(super) use owner_review::classify_prepare_failure as classify_outreach_prepare_failure;

use super::super::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    outreach::register(builder, owners.clone())?;
    direct_messages::register(builder, owners.clone())?;
    triage::register(builder, owners.clone())?;
    mail_delivery::register(builder, owners.clone())?;
    mail_state::register(builder, owners)?;
    Ok(())
}

#[cfg(test)]
mod tests;
