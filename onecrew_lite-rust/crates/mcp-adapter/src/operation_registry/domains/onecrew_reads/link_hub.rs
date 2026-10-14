//! `onecrew.link_hub.extract`: the onecrew cell's own read of one allowlisted
//! public link-hub page. No connection, no snapshot, no Creator vocabulary:
//! the URL goes in, the links and addresses found come out as found.

use std::sync::Arc;

use onecrew_local_runtime::{
    LocalPlatform,
    toolkits::link_hub::{LinkHubExtractError, LinkHubExtraction, extract},
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::shared::metadata;
use crate::operation_registry::{
    descriptor::{ConnectionRequirement, EffectClass, SurfaceId},
    execution::{
        ClassifiedOwnerFailure, ExecutionDeclaration, PublicFailureCode, RecoveryDirective,
    },
    registration::{OperationPolicy, RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators];

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LinkHubExtractInput {
    /// The link-hub page URL as found on a profile (Linktree, Beacons, Lnk.Bio
    /// and peers). Only allowlisted link-hub hosts are fetched.
    url: String,
}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    _platform: Arc<LocalPlatform>,
) -> Result<(), RegistryBuildError> {
    builder
        .register_operation::<LinkHubExtractInput, LinkHubExtraction, LinkHubExtractError, _, _, _>(
            metadata(
                onecrew_local_runtime::OperationId::ONECREW_LINK_HUB_EXTRACT,
                "creators",
                SURFACES,
            ),
            OperationPolicy::read_only(
                EffectClass::ReadOpenWorld,
                ConnectionRequirement::None,
                ExecutionDeclaration::Sequential,
            ),
            |_context, input| async move { extract(&input.url).await },
            classify_link_hub_failure,
        )
}

/// The extraction writes nothing, so every failure is known-no-effect; the
/// message is the toolkit's own text, which names no URL beyond an
/// unsupported host.
fn classify_link_hub_failure(error: LinkHubExtractError) -> ClassifiedOwnerFailure {
    let (code, retry_safe, recovery) = match &error {
        LinkHubExtractError::InvalidUrl => (
            "link_hub_url_invalid",
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        LinkHubExtractError::UnsupportedHost(_) => (
            "link_hub_host_unsupported",
            false,
            Some(RecoveryDirective::CorrectInput),
        ),
        LinkHubExtractError::Request => (
            "link_hub_request_failed",
            true,
            Some(RecoveryDirective::RetryLater {
                minimum_delay_seconds: 30,
                max_attempts: 1,
            }),
        ),
        LinkHubExtractError::Timeout => (
            "link_hub_request_timed_out",
            true,
            Some(RecoveryDirective::RetryLater {
                minimum_delay_seconds: 30,
                max_attempts: 1,
            }),
        ),
        LinkHubExtractError::InvalidRedirect => ("link_hub_redirect_invalid", false, None),
        LinkHubExtractError::TooManyRedirects => ("link_hub_redirects_exceeded", false, None),
        LinkHubExtractError::BodyTooLarge => ("link_hub_body_too_large", false, None),
        LinkHubExtractError::InvalidUtf8 => ("link_hub_body_not_utf8", false, None),
    };
    ClassifiedOwnerFailure::known_no_effect(
        PublicFailureCode::parse(code).expect("static link hub failure code is valid"),
        retry_safe,
        recovery,
    )
    .with_message(&error.safe_message())
}

#[cfg(test)]
mod tests {
    use onecrew_local_runtime::toolkits::link_hub::LinkHubExtractError;

    use super::classify_link_hub_failure;
    use crate::operation_registry::execution::{
        ClassifiedOwnerFailure, PublicFailureCode, RecoveryDirective,
    };

    #[test]
    fn every_extract_failure_is_known_no_effect_with_the_toolkit_message() {
        for (error, code, retry_safe, recovery) in [
            (
                LinkHubExtractError::InvalidUrl,
                "link_hub_url_invalid",
                false,
                Some(RecoveryDirective::CorrectInput),
            ),
            (
                LinkHubExtractError::UnsupportedHost("example.com".into()),
                "link_hub_host_unsupported",
                false,
                Some(RecoveryDirective::CorrectInput),
            ),
            (
                LinkHubExtractError::Timeout,
                "link_hub_request_timed_out",
                true,
                Some(RecoveryDirective::RetryLater {
                    minimum_delay_seconds: 30,
                    max_attempts: 1,
                }),
            ),
            (
                LinkHubExtractError::InvalidRedirect,
                "link_hub_redirect_invalid",
                false,
                None,
            ),
            (
                LinkHubExtractError::TooManyRedirects,
                "link_hub_redirects_exceeded",
                false,
                None,
            ),
            (
                LinkHubExtractError::BodyTooLarge,
                "link_hub_body_too_large",
                false,
                None,
            ),
            (
                LinkHubExtractError::InvalidUtf8,
                "link_hub_body_not_utf8",
                false,
                None,
            ),
        ] {
            let message = error.safe_message();
            assert_eq!(
                classify_link_hub_failure(error),
                ClassifiedOwnerFailure::known_no_effect(
                    PublicFailureCode::parse(code).unwrap(),
                    retry_safe,
                    recovery,
                )
                .with_message(&message),
                "{code}"
            );
        }
    }
}
