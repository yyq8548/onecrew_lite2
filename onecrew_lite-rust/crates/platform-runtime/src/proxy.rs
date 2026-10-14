//! Per-profile residential proxy (家宽): store/clear the config, and prove a
//! proxy actually routes by fetching the egress IP through it. Account
//! reachability through a stored proxy belongs to the typed connection check.

use onecrew_local_domain::Platform;
use uuid::Uuid;

use crate::LocalPlatform;

mod config;

pub use config::{ProxyConfig, ProxyKind};

/// Probe a proxy and format its egress as `ip · country` (or just `ip` when the
/// country is unknown) — best-effort, `None` if the probe fails. Stored on the
/// profile so the list shows the real exit rather than the gateway host:port.
pub(crate) async fn proxy_egress_label(proxy: &onecrew_proxy::ProxyConfig) -> Option<String> {
    let info = onecrew_proxy::probe(proxy).await.ok()?;
    if info.country.is_empty() {
        Some(info.ip)
    } else {
        Some(format!("{} · {}", info.ip, info.country))
    }
}

/// Result of a config-time proxy probe: the egress identity plus the 家宽-
/// aligned soft fingerprint (timezone / language) derived from the egress
/// country, so the UI's fingerprint-confirm card can show what the login will
/// present — and `browser_open_login` launches Chrome with the same values.
#[derive(
    Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct ProxyProbe {
    pub ip: String,
    pub country: String,
    pub timezone: Option<String>,
    pub accept_language: Option<String>,
    /// `host:port` label of the proxy that was probed.
    pub endpoint: String,
}

/// Whether a profile may store this egress: a Telegram profile's proxy must pass
/// the SOCKS5 rule every Telegram connection uses. Other platforms accept any
/// proxy, or none.
fn stored_egress_allowed(platform: Platform, proxy: Option<&onecrew_proxy::ProxyConfig>) -> bool {
    platform != Platform::Telegram || onecrew_sessions::telegram_socks5_proxy_url(proxy).is_ok()
}

impl LocalPlatform {
    /// Store (or replace) a profile's residential proxy; all subsequent MCP
    /// requests for this profile route through it. Also records the proxy's real
    /// egress (probed now) so the list shows the exit IP, not the gateway.
    pub async fn set_profile_proxy(&self, id: &str, proxy: ProxyConfig) -> Result<(), String> {
        let proxy = onecrew_proxy::ProxyConfig::from(proxy);
        let id = Uuid::parse_str(id).map_err(|_| "invalid profile id".to_string())?;
        let (profile, mut secret) = self.vault.get(id).map_err(|error| error.to_string())?;
        if !stored_egress_allowed(profile.platform, Some(&proxy)) {
            return Err("Telegram profiles require a SOCKS5 proxy".into());
        }
        // Probe before touching the vault so no lock is held across the await.
        let egress = proxy_egress_label(&proxy).await;
        secret.proxy_egress = egress;
        secret.proxy = Some(proxy);
        self.vault
            .upsert(profile.platform, profile.username.as_str(), &secret)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Remove a profile's proxy (revert to a direct connection).
    pub fn clear_profile_proxy(&self, id: &str) -> Result<(), String> {
        let id = Uuid::parse_str(id).map_err(|_| "invalid profile id".to_string())?;
        let (profile, mut secret) = self.vault.get(id).map_err(|error| error.to_string())?;
        if !stored_egress_allowed(profile.platform, None) {
            return Err(
                "Telegram profiles require a SOCKS5 proxy and cannot use a direct connection"
                    .into(),
            );
        }
        secret.proxy = None;
        secret.proxy_egress = None;
        self.vault
            .upsert(profile.platform, profile.username.as_str(), &secret)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    /// Probe an arbitrary proxy *before* it is saved or used to open a login:
    /// the connection flows call it directly, and the config dialog's "检测家宽"
    /// button reaches it through [`Self::probe_proxy`]. Confirms the tunnel
    /// works and reports the egress IP so you never persist a dead proxy.
    pub(crate) async fn proxy_validate(
        &self,
        proxy: onecrew_proxy::ProxyConfig,
    ) -> Result<ProxyProbe, String> {
        validate_proxy_config(&proxy)?;
        let endpoint = proxy.endpoint_label();
        let info = onecrew_proxy::probe(&proxy)
            .await
            // Do not forward reqwest/proxy parser diagnostics: depending on the
            // upstream implementation they may include credential-bearing URLs.
            .map_err(|_| format!("proxy validation failed for {endpoint}"))?;
        // Timezone comes from the IP's own geolocation (accurate even for
        // multi-timezone countries); language is a country-level default.
        let timezone = Some(info.timezone).filter(|value| !value.is_empty());
        let accept_language = crate::browser::language_for_country(&info.country);
        Ok(ProxyProbe {
            ip: info.ip,
            country: info.country,
            timezone,
            accept_language,
            endpoint,
        })
    }

    /// Config dialog probe: maps the inbound carrier to the value model and
    /// runs the same validation and egress probe as the connection flows.
    pub async fn probe_proxy(&self, proxy: ProxyConfig) -> Result<ProxyProbe, String> {
        self.proxy_validate(proxy.into()).await
    }
}

fn validate_proxy_config(proxy: &onecrew_proxy::ProxyConfig) -> Result<(), String> {
    if proxy.kind == onecrew_proxy::ProxyKind::Https {
        return Err("HTTPS upstream proxies are not supported; use HTTP or SOCKS5".into());
    }
    if proxy.host.trim().is_empty() || proxy.port == 0 {
        return Err("proxy host and port are required".into());
    }
    if proxy
        .username
        .as_deref()
        .is_some_and(|username| username.trim().is_empty())
    {
        return Err("proxy username cannot be empty when provided".into());
    }
    if proxy.password.is_some() && proxy.username.is_none() {
        return Err("proxy password requires a username".into());
    }
    Ok(())
}

#[cfg(test)]
mod decode_tests;

#[cfg(test)]
mod tests {
    use onecrew_local_domain::{Platform, ProfileSecret};

    use super::*;

    #[tokio::test]
    async fn invalid_proxy_is_rejected_before_network_access() {
        let directory = std::env::temp_dir().join(format!("proxy-validate-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        let error = platform
            .proxy_validate(onecrew_proxy::ProxyConfig {
                kind: onecrew_proxy::ProxyKind::Http,
                host: "".into(),
                port: 0,
                username: Some("private-user".into()),
                password: Some("private-password".into()),
            })
            .await
            .unwrap_err();
        assert_eq!(error, "proxy host and port are required");
        assert!(!error.contains("private-user"));
        assert!(!error.contains("private-password"));
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn https_upstream_is_rejected_before_network_access() {
        let directory = std::env::temp_dir().join(format!("proxy-https-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        let error = platform
            .proxy_validate(onecrew_proxy::ProxyConfig {
                kind: onecrew_proxy::ProxyKind::Https,
                host: "proxy.example".into(),
                port: 443,
                username: Some("private-user".into()),
                password: Some("private-password".into()),
            })
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "HTTPS upstream proxies are not supported; use HTTP or SOCKS5"
        );
        assert!(!error.contains("private-user"));
        assert!(!error.contains("private-password"));
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn telegram_profile_proxy_cannot_be_cleared_or_changed_to_non_socks5() {
        let directory = std::env::temp_dir().join(format!("proxy-tg-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        let original_proxy = onecrew_proxy::ProxyConfig {
            kind: onecrew_proxy::ProxyKind::Socks5,
            host: "socks.example".into(),
            port: 1080,
            username: None,
            password: None,
        };
        let profile = platform
            .vault
            .upsert(
                Platform::Telegram,
                "telegram_user",
                &ProfileSecret {
                    provider_account_id: "tg-1".into(),
                    cookies: Vec::new(),
                    x_chat_passcode: None,
                    instagram_session_settings: None,
                    telegram_api_id: Some(1),
                    telegram_api_hash: Some("hash".into()),
                    telegram_session: Some("session".into()),
                    profile_name: "Telegram".into(),
                    browser_profile_dir: String::new(),
                    fingerprint_seed: String::new(),
                    proxy: Some(original_proxy.clone()),
                    proxy_egress: None,
                },
            )
            .unwrap();

        assert_eq!(
            platform
                .clear_profile_proxy(&profile.id.to_string())
                .unwrap_err(),
            "Telegram profiles require a SOCKS5 proxy and cannot use a direct connection"
        );
        assert_eq!(
            platform
                .set_profile_proxy(
                    &profile.id.to_string(),
                    ProxyConfig {
                        kind: ProxyKind::Http,
                        host: "http.example".into(),
                        port: 8080,
                        username: None,
                        password: None,
                    },
                )
                .await
                .unwrap_err(),
            "Telegram profiles require a SOCKS5 proxy"
        );
        let (_, stored) = platform.vault.get(profile.id).unwrap();
        assert_eq!(stored.proxy, Some(original_proxy));
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
