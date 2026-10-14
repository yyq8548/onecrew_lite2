//! Telegram toolkit functions over the `onecrew_tg` facade.
//!
//! A read validates its input with Telegram's own checks, then delegates to the
//! shared operator-account read. The admitted session is prepared on the
//! blocking pool, and its MTProto connection opens only after the bound search
//! was checked again. Preparing opens no connection, so a bound search is
//! checked once more after the connection opens, immediately before the
//! Telegram call. Every outcome the read returns closes an opened connection; a
//! read that is cancelled or panics drops it instead, and the transport
//! connection's drop aborts its pool task. A failed Telegram call crosses as
//! Telegram's evidence. Peer-facing reads return platform-native data, and
//! runtime orchestration keeps the typed Telegram models.

use super::{
    DmToolkitError, PlatformNativeData, PlatformReadError,
    read::{BoundSearch, OperatorSession, operator_account_read, platform_native_data},
};
use onecrew_dm_contracts::{
    DmCapability, DmDiagnostic, DmReceipt, DmSendRequest, DmTarget, DmThreadReadback,
};
use onecrew_sessions::TelegramPreparedSession;
use onecrew_tg::TgAccount;
pub use onecrew_tg::{
    COMMUNITY_SEARCH_DEFAULT_COUNT, COMMUNITY_SEARCH_MAX_COUNT, COMMUNITY_SEARCH_MIN_COUNT,
    TgAdministratorRole, TgCommunityKind, TgCommunityLocator, TgPublicAdministrator,
    TgPublicAdministrators, TgPublicCommunity, TgPublicUser, accepts_community_search_count,
    accepts_community_search_query,
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
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Telegram)
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
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Telegram)
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
        .admitted_direct_messages(operator_account_id, OperatorAccountPlatform::Telegram)
        .await?
        .readback(target, external_thread_id, limit)
        .await
        .map_err(DmToolkitError::from)
}

/// Telegram public community-search parameters. The provider returns chats
/// without a continuation cursor or reusable peer authorization.
#[derive(Debug, Clone, Serialize)]
pub struct CommunitiesSearchInput {
    pub operator_account_id: String,
    pub query: String,
    pub count: u16,
}

/// The Telegram session of one operator account, prepared from its stored
/// material on the blocking pool. Preparing opens no connection.
fn telegram_session(operator_account_id: &str) -> OperatorSession<'_, TelegramPreparedSession> {
    OperatorSession {
        platform: OperatorAccountPlatform::Telegram,
        operator_account_id,
        open: |sessions, profile_id| sessions.prepare_telegram(profile_id),
    }
}

/// Binds a search authorization to its exact request, once for each check.
type BindSearch<'b, 's> = &'b (dyn Fn() -> BoundSearch<'s> + Sync);

/// One Telegram read after its input was validated, on a connection to the
/// admitted operator account.
async fn telegram_read<T, E>(
    platform: &LocalPlatform,
    operator_account_id: &str,
    search: Option<BindSearch<'_, '_>>,
    read: impl AsyncFnOnce(&TgAccount) -> Result<T, E>,
    evidence: fn(&E) -> DmDiagnostic,
) -> Result<T, PlatformReadError> {
    connected_read(
        platform,
        telegram_session(operator_account_id),
        search,
        TelegramPreparedSession::connect,
        read,
        TgAccount::close,
        evidence,
    )
    .await
}

/// The shared operator-account read over a session that connects before it
/// reads. The shared read checks the bound search before admission and once
/// the session is open; `connect` runs only after both checks. A connection
/// that cannot open is an unavailable session, its diagnostic is discarded,
/// and a failed connect leaves nothing open. Once connected, the search is
/// checked again immediately before `read`, and a search refused there closes
/// the connection without reading. Every outcome this read returns closes an
/// opened connection. A read that is cancelled or panics never reaches `close`;
/// dropping the Telegram connection aborts its pool task instead.
async fn connected_read<P, C, CE, T, E>(
    platform: &LocalPlatform,
    session: OperatorSession<'_, P>,
    search: Option<BindSearch<'_, '_>>,
    connect: impl AsyncFnOnce(P) -> Result<C, CE>,
    read: impl AsyncFnOnce(&C) -> Result<T, E>,
    close: impl AsyncFnOnce(C),
    evidence: fn(&E) -> DmDiagnostic,
) -> Result<T, PlatformReadError>
where
    P: Send + 'static,
{
    operator_account_read(
        platform,
        session,
        search.map(|bind| bind()),
        |prepared| async move {
            let Ok(connection) = connect(prepared).await else {
                return Ok(Err(PlatformReadError::SessionUnavailable));
            };
            if let Some(refused) = search.and_then(|bind| bind().validate().err()) {
                close(connection).await;
                return Ok(Err(refused));
            }
            let result = read(&connection).await;
            close(connection).await;
            result.map(Ok)
        },
        evidence,
    )
    .await?
}

/// Classifies first, returning static browser-only answers before session
/// admission. Account-backed results carry only the provider's public target.
///
/// # Errors
/// Returns [`PlatformReadError`] when the operator account is not admitted,
/// its session cannot be opened, or Telegram fails the resolution.
pub async fn resolve_target(
    platform: &LocalPlatform,
    operator_account_id: &str,
    locator: &str,
) -> Result<onecrew_tg::TgTargetResolution, PlatformReadError> {
    let locator = onecrew_tg::parse_locator(locator);
    if let Some(resolution) = onecrew_tg::static_target_resolution(&locator) {
        return Ok(resolution);
    }
    let locator = &locator;
    telegram_read(
        platform,
        operator_account_id,
        None,
        async |account: &TgAccount| account.resolve_target(locator).await,
        onecrew_tg::evidence,
    )
    .await
}

/// Search Telegram public communities; each chat is platform-native data with
/// Telegram's public fields.
///
/// # Errors
/// Rejects a blank query, an out-of-range count and a mismatched bound request
/// before admission, checks the live bound request again once the session is
/// open and once more after connecting, immediately before Telegram's search,
/// and returns [`PlatformReadError`] for every later local outcome or Telegram
/// failure.
pub async fn search_communities(
    platform: &LocalPlatform,
    input: CommunitiesSearchInput,
    authorization: BoundSearchAuthorization,
) -> Result<Vec<PlatformNativeData>, PlatformReadError> {
    if !accepts_community_search_query(&input.query) {
        return Err(PlatformReadError::InputInvalid(
            "Telegram search query is required",
        ));
    }
    if !accepts_community_search_count(input.count) {
        return Err(PlatformReadError::InputInvalid(
            "Telegram search count must be between 1 and 100",
        ));
    }
    let input = &input;
    let authorization = &authorization;
    let chats = telegram_read(
        platform,
        &input.operator_account_id,
        Some(&|| BoundSearch::new(authorization, input)),
        async |account: &TgAccount| {
            account
                .search_chats(&input.query, i32::from(input.count))
                .await
        },
        onecrew_tg::evidence,
    )
    .await?;
    chats.iter().map(platform_native_data).collect()
}

/// One public community identity by public username or saved stable ID and
/// kind, as platform-native data.
///
/// # Errors
/// Rejects an invalid locator before admission, and returns
/// [`PlatformReadError`] for every later local outcome or Telegram failure.
pub async fn community(
    platform: &LocalPlatform,
    operator_account_id: &str,
    locator: &TgCommunityLocator,
) -> Result<PlatformNativeData, PlatformReadError> {
    validate_locator(locator)?;
    let community = telegram_read(
        platform,
        operator_account_id,
        None,
        async |account: &TgAccount| account.community(locator).await,
        onecrew_tg::community_read_evidence,
    )
    .await?;
    platform_native_data(&community)
}

/// One administrator page as platform-native data: at most 200 entries and
/// without a cursor. An absent member was not in this response; this is not a
/// complete scan.
///
/// # Errors
/// Rejects an invalid locator before admission, and returns
/// [`PlatformReadError`] for every later local outcome or Telegram failure.
pub async fn admins(
    platform: &LocalPlatform,
    operator_account_id: &str,
    locator: &TgCommunityLocator,
) -> Result<PlatformNativeData, PlatformReadError> {
    platform_native_data(&administrators(platform, operator_account_id, locator).await?)
}

/// One administrator page as the typed Telegram page, for runtime
/// orchestration: the Creator save administrator reads.
///
/// # Errors
/// Rejects an invalid locator before admission, and returns
/// [`PlatformReadError`] for every later local outcome or Telegram failure.
pub(crate) async fn administrators(
    platform: &LocalPlatform,
    operator_account_id: &str,
    locator: &TgCommunityLocator,
) -> Result<TgPublicAdministrators, PlatformReadError> {
    validate_locator(locator)?;
    telegram_read(
        platform,
        operator_account_id,
        None,
        async |account: &TgAccount| account.community_admins(locator).await,
        onecrew_tg::community_read_evidence,
    )
    .await
}

/// Telegram's own locator validation, before admission or network access.
fn validate_locator(locator: &TgCommunityLocator) -> Result<(), PlatformReadError> {
    locator
        .validate()
        .map_err(|_| PlatformReadError::InputInvalid("Telegram community locator is invalid"))
}

#[cfg(test)]
mod tests;
