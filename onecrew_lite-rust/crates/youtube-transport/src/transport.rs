use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use reqwest::{Client, redirect::Policy};
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;
use url::Url;

use crate::{YoutubeApiFailureKind, YoutubeTransportError};

const API_BASE: &str = "https://www.googleapis.com/youtube/v3/";
const MAX_API_BODY: usize = 8 << 20;
const REQUEST_LIMITER_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
/// Initial maximum number of requests a YouTube credential may have in flight.
pub const MAX_IN_FLIGHT_REQUESTS: usize = 6;
/// Shared request-budget handle used by credential-owned transports.
pub type YoutubeRequestLimiter = Arc<Semaphore>;

/// Process-local sequence for safe provider request events. It is deliberately
/// independent of credentials and contains no URL, query, body, or secret.
static PROVIDER_REQUEST_COUNT: AtomicU64 = AtomicU64::new(0);

/// Stateless YouTube HTTP transport. It owns one API key but never exposes it
/// through `Debug`, response models, or error strings.
#[derive(Clone)]
pub struct YoutubeTransport {
    api_key: Arc<str>,
    api_base: Url,
    api: Client,
    public: Client,
    request_limiter: Arc<Semaphore>,
}

impl YoutubeTransport {
    pub fn new(api_key: impl Into<String>) -> Result<Self, YoutubeTransportError> {
        Self::with_request_limiter(api_key, Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS)))
    }

    /// Builds a transport using a caller-owned request budget. The API and
    /// public-page clients retain their own credentials, while every request
    /// acquires from this shared semaphore through response-body decoding.
    pub fn with_request_limiter(
        api_key: impl Into<String>,
        request_limiter: Arc<Semaphore>,
    ) -> Result<Self, YoutubeTransportError> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(YoutubeTransportError::InvalidUrl);
        }
        let api = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| YoutubeTransportError::request("building API client", &error))?;
        let public = Client::builder()
            .timeout(Duration::from_secs(20))
            // A redirect target would need the same explicit host check as the
            // initial URL, so redirects are refused outright rather than followed:
            // a 3xx surfaces as a non-success status like any other.
            .redirect(Policy::none())
            .user_agent("Mozilla/5.0 (compatible; AgentPlatformLocal/0.1)")
            .build()
            .map_err(|error| YoutubeTransportError::request("building public client", &error))?;
        Ok(Self {
            api_key: Arc::from(api_key),
            api_base: Url::parse(API_BASE).map_err(|_| YoutubeTransportError::InvalidUrl)?,
            api,
            public,
            request_limiter,
        })
    }

    #[cfg(test)]
    fn for_test(
        api_key: impl Into<String>,
        api_base: Url,
        request_limiter: Arc<Semaphore>,
    ) -> Result<Self, YoutubeTransportError> {
        let mut transport = Self::with_request_limiter(api_key, request_limiter)?;
        transport.api_base = api_base;
        Ok(transport)
    }

    pub async fn api_get<T>(
        &self,
        resource: &str,
        query: &[(&str, String)],
    ) -> Result<T, YoutubeTransportError>
    where
        T: DeserializeOwned,
    {
        let request_count = PROVIDER_REQUEST_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let started = Instant::now();
        let limiter_started = Instant::now();
        let permit_result = tokio::time::timeout(
            REQUEST_LIMITER_WAIT_TIMEOUT,
            Arc::clone(&self.request_limiter).acquire_owned(),
        )
        .await;
        let wait_ms = limiter_started.elapsed().as_millis();
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=limiter_wait phase=provider class=api request_count={} wait_ms={}",
            request_count,
            wait_ms,
        );
        let _permit = match permit_result {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                let error = YoutubeTransportError::Request {
                    operation: "acquiring request permit",
                    failure_kind: crate::YoutubeRequestFailureKind::Network,
                };
                log::debug!(
                    target: "onecrew.youtube",
                    "youtube provider event=request phase=provider class=api request_count={} elapsed_ms={} outcome=failure",
                    request_count,
                    started.elapsed().as_millis(),
                );
                return Err(error);
            }
            Err(_) => {
                let error = YoutubeTransportError::Request {
                    operation: "waiting for request permit",
                    failure_kind: crate::YoutubeRequestFailureKind::Timeout,
                };
                log::debug!(
                    target: "onecrew.youtube",
                    "youtube provider event=request phase=provider class=api request_count={} elapsed_ms={} outcome=failure",
                    request_count,
                    started.elapsed().as_millis(),
                );
                return Err(error);
            }
        };
        let result = async {
            let mut url = self
                .api_base
                .join(resource)
                .map_err(|_| YoutubeTransportError::InvalidUrl)?;
            {
                let mut pairs = url.query_pairs_mut();
                for (key, value) in query {
                    pairs.append_pair(key, value);
                }
                pairs.append_pair("key", &self.api_key);
            }
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=request_started class=api request_count={} occupied_permits={} request_budget={}",
                request_count,
                MAX_IN_FLIGHT_REQUESTS.saturating_sub(self.request_limiter.available_permits()),
                MAX_IN_FLIGHT_REQUESTS,
            );
            let response =
                self.api.get(url).send().await.map_err(|error| {
                    YoutubeTransportError::request("sending API request", &error)
                })?;
            let status = response.status();
            let body = read_limited(response, MAX_API_BODY, "reading API response").await?;
            if !status.is_success() {
                return Err(api_failure(status.as_u16(), &body));
            }
            serde_json::from_slice(&body).map_err(|_| YoutubeTransportError::Decode)
        }
        .await;
        if let Err(error) = &result {
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=request_failure phase=provider class=api request_count={} failure_kind={} http_status={:?}",
                request_count,
                safe_failure_kind(error),
                safe_http_status(error),
            );
        }
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=request phase=provider class=api request_count={} elapsed_ms={} outcome={}",
            request_count,
            started.elapsed().as_millis(),
            if result.is_ok() { "success" } else { "failure" },
        );
        result
    }

    /// Fetch one known public page without executing JavaScript. The caller
    /// supplies an allow-list because discovery links are untrusted input.
    pub async fn public_text(
        &self,
        url: &Url,
        allowed_hosts: &BTreeSet<String>,
        limit: usize,
    ) -> Result<String, YoutubeTransportError> {
        let request_count = PROVIDER_REQUEST_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        let started = Instant::now();
        let limiter_started = Instant::now();
        let permit_result = tokio::time::timeout(
            REQUEST_LIMITER_WAIT_TIMEOUT,
            Arc::clone(&self.request_limiter).acquire_owned(),
        )
        .await;
        let wait_ms = limiter_started.elapsed().as_millis();
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=limiter_wait phase=provider class=public_page request_count={} wait_ms={}",
            request_count,
            wait_ms,
        );
        let _permit = match permit_result {
            Ok(Ok(permit)) => permit,
            Ok(Err(_)) => {
                let error = YoutubeTransportError::Request {
                    operation: "acquiring request permit",
                    failure_kind: crate::YoutubeRequestFailureKind::Network,
                };
                log::debug!(
                    target: "onecrew.youtube",
                    "youtube provider event=request phase=provider class=public_page request_count={} elapsed_ms={} outcome=failure",
                    request_count,
                    started.elapsed().as_millis(),
                );
                return Err(error);
            }
            Err(_) => {
                let error = YoutubeTransportError::Request {
                    operation: "waiting for request permit",
                    failure_kind: crate::YoutubeRequestFailureKind::Timeout,
                };
                log::debug!(
                    target: "onecrew.youtube",
                    "youtube provider event=request phase=provider class=public_page request_count={} elapsed_ms={} outcome=failure",
                    request_count,
                    started.elapsed().as_millis(),
                );
                return Err(error);
            }
        };
        let result = async {
            if url.scheme() != "https" {
                return Err(YoutubeTransportError::InvalidUrl);
            }
            let host = url
                .host_str()
                .map(|value| value.trim_start_matches("www.").to_ascii_lowercase())
                .ok_or(YoutubeTransportError::InvalidUrl)?;
            if !allowed_hosts.contains(&host) {
                return Err(YoutubeTransportError::HostNotAllowed);
            }
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=request_started class=public_page request_count={} occupied_permits={} request_budget={}",
                request_count,
                MAX_IN_FLIGHT_REQUESTS.saturating_sub(self.request_limiter.available_permits()),
                MAX_IN_FLIGHT_REQUESTS,
            );
            let response =
                self.public.get(url.clone()).send().await.map_err(|error| {
                    YoutubeTransportError::request("fetching public page", &error)
                })?;
            let status = response.status();
            let body = read_limited(response, limit, "reading public page").await?;
            if !status.is_success() {
                return Err(YoutubeTransportError::Api {
                    status: status.as_u16(),
                    failure_kind: if status.is_server_error() {
                        YoutubeApiFailureKind::Server
                    } else {
                        YoutubeApiFailureKind::Client
                    },
                    reason: None,
                });
            }
            Ok(String::from_utf8_lossy(&body).into_owned())
        }
        .await;
        if let Err(error) = &result {
            log::debug!(
                target: "onecrew.youtube",
                "youtube provider event=request_failure phase=provider class=public_page request_count={} failure_kind={} http_status={:?}",
                request_count,
                safe_failure_kind(error),
                safe_http_status(error),
            );
        }
        log::debug!(
            target: "onecrew.youtube",
            "youtube provider event=request phase=provider class=public_page request_count={} elapsed_ms={} outcome={}",
            request_count,
            started.elapsed().as_millis(),
            if result.is_ok() { "success" } else { "failure" },
        );
        result
    }
}

fn safe_failure_kind(error: &YoutubeTransportError) -> &'static str {
    match error {
        YoutubeTransportError::InvalidUrl => "invalid_url",
        YoutubeTransportError::HostNotAllowed => "host_not_allowed",
        YoutubeTransportError::Request { failure_kind, .. } => match failure_kind {
            crate::YoutubeRequestFailureKind::Timeout => "timeout",
            crate::YoutubeRequestFailureKind::Connect => "connect",
            crate::YoutubeRequestFailureKind::Body => "body",
            crate::YoutubeRequestFailureKind::Decode => "decode",
            crate::YoutubeRequestFailureKind::Status => "status",
            crate::YoutubeRequestFailureKind::Network => "network",
        },
        YoutubeTransportError::Api { failure_kind, .. } => match failure_kind {
            crate::YoutubeApiFailureKind::CredentialRejected => "credential_rejected",
            crate::YoutubeApiFailureKind::AccessNotConfigured => "access_not_configured",
            crate::YoutubeApiFailureKind::QuotaExceeded => "quota_exceeded",
            crate::YoutubeApiFailureKind::RateLimited => "rate_limited",
            crate::YoutubeApiFailureKind::AmbiguousForbidden => "ambiguous_forbidden",
            crate::YoutubeApiFailureKind::Client => "client",
            crate::YoutubeApiFailureKind::Server => "server",
        },
        YoutubeTransportError::BodyTooLarge { .. } => "body_too_large",
        YoutubeTransportError::Decode => "decode",
    }
}

fn safe_http_status(error: &YoutubeTransportError) -> Option<u16> {
    match error {
        YoutubeTransportError::Api { status, .. } => Some(*status),
        _ => None,
    }
}

/// One unsuccessful API response: its failure kind and its first native
/// reason token. The body is parsed once here and then discarded.
fn api_failure(status: u16, body: &[u8]) -> YoutubeTransportError {
    let value = serde_json::from_slice::<serde_json::Value>(body).ok();
    YoutubeTransportError::Api {
        status,
        failure_kind: classify_api_failure(status, value.as_ref()),
        reason: first_reason_token(value.as_ref()),
    }
}

/// The first `error.errors[].reason` of an API failure. Only the protocol's
/// machine field is evidence, so it is kept only in the bounded token shape
/// Instagram's `error_type` keeps: 1 to 80 ASCII letters, digits or
/// underscores. A first reason of any other shape is dropped, never replaced
/// by a later one.
fn first_reason_token(value: Option<&serde_json::Value>) -> Option<String> {
    value?
        .pointer("/error/errors")?
        .as_array()?
        .iter()
        .find_map(|error| error.get("reason"))?
        .as_str()
        .filter(|reason| {
            !reason.is_empty()
                && reason.len() <= 80
                && reason
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned)
}

fn classify_api_failure(status: u16, value: Option<&serde_json::Value>) -> YoutubeApiFailureKind {
    if status == 429 {
        return YoutubeApiFailureKind::RateLimited;
    }
    if status >= 500 {
        return YoutubeApiFailureKind::Server;
    }
    let reasons = value
        .and_then(|value| value.pointer("/error/errors"))
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|error| error.get("reason").and_then(serde_json::Value::as_str))
        .map(|reason| reason.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let message = value
        .and_then(|value| value.pointer("/error/message"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if reasons.iter().any(|reason| {
        matches!(
            reason.as_str(),
            "keyinvalid" | "iprefererblocked" | "forbiddenfornonpartnere"
        )
    }) || message.contains("api key not valid")
    {
        return YoutubeApiFailureKind::CredentialRejected;
    }
    if reasons.iter().any(|reason| reason == "accessnotconfigured") {
        return YoutubeApiFailureKind::AccessNotConfigured;
    }
    if reasons
        .iter()
        .any(|reason| matches!(reason.as_str(), "quotaexceeded" | "dailylimitexceeded"))
    {
        return YoutubeApiFailureKind::QuotaExceeded;
    }
    if reasons.iter().any(|reason| {
        matches!(
            reason.as_str(),
            "ratelimitexceeded" | "userratelimitexceeded"
        )
    }) {
        return YoutubeApiFailureKind::RateLimited;
    }
    if status == 401 {
        return YoutubeApiFailureKind::CredentialRejected;
    }
    if status == 403 {
        return YoutubeApiFailureKind::AmbiguousForbidden;
    }
    YoutubeApiFailureKind::Client
}

async fn read_limited(
    mut response: reqwest::Response,
    limit: usize,
    operation: &'static str,
) -> Result<Vec<u8>, YoutubeTransportError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| YoutubeTransportError::request(operation, &error))?
    {
        if body.len().saturating_add(chunk.len()) > limit {
            return Err(YoutubeTransportError::BodyTooLarge { limit });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use serde_json::Value;
    use tokio::sync::Semaphore;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::{Notify, mpsc},
    };
    use url::Url;

    use super::{
        MAX_IN_FLIGHT_REQUESTS, YoutubeApiFailureKind, YoutubeTransport, YoutubeTransportError,
        api_failure,
    };

    fn kind(status: u16, body: &[u8]) -> YoutubeApiFailureKind {
        match api_failure(status, body) {
            YoutubeTransportError::Api {
                status: failed,
                failure_kind,
                ..
            } if failed == status => failure_kind,
            other => panic!("an unsuccessful API response is an API failure: {other:?}"),
        }
    }

    fn reason(status: u16, body: &serde_json::Value) -> Option<String> {
        match api_failure(status, &serde_json::to_vec(body).unwrap()) {
            YoutubeTransportError::Api { reason, .. } => reason,
            other => panic!("an unsuccessful API response is an API failure: {other:?}"),
        }
    }

    #[tokio::test]
    async fn cloned_transports_share_the_global_request_limit() {
        let transport = YoutubeTransport::new("test-key").unwrap();
        let cloned = transport.clone();
        let permits = cloned
            .request_limiter
            .clone()
            .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32)
            .await
            .unwrap();

        assert!(transport.request_limiter.try_acquire().is_err());
        drop(permits);
        assert!(transport.request_limiter.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn separately_constructed_transports_can_share_one_request_budget() {
        let limiter = Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS));
        let first = YoutubeTransport::with_request_limiter("first-key", Arc::clone(&limiter))
            .expect("first transport");
        let second = YoutubeTransport::with_request_limiter("second-key", limiter.clone())
            .expect("second transport");
        let permits = limiter
            .clone()
            .acquire_many_owned(MAX_IN_FLIGHT_REQUESTS as u32)
            .await
            .unwrap();

        assert!(first.request_limiter.try_acquire().is_err());
        assert!(second.request_limiter.try_acquire().is_err());
        drop(permits);
        assert!(first.request_limiter.try_acquire().is_ok());
        assert!(second.request_limiter.try_acquire().is_ok());
    }

    #[tokio::test]
    async fn real_api_requests_from_old_and_rotated_transports_never_exceed_six_in_flight() {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());
        let released = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (started_tx, mut started_rx) = mpsc::channel(8);
        let server_active = Arc::clone(&active);
        let server_maximum = Arc::clone(&maximum);
        let server_release = Arc::clone(&release);
        let server_released = Arc::clone(&released);
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let active = Arc::clone(&server_active);
                let maximum = Arc::clone(&server_maximum);
                let release = Arc::clone(&server_release);
                let released = Arc::clone(&server_released);
                let started = started_tx.clone();
                tokio::spawn(async move {
                    let mut request = [0_u8; 4096];
                    let mut received = 0;
                    while received < request.len()
                        && !request[..received]
                            .windows(4)
                            .any(|window| window == b"\r\n\r\n")
                    {
                        let read = stream.read(&mut request[received..]).await.unwrap();
                        if read == 0 {
                            return;
                        }
                        received += read;
                    }
                    let running = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(running, Ordering::SeqCst);
                    started.send(()).await.unwrap();
                    while !released.load(Ordering::SeqCst) {
                        release.notified().await;
                    }
                    let response = b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}";
                    stream.write_all(response).await.unwrap();
                    active.fetch_sub(1, Ordering::SeqCst);
                });
            }
        });
        let limiter = Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS));
        let old = YoutubeTransport::for_test(
            "old-key",
            Url::parse(&format!("http://{address}/")).unwrap(),
            Arc::clone(&limiter),
        )
        .unwrap();
        let rotated = YoutubeTransport::for_test(
            "rotated-key",
            Url::parse(&format!("http://{address}/")).unwrap(),
            limiter,
        )
        .unwrap();
        let requests = (0..8)
            .map(|index| {
                let transport = if index < 4 {
                    old.clone()
                } else {
                    rotated.clone()
                };
                tokio::spawn(async move {
                    transport
                        .api_get::<Value>("channels", &[("part", "id".into())])
                        .await
                })
            })
            .collect::<Vec<_>>();
        for _ in 0..MAX_IN_FLIGHT_REQUESTS {
            started_rx.recv().await.unwrap();
        }
        assert_eq!(active.load(Ordering::SeqCst), MAX_IN_FLIGHT_REQUESTS);
        assert_eq!(maximum.load(Ordering::SeqCst), MAX_IN_FLIGHT_REQUESTS);
        released.store(true, Ordering::SeqCst);
        release.notify_waiters();
        for request in requests {
            request.await.unwrap().unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), MAX_IN_FLIGHT_REQUESTS);
        server.abort();
    }

    #[test]
    fn api_failure_classification_is_typed_before_provider_text_is_discarded() {
        let body = |reason: &str, message: &str| {
            serde_json::to_vec(&serde_json::json!({
                "error": { "message": message, "errors": [{ "reason": reason }] }
            }))
            .unwrap()
        };
        assert_eq!(
            kind(
                403,
                &body("keyInvalid", "API key not valid PROVIDER_SECRET")
            ),
            YoutubeApiFailureKind::CredentialRejected
        );
        assert_eq!(
            kind(403, &body("accessNotConfigured", "raw body")),
            YoutubeApiFailureKind::AccessNotConfigured
        );
        assert_eq!(
            kind(403, &body("quotaExceeded", "raw body")),
            YoutubeApiFailureKind::QuotaExceeded
        );
        assert_eq!(
            kind(403, &body("rateLimitExceeded", "raw body")),
            YoutubeApiFailureKind::RateLimited
        );
        assert_eq!(
            kind(403, &body("unknownReason", "PROVIDER_BODY_SENTINEL")),
            YoutubeApiFailureKind::AmbiguousForbidden
        );
        assert_eq!(
            kind(503, b"PROVIDER_BODY_SENTINEL"),
            YoutubeApiFailureKind::Server
        );
    }

    #[test]
    fn an_api_failure_keeps_only_its_first_reason_as_a_bounded_token() {
        let errors = |reasons: &[serde_json::Value]| {
            serde_json::json!({
                "error": {
                    "code": 403,
                    "message": "API key not valid PROVIDER_BODY_SENTINEL",
                    "errors": reasons
                        .iter()
                        .map(|reason| serde_json::json!({
                            "reason": reason,
                            "message": "PROVIDER_BODY_SENTINEL",
                            "domain": "usageLimits"
                        }))
                        .collect::<Vec<_>>()
                }
            })
        };
        // The first reason survives as the provider spelled it, whatever the
        // status and failure kind, while the body's text is discarded.
        for status in [400, 403, 429, 503] {
            let failure = api_failure(
                status,
                &serde_json::to_vec(&errors(&[
                    serde_json::json!("quotaExceeded"),
                    serde_json::json!("keyInvalid"),
                ]))
                .unwrap(),
            );
            let YoutubeTransportError::Api { reason, .. } = &failure else {
                panic!("an unsuccessful API response is an API failure");
            };
            assert_eq!(reason.as_deref(), Some("quotaExceeded"), "{status}");
            for wire in [
                failure.to_string(),
                format!("{failure:?}"),
                serde_json::to_string(&failure).unwrap(),
            ] {
                assert!(!wire.contains("PROVIDER_BODY_SENTINEL"), "{wire}");
                assert!(!wire.contains("usageLimits"), "{wire}");
            }
        }
        // An entry without a reason does not count as the first reason.
        assert_eq!(
            reason(
                403,
                &serde_json::json!({"error": {"errors": [
                    {"message": "no reason here"},
                    {"reason": "rateLimitExceeded"}
                ]}})
            )
            .as_deref(),
            Some("rateLimitExceeded")
        );
        let longest = "a".repeat(80);
        assert_eq!(
            reason(403, &errors(&[serde_json::json!(longest)])),
            Some(longest)
        );
        // A first reason that is not a bounded token, whatever its JSON type,
        // is dropped and never replaced by the valid reason after it.
        for first in [
            serde_json::json!(""),
            serde_json::json!("a".repeat(81)),
            serde_json::json!("https://example.test/?key=SECRET"),
            serde_json::json!("quota Exceeded"),
            serde_json::json!("quota-exceeded"),
            serde_json::json!("配额"),
            serde_json::json!(42),
            serde_json::json!(null),
            serde_json::json!(["keyInvalid"]),
        ] {
            assert_eq!(
                reason(
                    403,
                    &errors(&[first.clone(), serde_json::json!("keyInvalid")])
                ),
                None,
                "{first}"
            );
        }
        for body in [
            serde_json::json!({"error": {"message": "no errors"}}),
            serde_json::json!({"error": {"errors": []}}),
            serde_json::json!({"error": {"errors": [{"reason": 42}]}}),
            serde_json::json!(["not", "an", "object"]),
        ] {
            assert_eq!(reason(403, &body), None, "{body}");
        }
        assert!(matches!(
            api_failure(403, b"PROVIDER_BODY_SENTINEL is not JSON"),
            YoutubeTransportError::Api {
                failure_kind: YoutubeApiFailureKind::AmbiguousForbidden,
                reason: None,
                ..
            }
        ));
    }
}
