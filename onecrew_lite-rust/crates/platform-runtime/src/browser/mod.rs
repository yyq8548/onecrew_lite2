//! The X / Instagram account flow: create an isolated Chrome profile, route it
//! through the profile's 家宽 (a loopback forwarder that injects the upstream
//! credentials), let the user log in, then extract & decrypt the profile's own
//! cookies. Nothing is persisted until a login actually succeeds (mirrors the
//! Telegram pending-login pattern), so failed attempts leave no orphans.
//!
//! We drive the user's *own installed Chrome* (its real fingerprint), not a
//! bundled/anti-detect build — see the fingerprint-isolation analysis for why
//! that's sufficient here (the browser is only used for the one-time login;
//! operation goes through the wreq API client).
//!
//! The generic create → 家宽 → login → extract lifecycle lives here; the
//! platform-specific bits (login URL, cookie hosts, and turning the extracted
//! cookies into a validated [`ProfileSecret`]) are a [`PlatformBrowser`] impl per
//! platform in the sibling modules. Telegram doesn't use this flow — it only
//! borrows a throwaway 家宽 Chrome to read API credentials.

use std::path::PathBuf;
use std::process::Child;

use async_trait::async_trait;
use icu_locale::{LanguageIdentifier, LocaleExpander};
use onecrew_chrome::Fingerprint;
use onecrew_local_domain::{BrowserCookie, Platform, ProfileSecret};
use onecrew_proxy::Forwarder;
use uuid::Uuid;

mod capability;
mod instagram;
mod lifecycle;
mod telegram;
mod x;

#[cfg(test)]
pub(crate) use capability::FixedBrowserHost;
pub use capability::{
    BrowserCapabilityCode, BrowserCapabilityState, BrowserCompletionNotice, BrowserCookieProvider,
    BrowserStatus, ChromeChannel, ChromeInstallScope, DesktopOs,
};
pub(crate) use capability::{BrowserHostProbe, RealBrowserHost};
pub(crate) use telegram::TgCredsBrowser;

/// Per-platform browser-login behavior: where to log in, which cookies to read,
/// and how to turn them into a validated account + [`ProfileSecret`].
#[async_trait]
pub(crate) trait PlatformBrowser: Send + Sync {
    /// The URL Chrome opens for the user to log in.
    fn login_url(&self) -> &'static str;
    /// Cookie host suffixes to extract from the logged-in profile.
    fn cookie_hosts(&self) -> &'static [&'static str];
    /// Validate the extracted cookies against the platform API (through the same
    /// 家宽) and produce the signed-in `(@username, secret)`. `base` carries the
    /// shared secret fields; `pending` exposes the fingerprint + any X passcode.
    async fn finalize(
        &self,
        pending: &PendingBrowserLogin,
        cookies: Vec<BrowserCookie>,
        proxy_url: &str,
        base: ProfileSecret,
    ) -> Result<(String, String, ProfileSecret), String>;
}

/// Resolve the browser-login handler for a platform (Telegram has none).
fn platform_browser(platform: Platform) -> Result<Box<dyn PlatformBrowser>, String> {
    match platform {
        Platform::X => Ok(Box::new(x::XBrowser)),
        Platform::Instagram => Ok(Box::new(instagram::InstagramBrowser)),
        Platform::Telegram => Err("Telegram 不走浏览器登录流程".into()),
    }
}

/// A Chrome login in progress: the isolated profile, its 家宽, and (once the
/// browser is opened) the live forwarder + browser process.
pub(crate) struct PendingBrowserLogin {
    platform: Platform,
    name: String,
    profile_dir: PathBuf,
    proxy: onecrew_proxy::ProxyConfig,
    x_chat_passcode: Option<String>,
    /// The 家宽-aligned soft fingerprint (timezone / language), decided at
    /// open-login and reused to record a display summary at extract.
    fingerprint: Fingerprint,
    forwarder: Option<Forwarder>,
    child: Option<Child>,
    /// Reconnect always runs against a private copy. The selected account's
    /// persistent Chrome directory is replaced only after identity validation.
    reconnect_target: Option<ReconnectBrowserTarget>,
}

#[derive(Clone)]
struct ReconnectBrowserTarget {
    profile_id: Uuid,
    /// Stable identity captured from the selected vault record when the
    /// server creates this reconnect token. Callers never supply it.
    provider_account_id: String,
    persistent_profile_dir: PathBuf,
}

/// Returned by the crate-internal `LocalPlatform::browser_create` so the connection flow can
/// drive the remaining browser steps.
#[derive(Debug, Clone)]
pub(crate) struct BrowserLoginCreate {
    pub(crate) token: String,
}

/// Derive a representative `Accept-Language` for a 家宽 egress country (ISO
/// alpha-2) using **CLDR likely-subtags** (via `icu_locale`) — the region's
/// primary language, covering every country with no hand-maintained table
/// (e.g. `DE`→`de`, `JP`→`ja`, `SA`→`ar`). Builds `<lang>-<REGION>,<lang>;q=0.9`
/// plus an English fallback when the primary language isn't English. Returns
/// `None` for an empty/unresolvable region (then the browser keeps its default).
///
/// Timezone is NOT derived here — it comes from the IP's own geolocation (see
/// [`crate::proxy`]), which is accurate even for multi-timezone countries.
/// Language is a country-level signal with no user preference to read for a
/// fresh profile, so this is a best-effort default (multi-language regions get
/// the CLDR-dominant one).
pub(crate) fn language_for_country(country: &str) -> Option<String> {
    let region = country.trim().to_uppercase();
    if region.is_empty() {
        return None;
    }
    let mut id: LanguageIdentifier = format!("und-{region}").parse().ok()?;
    let _ = LocaleExpander::new_common().maximize(&mut id);
    let lang = id.language.to_string();
    if lang.is_empty() || lang == "und" {
        return None;
    }
    let mut header = format!("{lang}-{region},{lang};q=0.9");
    if lang != "en" {
        header.push_str(",en;q=0.8");
    }
    Some(header)
}
