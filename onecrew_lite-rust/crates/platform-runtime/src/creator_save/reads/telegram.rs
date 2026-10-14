use onecrew_creator_runtime::{
    AccountRefId, CommunityAdministratorRole, CommunityAdministratorSource, CommunityReference,
    CommunityTargetKind, CreatorContactRouteId, CreatorId, CreatorSaveProfile, Platform,
    PlatformAccountCandidate, TargetKind,
};
use uuid::Uuid;

use super::{ReadCreatorProfile, provider_error};
use crate::{
    AdministratorSelectionSource, CreatorSaveCandidateInput, CreatorSaveCommunityKind,
    CreatorSaveError, CreatorSaveProvenanceInput, CreatorSaveTelegramAdministratorRow,
    CreatorSaveTelegramAdministrators, LocalPlatform, TelegramCommunityLocator, toolkits::telegram,
};

/// One fact mapping is shared by list projection and the fresh save read.
pub(super) fn canonical_account(user: &telegram::TgPublicUser) -> PlatformAccountCandidate {
    PlatformAccountCandidate {
        platform: Platform::Telegram,
        external_id: Some(user.id.clone()),
        target_kind: Some(if user.is_bot {
            TargetKind::Bot
        } else {
            TargetKind::User
        }),
        handle: user.username.clone(),
        display_name: [user.first_name.as_deref(), user.last_name.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" "),
        profile_url: user
            .username
            .as_deref()
            .map(|username| format!("https://t.me/{username}")),
    }
}

pub(super) fn application_page(
    page: telegram::TgPublicAdministrators,
) -> CreatorSaveTelegramAdministrators {
    CreatorSaveTelegramAdministrators {
        community: page.community,
        administrators: page
            .administrators
            .into_iter()
            .map(|administrator| CreatorSaveTelegramAdministratorRow {
                eligibility: onecrew_creator_runtime::assess_telegram_administrator_eligibility(
                    &canonical_account(&administrator.user),
                ),
                role: administrator.role,
                user: administrator.user,
            })
            .collect(),
        page_limit: page.page_limit,
    }
}

pub(super) async fn resolve(
    platform: &LocalPlatform,
    candidate: &CreatorSaveCandidateInput,
    user_id: &str,
    locator: &TelegramCommunityLocator,
    account_id: Uuid,
) -> Result<ReadCreatorProfile, CreatorSaveError> {
    let CreatorSaveProvenanceInput::AdministratorSelection {
        community_id,
        community_kind,
        source,
    } = &candidate.provenance
    else {
        return Err(CreatorSaveError::SourceMismatch);
    };
    let account_ref = AccountRefId::new(account_id.to_string());
    let (lookup, canonical_source) = match (locator, source) {
        (
            TelegramCommunityLocator::PublicUsername { username },
            AdministratorSelectionSource::OperatorCommunity,
        ) => (
            telegram::TgCommunityLocator::PublicUsername {
                username: username.as_str().to_owned(),
            },
            CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: account_ref,
            },
        ),
        (
            TelegramCommunityLocator::StableCommunity { external_id, kind },
            AdministratorSelectionSource::OperatorCommunity,
        ) => (
            telegram::TgCommunityLocator::StableCommunity {
                external_id: external_id.as_str().to_owned(),
                kind: provider_kind(*kind),
            },
            CommunityAdministratorSource::OperatorAccount {
                operator_account_ref_id: account_ref,
            },
        ),
        (
            TelegramCommunityLocator::SavedCommunityRoute {
                creator_id,
                route_id,
            },
            AdministratorSelectionSource::SavedCommunityRoute { .. },
        ) => {
            let context = platform
                .creators
                .telegram_community_route_context(
                    CreatorId::new(creator_id.as_str()),
                    CreatorContactRouteId::new(route_id.as_str()),
                    account_ref,
                )
                .await
                .map_err(|_| CreatorSaveError::CreatorUnavailable)?;
            if context.external_id != community_id.as_str()
                || context.target_kind != canonical_kind(*community_kind)
            {
                return Err(CreatorSaveError::SourceMismatch);
            }
            (
                telegram::TgCommunityLocator::StableCommunity {
                    external_id: context.external_id,
                    kind: provider_kind(*community_kind),
                },
                context.source,
            )
        }
        _ => return Err(CreatorSaveError::SourceMismatch),
    };
    let page = telegram::administrators(platform, &account_id.to_string(), &lookup)
        .await
        .map_err(|error| {
            provider_error(
                "telegram_administrators_read",
                error.safe_message().to_owned(),
            )
        })?;
    if page.community.external_id != community_id.as_str()
        || page.community.kind != provider_kind(*community_kind)
    {
        return Err(CreatorSaveError::IdentityMismatch);
    }
    let member = page.administrators.into_iter().find(|member| member.user.id == user_id)
        .ok_or_else(|| provider_error("telegram_administrator_unavailable", "Selected administrator was not returned by this bounded community read; select again".into()))?;
    let role = match member.role {
        telegram::TgAdministratorRole::Owner => CommunityAdministratorRole::Owner,
        telegram::TgAdministratorRole::Administrator => CommunityAdministratorRole::Administrator,
    };
    Ok(ReadCreatorProfile {
        observed_at: std::time::SystemTime::now(),
        account: canonical_account(&member.user),
        profile: CreatorSaveProfile::default(),
        evidence: Vec::new(),
        warnings: Vec::new(),
        administrator: Some((
            role,
            CommunityReference {
                external_id: page.community.external_id,
                title: page.community.title,
                handle: page.community.username,
                target_kind: canonical_kind(*community_kind),
            },
            canonical_source,
        )),
    })
}

fn provider_kind(kind: CreatorSaveCommunityKind) -> telegram::TgCommunityKind {
    match kind {
        CreatorSaveCommunityKind::Channel => telegram::TgCommunityKind::Channel,
        CreatorSaveCommunityKind::Supergroup => telegram::TgCommunityKind::Supergroup,
    }
}

fn canonical_kind(kind: CreatorSaveCommunityKind) -> CommunityTargetKind {
    match kind {
        CreatorSaveCommunityKind::Channel => CommunityTargetKind::Channel,
        CreatorSaveCommunityKind::Supergroup => CommunityTargetKind::Supergroup,
    }
}
