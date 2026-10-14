use std::sync::Arc;

use grammers_client::client::{PasswordToken, SignInError};
use grammers_client::peer::User;
use grammers_client::sender::{ConnectionParams, SenderPoolFatHandle};
use grammers_client::{Client, SenderPool};
use grammers_session::Session;
use grammers_session::types::{PeerId, PeerInfo, PeerRef, UpdateState, UpdatesState};
use grammers_session::updates::UpdatesLike;
use grammers_tl_types as tl;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::error::TgError;
use crate::session::VaultSession;

mod login_code;
mod qr_login;

pub use login_code::{
    LoginCodeDelivery, LoginCodeMethod, LoginCodeRequest, LoginCodeToken, SignInStep,
};
pub use qr_login::{QrLoginChallenge, QrLoginExport};

/// Outcome of checking an account's two-step-verification password.
pub enum PasswordCheck {
    Complete,
    Invalid,
}

/// A live grammers connection: the driving pool task, the client, and the
/// in-memory session that can be snapshotted to the vault.
pub struct Connection {
    client: Client,
    session: Arc<VaultSession>,
    handle: SenderPoolFatHandle,
    updates: Mutex<mpsc::UnboundedReceiver<UpdatesLike>>,
    pool_task: Option<JoinHandle<()>>,
    api_id: i32,
    api_hash: String,
}

impl Connection {
    /// `proxy` is an optional **SOCKS5** URL (`socks5://[user:pass@]host:port`);
    /// grammers has no HTTP-proxy path, so callers pass `None` for http/https.
    fn build(
        session: Arc<VaultSession>,
        api_id: i32,
        api_hash: String,
        proxy: Option<String>,
    ) -> Self {
        // Present a fixed, canonical **Telegram Desktop (Windows)** identity — the
        // same values gotd's `DeviceTDesktopWindows` uses — instead of grammers'
        // defaults, which leak the host OS/locale and (worst) set app_version to
        // the grammers crate version, a dead giveaway it's not a real client.
        // Looking like the single most common real client blends in with millions
        // of identical tdesktop users; per-account isolation comes from the 家宽
        // IP (already per-profile), NOT a unique device string — a unique string
        // would stand out, not hide. grammers exposes no lang_pack field, so we
        // can't also send "tdesktop" there, but the rest matches.
        let params = ConnectionParams {
            proxy_url: proxy,
            device_model: "Desktop".to_string(),
            system_version: "Windows 10".to_string(),
            app_version: "6.9.1 x64".to_string(),
            system_lang_code: "en-US".to_string(),
            lang_code: "en".to_string(),
            ..Default::default()
        };
        let SenderPool {
            runner,
            handle,
            updates,
        } = SenderPool::with_configuration(Arc::clone(&session), api_id, params);
        let client = Client::new(handle.clone());
        let pool_task = Some(tokio::spawn(runner.run()));
        Self {
            client,
            session,
            handle,
            updates: Mutex::new(updates),
            pool_task,
            api_id,
            api_hash,
        }
    }

    /// Start a fresh, unauthorized connection to begin a phone login.
    pub fn connect_new(api_id: i32, api_hash: String, proxy: Option<String>) -> Self {
        Self::build(Arc::new(VaultSession::default()), api_id, api_hash, proxy)
    }

    /// Restore an authorized connection from a vault session JSON.
    pub async fn connect_session(
        api_id: i32,
        api_hash: String,
        session_json: &str,
        proxy: Option<String>,
    ) -> Result<Self, TgError> {
        let session = Arc::new(VaultSession::from_json(session_json)?);
        let connection = Self::build(session, api_id, api_hash, proxy);
        match connection.is_authorized().await {
            Ok(true) => Ok(connection),
            outcome => {
                let error = outcome.err().unwrap_or_else(|| {
                    TgError::Auth("telegram session is no longer authorized; sign in again".into())
                });
                connection.close().await;
                Err(error)
            }
        }
    }

    pub async fn is_authorized(&self) -> Result<bool, TgError> {
        self.client
            .is_authorized()
            .await
            .map_err(TgError::from_invocation)
    }

    async fn complete_login(&self, auth: tl::types::auth::Authorization) -> Result<(), TgError> {
        let update_state = self
            .client
            .invoke(&tl::functions::updates::GetState {})
            .await
            .ok();
        let user = User::from_raw(&self.client, auth.user);
        let peer = user
            .to_ref()
            .await
            .map_err(|error| TgError::Session(error.to_string()))?
            .ok_or_else(|| {
                TgError::Session("authorized Telegram user has no peer reference".into())
            })?;
        self.session
            .cache_peer(&PeerInfo::User {
                id: user.id().bare_id_unchecked(),
                auth: Some(peer.auth),
                bot: Some(user.is_bot()),
                is_self: Some(true),
            })
            .await?;
        if let Some(tl::enums::updates::State::State(state)) = update_state {
            self.session
                .set_update_state(UpdateState::All(UpdatesState {
                    pts: state.pts,
                    qts: state.qts,
                    date: state.date,
                    seq: state.seq,
                    channels: Vec::new(),
                }))
                .await?;
        }
        Ok(())
    }

    async fn password_token(&self) -> Result<PasswordToken, TgError> {
        let password: tl::types::account::Password = self
            .client
            .invoke(&tl::functions::account::GetPassword {})
            .await
            .map_err(|error| TgError::Network(error.to_string()))?
            .into();
        Ok(PasswordToken::new(password))
    }

    pub async fn check_password(&self, password: &str) -> Result<PasswordCheck, TgError> {
        // Telegram SRP challenges are single-use and short-lived. Fetch fresh
        // parameters for every user submission instead of retaining the token
        // returned when the QR/code step first requested 2FA.
        for attempt in 0..2 {
            let token = self.password_token().await?;
            match self.client.check_password(token, password.as_bytes()).await {
                Ok(_user) => return Ok(PasswordCheck::Complete),
                Err(SignInError::InvalidPassword(_)) => return Ok(PasswordCheck::Invalid),
                Err(SignInError::Other(error)) if error.is("SRP_ID_INVALID") && attempt == 0 => {
                    // The challenge can expire between getPassword and
                    // checkPassword. Recompute once with a new challenge.
                    continue;
                }
                Err(error) => return Err(TgError::Auth(error.to_string())),
            }
        }
        unreachable!("the SRP retry loop always returns")
    }

    /// Snapshot the authorization state as JSON for the vault.
    pub fn session_json(&self) -> Result<String, TgError> {
        self.session.to_json()
    }

    /// The underlying grammers client for read/write operations.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Recover only authorization actually observed by this live session.
    /// The vault snapshot deliberately does not persist the peer cache.
    pub async fn cached_peer_ref(&self, id: PeerId) -> Result<Option<PeerRef>, TgError> {
        Ok(self
            .session
            .peer(id)
            .await?
            .and_then(|peer| peer.auth().map(|auth| PeerRef { id, auth })))
    }

    /// Gracefully stop the driving pool task.
    pub async fn close(mut self) {
        self.handle.quit();
        // Keep the join handle in this owner while awaiting it: cancellation
        // must still let Drop abort the pool instead of detaching its task.
        if let Some(task) = self.pool_task.as_mut()
            && let Err(error) = task.await
        {
            if error.is_panic() {
                log::debug!("telegram sender pool task panicked");
            } else {
                log::debug!("telegram sender pool task was canceled");
            }
        }
        self.pool_task.take();
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.handle.quit();
        if let Some(task) = self.pool_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_session::types::PeerAuth;
    use std::future::{Future, pending, poll_fn};
    use std::task::Poll;

    // The real pool is never driven: these fixtures cannot contact Telegram.
    // A controlled task stands in for its lifecycle while exercising the real
    // Connection close/Drop owner and the real VaultSession peer store.
    fn fixture(task: JoinHandle<()>) -> Connection {
        let session = Arc::new(VaultSession::default());
        let SenderPool {
            runner: _,
            handle,
            updates,
        } = SenderPool::with_configuration(Arc::clone(&session), 1, ConnectionParams::default());
        Connection {
            client: Client::new(handle.clone()),
            session,
            handle,
            updates: Mutex::new(updates),
            pool_task: Some(task),
            api_id: 1,
            api_hash: "fixture".into(),
        }
    }

    #[tokio::test]
    async fn close_waits_for_its_pool_and_cancellation_still_aborts_it() {
        let task = tokio::spawn(pending::<()>());
        let abort = task.abort_handle();
        let mut close = Box::pin(fixture(task).close());
        poll_fn(|context| {
            assert!(close.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        assert!(!abort.is_finished());
        drop(close);
        tokio::task::yield_now().await;
        assert!(
            abort.is_finished(),
            "canceling close must not detach the pool"
        );
    }

    #[tokio::test]
    async fn success_and_failure_paths_join_the_owned_pool() {
        for failed in [false, true] {
            let task = tokio::spawn(async move {
                assert!(!failed, "fixture pool failure");
            });
            let abort = task.abort_handle();
            fixture(task).close().await;
            assert!(abort.is_finished());
        }
    }

    #[tokio::test]
    async fn cached_peer_auth_requires_an_observation_and_is_not_persisted() {
        let connection = fixture(tokio::spawn(async {}));
        let id = PeerId::channel(123).unwrap();
        assert!(connection.cached_peer_ref(id).await.unwrap().is_none());
        connection
            .session
            .cache_peer(&PeerInfo::Channel {
                id: 123,
                auth: None,
                kind: None,
            })
            .await
            .unwrap();
        assert!(connection.cached_peer_ref(id).await.unwrap().is_none());
        connection
            .session
            .cache_peer(&PeerInfo::Channel {
                id: 123,
                auth: Some(PeerAuth::from_hash(987654321)),
                kind: None,
            })
            .await
            .unwrap();
        let peer = connection.cached_peer_ref(id).await.unwrap().unwrap();
        assert_eq!(peer.id, id);
        assert_eq!(peer.auth.hash(), 987654321);
        let snapshot = connection.session_json().unwrap();
        assert!(!snapshot.contains("987654321"));
        assert!(!snapshot.contains("peer_infos"));
        connection.close().await;
    }
}
