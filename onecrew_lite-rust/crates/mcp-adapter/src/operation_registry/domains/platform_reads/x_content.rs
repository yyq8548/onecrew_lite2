use std::sync::Arc;

use onecrew_local_runtime::{
    LocalPlatform,
    toolkits::{PlatformNativeData, PlatformReadError, x},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy};
use crate::operation_registry::{
    descriptor::{ConnectionRef, ConnectionRequirement, EffectClass, OperatorToolkitId},
    execution::{BindingContext, ExecutionDeclaration},
    registration::{RegistryBuildError, RegistryBuilder},
};

/// The connection every X content read declares.
const X_CONNECTION: ConnectionRequirement = ConnectionRequirement::OperatorAccount {
    toolkit: OperatorToolkitId::X,
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct XUserPostsInput {
    external_user_id: String,
    #[serde(default)]
    cursor: Option<String>,
}

/// One native page of a user's posts as X returns it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct XUserPostsRead {
    /// X's own timeline page: `items` (posts, newest first, each with id,
    /// text, author, metrics, created_at, media, urls) and `next_cursor`.
    page: PlatformNativeData,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct XPostThreadInput {
    content_id: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    #[schemars(range(min = x::POST_THREAD_MIN_COUNT, max = x::POST_THREAD_MAX_COUNT))]
    count: Option<u8>,
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    register_user_posts(builder, Arc::clone(&platform))?;
    register_thread(builder, platform)
}

fn operator_account_id(context: &BindingContext) -> Option<&str> {
    match context.connection.as_ref()? {
        ConnectionRef::OperatorAccount {
            operator_account_id,
            ..
        } => Some(operator_account_id),
        ConnectionRef::ServiceCredential { .. } | ConnectionRef::Mailbox { .. } => None,
    }
}

fn register_user_posts(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<XUserPostsInput, XUserPostsRead, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::X_USER_POSTS_LIST),
        policy(
            EffectClass::ReadOpenWorld,
            X_CONNECTION,
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                let account = operator_account_id(&context).unwrap_or_default().to_owned();
                let page = x::user_posts(
                    &platform,
                    &account,
                    &input.external_user_id,
                    input.cursor.as_deref(),
                )
                .await?;
                Ok(XUserPostsRead { page })
            }
        },
        read_failure_projection(&X_CONNECTION),
    )
}

fn register_thread(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    builder.register_operation::<XPostThreadInput, PlatformNativeData, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::X_POST_THREAD_GET),
        policy(
            EffectClass::ReadOpenWorld,
            X_CONNECTION,
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                let account = operator_account_id(&context).ok_or(
                    PlatformReadError::InputInvalid("operator connection required"),
                )?;
                let count = input.count.map_or(x::POST_THREAD_DEFAULT_COUNT, u16::from);
                x::post_thread(
                    &platform,
                    account,
                    &input.content_id,
                    count,
                    input.cursor.as_deref(),
                )
                .await
            }
        },
        read_failure_projection(&X_CONNECTION),
    )
}
