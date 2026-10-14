//! Target-resolution composition. Facades parse their own locator syntax;
//! Creator owns saved-route policy, identity conflicts and the commit.

use onecrew_creator_runtime::{
    ContactTargetResolutionResult, CreatorContactTargetPreparation, Platform,
    PlatformResolvedTarget, PlatformTargetResolution, ResolveContactTargetInput, TargetKind,
};

use super::OutreachError;
use crate::{LocalPlatform, toolkits};

impl LocalPlatform {
    /// Resolve one saved contact route, reusing already-proven identity when
    /// Creator only needs to commit ownership of a cached target.
    ///
    /// # Errors
    /// Returns an error for unavailable context, failed provider reads, or a
    /// failed identity-validation/commit step. No profile observation is appended.
    pub async fn resolve_contact_target(
        &self,
        input: ResolveContactTargetInput,
    ) -> Result<ContactTargetResolutionResult, OutreachError> {
        let (context, evidence) = match self
            .creators
            .prepare_contact_target_resolution(input)
            .await
            .map_err(OutreachError::Target)?
        {
            CreatorContactTargetPreparation::Ready(result) => return Ok(*result),
            CreatorContactTargetPreparation::Read(context) => {
                let evidence = provider_target(
                    self,
                    context.operator_account().platform,
                    context.operator_account().id.as_str(),
                    &context.route().locator.normalized_value,
                )
                .await?;
                (context, evidence)
            }
            CreatorContactTargetPreparation::Commit { context, evidence } => (context, evidence),
        };
        self.creators
            .finish_contact_target_resolution(context, evidence)
            .await
            .map_err(OutreachError::Target)
    }
}

async fn provider_target(
    runtime: &LocalPlatform,
    platform: Platform,
    operator_id: &str,
    locator: &str,
) -> Result<PlatformTargetResolution, OutreachError> {
    match platform {
        Platform::X => {
            let Some(handle) = onecrew_x::parse_profile_handle(locator) else {
                return Ok(unsupported(
                    "X contact locator does not contain a public profile handle",
                ));
            };
            let user = toolkits::x::profile_user(runtime, operator_id, &handle)
                .await
                .map_err(|_| OutreachError::ProviderRead("X contact target"))?;
            Ok(PlatformTargetResolution::Resolved {
                target: PlatformResolvedTarget {
                    platform,
                    target_kind: TargetKind::User,
                    external_target_id: user.id,
                    handle: Some(user.username.clone()),
                    display_name: Some(user.name),
                    canonical_url: Some(format!("https://x.com/{}", user.username)),
                },
            })
        }
        Platform::Instagram => {
            let Some(handle) = onecrew_ig::parse_profile_handle(locator) else {
                return Ok(unsupported(
                    "Instagram contact locator does not contain a public profile handle",
                ));
            };
            let user = toolkits::instagram::profile_user(runtime, operator_id, &handle)
                .await
                .map_err(|_| OutreachError::ProviderRead("Instagram contact target"))?;
            Ok(PlatformTargetResolution::Resolved {
                target: PlatformResolvedTarget {
                    platform,
                    target_kind: TargetKind::User,
                    external_target_id: user.id,
                    handle: Some(user.username.clone()),
                    display_name: Some(user.full_name),
                    canonical_url: Some(format!("https://www.instagram.com/{}/", user.username)),
                },
            })
        }
        Platform::Telegram => {
            let result = toolkits::telegram::resolve_target(runtime, operator_id, locator)
                .await
                .map_err(|_| OutreachError::ProviderRead("Telegram contact target"))?;
            Ok(match result {
                onecrew_tg::TgTargetResolution::Resolved { target } => {
                    PlatformTargetResolution::Resolved {
                        target: PlatformResolvedTarget {
                            platform,
                            target_kind: match target.kind {
                                onecrew_tg::TgTargetKind::User => TargetKind::User,
                                onecrew_tg::TgTargetKind::Bot => TargetKind::Bot,
                                onecrew_tg::TgTargetKind::Channel => TargetKind::Channel,
                                onecrew_tg::TgTargetKind::Supergroup => TargetKind::Supergroup,
                                onecrew_tg::TgTargetKind::Group => TargetKind::Group,
                            },
                            external_target_id: target.external_id,
                            handle: target.username,
                            display_name: target.display_name,
                            canonical_url: target.canonical_url,
                        },
                    }
                }
                onecrew_tg::TgTargetResolution::Preview { canonical_url, .. } => {
                    PlatformTargetResolution::BrowserOnly {
                        canonical_url: Some(canonical_url),
                        reason: "Telegram invite preview has no verified stable target ID".into(),
                    }
                }
                onecrew_tg::TgTargetResolution::BrowserOnly { .. } => {
                    PlatformTargetResolution::BrowserOnly {
                        canonical_url: onecrew_tg::parse_locator(locator).canonical_url,
                        reason: "This Telegram locator requires browser interaction".into(),
                    }
                }
                onecrew_tg::TgTargetResolution::NotFound => PlatformTargetResolution::NotFound {
                    reason: "Telegram target was not found".into(),
                },
            })
        }
        Platform::YouTube => match onecrew_youtube::parse_channel_locator(locator) {
            onecrew_youtube::YoutubeChannelLocator::ChannelId(channel_id) => {
                Ok(youtube_identity(channel_id, None, None))
            }
            onecrew_youtube::YoutubeChannelLocator::Handle(handle) => {
                let channel = toolkits::youtube::resolve_handle(runtime, operator_id, &handle)
                    .await
                    .map_err(youtube_target_read_error)?;
                youtube_summary_identity(channel)
            }
            onecrew_youtube::YoutubeChannelLocator::BrowserOnly { canonical_url } => {
                Ok(PlatformTargetResolution::BrowserOnly {
                    canonical_url,
                    reason: "YouTube locator is neither a Channel ID nor a public @handle".into(),
                })
            }
        },
        Platform::TikTok => Ok(unsupported(
            "This platform has no external outreach target resolver",
        )),
    }
}

fn youtube_target_read_error(error: toolkits::PlatformReadError) -> OutreachError {
    match error {
        toolkits::PlatformReadError::CredentialHealthCommitUnknown => {
            OutreachError::ProviderStateCommitUnknown
        }
        _ => OutreachError::ProviderRead("YouTube contact target"),
    }
}

fn youtube_summary_identity(
    channel: onecrew_youtube::YoutubeChannelSummary,
) -> Result<PlatformTargetResolution, OutreachError> {
    let onecrew_youtube::YoutubeChannelLocator::ChannelId(channel_id) =
        onecrew_youtube::parse_channel_locator(&channel.channel_id)
    else {
        return Err(OutreachError::Target(
            onecrew_creator_runtime::CreatorTargetResolutionError::InvalidEvidence,
        ));
    };
    let handle =
        channel.custom_url.as_deref().and_then(
            |value| match onecrew_youtube::parse_channel_locator(value) {
                onecrew_youtube::YoutubeChannelLocator::Handle(handle) => Some(handle),
                _ => None,
            },
        );
    Ok(youtube_identity(channel_id, handle, Some(channel.title)))
}

fn youtube_identity(
    channel_id: String,
    handle: Option<String>,
    display_name: Option<String>,
) -> PlatformTargetResolution {
    PlatformTargetResolution::Resolved {
        target: PlatformResolvedTarget {
            platform: Platform::YouTube,
            target_kind: TargetKind::Channel,
            canonical_url: Some(format!("https://www.youtube.com/channel/{channel_id}")),
            external_target_id: channel_id,
            handle,
            display_name,
        },
    }
}

fn unsupported(reason: &str) -> PlatformTargetResolution {
    PlatformTargetResolution::Unsupported {
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_provider_health_commit_keeps_its_local_effect_classification() {
        assert!(matches!(
            youtube_target_read_error(toolkits::PlatformReadError::CredentialHealthCommitUnknown),
            OutreachError::ProviderStateCommitUnknown
        ));
        for error in [
            toolkits::PlatformReadError::CredentialUnavailable,
            toolkits::PlatformReadError::Platform {
                evidence: onecrew_youtube::YoutubePlatformError::Client(
                    onecrew_youtube::YoutubeClientError::ChannelNotFound,
                )
                .evidence(),
                after_local_commit: true,
            },
        ] {
            assert!(matches!(
                youtube_target_read_error(error),
                OutreachError::ProviderRead("YouTube contact target")
            ));
        }
        assert!(
            OutreachError::ProviderStateCommitUnknown
                .to_string()
                .contains("connection state")
        );
    }

    #[tokio::test]
    async fn a_stable_youtube_id_needs_no_credential_or_provider_read() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = LocalPlatform::open(directory.path()).unwrap();
        let id = "UC_x5XG1OV2P6uZZ5FSM9Ttw";
        let result = provider_target(&runtime, Platform::YouTube, "unavailable-credential", id)
            .await
            .unwrap();
        let PlatformTargetResolution::Resolved { target } = result else {
            panic!("stable channel target");
        };
        assert_eq!(target.external_target_id, id);
        assert_eq!(target.target_kind, TargetKind::Channel);
        assert_eq!(target.display_name, None);
        assert_eq!(target.handle, None);
    }

    #[test]
    fn a_handle_read_maps_only_the_canonical_youtube_identity() {
        let result = youtube_summary_identity(onecrew_youtube::YoutubeChannelSummary {
            channel_id: "UC_x5XG1OV2P6uZZ5FSM9Ttw".into(),
            title: "Channel".into(),
            custom_url: Some("https://www.youtube.com/@channel".into()),
            thumbnail_url: Some("https://example.com/avatar".into()),
            description: "Not a profile observation".into(),
            country: Some("US".into()),
        })
        .unwrap();
        let PlatformTargetResolution::Resolved { target } = result else {
            panic!("resolved target");
        };
        assert_eq!(target.handle.as_deref(), Some("channel"));
        let encoded = serde_json::to_value(target).unwrap();
        assert!(encoded.get("metadata").is_none());
        assert!(encoded.get("description").is_none());
        assert!(encoded.get("thumbnail_url").is_none());
    }
}
