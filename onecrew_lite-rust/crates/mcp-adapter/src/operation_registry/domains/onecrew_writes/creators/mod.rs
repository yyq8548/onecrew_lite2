//! Creator write registration, composed in the established descriptor order.
mod candidates_contacts;
mod contact_routes;
mod discovery;
mod groups;
mod records;
mod shared;

#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;

use crate::operation_registry::domains::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

const FAILURE_CODE: &str = "creator_write_outcome_unknown";

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    discovery::register_discovery_and_content(builder, owners.clone())?;
    candidates_contacts::register_candidate_and_contact(builder, owners.clone())?;
    records::register_local_creator_writes(builder, owners.clone())?;
    groups::register_creator_group_writes(builder, owners)
}
