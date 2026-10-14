use std::sync::Arc;

use onecrew_local_runtime::{
    LocalPlatform,
    toolkits::{self, PlatformNativeData, PlatformReadError},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::read_failure::read_failure_projection;
use super::shared::{metadata, policy};
use crate::operation_registry::{
    descriptor::{
        ConnectionRef, ConnectionRequirement, EffectClass, OperatorToolkitId, ServiceToolkitId,
    },
    execution::{BindingContext, ExecutionDeclaration},
    registration::{RegistryBuildError, RegistryBuilder},
};

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HandleInput {
    handle: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct YoutubeChannelInput {
    channel_id: String,
    /// The caller must choose whether comments are part of this read.
    depth: toolkits::youtube::YoutubeReadDepth,
}

/// One live X profile as X returns it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct XProfileRead {
    /// X's own user object: id, name, username, bio, location, profile_links,
    /// followers, following, posts, likes, verified, blue_verified,
    /// profile_image_url, viewer_follows, viewer_followed_by, can_dm.
    profile: PlatformNativeData,
}

/// One live Instagram profile as Instagram returns it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct InstagramProfileRead {
    /// Instagram's own user object: id, username, full_name, and the public
    /// fields the profile publishes (follower_count, biography, external_url,
    /// bio_links, public_email, ...).
    profile: PlatformNativeData,
}

/// One live YouTube channel as the client returns it.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct YoutubeChannelRead {
    /// The channel with its About-page links, recent videos and public
    /// comments as returned: channel_id, channel_url, title, description,
    /// description_source_url, custom_url, subscriber_count, full descriptions
    /// in bounded recent_videos, public_comments, about_links and unavailable_sections.
    channel: PlatformNativeData,
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    register_x(builder, Arc::clone(&platform))?;
    register_instagram(builder, Arc::clone(&platform))?;
    register_youtube(builder, platform)
}

/// The operator account a binding was invoked with; the registry has already
/// matched its toolkit against the operation's connection requirement.
fn operator_account_id(context: &BindingContext) -> Option<&str> {
    match context.connection.as_ref()? {
        ConnectionRef::OperatorAccount {
            operator_account_id,
            ..
        } => Some(operator_account_id),
        ConnectionRef::ServiceCredential { .. } | ConnectionRef::Mailbox { .. } => None,
    }
}

fn credential_id(context: &BindingContext) -> Option<&str> {
    match context.connection.as_ref()? {
        ConnectionRef::ServiceCredential { credential_id, .. } => Some(credential_id),
        ConnectionRef::OperatorAccount { .. } | ConnectionRef::Mailbox { .. } => None,
    }
}

fn register_x(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::X,
    };
    builder.register_operation::<HandleInput, XProfileRead, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::X_PROFILE_GET),
        policy(
            EffectClass::ReadOpenWorld,
            connection.clone(),
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                // An absent operator account cannot pass the registry's
                // connection check; the toolkit refuses the empty id anyway.
                let account = operator_account_id(&context).unwrap_or_default().to_owned();
                Ok(XProfileRead {
                    profile: toolkits::x::profile(&platform, &account, &input.handle).await?,
                })
            }
        },
        read_failure_projection(&connection),
    )
}

fn register_instagram(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let connection = ConnectionRequirement::OperatorAccount {
        toolkit: OperatorToolkitId::Instagram,
    };
    builder.register_operation::<HandleInput, InstagramProfileRead, PlatformReadError, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::INSTAGRAM_PROFILE_GET),
        policy(
            EffectClass::ReadOpenWorld,
            connection.clone(),
            ExecutionDeclaration::IndependentRead,
        ),
        move |context, input| {
            let platform = Arc::clone(&platform);
            async move {
                // An absent operator account cannot pass the registry's
                // connection check; the toolkit refuses the empty id anyway.
                let account = operator_account_id(&context).unwrap_or_default().to_owned();
                Ok(InstagramProfileRead {
                    profile: toolkits::instagram::profile(&platform, &account, &input.handle)
                        .await?,
                })
            }
        },
        read_failure_projection(&connection),
    )
}

fn register_youtube(
    builder: &mut RegistryBuilder,
    platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    let connection = ConnectionRequirement::ServiceCredential {
        toolkit: ServiceToolkitId::Youtube,
    };
    builder
        .register_operation::<YoutubeChannelInput, YoutubeChannelRead, PlatformReadError, _, _, _>(
            metadata(onecrew_local_runtime::OperationId::YOUTUBE_CHANNEL_GET),
            policy(
                EffectClass::ReadOpenWorld,
                connection.clone(),
                ExecutionDeclaration::IndependentRead,
            ),
            move |context, input| {
                let platform = Arc::clone(&platform);
                async move {
                    // An absent service credential cannot pass the registry's
                    // connection check; admission refuses the empty id anyway.
                    let credential = credential_id(&context).unwrap_or_default().to_owned();
                    Ok(YoutubeChannelRead {
                        channel: toolkits::youtube::channel(
                            &platform,
                            &credential,
                            &input.channel_id,
                            input.depth,
                        )
                        .await?,
                    })
                }
            },
            read_failure_projection(&connection),
        )
}
