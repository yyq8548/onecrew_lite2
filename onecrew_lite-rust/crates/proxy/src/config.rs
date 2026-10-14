use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Http,
    Https,
    Socks5,
}

impl ProxyKind {
    pub fn scheme(self) -> &'static str {
        match self {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5",
        }
    }

    pub fn is_socks5(self) -> bool {
        matches!(self, ProxyKind::Socks5)
    }
}

/// Residential (家宽) proxy a profile routes all of its outbound platform
/// traffic through. Persisted (with credentials) inside the encrypted profile
/// secret; `None` there means a direct connection (this machine's IP).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProxyConfig {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
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

impl ProxyConfig {
    /// Proxy URL for the HTTP client, e.g. `http://user:pass@host:port`
    /// (credentials percent-encoded), or `scheme://host:port` when anonymous.
    pub fn to_url(&self) -> String {
        let scheme = self.kind.scheme();
        match self.username.as_deref().filter(|user| !user.is_empty()) {
            Some(user) => format!(
                "{scheme}://{}:{}@{}:{}",
                encode_userinfo(user),
                encode_userinfo(self.password.as_deref().unwrap_or("")),
                self.host,
                self.port
            ),
            None => format!("{scheme}://{}:{}", self.host, self.port),
        }
    }

    /// Proxy URL for Telegram (grammers), which speaks **SOCKS5 only** — no
    /// HTTP-proxy path exists in grammers. Callers must treat `None` as an
    /// unsupported configuration, never as permission to connect directly.
    pub fn to_socks5_url(&self) -> Option<String> {
        self.kind.is_socks5().then(|| self.to_url())
    }

    /// Short host:port label for the UI (no credentials).
    pub fn endpoint_label(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Percent-encode a proxy username/password for safe embedding in the URL
/// userinfo (creds can contain `@`, `:`, `/`, etc.).
fn encode_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}
