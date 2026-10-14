//! Telegram QR or phone login, plus the persist helper shared with profile
//! refresh. Both are MTProto flows, not the Chrome browser flow used by X/IG.
//! Every login egress passes `onecrew_sessions::telegram_socks5_proxy_url`.

use onecrew_local_domain::LocalProfile;
use onecrew_tg::{LoginCodeDelivery, QrLoginChallenge, TgLogin, TgPassword, TgQrLogin, TgUser};

mod persist;
mod phone;
mod qr;

/// A Telegram login in progress, holding the live connection and credentials
/// between the code/password steps. `proxy` is the 家宽 the login egresses
/// through (SOCKS5-only for grammers) and is persisted on success.
pub(crate) struct PendingTgLogin {
    api_id: i32,
    api_hash: String,
    label: String,
    proxy: Option<onecrew_proxy::ProxyConfig>,
    step: TgLoginStep,
}

/// Rust-only state for one QR login. Telegram's raw token remains inside the
/// transport; the runtime indexes the live connection with an opaque UUID.
pub(crate) struct PendingTgQrLogin {
    api_id: i32,
    api_hash: String,
    proxy: Option<onecrew_proxy::ProxyConfig>,
    login: TgQrLogin,
}

pub(crate) struct TelegramProfileUpsert {
    pub(crate) api_id: i32,
    pub(crate) api_hash: String,
    pub(crate) session_json: String,
    pub(crate) proxy: Option<onecrew_proxy::ProxyConfig>,
    pub(crate) proxy_egress: Option<String>,
    pub(crate) account: TgUser,
    pub(crate) label: String,
}

enum TgLoginStep {
    AwaitingCode(TgLogin),
    AwaitingPassword(TgPassword),
}

// The login results below stay inside the runtime: the account workflow reads
// them and projects delivery and QR material through the runtime carriers in
// `account`, so no peer serializes these transport-typed values.

/// Result of submitting a Telegram login code.
#[derive(Debug, Clone)]
pub(crate) struct TgCodeResult {
    pub(crate) needs_password: bool,
    pub(crate) profile: Option<LocalProfile>,
}

/// Result of requesting Telegram's first login code. The opaque token indexes
/// Rust-only pending state; delivery contains only Telegram's non-secret route
/// metadata.
#[derive(Debug, Clone)]
pub(crate) struct TgLoginStartResult {
    pub(crate) token: String,
    pub(crate) delivery: LoginCodeDelivery,
}

#[derive(Debug, Clone)]
pub(crate) struct TgQrLoginStartResult {
    pub(crate) token: Option<String>,
    pub(crate) needs_password: bool,
    pub(crate) challenge: Option<QrLoginChallenge>,
    pub(crate) profile: Option<LocalProfile>,
}

#[derive(Debug, Clone)]
pub(crate) struct TgQrLoginPollResult {
    pub(crate) needs_password: bool,
    pub(crate) challenge: Option<QrLoginChallenge>,
    pub(crate) profile: Option<LocalProfile>,
}

/// Copy for a login egress that fails the Telegram SOCKS5 rule; profile refresh
/// and the in-app credential handoff reuse it.
pub(crate) const TELEGRAM_SOCKS5_REQUIRED: &str = "Telegram requires a SOCKS5 proxy";

#[cfg(test)]
mod tests {
    use crate::LocalPlatform;

    #[tokio::test]
    async fn telegram_rejects_missing_and_non_socks5_proxy_without_fallback() {
        let directory = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(directory.path()).unwrap();
        let mut errors = vec![
            platform
                .tg_start_qr_login(12_345, "api-hash".into(), None)
                .await
                .unwrap_err(),
            platform
                .tg_start_login(12_345, "api-hash".into(), "+85291234567".into(), None)
                .await
                .unwrap_err(),
        ];
        for kind in [
            onecrew_proxy::ProxyKind::Http,
            onecrew_proxy::ProxyKind::Https,
        ] {
            let proxy = Some(onecrew_proxy::ProxyConfig {
                kind,
                host: "proxy.example".into(),
                port: 8080,
                username: Some("secret-user".into()),
                password: Some("secret-password".into()),
            });
            errors.push(
                platform
                    .tg_start_qr_login(12_345, "api-hash".into(), proxy.clone())
                    .await
                    .unwrap_err(),
            );
            errors.push(
                platform
                    .tg_start_login(12_345, "api-hash".into(), "+85291234567".into(), proxy)
                    .await
                    .unwrap_err(),
            );
        }
        for error in errors {
            assert_eq!(error, "Telegram requires a SOCKS5 proxy");
            assert!(!error.contains("secret-user"));
            assert!(!error.contains("secret-password"));
        }
        // Neither login dialed Telegram, so no pending connection exists.
        assert!(platform.pending_tg.lock().unwrap().is_empty());
        assert!(platform.pending_tg_qr.lock().unwrap().is_empty());
    }
}
