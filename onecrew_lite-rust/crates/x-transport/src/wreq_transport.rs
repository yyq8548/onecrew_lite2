use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use serde_json::Value;
use tokio::{sync::OnceCell, time::sleep};
use url::Url;
use wreq::{
    Client, Method,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use wreq_util::Emulation;

use crate::{JsonTransport, TransportError, XSession, transaction::ClientTransaction};

const WEB_BEARER_TOKEN: &str = concat!(
    "AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs",
    "%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA"
);

#[derive(Clone)]
pub struct WreqXTransport {
    client: Client,
    session: Arc<XSession>,
    max_retries: usize,
    // Lazily initialized once, shared across clones; `None` result means init
    // failed and we proceed without the header (reads still work).
    transaction: Arc<OnceCell<ClientTransaction>>,
}

/// One egress policy for everything this account sends: Chrome emulation
/// (TLS/JA3/JA4, HTTP2 settings, browser default headers including the UA)
/// plus the profile's residential proxy. `Emulation::Chrome149` is a const
/// alias for `Profile::Chrome149` (wreq-util 3.x).
///
/// Exported because the X stack is not the only thing an account talks to:
/// X Chat's key recovery reaches X-operated Juicebox realms and must leave
/// by the same door with the same fingerprint, or the account's traffic
/// splits into two correlatable identities.
///
/// # Errors
///
/// Returns [`TransportError`] when the proxy URL is invalid or the underlying
/// HTTP client cannot be constructed.
pub fn emulated_client(proxy: Option<&str>) -> Result<Client, TransportError> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .emulation(Emulation::Chrome149);
    if let Some(url) = proxy.filter(|url| !url.is_empty()) {
        builder = builder.proxy(
            wreq::Proxy::all(url).map_err(|error| TransportError::Network(error.to_string()))?,
        );
    }
    builder
        .build()
        .map_err(|error| TransportError::Network(error.to_string()))
}

impl WreqXTransport {
    /// `proxy` is an optional proxy URL (e.g. `http://user:pass@host:port` or
    /// `socks5://host:port`); when set, all requests egress through it (家宽).
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] when the proxy URL is invalid or the
    /// emulated HTTP client cannot be constructed.
    pub fn new(session: XSession, proxy: Option<&str>) -> Result<Self, TransportError> {
        let client = emulated_client(proxy)?;
        Ok(Self {
            client,
            session: Arc::new(session),
            max_retries: 3,
            transaction: Arc::new(OnceCell::new()),
        })
    }

    /// Best-effort `x-client-transaction-id` for a request. Initializes the key
    /// material once (cached); on failure returns `None` so the request still
    /// goes out (only write endpoints like CreateTweet actually require it).
    async fn transaction_id(&self, method: &Method, url: &Url) -> Option<String> {
        let cookie = self.session.cookie_header();
        self.transaction
            .get_or_try_init(|| ClientTransaction::init(&self.client, &cookie))
            .await
            .ok()
            .map(|ct| ct.generate_transaction_id(method.as_str(), url.path()))
    }

    fn headers(
        &self,
        url: &Url,
        method: &Method,
        transaction_id: Option<&str>,
    ) -> Result<HeaderMap, TransportError> {
        // X-application headers only. The browser-fingerprint headers
        // (User-Agent, sec-ch-ua, sec-fetch-*, accept-language, accept-encoding)
        // all come from wreq's Chrome emulation, so UA and TLS stay consistent —
        // we deliberately do NOT override User-Agent here.
        let mut headers = HeaderMap::new();
        let values = [
            (AUTHORIZATION, format!("Bearer {WEB_BEARER_TOKEN}")),
            (wreq::header::COOKIE, self.session.cookie_header()),
            (
                wreq::header::HeaderName::from_static("x-csrf-token"),
                self.session.csrf_token.clone(),
            ),
            (
                wreq::header::HeaderName::from_static("x-twitter-active-user"),
                "yes".into(),
            ),
            (
                wreq::header::HeaderName::from_static("x-twitter-auth-type"),
                "OAuth2Session".into(),
            ),
            (
                wreq::header::HeaderName::from_static("x-twitter-client-language"),
                self.session.language.clone(),
            ),
            (ACCEPT, "*/*".into()),
            (wreq::header::ORIGIN, "https://x.com".into()),
            (wreq::header::REFERER, "https://x.com/".into()),
        ];
        for (name, value) in values {
            headers.insert(
                name,
                HeaderValue::from_str(&value).map_err(|_| TransportError::InvalidHeader)?,
            );
        }
        if method == Method::POST {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            // The web client posts from the composer; mirror its Referer.
            headers.insert(
                wreq::header::REFERER,
                HeaderValue::from_static("https://x.com/compose/post"),
            );
        }
        if url.host_str() == Some("api.x.com") {
            // DM traffic hits api.x.com (a sibling of x.com) — the browser sends
            // the chat Referer and a same-site Sec-Fetch-Site (twitter-cli parity).
            headers.insert(
                wreq::header::REFERER,
                HeaderValue::from_static("https://x.com/i/chat"),
            );
            headers.insert(
                wreq::header::HeaderName::from_static("sec-fetch-site"),
                HeaderValue::from_static("same-site"),
            );
        }
        if let Some(transaction_id) = transaction_id {
            headers.insert(
                wreq::header::HeaderName::from_static("x-client-transaction-id"),
                HeaderValue::from_str(transaction_id).map_err(|_| TransportError::InvalidHeader)?,
            );
        }
        Ok(headers)
    }
}

#[async_trait]
impl JsonTransport for WreqXTransport {
    async fn request(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
    ) -> Result<Value, TransportError> {
        let transaction_id = self.transaction_id(&method, &url).await;
        for attempt in 0..=self.max_retries {
            let mut request = self
                .client
                .request(method.clone(), url.as_str())
                .headers(self.headers(&url, &method, transaction_id.as_deref())?);
            if let Some(body) = &body {
                request = request.json(body);
            }
            let response = request
                .send()
                .await
                .map_err(|error| TransportError::Network(error.to_string()))?;
            let status = response.status();
            let text = response
                .text()
                .await
                .map_err(|error| TransportError::Network(error.to_string()))?;
            if status.as_u16() == 429 && attempt < self.max_retries {
                sleep(Duration::from_secs(2_u64.pow(attempt as u32 + 1))).await;
                continue;
            }
            if !status.is_success() {
                return Err(TransportError::Http {
                    status: status.as_u16(),
                    code: serde_json::from_str::<Value>(&text)
                        .ok()
                        .and_then(|value| value.pointer("/errors/0/code").and_then(Value::as_i64)),
                    message: text.chars().take(500).collect(),
                });
            }
            let payload: Value = serde_json::from_str(&text)
                .map_err(|error| TransportError::Network(error.to_string()))?;
            if let Some(error) = payload
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|errors| errors.first())
            {
                return Err(TransportError::ApiResponse {
                    status: status.as_u16(),
                    code: error.get("code").and_then(Value::as_i64),
                });
            }
            // Write mutations often bury the real failure at `data.<field>.errors`
            // rather than the top level (e.g. CreateTweet policy/visibility blocks).
            if let Some(error) = payload
                .get("data")
                .and_then(Value::as_object)
                .and_then(|data| data.values().find_map(|value| value.pointer("/errors/0")))
            {
                return Err(TransportError::ApiResponse {
                    status: status.as_u16(),
                    code: error.get("code").and_then(Value::as_i64),
                });
            }
            return Ok(payload);
        }
        Err(TransportError::Api("retry budget exhausted".into()))
    }

    async fn request_form(
        &self,
        method: Method,
        url: Url,
        form: BTreeMap<String, String>,
    ) -> Result<Value, TransportError> {
        let transaction_id = self.transaction_id(&method, &url).await;
        let mut headers = self.headers(&url, &method, transaction_id.as_deref())?;
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        let response = self
            .client
            .request(method.clone(), url.as_str())
            .headers(headers)
            .form(&form)
            .send()
            .await
            .map_err(|error| TransportError::Network(error.to_string()))?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|error| TransportError::Network(error.to_string()))?;
        if !status.is_success() {
            return Err(TransportError::Http {
                status: status.as_u16(),
                code: serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|value| value.pointer("/errors/0/code").and_then(Value::as_i64)),
                message: text.chars().take(500).collect(),
            });
        }
        if text.trim().is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_str(&text).map_err(|error| TransportError::Network(error.to_string()))
        }
    }

    async fn fetch_text(&self, url: Url) -> Result<String, TransportError> {
        // Plain GET with the browser emulation + session cookie — no X GraphQL
        // auth headers. Used to refresh stale queryIds from live sources.
        let response = self
            .client
            .get(url.as_str())
            .header(wreq::header::COOKIE, self.session.cookie_header())
            .header(wreq::header::REFERER, "https://x.com/")
            .send()
            .await
            .map_err(|error| TransportError::Network(error.to_string()))?;
        response
            .text()
            .await
            .map_err(|error| TransportError::Network(error.to_string()))
    }
}
