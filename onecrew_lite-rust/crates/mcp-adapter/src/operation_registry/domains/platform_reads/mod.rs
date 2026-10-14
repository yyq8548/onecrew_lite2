mod instagram_search;
mod profiles;
mod read_failure;
mod shared;
mod telegram;
mod telegram_gets;
#[cfg(test)]
mod tests;
mod x_advanced_search;
mod x_content;
mod youtube_search;

use std::sync::Arc;

use super::RegistryOwners;
use crate::operation_registry::registration::{RegistryBuildError, RegistryBuilder};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = Arc::clone(&owners.platform);
    x_advanced_search::register(builder, Arc::clone(&platform))?;
    instagram_search::register(builder, Arc::clone(&platform))?;
    youtube_search::register(builder, Arc::clone(&platform))?;
    profiles::register(builder, Arc::clone(&platform))?;
    x_content::register(builder, Arc::clone(&platform))?;
    telegram_gets::register(builder, Arc::clone(&platform))?;
    telegram::register(builder, platform)?;
    Ok(())
}
