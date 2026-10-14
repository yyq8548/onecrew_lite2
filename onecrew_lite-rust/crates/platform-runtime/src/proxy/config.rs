//! Runtime-owned proxy carrier for inbound peers.
//!
//! The MCP connection start inputs and the Web entries that set or probe a
//! profile proxy, configure an in-app browser or Telegram flow, and open the
//! Telegram credentials browser decode this carrier instead of the proxy
//! crate's value model, so a change to that model stops at the mappings below.
//! Its wire shape equals the value model's: the doc comment and serde
//! attributes feed the published MCP input schemas and their revisions, so
//! they stay verbatim. Like the other secret-bearing connection inputs, the
//! carrier is deserialize-only.

use schemars::JsonSchema;
use serde::Deserialize;

// No doc comment here or on the fields: a description would enter the inlined
// schema and change the published input schema revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Http,
    Https,
    Socks5,
}

/// Residential (家宽) proxy a profile routes all of its outbound platform
/// traffic through. Persisted (with credentials) inside the encrypted profile
/// secret; `None` there means a direct connection (this machine's IP).
#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    // `skip_serializing_if` is inert for this deserialize-only type, but
    // schemars reads it and omits `default`, as the published schema does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl std::fmt::Debug for ProxyConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProxyConfig")
            .field("kind", &self.kind)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &self.username)
            .field("password", &self.password.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl From<ProxyKind> for onecrew_proxy::ProxyKind {
    fn from(kind: ProxyKind) -> Self {
        match kind {
            ProxyKind::Http => Self::Http,
            ProxyKind::Https => Self::Https,
            ProxyKind::Socks5 => Self::Socks5,
        }
    }
}

impl From<ProxyConfig> for onecrew_proxy::ProxyConfig {
    fn from(proxy: ProxyConfig) -> Self {
        let ProxyConfig {
            kind,
            host,
            port,
            username,
            password,
        } = proxy;
        Self {
            kind: kind.into(),
            host,
            port,
            username,
            password,
        }
    }
}

// Runtime flows only map inbound carriers to the value model; the reverse
// mappings exist so owner tests can prove the field-by-field round trip.
#[cfg(test)]
impl From<onecrew_proxy::ProxyKind> for ProxyKind {
    fn from(kind: onecrew_proxy::ProxyKind) -> Self {
        match kind {
            onecrew_proxy::ProxyKind::Http => Self::Http,
            onecrew_proxy::ProxyKind::Https => Self::Https,
            onecrew_proxy::ProxyKind::Socks5 => Self::Socks5,
        }
    }
}

#[cfg(test)]
impl From<onecrew_proxy::ProxyConfig> for ProxyConfig {
    fn from(proxy: onecrew_proxy::ProxyConfig) -> Self {
        let onecrew_proxy::ProxyConfig {
            kind,
            host,
            port,
            username,
            password,
        } = proxy;
        Self {
            kind: kind.into(),
            host,
            port,
            username,
            password,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ProxyConfig, ProxyKind};

    fn carrier(kind: ProxyKind, username: Option<&str>, password: Option<&str>) -> ProxyConfig {
        ProxyConfig {
            kind,
            host: "proxy.example".into(),
            port: 1080,
            username: username.map(str::to_owned),
            password: password.map(str::to_owned),
        }
    }

    #[test]
    fn carrier_maps_every_field_to_the_value_model_and_back() {
        let kinds = [
            (ProxyKind::Http, onecrew_proxy::ProxyKind::Http),
            (ProxyKind::Https, onecrew_proxy::ProxyKind::Https),
            (ProxyKind::Socks5, onecrew_proxy::ProxyKind::Socks5),
        ];
        let credentials = [
            (None, None),
            (Some("proxy-user"), None),
            (None, Some("proxy-password")),
            (Some("proxy-user"), Some("proxy-password")),
        ];
        for (kind, value_kind) in kinds {
            for (username, password) in credentials {
                let inbound = carrier(kind, username, password);
                let value = onecrew_proxy::ProxyConfig::from(inbound.clone());
                assert_eq!(
                    value,
                    onecrew_proxy::ProxyConfig {
                        kind: value_kind,
                        host: "proxy.example".into(),
                        port: 1080,
                        username: username.map(str::to_owned),
                        password: password.map(str::to_owned),
                    }
                );
                assert_eq!(ProxyConfig::from(value), inbound);
            }
        }
    }

    #[test]
    fn carrier_debug_hides_the_password_like_the_value_model() {
        let inbound = carrier(
            ProxyKind::Socks5,
            Some("proxy-user"),
            Some("proxy-password"),
        );
        let rendered = format!("{inbound:?}");
        assert_eq!(
            rendered,
            r#"ProxyConfig { kind: Socks5, host: "proxy.example", port: 1080, username: Some("proxy-user"), password: Some("[REDACTED]") }"#
        );
        assert!(!rendered.contains("proxy-password"));
        assert_eq!(
            rendered,
            format!("{:?}", onecrew_proxy::ProxyConfig::from(inbound))
        );
    }
}
