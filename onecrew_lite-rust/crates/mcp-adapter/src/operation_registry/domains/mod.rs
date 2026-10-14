mod connections;
mod interactions;
mod onecrew_reads;
mod onecrew_writes;
mod platform_reads;
mod playbooks;
mod service_credentials;
mod toolkit_guides;
mod workflow_guides;

use std::sync::Arc;

use onecrew_creator_runtime::CreatorRuntime;
use onecrew_local_runtime::LocalPlatform;

use crate::accounts::OperatorAccountWorkflow;

use super::registration::{RegistryBuildError, RegistryBuilder};

#[derive(Clone)]
pub(crate) struct RegistryOwners {
    pub(crate) platform: Arc<LocalPlatform>,
    pub(crate) creators: CreatorRuntime,
    pub(crate) operator_accounts: Arc<dyn OperatorAccountWorkflow>,
}

pub(super) fn register_all(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    connections::register(builder, owners.clone())?;
    onecrew_reads::register(builder, owners.clone())?;
    platform_reads::register(builder, owners.clone())?;
    service_credentials::register(builder, owners.clone())?;
    onecrew_writes::register(builder, owners)?;
    interactions::register(builder)?;
    workflow_guides::register(builder)?;
    playbooks::register(builder)?;
    toolkit_guides::register(builder)?;
    Ok(())
}
