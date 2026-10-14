//! X toolkit functions over the `onecrew_x` facade.
//!
//! A read validates its input with X's own checks, then delegates to the
//! shared operator-account read; a failed X call crosses as X's evidence.
//! Peer-facing reads return platform-native data, and runtime orchestration
//! keeps the typed X models.

use std::future::Future;

use super::{
    DmToolkitError, PlatformNativeData, PlatformReadError,
    read::{
        BoundSearch, OperatorSession, admitted_session, operator_account_read, platform_native_data,
    },
};
use onecrew_dm_contracts::{DmCapability, DmReceipt, DmSendRequest, DmTarget, DmThreadReadback};
pub use onecrew_x::{
    ADVANCED_SEARCH_DEFAULT_COUNT, ADVANCED_SEARCH_MAX_COUNT, ADVANCED_SEARCH_MIN_COUNT,
    AdvancedSearchCriteria, AdvancedSearchFilter, AdvancedSearchResultMode,
    POST_THREAD_DEFAULT_COUNT, POST_THREAD_MAX_COUNT, POST_THREAD_MIN_COUNT, XRelationshipStatus,
    XReplyReceipt, XWriteFailure, accepts_advanced_search_count, accepts_post_thread_count,
};
use onecrew_x::{User, XAccount, XError};
use serde::Serialize;

use crate::{BoundSearchAuthorization, LocalPlatform, OperatorAccountPlatform};

/// Opens the shared DM provider after connection admission.
pub async fn message_capability(
    platform: &LocalPlatform,
    operator_account_id: &str,
    target: &DmTarget,
) -> Result<DmCapability, DmToolkitError> {
    platform
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::X)
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
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::X)
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
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::X)
        .await?
        .readback(target, external_thread_id, limit)
        .await
        .map_err(DmToolkitError::from)
}

/// X treats the posts page size as a hint, not a returned-item limit; the
/// complete native page and its matching cursor are returned as X built them.
const USER_POSTS_PAGE_HINT: u16 = 20;

/// X-owned search criteria and its native page parameters, bound together with
/// the selected operator account before admission.
#[derive(Debug, Clone, Serialize)]
pub struct AdvancedSearchInput {
    pub operator_account_id: String,
    pub criteria: AdvancedSearchCriteria,
    pub result_mode: AdvancedSearchResultMode,
    pub count: u16,
    pub cursor: Option<String>,
}

#[derive(Debug)]
pub enum XToolkitWriteError {
    /// Admission failed before a remote write could be invoked.
    Admission(PlatformReadError),
    Provider(XWriteFailure),
}

impl std::fmt::Display for XToolkitWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Admission(error) => error.fmt(formatter),
            Self::Provider(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for XToolkitWriteError {}

/// The X session of one operator account. The sealed X Chat passcode that
/// comes with the session is dropped, and zeroized, inside the blocking
/// segment because no read or write here has a use for it.
fn x_session(operator_account_id: &str) -> OperatorSession<'_, XAccount> {
    OperatorSession {
        platform: OperatorAccountPlatform::X,
        operator_account_id,
        open: |sessions, profile_id| sessions.x(profile_id).map(|(account, _passcode)| account),
    }
}

/// One X read after its input was validated.
async fn x_read<T, Fut>(
    platform: &LocalPlatform,
    operator_account_id: &str,
    search: Option<BoundSearch<'_>>,
    read: impl FnOnce(XAccount) -> Fut,
) -> Result<T, PlatformReadError>
where
    Fut: Future<Output = Result<T, XError>>,
{
    operator_account_read(
        platform,
        x_session(operator_account_id),
        search,
        read,
        XError::evidence,
    )
    .await
}

pub async fn relationship_get(
    platform: &LocalPlatform,
    operator_account_id: &str,
    stable_id: &str,
    handle: &str,
) -> Result<XRelationshipStatus, PlatformReadError> {
    let handle = handle.trim().trim_start_matches('@');
    if stable_id.trim().is_empty() || handle.is_empty() {
        return Err(PlatformReadError::InputInvalid(
            "X stable target ID and handle are required",
        ));
    }
    x_read(platform, operator_account_id, None, |account| async move {
        account.relationship_status(stable_id, handle).await
    })
    .await
}

pub async fn relationship_update(
    platform: &LocalPlatform,
    operator_account_id: &str,
    action: onecrew_x::RelationshipAction,
    stable_id: &str,
) -> Result<(), XToolkitWriteError> {
    admitted_session(platform, x_session(operator_account_id))
        .await
        .map_err(XToolkitWriteError::Admission)?
        .update_relationship(action, stable_id)
        .await
        .map_err(XToolkitWriteError::Provider)
}

pub async fn reply(
    platform: &LocalPlatform,
    operator_account_id: &str,
    reply_to_id: &str,
    body: &str,
) -> Result<XReplyReceipt, XToolkitWriteError> {
    admitted_session(platform, x_session(operator_account_id))
        .await
        .map_err(XToolkitWriteError::Admission)?
        .reply_with_receipt(reply_to_id, body)
        .await
        .map_err(XToolkitWriteError::Provider)
}

pub async fn reply_readback(
    platform: &LocalPlatform,
    operator_account_id: &str,
    receipt_id: &str,
) -> Result<Option<onecrew_x::Tweet>, PlatformReadError> {
    if receipt_id.trim().is_empty() {
        return Err(PlatformReadError::InputInvalid(
            "X reply receipt ID is required",
        ));
    }
    x_read(platform, operator_account_id, None, |account| async move {
        account.reply_readback(receipt_id).await
    })
    .await
}

/// Search X with its typed advanced criteria and return its complete native
/// page, effective query and continuation cursor as platform-native data.
///
/// # Errors
/// Rejects invalid inputs and mismatched authorization before admission, then
/// checks the live bound request again after opening the admitted session.
pub async fn search_advanced(
    platform: &LocalPlatform,
    input: AdvancedSearchInput,
    authorization: BoundSearchAuthorization,
) -> Result<PlatformNativeData, PlatformReadError> {
    if !accepts_advanced_search_count(input.count) {
        return Err(PlatformReadError::InputInvalid(
            "X search count must be between 1 and 100",
        ));
    }
    input
        .criteria
        .validate()
        .map_err(|_| PlatformReadError::InputInvalid("X advanced search criteria are invalid"))?;
    let input = &input;
    let page = x_read(
        platform,
        &input.operator_account_id,
        Some(BoundSearch::new(&authorization, input)),
        |account| async move {
            account
                .search_advanced(
                    &input.criteria,
                    input.result_mode,
                    input.count,
                    input.cursor.as_deref(),
                )
                .await
        },
    )
    .await?;
    platform_native_data(&page)
}

/// One X profile by handle as platform-native data: X's own user object.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the handle is empty, the operator account
/// is not admitted, its session cannot be opened, X rejects the read, or the
/// profile is not a JSON object.
pub async fn profile(
    platform: &LocalPlatform,
    operator_account_id: &str,
    handle: &str,
) -> Result<PlatformNativeData, PlatformReadError> {
    platform_native_data(&profile_user(platform, operator_account_id, handle).await?)
}

/// One X profile by handle as the typed X user, for runtime orchestration:
/// Creator save reads and outreach target resolution.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the handle is empty, the operator account
/// is not admitted, its session cannot be opened, or X rejects the read.
pub(crate) async fn profile_user(
    platform: &LocalPlatform,
    operator_account_id: &str,
    handle: &str,
) -> Result<User, PlatformReadError> {
    let username = handle.trim().trim_start_matches('@');
    if username.is_empty() {
        return Err(PlatformReadError::InputInvalid("X handle is required"));
    }
    x_read(platform, operator_account_id, None, |account| async move {
        account.profile(username).await
    })
    .await
}

/// One native page of a user's recent posts as platform-native data, newest
/// first, with X's own continuation cursor.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the user id is empty, the operator account
/// is not admitted, its session cannot be opened, or X rejects the read.
pub async fn user_posts(
    platform: &LocalPlatform,
    operator_account_id: &str,
    user_id: &str,
    cursor: Option<&str>,
) -> Result<PlatformNativeData, PlatformReadError> {
    let user_id = user_id.trim();
    if user_id.is_empty() {
        return Err(PlatformReadError::InputInvalid("X user id is required"));
    }
    let page = x_read(platform, operator_account_id, None, |account| async move {
        account
            .user_posts(user_id, USER_POSTS_PAGE_HINT, cursor)
            .await
    })
    .await?;
    platform_native_data(&page)
}

/// One page of a post's thread as platform-native data: the post and its
/// replies, with X's own continuation cursor. `count` must lie within
/// [`POST_THREAD_MIN_COUNT`]`..=`[`POST_THREAD_MAX_COUNT`]; X itself is asked
/// for at most 40 posts, which is X's provider request behavior.
///
/// # Errors
///
/// Returns [`PlatformReadError`] when the post id is empty or the count is out
/// of range, the operator account is not admitted, its session cannot be
/// opened, or X rejects the read.
pub async fn post_thread(
    platform: &LocalPlatform,
    operator_account_id: &str,
    tweet_id: &str,
    count: u16,
    cursor: Option<&str>,
) -> Result<PlatformNativeData, PlatformReadError> {
    let tweet_id = tweet_id.trim();
    if tweet_id.is_empty() {
        return Err(PlatformReadError::InputInvalid("X post id is required"));
    }
    if !accepts_post_thread_count(count) {
        return Err(PlatformReadError::InputInvalid(
            "X thread count must be between 1 and 100",
        ));
    }
    let page = x_read(platform, operator_account_id, None, |account| async move {
        account.tweet_thread(tweet_id, count, cursor).await
    })
    .await?;
    platform_native_data(&page)
}

#[cfg(test)]
mod tests;
