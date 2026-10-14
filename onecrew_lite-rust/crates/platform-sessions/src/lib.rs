//! Shared adapter from session material to platform facades.
//!
//! Platform crates own transport/protocol behavior; this crate is the only
//! place that turns session material into a platform session. It restores a
//! selected profile from the vault, and it builds a session from material a
//! caller already holds with the egress that caller chose: a connection check's
//! probe route, a browser login's captured cookies, a profile refresh. Creator
//! workflows, toolkit functions, and account flows all use it, so cookie and
//! session reconstruction and Telegram's SOCKS5 egress rule are not duplicated.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use onecrew_dm_contracts::DirectMessageProvider;
use onecrew_ig::{IgAccount, IgAccountSource, IgDirectMessages};
use onecrew_local_domain::{BrowserCookie, LocalProfile, Platform, ProfileSecret};
use onecrew_local_vault::LocalVault;
use onecrew_proxy::ProxyConfig;
use onecrew_tg::{TgAccount, TgAccountSource, TgDirectMessages};
use onecrew_x::{XAccount, XAccountSource, XChatPasscode, XDirectMessages, XSession};
use uuid::Uuid;

/// One live X session plus the fingerprint of the secret material it was
/// built from.
struct CachedXAccount {
    fingerprint: u64,
    account: XAccount,
}

#[derive(Clone)]
pub struct PlatformSessions {
    vault: LocalVault,
    /// Live X sessions, kept for the app's lifetime and shared by every
    /// clone of this adapter. Rebuilding an `XAccount` per call threw away
    /// the connection pool AND the `x-client-transaction-id` key material
    /// whose own initializer documents that callers must cache it for the
    /// app's lifetime — two extra network round trips plus an HTML parse on
    /// every single X command. An entry leaves when its material changes
    /// (fingerprint) or its account is disconnected ([`Self::forget_x_account`]).
    x_sessions: Arc<Mutex<HashMap<Uuid, CachedXAccount>>>,
    /// How many times [`Self::forget_x_account`] ran. A session built from a
    /// vault read observed before a forget is returned but not cached, so a
    /// disconnect racing an in-flight build cannot leave the account's cookies
    /// behind.
    x_session_forgets: Arc<AtomicU64>,
}

/// Short-lived Telegram connection material prepared on the blocking pool.
/// Fields deliberately remain private and this type has no diagnostic view.
pub struct TelegramPreparedSession {
    api_id: i32,
    api_hash: String,
    session: String,
    proxy: String,
}

impl TelegramPreparedSession {
    /// The Telegram API id the session was prepared with.
    pub fn api_id(&self) -> i32 {
        self.api_id
    }

    /// The Telegram API hash the session was prepared with, for a caller that
    /// stores the session again. It is a credential and never enters a
    /// diagnostic.
    pub fn api_hash(&self) -> &str {
        &self.api_hash
    }

    /// Open the prepared Telegram session without synchronous vault access.
    ///
    /// # Errors
    /// Returns the provider connection error; public callers must sanitize it.
    pub async fn connect(self) -> Result<TgAccount, onecrew_tg::TgError> {
        TgAccount::from_session(self.api_id, self.api_hash, &self.session, Some(self.proxy)).await
    }
}

/// Why session material could not become a platform session.
///
/// Every message is a fixed sentence. No cookie, session JSON, API hash, or
/// proxy URL reaches it, and facade construction diagnostics are dropped
/// because they can quote the proxy URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMaterialError {
    /// The X cookie jar has no `auth_token` cookie.
    MissingXAuthToken,
    /// The X cookie jar has no `ct0` cookie.
    MissingXCsrfToken,
    /// The X facade rejected the cookies or the proxy URL.
    XSessionRejected,
    /// There is no Instagram session JSON, or it is empty.
    MissingInstagramSession,
    /// The Instagram facade rejected the session JSON or the proxy URL.
    InstagramSessionRejected,
    /// There is no Telegram API id.
    MissingTelegramApiId,
    /// There is no Telegram API hash.
    MissingTelegramApiHash,
    /// There is no Telegram session, or it is empty.
    MissingTelegramSession,
    /// The egress fails [`telegram_socks5_proxy_url`].
    TelegramProxy(TelegramProxyError),
}

impl fmt::Display for SessionMaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingXAuthToken => "profile is missing X auth_token cookie",
            Self::MissingXCsrfToken => "profile is missing X ct0 cookie",
            Self::XSessionRejected => "X session could not be built from its cookies and proxy",
            Self::MissingInstagramSession => "profile has no Instagram session",
            Self::InstagramSessionRejected => {
                "Instagram session could not be restored with its proxy"
            }
            Self::MissingTelegramApiId => "profile is missing Telegram api_id",
            Self::MissingTelegramApiHash => "profile is missing Telegram api_hash",
            Self::MissingTelegramSession => "profile has no Telegram session",
            Self::TelegramProxy(TelegramProxyError::Missing) => {
                "Telegram profile is missing its required SOCKS5 proxy"
            }
            Self::TelegramProxy(TelegramProxyError::NotSocks5) => {
                "Telegram profile requires a SOCKS5 proxy"
            }
        })
    }
}

impl std::error::Error for SessionMaterialError {}

/// Why an egress fails Telegram's SOCKS5 rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramProxyError {
    /// No proxy is configured.
    Missing,
    /// The proxy is HTTP or HTTPS.
    NotSocks5,
}

impl fmt::Display for TelegramProxyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Missing => "Telegram requires a SOCKS5 proxy and none is configured",
            Self::NotSocks5 => "Telegram requires a SOCKS5 proxy, not an HTTP or HTTPS proxy",
        })
    }
}

impl std::error::Error for TelegramProxyError {}

impl PlatformSessions {
    pub fn new(vault: LocalVault) -> Self {
        Self {
            vault,
            x_sessions: Arc::new(Mutex::new(HashMap::new())),
            x_session_forgets: Arc::new(AtomicU64::new(0)),
        }
    }

    /// One live X session plus its sealed X Chat passcode.
    ///
    /// The passcode is read fresh every call and never cached: it is a durable
    /// secret whose in-memory lifetime stays as short as before, and the seal
    /// zeroizes it as soon as the caller drops it. A profile with no configured
    /// passcode answers `None`, which the X provider turns into a typed
    /// capability reason rather than an empty passcode it would try to unlock
    /// with.
    pub fn x(&self, profile_id: &str) -> Result<(XAccount, Option<XChatPasscode>), String> {
        let id = parse_profile_id(profile_id)?;
        // Observed before the vault read: a forget that lands after this point
        // keeps the session built from that read out of the cache.
        let forgets = self.x_session_forgets.load(Ordering::SeqCst);
        let (profile, secret) = self.stored_profile(id)?;
        let passcode = passcode(&secret);
        let fingerprint = x_session_fingerprint(&secret);
        let account = cached_x_account(
            &self.x_sessions,
            id,
            fingerprint,
            || self.x_session_forgets.load(Ordering::SeqCst) == forgets,
            || x_account(profile.platform, &secret),
        )?;
        Ok((account, passcode))
    }

    pub fn instagram(&self, profile_id: &str) -> Result<IgAccount, String> {
        let id = parse_profile_id(profile_id)?;
        let (profile, secret) = self.stored_profile(id)?;
        instagram_account(profile.platform, &secret)
    }

    /// Open a stored Telegram profile. The vault read runs on the blocking
    /// pool; only the connect runs on the caller's async worker.
    pub async fn telegram(&self, profile_id: &str) -> Result<TgAccount, String> {
        self.open_blocking(profile_id, Self::prepare_telegram)
            .await?
            .connect()
            .await
            .map_err(|error| error.to_string())
    }

    /// Read and validate stored Telegram material before asynchronous connect.
    /// Callers with an async executor run this bounded vault segment on its
    /// blocking pool after connection admission.
    ///
    /// # Errors
    /// Returns a local profile, secret or proxy validation diagnostic.
    pub fn prepare_telegram(&self, profile_id: &str) -> Result<TelegramPreparedSession, String> {
        let id = parse_profile_id(profile_id)?;
        let (profile, secret) = self.stored_profile(id)?;
        telegram_session_material(profile.platform, &secret)
    }

    /// Drop the live X session cached for one profile.
    ///
    /// Account disconnect calls this once the stored credential is gone, so the
    /// session's cookies do not outlive the account in process memory. Changed
    /// material needs no call: its fingerprint already replaces the entry. An id
    /// that names no cached session changes nothing. A session still being
    /// built from a vault read observed before this call is not cached.
    pub fn forget_x_account(&self, profile_id: &str) {
        if let Ok(id) = Uuid::parse_str(profile_id) {
            let mut sessions = self.x_sessions.lock().expect("x session cache");
            // Counted under the cache lock that an insert also takes, so an
            // in-flight build either sees this forget or is removed by it.
            self.x_session_forgets.fetch_add(1, Ordering::SeqCst);
            sessions.remove(&id);
        }
    }

    /// Whether a live X session is cached for one profile. Owner-test evidence
    /// only.
    #[cfg(feature = "test-support")]
    pub fn caches_x_account(&self, profile_id: &str) -> bool {
        Uuid::parse_str(profile_id).is_ok_and(|id| {
            self.x_sessions
                .lock()
                .expect("x session cache")
                .contains_key(&id)
        })
    }

    /// The direct-message provider for one stored profile.
    ///
    /// Which provider it is follows from the profile's own platform, so no
    /// caller has to name one. Every provider asks for its session per call,
    /// exactly as each accessor above is called per operation today.
    /// One direct-message provider per platform, chosen by the caller that
    /// already knows which platform the account claims.
    ///
    /// Selecting from the stored profile instead would read the vault here,
    /// decrypting a whole `ProfileSecret` — cookies, the X Chat passcode, the
    /// Telegram session — only to learn the platform, and `ProfileSecret` does
    /// not zeroize on drop. It would also make the platform self-consistent by
    /// construction: the session call below verifies the stored profile against
    /// the platform it was opened for, and that check only means something when
    /// the platform comes from the caller.
    pub fn x_direct_messages(&self, profile_id: &str) -> Arc<dyn DirectMessageProvider> {
        Arc::new(XDirectMessages::new(
            Arc::new(self.clone()),
            profile_id.to_owned(),
        ))
    }

    pub fn instagram_direct_messages(&self, profile_id: &str) -> Arc<dyn DirectMessageProvider> {
        Arc::new(IgDirectMessages::new(
            Arc::new(self.clone()),
            profile_id.to_owned(),
        ))
    }

    pub fn telegram_direct_messages(&self, profile_id: &str) -> Arc<dyn DirectMessageProvider> {
        Arc::new(TgDirectMessages::new(
            Arc::new(self.clone()),
            profile_id.to_owned(),
        ))
    }

    /// The one synchronous vault read and decryption behind every opener.
    fn stored_profile(&self, id: Uuid) -> Result<(LocalProfile, ProfileSecret), String> {
        #[cfg(test)]
        tests::observe_vault_read(id);
        self.vault.get(id).map_err(|error| error.to_string())
    }

    /// Run one vault-reading opener on the blocking pool, so an async session
    /// source never reads or decrypts the vault on its worker. The opener's
    /// value moves out unchanged; a sealed X Chat passcode stays sealed, and a
    /// lost segment drops, and so zeroizes, it on the blocking thread.
    async fn open_blocking<T>(
        &self,
        profile_id: &str,
        open: fn(&Self, &str) -> Result<T, String>,
    ) -> Result<T, String>
    where
        T: Send + 'static,
    {
        let sessions = self.clone();
        let profile_id = profile_id.to_owned();
        tokio::task::spawn_blocking(move || open(&sessions, &profile_id))
            .await
            .map_err(|_| "profile session read was interrupted".to_string())?
    }
}

#[async_trait]
impl XAccountSource for PlatformSessions {
    async fn open_x_account(
        &self,
        profile_id: &str,
    ) -> Result<(XAccount, Option<XChatPasscode>), String> {
        self.open_blocking(profile_id, Self::x).await
    }
}

#[async_trait]
impl IgAccountSource for PlatformSessions {
    async fn open_ig_account(&self, profile_id: &str) -> Result<IgAccount, String> {
        self.open_blocking(profile_id, Self::instagram).await
    }
}

#[async_trait]
impl TgAccountSource for PlatformSessions {
    async fn open_tg_account(&self, profile_id: &str) -> Result<TgAccount, String> {
        self.telegram(profile_id).await
    }
}

/// Build an X session from a cookie jar and the egress the caller chose.
///
/// A stored profile passes its own proxy, a connection check its probe route,
/// and a browser login the proxy it logged in through. `None` connects
/// directly.
///
/// # Errors
/// Returns [`SessionMaterialError`] when `auth_token` or `ct0` is missing, or
/// when the X facade rejects the cookies or the proxy URL.
pub fn x_account_with_proxy(
    cookies: &[BrowserCookie],
    proxy_url: Option<&str>,
) -> Result<XAccount, SessionMaterialError> {
    let cookies: BTreeMap<String, String> = cookies
        .iter()
        .map(|cookie| (cookie.name.clone(), cookie.value.clone()))
        .collect();
    let auth_token = cookies
        .get("auth_token")
        .cloned()
        .ok_or(SessionMaterialError::MissingXAuthToken)?;
    let csrf_token = cookies
        .get("ct0")
        .cloned()
        .ok_or(SessionMaterialError::MissingXCsrfToken)?;
    let session = XSession::new(auth_token, csrf_token, cookies)
        .map_err(|_| SessionMaterialError::XSessionRejected)?;
    XAccount::new(session, proxy_url).map_err(|_| SessionMaterialError::XSessionRejected)
}

/// Build an Instagram session from its stored session JSON and the egress the
/// caller chose. `None` connects directly.
///
/// # Errors
/// Returns [`SessionMaterialError`] when the session JSON is absent or empty,
/// or when the Instagram facade rejects it or the proxy URL.
pub fn instagram_account_with_proxy(
    session_json: Option<&str>,
    proxy_url: Option<&str>,
) -> Result<IgAccount, SessionMaterialError> {
    let session_json = session_json
        .filter(|value| !value.is_empty())
        .ok_or(SessionMaterialError::MissingInstagramSession)?;
    IgAccount::from_session_json(session_json, proxy_url)
        .map_err(|_| SessionMaterialError::InstagramSessionRejected)
}

/// Prepare a Telegram session from its API credentials, session JSON, and the
/// egress the caller chose, which must pass [`telegram_socks5_proxy_url`].
///
/// # Errors
/// Returns [`SessionMaterialError`] for a missing API id or hash, an absent or
/// empty session, or an egress that fails the SOCKS5 rule, checked in that
/// order.
pub fn telegram_session_with_proxy(
    api_id: Option<i32>,
    api_hash: Option<&str>,
    session_json: Option<&str>,
    proxy: Option<&ProxyConfig>,
) -> Result<TelegramPreparedSession, SessionMaterialError> {
    let api_id = api_id.ok_or(SessionMaterialError::MissingTelegramApiId)?;
    let api_hash = api_hash.ok_or(SessionMaterialError::MissingTelegramApiHash)?;
    let session = session_json
        .filter(|value| !value.is_empty())
        .ok_or(SessionMaterialError::MissingTelegramSession)?;
    let proxy = telegram_socks5_proxy_url(proxy).map_err(SessionMaterialError::TelegramProxy)?;
    Ok(TelegramPreparedSession {
        api_id,
        api_hash: api_hash.to_owned(),
        session: session.to_owned(),
        proxy,
    })
}

/// Telegram's egress rule: every Telegram connection leaves through a SOCKS5
/// proxy, because grammers has no HTTP-proxy path, and nothing falls back to a
/// direct connection. Returns the SOCKS5 URL to connect through.
///
/// Login, session opening, connection checks, and proxy configuration all
/// decide Telegram's egress with this one rule.
///
/// # Errors
/// Returns [`TelegramProxyError::Missing`] without a proxy and
/// [`TelegramProxyError::NotSocks5`] for an HTTP or HTTPS proxy.
pub fn telegram_socks5_proxy_url(
    proxy: Option<&ProxyConfig>,
) -> Result<String, TelegramProxyError> {
    proxy
        .ok_or(TelegramProxyError::Missing)?
        .to_socks5_url()
        .ok_or(TelegramProxyError::NotSocks5)
}

fn parse_profile_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| "invalid profile_id".to_string())
}

/// Reuse the live session when the secret material is unchanged; a
/// reconnect, a proxy change, or any other edit to the stored material
/// yields a different fingerprint and therefore a fresh session, so nothing
/// has to remember to invalidate this cache. A disconnect does invalidate it
/// through [`PlatformSessions::forget_x_account`]; `still_current` reports
/// whether such a forget has happened since the caller read the material.
fn cached_x_account(
    cache: &Mutex<HashMap<Uuid, CachedXAccount>>,
    id: Uuid,
    fingerprint: u64,
    still_current: impl FnOnce() -> bool,
    build: impl FnOnce() -> Result<XAccount, String>,
) -> Result<XAccount, String> {
    {
        let sessions = cache.lock().expect("x session cache");
        if let Some(cached) = sessions.get(&id)
            && cached.fingerprint == fingerprint
        {
            // Cloning shares the wreq connection pool and the transaction-id
            // OnceCell behind their Arcs — that reuse is the whole point.
            return Ok(cached.account.clone());
        }
    }
    let account = build()?;
    let mut sessions = cache.lock().expect("x session cache");
    // Checked under the same lock a forget takes: a forget either precedes this
    // check and keeps the session out, or follows the insert and removes it.
    if still_current() {
        sessions.insert(
            id,
            CachedXAccount {
                fingerprint,
                account: account.clone(),
            },
        );
    }
    Ok(account)
}

fn fold_bytes(hash: u64, bytes: &[u8]) -> u64 {
    let mut hash = hash;
    for byte in bytes.iter().chain(std::iter::once(&0u8)) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Everything `x_account` builds a session from, folded into one value:
/// the cookie jar (order-independent) and the egress proxy. Nothing else may
/// enter this, or a cached session could outlive the material it was built
/// from.
fn x_session_fingerprint(secret: &ProfileSecret) -> u64 {
    let mut pairs: Vec<(&str, &str)> = secret
        .cookies
        .iter()
        .map(|cookie| (cookie.name.as_str(), cookie.value.as_str()))
        .collect();
    pairs.sort_unstable();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for (name, value) in pairs {
        hash = fold_bytes(hash, name.as_bytes());
        hash = fold_bytes(hash, value.as_bytes());
    }
    fold_bytes(
        hash,
        secret
            .proxy
            .as_ref()
            .map(|proxy| proxy.to_url())
            .unwrap_or_default()
            .as_bytes(),
    )
}

fn x_account(platform: Platform, secret: &ProfileSecret) -> Result<XAccount, String> {
    if platform != Platform::X {
        return Err("X operation requires an X profile".into());
    }
    let proxy = secret.proxy.as_ref().map(|proxy| proxy.to_url());
    x_account_with_proxy(&secret.cookies, proxy.as_deref()).map_err(|error| error.to_string())
}

fn instagram_account(platform: Platform, secret: &ProfileSecret) -> Result<IgAccount, String> {
    if platform != Platform::Instagram {
        return Err("Instagram operation requires an Instagram profile".into());
    }
    let proxy = secret.proxy.as_ref().map(|proxy| proxy.to_url());
    instagram_account_with_proxy(
        secret.instagram_session_settings.as_deref(),
        proxy.as_deref(),
    )
    .map_err(|error| error.to_string())
}

fn telegram_session_material(
    platform: Platform,
    secret: &ProfileSecret,
) -> Result<TelegramPreparedSession, String> {
    if platform != Platform::Telegram {
        return Err("Telegram operation requires a Telegram profile".into());
    }
    telegram_session_with_proxy(
        secret.telegram_api_id,
        secret.telegram_api_hash.as_deref(),
        secret.telegram_session.as_deref(),
        secret.proxy.as_ref(),
    )
    .map_err(|error| error.to_string())
}

fn passcode(secret: &ProfileSecret) -> Option<XChatPasscode> {
    secret
        .x_chat_passcode
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(|value| XChatPasscode::new(value.as_bytes().to_vec()))
}

#[cfg(test)]
mod tests;
