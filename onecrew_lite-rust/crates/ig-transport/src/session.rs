//! Instagram private-API session: the persisted credential/device bundle the
//! transport replays on every request. Field layout mirrors instagrapi's
//! `get_settings()` so a session imported once (from a browser `sessionid`) can
//! be stored in the vault and reused without re-importing.

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::device::{DeviceSettings, Uuids};
use crate::error::IgError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizationData {
    pub ds_user_id: String,
    pub sessionid: String,
    pub should_use_header_over_cookies: bool,
}

/// The 家宽-aligned geo signals for a session. instagrapi advises setting
/// `country`/`locale`/`timezone_offset` to the proxy's region so the device geo
/// matches the login/operating IP; we derive these from the egress at import.
#[derive(Debug, Clone)]
pub struct IgGeo {
    /// ISO country of the egress (e.g. `US`, `DE`).
    pub country: String,
    /// App locale, `<lang>_<REGION>` (e.g. `en_US`).
    pub locale: String,
    /// UTC offset in seconds (DST-aware) of the egress timezone.
    pub timezone_offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgSession {
    pub uuids: Uuids,
    pub device_settings: DeviceSettings,
    pub user_agent: String,
    #[serde(default)]
    pub cookies: BTreeMap<String, String>,
    pub authorization_data: AuthorizationData,
    #[serde(default)]
    pub mid: Option<String>,
    #[serde(default)]
    pub ig_www_claim: Option<String>,
    /// `IG-U-RUR` routing token, captured from the server's `ig-set-ig-u-rur`
    /// response header (instagrapi does the same; a stale/absent one is dropped).
    #[serde(default)]
    pub ig_u_rur: Option<String>,
    #[serde(default = "default_country")]
    pub country: String,
    #[serde(default = "default_locale")]
    pub locale: String,
    #[serde(default)]
    pub timezone_offset: i64,
}

impl IgSession {
    /// Build a session from the **full** set of Instagram cookies extracted from
    /// the browser login. instagrapi keeps the whole cookie jar, not just
    /// `sessionid`: `rur`/`shbid`/`shbts`/`mid`/`csrftoken` are the real
    /// per-account routing/shard values Instagram issued to that session, and are
    /// replayed via the `Cookie` header (instagrapi `init()` loads the jar the same
    /// way). `ds_user_id` comes from its cookie (or the leading digits of
    /// `sessionid`); `X-MID` is seeded from the `mid` cookie; device ids are
    /// generated once.
    pub fn from_browser_cookies(cookies: &BTreeMap<String, String>) -> Result<Self, IgError> {
        let sessionid = cookies
            .get("sessionid")
            .map(|value| value.trim().to_string())
            .filter(|value| value.len() >= 30)
            .ok_or_else(|| IgError::Session("invalid Instagram sessionid".into()))?;
        let ds_user_id = cookies
            .get("ds_user_id")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| sessionid.chars().take_while(char::is_ascii_digit).collect());
        if ds_user_id.is_empty() {
            return Err(IgError::Session("invalid Instagram sessionid".into()));
        }
        let device_settings = DeviceSettings::default();
        let locale = default_locale();
        let user_agent = device_settings.user_agent(&locale);
        let mid = cookies
            .get("mid")
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(Self {
            uuids: Uuids::generate(),
            device_settings,
            user_agent,
            cookies: cookies.clone(),
            authorization_data: AuthorizationData {
                ds_user_id,
                sessionid,
                should_use_header_over_cookies: true,
            },
            mid,
            ig_www_claim: None,
            ig_u_rur: None,
            country: default_country(),
            locale,
            timezone_offset: 0,
        })
    }

    /// Align the session's geo signals to the profile's 家宽 egress. The locale is
    /// embedded in the `User-Agent`, so it is rebuilt. Mirrors instagrapi's
    /// `set_country` / `set_locale` / `set_timezone_offset`.
    pub fn apply_geo(&mut self, geo: &IgGeo) {
        if !geo.country.is_empty() {
            self.country = geo.country.clone();
        }
        if !geo.locale.is_empty() {
            self.locale = geo.locale.clone();
            self.user_agent = self.device_settings.user_agent(&geo.locale);
        }
        self.timezone_offset = geo.timezone_offset;
    }

    /// `Authorization` header value: `Bearer IGT:2:<base64(authorization_data)>`.
    pub fn authorization(&self) -> String {
        let json = serde_json::to_string(&self.authorization_data).unwrap_or_default();
        format!("Bearer IGT:2:{}", STANDARD.encode(json))
    }

    pub fn user_id(&self) -> &str {
        &self.authorization_data.ds_user_id
    }

    pub fn to_json(&self) -> Result<String, IgError> {
        serde_json::to_string(self).map_err(|error| IgError::Session(error.to_string()))
    }

    pub fn from_json(raw: &str) -> Result<Self, IgError> {
        serde_json::from_str(raw).map_err(|error| IgError::Session(error.to_string()))
    }
}

fn default_locale() -> String {
    "en_US".into()
}

fn default_country() -> String {
    "US".into()
}
