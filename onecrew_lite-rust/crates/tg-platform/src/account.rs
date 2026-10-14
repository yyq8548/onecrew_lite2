use onecrew_tg_client as ops;
use onecrew_tg_client::{
    TgChat, TgCommunityAdministrators, TgCommunityReadError, TgCommunityTarget, TgLocator,
    TgMessage, TgTargetResolution, TgUser, TgUserTarget,
};
use onecrew_tg_transport::{
    Connection, LoginCodeDelivery, LoginCodeToken, PasswordCheck, QrLoginChallenge, QrLoginExport,
    SignInStep, TgError,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A pending phone login waiting for the code that was just sent.
pub struct TgLogin {
    conn: Connection,
    token: LoginCodeToken,
    delivery: LoginCodeDelivery,
}

/// A pending login that needs the 2FA password to finish.
pub struct TgPassword {
    conn: Connection,
}

/// A pending QR login. The raw login token remains owned by Telegram transport;
/// this facade carries only the rendered, short-lived challenge.
pub struct TgQrLogin {
    conn: Connection,
    challenge: QrLoginChallenge,
}

/// Result of submitting the login code.
pub enum CodeStep {
    /// Login finished; the account is ready to use and persist.
    Done(TgAccount),
    /// The account has 2FA; call [`TgPassword::submit`] with the password.
    PasswordRequired(TgPassword),
    /// The submitted code was rejected. The live login and its login token are
    /// returned intact so another code can be submitted to the same operation.
    Invalid(TgLogin),
    /// A transport or Telegram error occurred before authorization completed.
    /// Preserve the live login for a safe retry instead of dropping its token.
    Failed(TgLogin, TgError),
}

enum CodeSignInDisposition<State> {
    Complete(State),
    PasswordRequired(State),
    Invalid(State),
    Failed(State, TgError),
}

fn preserve_code_login_state<State>(
    state: State,
    result: Result<SignInStep, TgError>,
) -> CodeSignInDisposition<State> {
    match result {
        Ok(SignInStep::Complete) => CodeSignInDisposition::Complete(state),
        Ok(SignInStep::PasswordRequired) => CodeSignInDisposition::PasswordRequired(state),
        Err(TgError::Auth(message)) if message == "invalid or expired login code" => {
            CodeSignInDisposition::Invalid(state)
        }
        Err(error) => CodeSignInDisposition::Failed(state, error),
    }
}

/// Result of checking the account's two-step-verification password.
pub enum PasswordStep {
    Done(TgAccount),
    /// The password was rejected. The returned state remains valid for retry.
    Invalid(TgPassword),
    /// A transport or Telegram error occurred before authorization completed.
    /// The live login is returned so the caller can keep the same flow open.
    Failed(TgPassword, TgError),
}

/// First result from starting a QR login.
pub enum QrStart {
    Pending(TgQrLogin),
    PasswordRequired(TgPassword),
    Done(TgAccount),
}

/// Result from checking a pending QR login.
pub enum QrStep {
    Pending(TgQrLogin),
    PasswordRequired(TgPassword),
    Done(TgAccount),
    /// A temporary transport/Telegram error occurred while polling. The live
    /// QR login remains valid and is returned for retry by the same operation.
    Failed(TgQrLogin, TgError),
}

enum QrExportDisposition<State> {
    Pending(State, QrLoginChallenge),
    PasswordRequired(State),
    Complete(State),
    Failed(State, TgError),
}

fn preserve_qr_login_state<State>(
    state: State,
    result: Result<QrLoginExport, TgError>,
) -> QrExportDisposition<State> {
    match result {
        Ok(QrLoginExport::Pending(challenge)) => QrExportDisposition::Pending(state, challenge),
        Ok(QrLoginExport::PasswordRequired) => QrExportDisposition::PasswordRequired(state),
        Ok(QrLoginExport::Complete) => QrExportDisposition::Complete(state),
        Err(error) => QrExportDisposition::Failed(state, error),
    }
}

impl TgLogin {
    /// Begin a phone login: connect and request the login code. `proxy` is an
    /// optional SOCKS5 URL (家宽); grammers only supports SOCKS5.
    pub async fn start(
        api_id: i32,
        api_hash: String,
        phone: &str,
        proxy: Option<String>,
    ) -> Result<Self, TgError> {
        let conn = Connection::connect_new(api_id, api_hash, proxy);
        let request = conn.request_login_code(phone).await?;
        Ok(Self {
            conn,
            token: request.token,
            delivery: request.delivery,
        })
    }

    pub fn delivery(&self) -> &LoginCodeDelivery {
        &self.delivery
    }

    /// Submit the received login code.
    pub async fn submit_code(self, code: &str) -> CodeStep {
        let result = self.conn.sign_in(&self.token, code).await;
        match preserve_code_login_state(self, result) {
            CodeSignInDisposition::Complete(login) => {
                CodeStep::Done(TgAccount { conn: login.conn })
            }
            CodeSignInDisposition::PasswordRequired(login) => {
                CodeStep::PasswordRequired(TgPassword { conn: login.conn })
            }
            CodeSignInDisposition::Invalid(login) => CodeStep::Invalid(login),
            CodeSignInDisposition::Failed(login, error) => CodeStep::Failed(login, error),
        }
    }

    /// Close a pending phone-login connection without submitting a code.
    pub async fn cancel(self) {
        self.conn.close().await;
    }
}

impl TgPassword {
    /// Submit the 2FA password to finish login.
    pub async fn submit(self, password: &str) -> PasswordStep {
        match self.conn.check_password(password).await {
            Ok(PasswordCheck::Complete) => PasswordStep::Done(TgAccount { conn: self.conn }),
            Ok(PasswordCheck::Invalid) => PasswordStep::Invalid(self),
            Err(error) => PasswordStep::Failed(self, error),
        }
    }

    /// Close a pending two-factor login connection without submitting a value.
    pub async fn cancel(self) {
        self.conn.close().await;
    }
}

impl TgQrLogin {
    /// Begin Telegram's official QR login flow. The QR can only be approved by
    /// an already signed-in Telegram mobile client.
    pub async fn start(
        api_id: i32,
        api_hash: String,
        proxy: Option<String>,
    ) -> Result<QrStart, TgError> {
        let conn = Connection::connect_new(api_id, api_hash, proxy);
        Ok(match conn.export_qr_login().await? {
            QrLoginExport::Pending(challenge) => QrStart::Pending(Self { conn, challenge }),
            QrLoginExport::PasswordRequired => QrStart::PasswordRequired(TgPassword { conn }),
            QrLoginExport::Complete => QrStart::Done(TgAccount { conn }),
        })
    }

    pub fn challenge(&self) -> &QrLoginChallenge {
        &self.challenge
    }

    /// Consume one pending state and return its next state. The brief wait is
    /// against the local passive update receiver, not repeated Telegram RPCs.
    pub async fn poll(self) -> QrStep {
        let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as i32,
            Err(error) => return QrStep::Failed(self, TgError::Session(error.to_string())),
        };
        let should_export = now >= self.challenge.expires_at
            || match self
                .conn
                .wait_for_qr_login_update(Duration::from_millis(850))
                .await
            {
                Ok(ready) => ready,
                Err(error) => return QrStep::Failed(self, error),
            };
        if !should_export {
            return QrStep::Pending(self);
        }

        let export = self.conn.export_qr_login().await;
        match preserve_qr_login_state(self, export) {
            QrExportDisposition::Pending(mut login, challenge) => {
                login.challenge = challenge;
                QrStep::Pending(login)
            }
            QrExportDisposition::PasswordRequired(login) => {
                QrStep::PasswordRequired(TgPassword { conn: login.conn })
            }
            QrExportDisposition::Complete(login) => QrStep::Done(TgAccount { conn: login.conn }),
            QrExportDisposition::Failed(login, error) => QrStep::Failed(login, error),
        }
    }

    pub async fn cancel(self) {
        self.conn.close().await;
    }
}

/// Application-facing Telegram account. The desktop app and MCP layer call this
/// and never see MTProto, session bytes, or raw TL details.
pub struct TgAccount {
    conn: Connection,
}

impl TgAccount {
    /// Restore an account from a session JSON previously produced by a login
    /// (stored in the vault).
    pub async fn from_session(
        api_id: i32,
        api_hash: String,
        session_json: &str,
        proxy: Option<String>,
    ) -> Result<Self, TgError> {
        Ok(Self {
            conn: Connection::connect_session(api_id, api_hash, session_json, proxy).await?,
        })
    }

    /// Current session JSON to persist in the vault.
    pub fn session_json(&self) -> Result<String, TgError> {
        self.conn.session_json()
    }

    pub async fn who_am_i(&self) -> Result<TgUser, TgError> {
        ops::who_am_i(self.conn.client()).await
    }

    pub async fn user(&self, username: &str) -> Result<TgUser, TgError> {
        ops::user(self.conn.client(), username).await
    }

    pub async fn resolve_target(&self, locator: &TgLocator) -> Result<TgTargetResolution, TgError> {
        ops::resolve_target(self.conn.client(), locator).await
    }

    pub async fn search_chats(&self, query: &str, limit: i32) -> Result<Vec<TgChat>, TgError> {
        ops::search_chats(self.conn.client(), query, limit).await
    }

    pub async fn members(&self, source: &str, limit: usize) -> Result<Vec<TgUser>, TgError> {
        ops::members(self.conn.client(), source, limit).await
    }

    pub async fn admins(
        &self,
        target: &TgCommunityTarget,
    ) -> Result<TgCommunityAdministrators, TgError> {
        ops::admins(self.conn.client(), target).await
    }

    /// Read public community fields using a safe locator.
    ///
    /// # Errors
    /// Returns the community read failure, whose protocol-native evidence
    /// [`crate::community_read_evidence`] extracts.
    pub async fn community(
        &self,
        locator: &crate::TgCommunityLocator,
    ) -> Result<crate::TgPublicCommunity, TgCommunityReadError> {
        ops::community(&self.conn, locator).await
    }

    /// Read at most 200 administrators, with phone and peer authorization omitted.
    ///
    /// # Errors
    /// Returns the resolution failure or Telegram's own failure, whose
    /// protocol-native evidence [`crate::community_read_evidence`] extracts.
    pub async fn community_admins(
        &self,
        locator: &crate::TgCommunityLocator,
    ) -> Result<crate::TgPublicAdministrators, TgCommunityReadError> {
        ops::community_admins(&self.conn, locator)
            .await
            .map(crate::TgPublicAdministrators::from)
    }

    pub async fn thread(&self, username: &str, count: usize) -> Result<Vec<TgMessage>, TgError> {
        ops::thread(self.conn.client(), username, count).await
    }

    pub async fn send_dm(&self, target: &TgUserTarget, text: &str) -> Result<String, TgError> {
        ops::send_dm(self.conn.client(), target, text).await
    }

    pub async fn user_thread(
        &self,
        target: &TgUserTarget,
        count: usize,
    ) -> Result<Vec<TgMessage>, TgError> {
        ops::user_thread(self.conn.client(), target, count).await
    }

    /// Gracefully close the connection.
    pub async fn close(self) {
        self.conn.close().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_otp_returns_the_same_retryable_login_state() {
        let disposition = preserve_code_login_state(
            "opaque-login-state",
            Err(TgError::Auth("invalid or expired login code".into())),
        );
        match disposition {
            CodeSignInDisposition::Invalid(state) => assert_eq!(state, "opaque-login-state"),
            _ => panic!("invalid OTP must preserve its login state for retry"),
        }
    }

    #[test]
    fn transport_error_also_preserves_login_state_for_safe_retry() {
        let disposition = preserve_code_login_state(
            "opaque-login-state",
            Err(TgError::Network("temporary".into())),
        );
        match disposition {
            CodeSignInDisposition::Failed(state, TgError::Network(message)) => {
                assert_eq!(state, "opaque-login-state");
                assert_eq!(message, "temporary");
            }
            _ => panic!("transport failure must preserve its login state for retry"),
        }
    }

    #[test]
    fn qr_export_error_preserves_the_same_retryable_login_state() {
        let disposition = preserve_qr_login_state(
            "opaque-qr-login-state",
            Err(TgError::Network("temporary".into())),
        );
        match disposition {
            QrExportDisposition::Failed(state, TgError::Network(message)) => {
                assert_eq!(state, "opaque-qr-login-state");
                assert_eq!(message, "temporary");
            }
            _ => panic!("QR poll failure must preserve its login state for retry"),
        }
    }
}
