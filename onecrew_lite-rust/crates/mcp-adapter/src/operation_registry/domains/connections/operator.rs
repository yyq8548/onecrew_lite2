use onecrew_local_runtime::{
    InstagramPlatformGetInput, InstagramPlatformGetOutcome, TelegramPlatformGetInput,
    TelegramPlatformGetOutcome, XPlatformGetInput, XPlatformGetOutcome,
};

use super::{
    super::RegistryOwners,
    shared::{
        ConnectionBindingError, EmptyInput, classify_detail_failure, metadata, operator_id, policy,
    },
};
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, OperatorToolkitId},
    execution::run_blocking,
    registration::{RegistryBuildError, RegistryBuilder},
};

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    register_x(builder, owners.clone())?;
    register_instagram(builder, owners.clone())?;
    register_telegram(builder, owners)?;
    Ok(())
}

fn register_x(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<EmptyInput, XPlatformGetOutcome, ConnectionBindingError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::X_CONNECTION_GET),
        policy(ConnectionRequirement::OperatorAccount {
            toolkit: OperatorToolkitId::X,
        }),
        move |context, _input| {
            let platform = owners.platform.clone();
            async move {
                let operator_account_id = operator_id(context, OperatorToolkitId::X)?;
                run_blocking(move || {
                    platform
                        .platform_x_get(XPlatformGetInput {
                            operator_account_id,
                        })
                        .map_err(ConnectionBindingError::Owner)
                })
                .await
            }
        },
        classify_detail_failure,
    )
}

fn register_instagram(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<
        EmptyInput,
        InstagramPlatformGetOutcome,
        ConnectionBindingError,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::INSTAGRAM_CONNECTION_GET),
        policy(ConnectionRequirement::OperatorAccount {
            toolkit: OperatorToolkitId::Instagram,
        }),
        move |context, _input| {
            let platform = owners.platform.clone();
            async move {
                let operator_account_id =
                    operator_id(context, OperatorToolkitId::Instagram)?;
                run_blocking(move || {
                    platform
                        .platform_instagram_get(InstagramPlatformGetInput { operator_account_id })
                        .map_err(ConnectionBindingError::Owner)
                })
                .await
            }
        },
        classify_detail_failure,
    )
}

fn register_telegram(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<
        EmptyInput,
        TelegramPlatformGetOutcome,
        ConnectionBindingError,
        _,
        _,
        _,
    >(
        metadata(onecrew_local_runtime::OperationId::TELEGRAM_CONNECTION_GET),
        policy(ConnectionRequirement::OperatorAccount {
            toolkit: OperatorToolkitId::Telegram,
        }),
        move |context, _input| {
            let platform = owners.platform.clone();
            async move {
                let operator_account_id =
                    operator_id(context, OperatorToolkitId::Telegram)?;
                run_blocking(move || {
                    platform
                        .platform_telegram_get(TelegramPlatformGetInput { operator_account_id })
                        .map_err(ConnectionBindingError::Owner)
                })
                .await
            }
        },
        classify_detail_failure,
    )
}
