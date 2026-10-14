use onecrew_creator_runtime::{
    AccountAvailability, AccountKind, AccountRef, AccountRefId, Platform,
};
use onecrew_local_domain::{ServiceCredential, ServiceProvider};
use uuid::Uuid;

use super::{shared, youtube};
use crate::{
    LocalPlatform, OperatorAccountPlatform, OperatorAccountProjection,
    credentials::services::blocking_segment,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformConnectionSelector {
    OperatorAccount {
        platform: OperatorAccountPlatform,
        operator_account_id: String,
    },
    YoutubeCredential {
        credential_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformConnectionResolutionError {
    InvalidIdentifier,
    NotFoundOrMismatch,
    Unavailable,
    Internal,
}

impl PlatformConnectionResolutionError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidIdentifier => "invalid_identifier",
            Self::NotFoundOrMismatch => "not_found_or_mismatch",
            Self::Unavailable => "unavailable",
            Self::Internal => "internal",
        }
    }

    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::InvalidIdentifier => "connection identifier is invalid",
            Self::NotFoundOrMismatch => {
                "connection was not found or does not match the requested kind and platform"
            }
            Self::Unavailable => "connection is unavailable",
            Self::Internal => "connection resolution is unavailable",
        }
    }
}

pub(super) async fn resolve(
    platform: &LocalPlatform,
    selector: PlatformConnectionSelector,
) -> Result<AccountRefId, PlatformConnectionResolutionError> {
    match selector {
        PlatformConnectionSelector::OperatorAccount {
            platform: expected_platform,
            operator_account_id,
        } => resolve_operator_account(platform, expected_platform, &operator_account_id).await,
        PlatformConnectionSelector::YoutubeCredential { credential_id } => {
            resolve_youtube_credential(platform, &credential_id).await
        }
    }
}

async fn resolve_operator_account(
    platform: &LocalPlatform,
    expected_platform: OperatorAccountPlatform,
    operator_account_id: &str,
) -> Result<AccountRefId, PlatformConnectionResolutionError> {
    let local_id = parse_identifier(operator_account_id)?;
    let account_ref_id = AccountRefId::new(local_id.to_string());
    let vault = platform.vault.clone();
    let operator_account =
        blocking_segment(PlatformConnectionResolutionError::Internal, move || {
            crate::account::operator_account_projection(&vault, local_id)
                .map_err(|_| PlatformConnectionResolutionError::Internal)
        })
        .await?
        .ok_or(PlatformConnectionResolutionError::NotFoundOrMismatch)?;
    if operator_account.platform != expected_platform {
        return Err(PlatformConnectionResolutionError::NotFoundOrMismatch);
    }

    let creator_platform = creator_operator_platform(expected_platform);
    let creator_account = admitted_creator_account(
        platform,
        &account_ref_id,
        creator_platform,
        AccountKind::Profile,
    )
    .await?;
    if !operator_is_locally_available(&operator_account)
        || !creator_account.active
        || creator_account.availability != AccountAvailability::Available
    {
        return Err(PlatformConnectionResolutionError::Unavailable);
    }
    Ok(account_ref_id)
}

fn operator_is_locally_available(account: &OperatorAccountProjection) -> bool {
    shared::operator_is_locally_available(account)
}

async fn resolve_youtube_credential(
    platform: &LocalPlatform,
    credential_id: &str,
) -> Result<AccountRefId, PlatformConnectionResolutionError> {
    let local_id = parse_identifier(credential_id)?;
    let account_ref_id = AccountRefId::new(local_id.to_string());
    let vault = platform.vault.clone();
    let (credential, locally_available) =
        blocking_segment(PlatformConnectionResolutionError::Internal, move || {
            let credential = vault
                .list_service_credentials(ServiceProvider::Youtube, false)
                .map_err(|_| PlatformConnectionResolutionError::Internal)?
                .into_iter()
                .find(|credential| credential.id == local_id)
                .ok_or(PlatformConnectionResolutionError::NotFoundOrMismatch)?;
            let locally_available = youtube_credential_is_locally_available(&vault, &credential);
            Ok((credential, locally_available))
        })
        .await?;
    if credential.provider != ServiceProvider::Youtube {
        return Err(PlatformConnectionResolutionError::NotFoundOrMismatch);
    }

    let creator_account = admitted_creator_account(
        platform,
        &account_ref_id,
        Platform::YouTube,
        AccountKind::ServiceCredential,
    )
    .await?;
    if !locally_available
        || !creator_account.active
        || creator_account.availability != AccountAvailability::Available
    {
        return Err(PlatformConnectionResolutionError::Unavailable);
    }
    Ok(account_ref_id)
}

fn youtube_credential_is_locally_available(
    vault: &onecrew_local_vault::LocalVault,
    credential: &ServiceCredential,
) -> bool {
    youtube::has_youtube_shape(credential)
        && shared::service_credential_is_locally_available(vault, credential)
}

/// The Creator `account_refs` gate, read under bounded Creator admission.
async fn admitted_creator_account(
    platform: &LocalPlatform,
    account_ref_id: &AccountRefId,
    expected_platform: Platform,
    expected_kind: AccountKind,
) -> Result<AccountRef, PlatformConnectionResolutionError> {
    let account = platform
        .creators
        .read_account_ref_gate(expected_platform, expected_kind, account_ref_id.clone())
        .await
        .map_err(|_| PlatformConnectionResolutionError::Internal)?
        .ok_or(PlatformConnectionResolutionError::NotFoundOrMismatch)?;
    if account.platform != expected_platform || account.kind != expected_kind {
        return Err(PlatformConnectionResolutionError::NotFoundOrMismatch);
    }
    Ok(account)
}

fn parse_identifier(value: &str) -> Result<Uuid, PlatformConnectionResolutionError> {
    Uuid::parse_str(value).map_err(|_| PlatformConnectionResolutionError::InvalidIdentifier)
}

const fn creator_operator_platform(platform: OperatorAccountPlatform) -> Platform {
    match platform {
        OperatorAccountPlatform::X => Platform::X,
        OperatorAccountPlatform::Instagram => Platform::Instagram,
        OperatorAccountPlatform::Telegram => Platform::Telegram,
    }
}
