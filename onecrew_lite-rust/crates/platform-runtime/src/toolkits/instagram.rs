//! Instagram toolkit functions over the `onecrew_ig` facade.
//!
//! A read validates its input with Instagram's own checks, then delegates to
//! the shared operator-account read; a failed Instagram call crosses as
//! Instagram's evidence. Peer-facing reads return platform-native data, and
//! runtime orchestration keeps the typed Instagram models.

use std::{fmt, future::Future};

use super::{
    DmToolkitError, PlatformNativeData, PlatformReadError,
    read::{
        BoundSearch, OperatorSession, admitted_session, operator_account_read, platform_native_data,
    },
};
use onecrew_dm_contracts::{DmCapability, DmReceipt, DmSendRequest, DmTarget, DmThreadReadback};
use onecrew_ig::{IgAccount, IgError, IgUser, accepts_stable_user_id};
pub use onecrew_ig::{
    IgRelationshipAction, IgRelationshipStatus, IgWriteFailure, USER_SEARCH_DEFAULT_COUNT,
    USER_SEARCH_MAX_COUNT, USER_SEARCH_MIN_COUNT, accepts_user_search_count,
    accepts_user_search_query,
};
use serde::Serialize;

use crate::{BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform};

/// Opens the shared DM provider after connection admission.
pub async fn message_capability(
    platform: &LocalPlatform,
    operator_account_id: &str,
    target: &DmTarget,
) -> Result<DmCapability, DmToolkitError> {
    platform
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Instagram)
        .await?
        .capability(target)
        .await
        .map_err(DmToolkitError::from)
}

/// Sends through the same facade instance factory retained by conversation sync.
/// The owning workflow supplies the confirmed immutable request.
pub async fn message_send(
    platform: &LocalPlatform,
    operator_account_id: &str,
    request: DmSendRequest,
) -> Result<DmReceipt, DmToolkitError> {
    if request.body.trim().is_empty() {
        return Err(DmToolkitError::InvalidInput(
            "Direct message body is required",
        ));
    }
    platform
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Instagram)
        .await?
        .send(request)
        .await
        .map_err(DmToolkitError::from)
}

/// A bounded thread page with explicit coverage; missing messages do not
/// authorize a resend when the provider cannot prove complete history.
pub async fn message_thread_readback(
    platform: &LocalPlatform,
    operator_account_id: &str,
    target: &DmTarget,
    external_thread_id: Option<&str>,
    limit: u16,
) -> Result<DmThreadReadback, DmToolkitError> {
    if !(1..=100).contains(&limit) {
        return Err(DmToolkitError::InvalidInput(
            "Direct message readback limit must be between 1 and 100",
        ));
    }
    platform
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Instagram)
        .await?
        .readback(target, external_thread_id, limit)
        .await
        .map_err(DmToolkitError::from)
}

/// Instagram user-search parameters. The provider returns one user collection
/// without a continuation cursor.
#[derive(Debug, Clone, Serialize)]
pub struct UsersSearchInput {
    pub operator_account_id: String,
    pub query: String,
    pub count: u16,
}

#[derive(Debug)]
pub enum IgToolkitWriteError {
    /// Admission failed before a remote write could be invoked.
    Admission(PlatformReadError),
    Provider(IgWriteFailure),
}

impl fmt::Display for IgToolkitWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::Provider(IgWriteFailure::Refused) => {
                formatter.write_str("Instagram refused the write before any accepted effect")
            }
            Self::Provider(IgWriteFailure::Unknown) => {
                formatter.write_str("Instagram write outcome is unknown")
            }
        }
    }
}

impl std::error::Error for IgToolkitWriteError {}

/// The Instagram session of one operator account.
fn instagram_session(operator_account_id: &str) -> OperatorSession<'_, IgAccount> {
    OperatorSession {
        platform: OperatorAccountPlatform::Instagram,
        operator_account_id,
        open: |sessions, profile_id| sessions.instagram(profile_id),
    }
}

/// One Instagram read through `session` after its input was validated; a
/// failed Instagram call crosses as Instagram's evidence.
async fn instagram_read<S, T, Fut>(
    platform: &LocalPlatform,
    session: OperatorSession<'_, S>,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(S) -> Fut,
) -> Result<T, PlatformReadError>
where
    S: Send + 'static,
    Fut: Future<Output = Result<T, IgError>>,
{
    operator_account_read(platform, session, search, read, onecrew_ig::evidence).await
}

/// The operator account's relationship with one Instagram user, read by the
/// user's stable ID.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the stable ID is not an Instagram stable
/// user ID, the operator account is not admitted, its session cannot be
/// opened, or Instagram rejects the read.
pub async fn relationship_get(
    platform: &LocalPlatform,
    operator_account_id: &str,
    stable_id: &str,
) -> Result<IgRelationshipStatus, PlatformReadError> {
    if !accepts_stable_user_id(stable_id) {
        return Err(PlatformReadError::InputInvalid(
            "Instagram stable user ID is required",
        ));
    }
    instagram_read(
        platform,
        instagram_session(operator_account_id),
        None,
        |account| async move { account.relationship_status_by_id(stable_id).await },
    )
    .await
}

pub async fn relationship_update(
    platform: &LocalPlatform,
    operator_account_id: &str,
    action: IgRelationshipAction,
    stable_id: &str,
) -> Result<(), IgToolkitWriteError> {
    admitted_session(platform, instagram_session(operator_account_id))
        .await
        .map_err(IgToolkitWriteError::Admission)?
        .update_relationship(action, stable_id)
        .await
        .map_err(IgToolkitWriteError::Provider)
}

/// Search Instagram users and return each user Instagram returned as
/// platform-native data, in Instagram's order.
///
/// # Errors
/// Rejects a blank query and an out-of-range page size before admission, and
/// checks the bound request before admission and again once the session is
/// open, before the provider call.
pub async fn search_users(
    platform: &LocalPlatform,
    input: UsersSearchInput,
    authorization: BoundSearchAuthorization,
) -> Result<Vec<PlatformNativeData>, PlatformReadError> {
    if !accepts_user_search_query(&input.query) {
        return Err(PlatformReadError::InputInvalid(
            "Instagram search query is required",
        ));
    }
    if !accepts_user_search_count(input.count) {
        return Err(PlatformReadError::InputInvalid(
            "Instagram search count must be between 1 and 100",
        ));
    }
    let input = &input;
    let users = instagram_read(
        platform,
        instagram_session(&input.operator_account_id),
        Some(BoundSearch::new(&authorization, input)),
        |account| async move { account.search_users(&input.query, input.count).await },
    )
    .await?;
    users_native_data(&users)
}

/// Each Instagram user as platform-native data, in the given order.
fn users_native_data(users: &[IgUser]) -> Result<Vec<PlatformNativeData>, PlatformReadError> {
    users.iter().map(platform_native_data).collect()
}

/// One Instagram profile by handle as platform-native data: Instagram's own
/// user object.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the handle is empty, the operator account
/// is not admitted, its session cannot be opened, Instagram rejects the read,
/// or the profile is not a JSON object.
pub async fn profile(
    platform: &LocalPlatform,
    operator_account_id: &str,
    handle: &str,
) -> Result<PlatformNativeData, PlatformReadError> {
    platform_native_data(&profile_user(platform, operator_account_id, handle).await?)
}

/// One Instagram profile by handle as the typed Instagram user, for runtime
/// orchestration: Creator save reads and outreach target resolution.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the handle is empty, the operator account
/// is not admitted, its session cannot be opened, or Instagram rejects the
/// read.
pub(crate) async fn profile_user(
    platform: &LocalPlatform,
    operator_account_id: &str,
    handle: &str,
) -> Result<IgUser, PlatformReadError> {
    let username = handle.trim().trim_start_matches('@');
    if username.is_empty() {
        return Err(PlatformReadError::InputInvalid(
            "Instagram handle is required",
        ));
    }
    instagram_read(
        platform,
        instagram_session(operator_account_id),
        None,
        |account| async move { account.user(username).await },
    )
    .await
}

#[cfg(test)]
mod tests;
