//! Instagram browser-login: keep the *full* cookie jar (rur/shbid/shbts/mid/
//! csrftoken are the real per-account routing values IG issued to this login,
//! replayed via the Cookie header — not just sessionid), align the device geo to
//! the 家宽, and validate via the mobile private API.

use std::collections::BTreeMap;

use async_trait::async_trait;
use onecrew_chrome::Fingerprint;
use onecrew_ig::{IgAccount, IgGeo};
use onecrew_local_domain::{BrowserCookie, ProfileSecret};

use super::{PendingBrowserLogin, PlatformBrowser};

pub(crate) struct InstagramBrowser;

#[async_trait]
impl PlatformBrowser for InstagramBrowser {
    fn login_url(&self) -> &'static str {
        "https://www.instagram.com/accounts/login/"
    }

    fn cookie_hosts(&self) -> &'static [&'static str] {
        &["instagram.com"]
    }

    async fn finalize(
        &self,
        pending: &PendingBrowserLogin,
        cookies: Vec<BrowserCookie>,
        proxy_url: &str,
        base: ProfileSecret,
    ) -> Result<(String, String, ProfileSecret), String> {
        let cookie_map: BTreeMap<String, String> = cookies
            .iter()
            .map(|c| (c.name.clone(), c.value.clone()))
            .collect();
        if !cookie_map.contains_key("sessionid") {
            return Err("缺少 Instagram sessionid".into());
        }
        let geo = ig_geo_from_fingerprint(&pending.fingerprint);
        let (user, session_json) = IgAccount::import(&cookie_map, Some(proxy_url), geo)
            .await
            .map_err(|e| e.to_string())?;
        let secret = ProfileSecret {
            provider_account_id: user.id.clone(),
            instagram_session_settings: Some(session_json),
            ..base
        };
        Ok((user.id, user.username, secret))
    }
}

/// Derive the IG session's geo (locale / country / timezone offset) from the
/// 家宽-aligned browser fingerprint, so the mobile-API device geo matches the
/// login/operating IP (instagrapi advises the same). `locale` / `country` come
/// from the first `Accept-Language` tag (e.g. `en-US` → `en_US` / `US`); the
/// offset is resolved from the IANA timezone. Returns `None` when nothing is
/// aligned, leaving the session's defaults.
fn ig_geo_from_fingerprint(fingerprint: &Fingerprint) -> Option<IgGeo> {
    let tag = fingerprint
        .accept_language
        .as_deref()
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let timezone_offset = fingerprint
        .timezone
        .as_deref()
        .and_then(onecrew_proxy::iana_offset_seconds);
    if tag.is_none() && timezone_offset.is_none() {
        return None;
    }
    let (locale, country) = match tag {
        Some(tag) => (
            tag.replace('-', "_"),
            tag.split('-').nth(1).unwrap_or_default().to_uppercase(),
        ),
        None => (String::new(), String::new()),
    };
    Some(IgGeo {
        country,
        locale,
        timezone_offset: timezone_offset.unwrap_or(0),
    })
}
