//! Telegram QR login: start / poll / cancel over the passive `updateLoginToken`
//! stream, re-issuing the QR when Telegram expires the short-lived token. A QR
//! login that hits 2FA hands off into the shared `pending_tg` password state.

use onecrew_tg::{QrStart, QrStep, TgQrLogin};
use uuid::Uuid;

use crate::LocalPlatform;

use super::{
    PendingTgLogin, PendingTgQrLogin, TELEGRAM_SOCKS5_REQUIRED, TgLoginStep, TgQrLoginPollResult,
    TgQrLoginStartResult,
};

impl LocalPlatform {
    /// Begin Telegram's official QR login flow. No phone number is sent and no
    /// automatic fallback to phone login is attempted. A missing or non-SOCKS5
    /// proxy is rejected rather than falling back to a direct connection.
    pub(crate) async fn tg_start_qr_login(
        &self,
        api_id: i32,
        api_hash: String,
        proxy: Option<onecrew_proxy::ProxyConfig>,
    ) -> Result<TgQrLoginStartResult, String> {
        let socks5 = onecrew_sessions::telegram_socks5_proxy_url(proxy.as_ref())
            .map_err(|_| TELEGRAM_SOCKS5_REQUIRED.to_string())?;
        match TgQrLogin::start(api_id, api_hash.clone(), Some(socks5))
            .await
            .map_err(|error| error.to_string())?
        {
            QrStart::Pending(login) => {
                let challenge = login.challenge().clone();
                let token = Uuid::new_v4().to_string();
                self.pending_tg_qr
                    .lock()
                    .map_err(|_| "telegram QR login lock poisoned".to_string())?
                    .insert(
                        token.clone(),
                        PendingTgQrLogin {
                            api_id,
                            api_hash,
                            proxy,
                            login,
                        },
                    );
                Ok(TgQrLoginStartResult {
                    token: Some(token),
                    needs_password: false,
                    challenge: Some(challenge),
                    profile: None,
                })
            }
            QrStart::PasswordRequired(password) => {
                let token = Uuid::new_v4().to_string();
                self.pending_tg
                    .lock()
                    .map_err(|_| "telegram login lock poisoned".to_string())?
                    .insert(
                        token.clone(),
                        PendingTgLogin {
                            api_id,
                            api_hash,
                            label: "Telegram · QR 扫码".into(),
                            proxy,
                            step: TgLoginStep::AwaitingPassword(password),
                        },
                    );
                Ok(TgQrLoginStartResult {
                    token: Some(token),
                    needs_password: true,
                    challenge: None,
                    profile: None,
                })
            }
            QrStart::Done(account) => {
                let profile = self
                    .tg_finish(
                        api_id,
                        api_hash,
                        proxy,
                        account,
                        "Telegram · QR 扫码".into(),
                    )
                    .await?;
                Ok(TgQrLoginStartResult {
                    token: None,
                    needs_password: false,
                    challenge: None,
                    profile: Some(profile),
                })
            }
        }
    }

    /// Advance a pending QR login from the passive `updateLoginToken` stream.
    /// Pending responses may contain a fresh QR when Telegram expires the old
    /// short-lived token.
    pub(crate) async fn tg_poll_qr_login(
        &self,
        token: &str,
    ) -> Result<TgQrLoginPollResult, String> {
        let pending = self
            .pending_tg_qr
            .lock()
            .map_err(|_| "telegram QR login lock poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "telegram QR login expired; start again".to_string())?;
        match pending.login.poll().await {
            QrStep::Pending(login) => {
                let challenge = login.challenge().clone();
                self.pending_tg_qr
                    .lock()
                    .map_err(|_| "telegram QR login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgQrLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            proxy: pending.proxy,
                            login,
                        },
                    );
                Ok(TgQrLoginPollResult {
                    needs_password: false,
                    challenge: Some(challenge),
                    profile: None,
                })
            }
            QrStep::PasswordRequired(password) => {
                self.pending_tg
                    .lock()
                    .map_err(|_| "telegram login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            label: "Telegram · QR 扫码".into(),
                            proxy: pending.proxy,
                            step: TgLoginStep::AwaitingPassword(password),
                        },
                    );
                Ok(TgQrLoginPollResult {
                    needs_password: true,
                    challenge: None,
                    profile: None,
                })
            }
            QrStep::Done(account) => {
                let profile = self
                    .tg_finish(
                        pending.api_id,
                        pending.api_hash,
                        pending.proxy,
                        account,
                        "Telegram · QR 扫码".into(),
                    )
                    .await?;
                Ok(TgQrLoginPollResult {
                    needs_password: false,
                    challenge: None,
                    profile: Some(profile),
                })
            }
            QrStep::Failed(login, error) => {
                self.pending_tg_qr
                    .lock()
                    .map_err(|_| "telegram QR login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgQrLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            proxy: pending.proxy,
                            login,
                        },
                    );
                Err(error.to_string())
            }
        }
    }

    /// Cancel a pending QR flow and close its live MTProto connection. The
    /// operation is idempotent so dialog cleanup is safe after completion.
    pub async fn tg_cancel_qr_login(&self, token: &str) {
        let pending = self
            .pending_tg_qr
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(token));
        if let Some(pending) = pending {
            pending.login.cancel().await;
        }
    }
}
