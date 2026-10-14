mod profiles;
mod telegram;

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use futures::StreamExt;

use onecrew_creator_runtime::{
    CommunityAdministratorRole, CommunityAdministratorSource, CommunityReference, ContactChannel,
    CreatorContactProfileField, CreatorContactProfileLink, CreatorSaveContactEvidence,
    CreatorSavePublicLink, discover_creator_contacts,
};

use super::CreatorSaveError;
use crate::{
    CreatorSaveCandidateInput, CreatorSaveConnection, CreatorSaveTarget, LocalPlatform, toolkits,
};

pub(super) struct ReadCreatorProfile {
    pub observed_at: std::time::SystemTime,
    pub account: onecrew_creator_runtime::PlatformAccountCandidate,
    pub profile: onecrew_creator_runtime::CreatorSaveProfile,
    pub evidence: Vec<CreatorSaveContactEvidence>,
    pub warnings: Vec<String>,
    pub administrator: Option<(
        CommunityAdministratorRole,
        CommunityReference,
        CommunityAdministratorSource,
    )>,
}

// Provider admission is an execution budget independent of Creator batch size.
const PROVIDER_READ_CONCURRENCY: usize = 4;
// Link-hub pages are read inside the same concurrent stage as the profile they
// belong to, so this budget is per candidate: every selection in a batch gets
// its own pages instead of the first two spending the whole batch's allowance.
const MAX_HUB_PAGES: usize = 2;
const HUB_CANDIDATE_BUDGET: Duration = Duration::from_secs(30);
// One ceiling still bounds the preparation as a whole, for the case where many
// hub pages are individually slow rather than unavailable.
const HUB_BATCH_BUDGET: Duration = Duration::from_secs(120);

async fn collect_profile_reads<Read: std::future::Future>(
    reads: impl IntoIterator<Item = Read>,
) -> Vec<Read::Output> {
    futures::stream::iter(reads)
        .buffered(PROVIDER_READ_CONCURRENCY)
        .collect()
        .await
}

pub(super) async fn resolve_many(
    platform: &LocalPlatform,
    candidates: Vec<CreatorSaveCandidateInput>,
) -> Vec<(
    CreatorSaveCandidateInput,
    Result<ReadCreatorProfile, CreatorSaveError>,
)> {
    let batch_deadline = Instant::now() + HUB_BATCH_BUDGET;
    collect_profile_reads(candidates.into_iter().map(|candidate| async move {
        let resolved = match resolve(platform, &candidate).await {
            Ok(mut resolved) => {
                // Hub pages belong to the concurrent read stage. Reading them
                // in a later serial loop is what forced a batch-wide page
                // budget, and with it left all but the first two selections
                // without their link-hub contacts.
                add_hubs(&mut resolved, batch_deadline).await;
                Ok(resolved)
            }
            Err(error) => Err(error),
        };
        (candidate, resolved)
    }))
    .await
}

/// Read this candidate's own link-hub pages and hold what they publish. Its
/// warnings join the candidate's own, so the caller can name the Creator they
/// belong to instead of collapsing every candidate's copy into one line.
async fn add_hubs(resolved: &mut ReadCreatorProfile, batch_deadline: Instant) {
    let fields: Vec<_> = resolved
        .evidence
        .iter()
        .filter_map(|item| match item {
            CreatorSaveContactEvidence::ProfileField {
                source,
                source_url,
                text,
            } => Some(CreatorContactProfileField {
                source: *source,
                source_url: source_url.as_deref(),
                text,
            }),
            _ => None,
        })
        .collect();
    let links: Vec<_> = resolved
        .evidence
        .iter()
        .filter_map(|item| match item {
            CreatorSaveContactEvidence::ProfileLink {
                source,
                source_url,
                display_text,
                target_url,
            } => Some(CreatorContactProfileLink {
                source: *source,
                source_url: source_url.as_deref(),
                display_text,
                target_url,
            }),
            _ => None,
        })
        .collect();
    let mut hubs: BTreeSet<_> =
        discover_creator_contacts(resolved.account.platform, &fields, &links)
            .into_iter()
            .filter(|item| item.channel == ContactChannel::LinkHub)
            .map(|item| item.value)
            .collect();
    for evidence in &resolved.evidence {
        if let CreatorSaveContactEvidence::Observation {
            channel: ContactChannel::LinkHub,
            value,
            ..
        } = evidence
        {
            hubs.insert(value.clone());
        }
    }
    let mut used = Duration::ZERO;
    for (pages, url) in hubs.into_iter().enumerate() {
        let remaining = batch_deadline
            .checked_duration_since(Instant::now())
            .unwrap_or(Duration::ZERO)
            .min(HUB_CANDIDATE_BUDGET.saturating_sub(used));
        if pages >= MAX_HUB_PAGES || remaining.is_zero() {
            resolved.warnings.push(
                "Additional link-hub pages were not read because this preview's page or time limit was reached."
                    .to_owned(),
            );
            break;
        }
        let started = Instant::now();
        let extracted = tokio::time::timeout(remaining, toolkits::link_hub::extract(&url)).await;
        used += started.elapsed();
        match extracted {
            Ok(Ok(page)) => resolved.evidence.push(CreatorSaveContactEvidence::HeldHub {
                source_url: page.source_url,
                text: page
                    .emails
                    .into_iter()
                    .map(|email| email.value)
                    .collect::<Vec<_>>()
                    .join(" "),
                links: page
                    .links
                    .into_iter()
                    .map(|link| CreatorSavePublicLink {
                        display_text: link.text.unwrap_or_default(),
                        target_url: link.url,
                    })
                    .collect(),
            }),
            _ => resolved.warnings.push(
                "A linked contact page could not be read; its unverified contacts are excluded from this preview."
                    .to_owned(),
            ),
        }
    }
}

impl LocalPlatform {
    pub async fn read_creator_save_telegram_administrators(
        &self,
        input: super::TelegramCommunityAdministratorsInput,
    ) -> Result<super::CreatorSaveTelegramAdministrators, CreatorSaveError> {
        let account_id = input.operator_account_ref_id.as_str().to_owned();
        let context = self
            .creators
            .telegram_community_route_context(
                input.source_creator_id,
                input.source_contact_route_id,
                input.operator_account_ref_id,
            )
            .await
            .map_err(|_| CreatorSaveError::CreatorUnavailable)?;
        let kind = match context.target_kind {
            onecrew_creator_runtime::CommunityTargetKind::Channel => {
                toolkits::telegram::TgCommunityKind::Channel
            }
            onecrew_creator_runtime::CommunityTargetKind::Supergroup => {
                toolkits::telegram::TgCommunityKind::Supergroup
            }
        };
        let page = toolkits::telegram::administrators(
            self,
            &account_id,
            &toolkits::telegram::TgCommunityLocator::StableCommunity {
                external_id: context.external_id.clone(),
                kind,
            },
        )
        .await
        .map_err(|error| {
            provider_error(
                "telegram_administrators_read",
                error.safe_message().to_owned(),
            )
        })?;
        if page.community.external_id != context.external_id || page.community.kind != kind {
            return Err(CreatorSaveError::IdentityMismatch);
        }
        Ok(telegram::application_page(page))
    }
}

pub(super) async fn resolve(
    platform: &LocalPlatform,
    candidate: &CreatorSaveCandidateInput,
) -> Result<ReadCreatorProfile, CreatorSaveError> {
    let result = match (&candidate.target, &candidate.connection) {
        (CreatorSaveTarget::X { user_id, handle }, CreatorSaveConnection::X { account_ref_id }) => {
            let user =
                toolkits::x::profile_user(platform, &account_ref_id.to_string(), handle.as_str())
                    .await
                    .map_err(|error| {
                        provider_error("x_profile_read", error.safe_message().to_owned())
                    })?;
            require_identity(user_id.as_str(), &user.id)?;
            profiles::x(user)
        }
        (
            CreatorSaveTarget::Instagram { user_id, handle },
            CreatorSaveConnection::Instagram { account_ref_id },
        ) => {
            let user = toolkits::instagram::profile_user(
                platform,
                &account_ref_id.to_string(),
                handle.as_str(),
            )
            .await
            .map_err(|error| {
                provider_error("instagram_profile_read", error.safe_message().to_owned())
            })?;
            require_identity(user_id.as_str(), &user.id)?;
            profiles::instagram(user)
        }
        (
            CreatorSaveTarget::Youtube { channel_id },
            CreatorSaveConnection::Youtube {
                service_credential_ref_id,
            },
        ) => {
            let user = toolkits::youtube::channel_creator(
                platform,
                &service_credential_ref_id.to_string(),
                channel_id.as_str(),
                onecrew_youtube::YoutubeReadDepth::Light,
            )
            .await
            .map_err(|error| {
                provider_error("youtube_channel_read", error.safe_message().to_owned())
            })?;
            require_identity(channel_id.as_str(), &user.channel_id)?;
            profiles::youtube(user)
        }
        (
            CreatorSaveTarget::TelegramAdministrator { user_id, community },
            CreatorSaveConnection::Telegram { account_ref_id },
        ) => {
            telegram::resolve(
                platform,
                candidate,
                user_id.as_str(),
                community,
                *account_ref_id,
            )
            .await?
        }
        _ => return Err(CreatorSaveError::SourceMismatch),
    };
    Ok(result)
}

fn provider_error(code: &str, message: String) -> CreatorSaveError {
    CreatorSaveError::ProviderRead {
        code: code.to_owned(),
        message,
    }
}

fn require_identity(selected: &str, observed: &str) -> Result<(), CreatorSaveError> {
    if selected != observed {
        return Err(CreatorSaveError::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
