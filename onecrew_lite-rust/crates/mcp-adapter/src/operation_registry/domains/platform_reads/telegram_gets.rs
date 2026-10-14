//! Telegram community and administrator reads. Each decodes the runtime's
//! typed locator and returns Telegram's own model as platform-native data; a
//! failure projects through the shared read failure projection. This adapter
//! owns no Creator conversion.
use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy};
use crate::operation_registry::{
    descriptor::{ConnectionRef, ConnectionRequirement, EffectClass, OperatorToolkitId},
    execution::{BindingContext, ExecutionDeclaration},
    registration::{RegistryBuildError, RegistryBuilder},
};
use onecrew_local_runtime::{
    LocalPlatform,
    toolkits::{
        PlatformNativeData, PlatformReadError,
        telegram::{self, TgCommunityLocator},
    },
};
use schemars::JsonSchema;
use serde::Deserialize;
use std::sync::Arc;

/// The connection every Telegram read declares.
pub(super) const TELEGRAM_CONNECTION: ConnectionRequirement =
    ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::Telegram,
    };

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CommunityInput {
    #[schemars(with = "CommunityLocatorSchema")]
    locator: TgCommunityLocator,
}

// Adapter-owned schema declarations keep provider crates free of protocol
// schema dependencies. Actual decoding uses the facade's typed locator.
#[derive(JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "variants define the protocol schema for the typed locator"
)]
enum CommunityLocatorSchema {
    PublicUsername {
        username: String,
    },
    StableCommunity {
        external_id: String,
        kind: CommunityKindSchema,
    },
}

#[derive(JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(
    dead_code,
    reason = "variants define the protocol community-kind schema"
)]
enum CommunityKindSchema {
    Channel,
    Supergroup,
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let runtime = platform.clone();
    builder.register_operation::<CommunityInput, PlatformNativeData, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_GET),
        policy(
            EffectClass::ReadOpenWorld,
            TELEGRAM_CONNECTION,
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = runtime.clone();
            async move { telegram::community(&platform, operator(&context)?, &input.locator).await }
        },
        read_failure_projection(&TELEGRAM_CONNECTION),
    )?;
    builder.register_operation::<CommunityInput, PlatformNativeData, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::TELEGRAM_COMMUNITY_ADMINISTRATORS_LIST),
        policy(
            EffectClass::ReadOpenWorld,
            TELEGRAM_CONNECTION,
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = platform.clone();
            async move { telegram::admins(&platform, operator(&context)?, &input.locator).await }
        },
        read_failure_projection(&TELEGRAM_CONNECTION),
    )
}

fn operator(context: &BindingContext) -> Result<&str, PlatformReadError> {
    match context.connection.as_ref() {
        Some(ConnectionRef::OperatorAccount {
            operator_account_id,
            ..
        }) => Ok(operator_account_id),
        _ => Err(PlatformReadError::InputInvalid(
            "operator connection required",
        )),
    }
}
