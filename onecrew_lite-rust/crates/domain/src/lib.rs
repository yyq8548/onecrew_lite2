//! Shared local-account identities and secret-bearing credential models.
//!
//! The crate defines platform-neutral values passed between the vault,
//! application runtimes, and provider adapters. It owns no persistence,
//! provider I/O, or public adapter projection.

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    X,
    Instagram,
    Telegram,
}

/// Non-account credentials used by service integrations. These deliberately do
/// not appear in [`Platform`] or [`LocalProfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceProvider {
    Youtube,
    Resend,
    CreatorSync,
}

impl ServiceProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Youtube => "youtube",
            Self::Resend => "resend",
            Self::CreatorSync => "creator_sync",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreatorSyncClientPlatform {
    Macos,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CreatorSyncCredentialStatus {
    Active,
    Revoked,
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatorSyncProtocol {
    #[serde(rename = "reacher.creator_sync")]
    ReacherCreatorSync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatorSyncCapability {
    #[serde(rename = "creator_snapshots:write_chunked_quarantine")]
    WriteChunkedQuarantine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncBundleProfile {
    PortablePublic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorSyncAcceptedBundle {
    pub format: String,
    pub format_version: u64,
    pub profile: CreatorSyncBundleProfile,
    pub max_snapshot_bytes: u64,
    pub max_chunk_bytes: u64,
    pub max_chunks: u64,
    pub max_namespace_staged_bytes: u64,
    pub max_access_user_staged_bytes: u64,
}

/// Safe Worker identity/capability and local lifecycle metadata stored beside
/// the encrypted Creator Sync bearer. Pairing codes, PKCE verifiers, browser
/// state, and admission state are not part of this persistence shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreatorSyncCredentialMetadata {
    pub installation_id: Uuid,
    pub credential_id: Uuid,
    pub access_user_id: String,
    pub access_device_id: String,
    pub source_instance_id: Uuid,
    pub access_pairing_id: String,
    pub client_platform: CreatorSyncClientPlatform,
    pub app_version: String,
    pub status: CreatorSyncCredentialStatus,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub last_seen_at: Option<u64>,
    pub revoked_at: Option<u64>,
    pub protocol: CreatorSyncProtocol,
    pub protocol_version: u64,
    pub capabilities: [CreatorSyncCapability; 1],
    pub accepted_bundle: CreatorSyncAcceptedBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceCredential {
    pub id: Uuid,
    pub provider: ServiceProvider,
    pub label: String,
    pub metadata: serde_json::Value,
    pub disabled: bool,
    pub cooldown_until: Option<u64>,
    pub last_used_at: Option<u64>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Encrypted payload for one service credential. The type intentionally has
/// no `Debug`, `Serialize`, or public field surface so secrets cannot be
/// accidentally returned by UI/MCP DTOs or logs.
pub struct ServiceCredentialSecret {
    value: Zeroizing<String>,
}

impl ServiceCredentialSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Zeroizing::new(value.into()),
        }
    }

    pub fn from_zeroizing(value: Zeroizing<String>) -> Self {
        Self { value }
    }

    /// Access is limited to provider composition code after a Vault read.
    pub fn expose_secret(&self) -> &str {
        &self.value
    }

    pub fn into_zeroizing(mut self) -> Zeroizing<String> {
        Zeroizing::new(std::mem::take(&mut *self.value))
    }
}

impl Zeroize for ServiceCredentialSecret {
    fn zeroize(&mut self) {
        self.value.zeroize();
    }
}

impl ZeroizeOnDrop for ServiceCredentialSecret {}

#[cfg(test)]
mod service_credential_secret_tests {
    use super::ServiceCredentialSecret;
    use zeroize::{ZeroizeOnDrop, Zeroizing};

    fn assert_zeroize_on_drop<T: ZeroizeOnDrop>() {}

    #[test]
    fn ownership_conversions_keep_the_allocation_under_zeroizing_ownership() {
        assert_zeroize_on_drop::<ServiceCredentialSecret>();
        let source = String::from("generated-sensitive-value");
        let allocation = source.as_ptr();
        let secret = ServiceCredentialSecret::new(source);
        assert_eq!(secret.expose_secret().as_ptr(), allocation);

        let moved: Zeroizing<String> = secret.into_zeroizing();
        assert_eq!(moved.as_ptr(), allocation);
        let wrapped = ServiceCredentialSecret::from_zeroizing(moved);
        assert_eq!(wrapped.expose_secret().as_ptr(), allocation);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProfile {
    pub id: Uuid,
    pub platform: Platform,
    /// Stable provider-side account identifier. Usernames are display labels
    /// and may change without changing this identity.
    pub provider_account_id: String,
    pub username: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSecret {
    /// Stable provider-side account identifier validated by `who_am_i`.
    /// This is duplicated in the public profile metadata so callers can select
    /// an account without decrypting credentials; both values are written
    /// atomically by the vault.
    pub provider_account_id: String,
    /// Login cookies extracted (and decrypted) from this profile's Chrome
    /// `Cookies` store (X / Instagram). Reused by the API clients (wreq).
    /// Empty for Telegram.
    pub cookies: Vec<BrowserCookie>,
    pub x_chat_passcode: Option<String>,
    pub instagram_session_settings: Option<String>,
    /// Telegram MTProto credentials and session (phone login, not browser).
    pub telegram_api_id: Option<i32>,
    pub telegram_api_hash: Option<String>,
    pub telegram_session: Option<String>,
    /// User-given (or random) profile name, set at creation and stable even
    /// before the platform @handle is known. For Telegram it is the connect
    /// flow's own label rather than a user-chosen name.
    #[serde(default)]
    pub profile_name: String,
    /// This profile's isolated Chrome `user-data-dir` (under app_data_dir/
    /// `profiles/{id}`). Empty for Telegram (phone login, no browser).
    #[serde(default)]
    pub browser_profile_dir: String,
    /// Short human-readable fingerprint summary for display (e.g. the aligned
    /// browser · language · timezone). Empty for Telegram / when unset.
    #[serde(default)]
    pub fingerprint_seed: String,
    /// Residential proxy this profile routes traffic through (家宽). `default`
    /// so profiles stored before this field load fine (as a direct connection).
    #[serde(default)]
    pub proxy: Option<onecrew_proxy::ProxyConfig>,
    /// The proxy's *actual detected egress* (e.g. `104.28.16.5 · US`), probed at
    /// login / proxy-config time — the real exit, not the gateway host:port you
    /// typed (which for a local forwarder is `127.0.0.1:...`). Shown in the list.
    #[serde(default)]
    pub proxy_egress: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
}
