use onecrew_local_runtime::{YoutubePlatformGetInput, YoutubePlatformGetOutcome};

use super::{
    super::RegistryOwners,
    shared::{
        ConnectionBindingError, EmptyInput, classify_detail_failure, metadata, policy, youtube_id,
    },
};
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, ServiceToolkitId},
    execution::run_blocking,
    registration::{RegistryBuildError, RegistryBuilder},
};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<
        EmptyInput,
        YoutubePlatformGetOutcome,
        ConnectionBindingError,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::YOUTUBE_CONNECTION_GET),
        policy(ConnectionRequirement::ServiceCredential {
            toolkit: ServiceToolkitId::Youtube,
        }),
        move |context, _input| {
            let platform = owners.platform.clone();
            async move {
                let credential_id = youtube_id(context)?;
                run_blocking(move || {
                    platform
                        .platform_youtube_get(YoutubePlatformGetInput { credential_id })
                        .map_err(ConnectionBindingError::Owner)
                })
                .await
            }
        },
        classify_detail_failure,
    )
}
