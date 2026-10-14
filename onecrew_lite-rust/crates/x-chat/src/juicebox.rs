//! X Chat key recovery over the account's own egress.
//!
//! The Juicebox realms X Chat recovers keys from are X's own hosts
//! (`realm-*.x.com`), reached with a bearer token X mints for this account.
//! The SDK's stock transport is a bare reqwest client: no proxy field at
//! all, a `JuiceboxSdk-Rust/<version>` User-Agent, and a rustls fingerprint
//! — so key recovery left by a different door, with a different identity,
//! than every other request this account makes. X sits on both ends of that
//! split and can join them.
//!
//! This module keeps the protocol exactly as the SDK implements it and
//! changes only which HTTP client carries it: the same Chrome-emulated,
//! residential-proxied client as the rest of the X stack. Both seams are
//! the SDK's own — `ClientBuilder::http` for the transport and
//! `Chat::with_juicebox` for the implementation — so no vendored source
//! changes, and the crypto is untouched.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use chat_xdk_core::keys::juicebox::{
    DeleteFailureReason, DeleteResult, JuiceboxApi, JuiceboxConfig, RecoverFailureReason,
    RecoverResult, RegisterFailureReason, RegisterResult,
};
use juicebox_sdk::{
    AuthToken, AuthTokenManager, ClientBuilder, Configuration, DeleteError as JbDeleteError,
    JUICEBOX_VERSION_HEADER, Pin, Policy, RealmId, RecoverError as JbRecoverError,
    RegisterError as JbRegisterError, UserInfo, UserSecret, VERSION, http,
};
use onecrew_x_transport::emulated_client;
use wreq::Method;
use zeroize::Zeroizing;

use crate::XChatError;

/// The realm auth tokens for one recovery, keyed by hex realm id. They are
/// short-lived values X issues per unlock; they are never logged.
struct StaticAuthTokens {
    tokens: HashMap<String, String>,
}

fn realm_id_hex(realm_id: &RealmId) -> String {
    realm_id
        .0
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[async_trait]
impl AuthTokenManager for StaticAuthTokens {
    async fn get(&self, realm_id: &RealmId) -> Option<AuthToken> {
        self.tokens
            .get(&realm_id_hex(realm_id))
            .map(|token| AuthToken::from(token.clone()))
    }
}

/// The SDK's HTTP seam, implemented over the account's emulated + proxied
/// client. Cloning shares wreq's connection pool, so the realm round trips
/// of one recovery reuse a connection.
#[derive(Clone)]
pub struct WreqJuiceboxHttp {
    client: wreq::Client,
}

impl WreqJuiceboxHttp {
    pub fn new(proxy: Option<&str>) -> Result<Self, XChatError> {
        Ok(Self {
            client: emulated_client(proxy)
                .map_err(|error| XChatError::Crypto(error.to_string()))?,
        })
    }
}

#[async_trait]
impl http::Client for WreqJuiceboxHttp {
    async fn send(&self, request: http::Request) -> Option<http::Response> {
        let method = match request.method {
            http::Method::Get => Method::GET,
            http::Method::Put => Method::PUT,
            http::Method::Post => Method::POST,
            http::Method::Delete => Method::DELETE,
        };
        // The protocol version header is a client-level default in the SDK's
        // own transport, so it must be replayed here. The User-Agent is
        // deliberately NOT replayed: Chrome emulation supplies it, and
        // announcing the SDK is exactly the tell this change removes.
        let mut builder = self
            .client
            .request(method, &request.url)
            .header(JUICEBOX_VERSION_HEADER, VERSION);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        builder = builder.timeout(request.timeout.unwrap_or(Duration::from_secs(30)));
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        // The trait's contract is `None` for "could not perform the request";
        // the SDK owns retry and threshold policy from there. Errors are not
        // logged: they can carry the realm URL and the auth header.
        let response = builder.send().await.ok()?;
        let status_code = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (name.as_str().to_string(), value.to_string()))
            })
            .collect();
        let body = response.bytes().await.ok()?.to_vec();
        Some(http::Response {
            status_code,
            headers,
            body,
        })
    }
}

/// `JuiceboxApi` over the account's egress. Every branch mirrors the SDK's
/// stock implementation one-for-one — same configuration parsing, same
/// policy, same empty `UserInfo` (it must match between register and
/// recover or recovery fails), same error mapping — and differs only in
/// which HTTP client the builder is given.
pub struct WreqJuiceboxApi {
    http: WreqJuiceboxHttp,
}

impl WreqJuiceboxApi {
    pub fn new(proxy: Option<&str>) -> Result<Self, XChatError> {
        Ok(Self {
            http: WreqJuiceboxHttp::new(proxy)?,
        })
    }

    fn client(
        &self,
        configuration: Configuration,
        config: &JuiceboxConfig,
    ) -> juicebox_sdk::Client<juicebox_sdk::TokioSleeper, WreqJuiceboxHttp, StaticAuthTokens> {
        ClientBuilder::new()
            .configuration(configuration)
            .tokio_sleeper()
            .http(self.http.clone())
            .auth_token_manager(StaticAuthTokens {
                tokens: config.tokens.clone(),
            })
            .build()
    }
}

#[async_trait]
impl JuiceboxApi for WreqJuiceboxApi {
    async fn register_private_key(
        &self,
        pin: &[u8],
        config: &JuiceboxConfig,
        secret: &[u8],
    ) -> RegisterResult {
        let Ok(configuration) = Configuration::from_json(&config.config_json) else {
            return RegisterResult::Failure(RegisterFailureReason::StorageFailed);
        };
        let client = self.client(configuration, config);
        // Pin and UserSecret zeroize on drop, so moving the copies in leaves
        // no transient buffer to wipe here.
        let pin = Pin::from(pin.to_vec());
        let user_secret = UserSecret::from(secret.to_vec());
        let user_info = UserInfo::from(Vec::new());
        let policy = Policy {
            num_guesses: config.max_guess_count,
        };
        match client
            .register(&pin, &user_secret, &user_info, policy)
            .await
        {
            Ok(()) => RegisterResult::Success,
            Err(error) => RegisterResult::Failure(match error {
                JbRegisterError::InvalidAuth => RegisterFailureReason::InvalidAuth,
                JbRegisterError::UpgradeRequired => RegisterFailureReason::UpgradeRequired,
                JbRegisterError::Assertion => RegisterFailureReason::Assertion,
                JbRegisterError::Transient => RegisterFailureReason::Transient,
                JbRegisterError::RateLimitExceeded => RegisterFailureReason::RateLimitExceeded,
            }),
        }
    }

    async fn recover_private_key(&self, pin: &[u8], config: &JuiceboxConfig) -> RecoverResult {
        let Ok(configuration) = Configuration::from_json(&config.config_json) else {
            return RecoverResult::Failure {
                reason: RecoverFailureReason::Transient,
                guesses_remaining: None,
            };
        };
        let client = self.client(configuration, config);
        let pin = Pin::from(pin.to_vec());
        let user_info = UserInfo::from(Vec::new());
        match client.recover(&pin, &user_info).await {
            Ok(secret) => RecoverResult::Success(Zeroizing::new(secret.expose_secret().to_vec())),
            Err(error) => {
                let (reason, guesses_remaining) = match error {
                    JbRecoverError::InvalidPin { guesses_remaining } => {
                        (RecoverFailureReason::InvalidPin, Some(guesses_remaining))
                    }
                    JbRecoverError::NotRegistered => (RecoverFailureReason::NotRegistered, None),
                    JbRecoverError::InvalidAuth => (RecoverFailureReason::InvalidAuth, None),
                    JbRecoverError::UpgradeRequired => {
                        (RecoverFailureReason::UpgradeRequired, None)
                    }
                    JbRecoverError::Assertion => (RecoverFailureReason::Assertion, None),
                    JbRecoverError::Transient => (RecoverFailureReason::Transient, None),
                    JbRecoverError::RateLimitExceeded => {
                        (RecoverFailureReason::RateLimitExceeded, None)
                    }
                };
                RecoverResult::Failure {
                    reason,
                    guesses_remaining,
                }
            }
        }
    }

    async fn delete_keys(&self, config: &JuiceboxConfig) -> DeleteResult {
        let Ok(configuration) = Configuration::from_json(&config.config_json) else {
            return DeleteResult::Failure(DeleteFailureReason::Transient);
        };
        let client = self.client(configuration, config);
        match client.delete().await {
            Ok(()) => DeleteResult::Success,
            Err(error) => DeleteResult::Failure(match error {
                JbDeleteError::InvalidAuth => DeleteFailureReason::InvalidAuth,
                JbDeleteError::UpgradeRequired => DeleteFailureReason::UpgradeRequired,
                JbDeleteError::RateLimitExceeded => DeleteFailureReason::RateLimitExceeded,
                JbDeleteError::Assertion => DeleteFailureReason::Assertion,
                JbDeleteError::Transient => DeleteFailureReason::Transient,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use juicebox_sdk::http::{Client as _, Method as JbMethod, Request};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::WreqJuiceboxHttp;

    /// Accept one request, hand back its head, answer 200.
    async fn capture_one_request() -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address").to_string();
        let handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut head = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !head.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut buffer).await.expect("read");
                if read == 0 {
                    break;
                }
                head.extend_from_slice(&buffer[..read]);
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await
                .expect("write");
            stream.flush().await.expect("flush");
            String::from_utf8_lossy(&head).to_string()
        });
        (address, handle)
    }

    #[tokio::test]
    async fn realm_requests_keep_the_protocol_header_and_stop_announcing_the_sdk() {
        let (address, server) = capture_one_request().await;
        let http = WreqJuiceboxHttp::new(None).expect("client");

        let response = http
            .send(Request {
                method: JbMethod::Post,
                url: format!("http://{address}/req"),
                headers: HashMap::from([(
                    "Authorization".to_string(),
                    "Bearer realm-token".to_string(),
                )]),
                body: Some(b"noise".to_vec()),
                timeout: None,
            })
            .await
            .expect("response");
        assert_eq!(response.status_code, 200);
        assert_eq!(response.body, b"ok");

        let head = server.await.expect("server").to_lowercase();
        assert!(head.starts_with("post /req "), "{head}");
        // The SDK sets this as a client-level default, so replaying it is on us.
        assert!(head.contains("x-juicebox-version:"), "{head}");
        // The caller's own headers must survive verbatim — recovery fails
        // without the realm auth token.
        assert!(head.contains("authorization: bearer realm-token"), "{head}");
        // The whole point: the account no longer announces the SDK, and the
        // User-Agent comes from the same Chrome emulation as its X traffic.
        assert!(!head.contains("juiceboxsdk-rust"), "{head}");
        assert!(head.contains("user-agent: mozilla/"), "{head}");
    }
}
