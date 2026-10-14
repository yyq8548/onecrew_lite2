//! Telegram phone login: request a code, submit it, then finish or continue
//! into the 2FA password step. Also completes the 2FA password for a QR login
//! that required one (both flows share the same `pending_tg` state).

use onecrew_local_domain::LocalProfile;
use onecrew_tg::{CodeStep, PasswordStep, TgLogin};
use uuid::Uuid;

use crate::LocalPlatform;

use super::{
    PendingTgLogin, TELEGRAM_SOCKS5_REQUIRED, TgCodeResult, TgLoginStartResult, TgLoginStep,
};

impl LocalPlatform {
    /// Cancel either kind of pending phone/2FA login. This is idempotent and is
    /// used by the high-level account workflow so MCP can clean up an opaque
    /// flow without learning the Telegram credentials it contains.
    pub async fn tg_cancel_login(&self, token: &str) {
        let pending = self
            .pending_tg
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(token));
        if let Some(pending) = pending {
            match pending.step {
                TgLoginStep::AwaitingCode(login) => login.cancel().await,
                TgLoginStep::AwaitingPassword(password) => password.cancel().await,
            }
        }
    }

    /// Begin a Telegram login: request the code, return a token for the next step.
    /// The login egresses through the profile's 家宽 (SOCKS5 only — grammers has no
    /// HTTP-proxy path), so the login IP matches the later operating IP. A
    /// missing or non-SOCKS5 proxy is rejected
    /// rather than silently falling back to a direct connection.
    pub(crate) async fn tg_start_login(
        &self,
        api_id: i32,
        api_hash: String,
        phone: String,
        proxy: Option<onecrew_proxy::ProxyConfig>,
    ) -> Result<TgLoginStartResult, String> {
        // grammers forwards the phone to Telegram verbatim — normalize to E.164
        // (`+` + digits) so any spacing/format the UI let through can't matter.
        let phone = to_e164(&phone);
        let socks5 = onecrew_sessions::telegram_socks5_proxy_url(proxy.as_ref())
            .map_err(|_| TELEGRAM_SOCKS5_REQUIRED.to_string())?;
        let login = TgLogin::start(api_id, api_hash.clone(), &phone, Some(socks5))
            .await
            .map_err(|error| error.to_string())?;
        let delivery = login.delivery().clone();
        let token = Uuid::new_v4().to_string();
        self.pending_tg
            .lock()
            .map_err(|_| "telegram login lock poisoned".to_string())?
            .insert(
                token.clone(),
                PendingTgLogin {
                    api_id,
                    api_hash,
                    label: format!("Telegram · {phone}"),
                    proxy,
                    step: TgLoginStep::AwaitingCode(login),
                },
            );
        Ok(TgLoginStartResult { token, delivery })
    }

    /// Submit the login code. Finishes the import, or reports that 2FA is needed.
    pub(crate) async fn tg_submit_code(
        &self,
        token: &str,
        code: &str,
    ) -> Result<TgCodeResult, String> {
        let pending = self.take_pending_tg(token)?;
        let login = match pending.step {
            TgLoginStep::AwaitingCode(login) => login,
            TgLoginStep::AwaitingPassword(_) => {
                return Err("this login is waiting for a 2FA password".into());
            }
        };
        match login.submit_code(code).await {
            CodeStep::Done(account) => {
                let profile = self
                    .tg_finish(
                        pending.api_id,
                        pending.api_hash,
                        pending.proxy,
                        account,
                        pending.label,
                    )
                    .await?;
                Ok(TgCodeResult {
                    needs_password: false,
                    profile: Some(profile),
                })
            }
            CodeStep::PasswordRequired(password) => {
                self.pending_tg
                    .lock()
                    .map_err(|_| "telegram login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            label: pending.label,
                            proxy: pending.proxy,
                            step: TgLoginStep::AwaitingPassword(password),
                        },
                    );
                Ok(TgCodeResult {
                    needs_password: true,
                    profile: None,
                })
            }
            CodeStep::Invalid(login) => {
                self.restore_pending_tg_code(
                    token,
                    pending.api_id,
                    pending.api_hash,
                    pending.label,
                    pending.proxy,
                    login,
                )?;
                Err("telegram auth: invalid or expired login code".into())
            }
            CodeStep::Failed(login, error) => {
                self.restore_pending_tg_code(
                    token,
                    pending.api_id,
                    pending.api_hash,
                    pending.label,
                    pending.proxy,
                    login,
                )?;
                Err(error.to_string())
            }
        }
    }

    fn restore_pending_tg_code(
        &self,
        token: &str,
        api_id: i32,
        api_hash: String,
        label: String,
        proxy: Option<onecrew_proxy::ProxyConfig>,
        login: TgLogin,
    ) -> Result<(), String> {
        self.pending_tg
            .lock()
            .map_err(|_| "telegram login lock poisoned".to_string())?
            .insert(
                token.to_string(),
                PendingTgLogin {
                    api_id,
                    api_hash,
                    label,
                    proxy,
                    step: TgLoginStep::AwaitingCode(login),
                },
            );
        Ok(())
    }

    /// Submit the 2FA password to finish a Telegram login.
    pub async fn tg_submit_password(
        &self,
        token: &str,
        password: &str,
    ) -> Result<LocalProfile, String> {
        let pending = self.take_pending_tg(token)?;
        let password_step = match pending.step {
            TgLoginStep::AwaitingPassword(password_step) => password_step,
            TgLoginStep::AwaitingCode(_) => {
                return Err("this login is waiting for the login code".into());
            }
        };
        match password_step.submit(password).await {
            PasswordStep::Done(account) => {
                self.tg_finish(
                    pending.api_id,
                    pending.api_hash,
                    pending.proxy,
                    account,
                    pending.label,
                )
                .await
            }
            PasswordStep::Invalid(password_step) => {
                self.pending_tg
                    .lock()
                    .map_err(|_| "telegram login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            label: pending.label,
                            proxy: pending.proxy,
                            step: TgLoginStep::AwaitingPassword(password_step),
                        },
                    );
                Err("telegram auth: invalid 2FA password".into())
            }
            PasswordStep::Failed(password_step, error) => {
                self.pending_tg
                    .lock()
                    .map_err(|_| "telegram login lock poisoned".to_string())?
                    .insert(
                        token.to_string(),
                        PendingTgLogin {
                            api_id: pending.api_id,
                            api_hash: pending.api_hash,
                            label: pending.label,
                            proxy: pending.proxy,
                            step: TgLoginStep::AwaitingPassword(password_step),
                        },
                    );
                Err(error.to_string())
            }
        }
    }

    fn take_pending_tg(&self, token: &str) -> Result<PendingTgLogin, String> {
        self.pending_tg
            .lock()
            .map_err(|_| "telegram login lock poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "telegram login expired; start again".to_string())
    }
}

/// Canonical E.164 phone: `+` followed by digits only (country code + national
/// number), stripping any spaces or symbols.
fn to_e164(phone: &str) -> String {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    format!("+{digits}")
}
