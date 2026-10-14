//! Bounded no-redirect Worker B HTTP transport.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Method, StatusCode, redirect::Policy};
use url::Url;
use zeroize::Zeroize;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
pub(super) const DEFAULT_MAX_RESPONSE_BYTES: usize = 64 * 1024;
pub(super) const MAX_PLAN_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct HttpRequest {
    pub(super) method: Method,
    pub(super) endpoint: Url,
    pub(super) authorization: Option<String>,
    pub(super) idempotency_key: Option<String>,
    pub(super) creator_bundle_version: Option<u16>,
    pub(super) json_body: bool,
    pub(super) body: Vec<u8>,
    pub(super) max_response_bytes: usize,
}

impl Drop for HttpRequest {
    fn drop(&mut self) {
        if let Some(authorization) = &mut self.authorization {
            authorization.zeroize();
        }
        if let Some(idempotency_key) = &mut self.idempotency_key {
            idempotency_key.zeroize();
        }
        self.body.zeroize();
    }
}

pub(super) struct HttpResponse {
    pub(super) status: StatusCode,
    pub(super) body: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum HttpFailure {
    Transport,
    ResponseTooLarge,
}

#[async_trait]
pub(super) trait HttpTransport: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpFailure>;
}

pub(super) struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub(super) fn new() -> Result<Self, ()> {
        let client = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| ())?;
        Ok(Self { client })
    }
}

#[async_trait]
impl HttpTransport for ReqwestTransport {
    async fn send(&self, mut request: HttpRequest) -> Result<HttpResponse, HttpFailure> {
        let mut builder = self
            .client
            .request(request.method.clone(), request.endpoint.clone())
            .header("Cache-Control", "no-store");
        if let Some(authorization) = request.authorization.take() {
            builder = builder.header("Authorization", authorization);
        }
        if let Some(idempotency_key) = request.idempotency_key.take() {
            builder = builder.header("Idempotency-Key", idempotency_key);
        }
        if let Some(version) = request.creator_bundle_version {
            builder = builder.header("X-OneCrew-Creator-Bundle-Version", version.to_string());
        }
        if request.json_body {
            builder = builder.header("Content-Type", "application/json");
        }
        if !request.body.is_empty() {
            builder = builder.body(std::mem::take(&mut request.body));
        }

        let mut response = builder.send().await.map_err(|_| HttpFailure::Transport)?;
        if response
            .content_length()
            .is_some_and(|length| length > request.max_response_bytes as u64)
        {
            return Err(HttpFailure::ResponseTooLarge);
        }
        let status = response.status();
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| HttpFailure::Transport)? {
            if body.len().saturating_add(chunk.len()) > request.max_response_bytes {
                return Err(HttpFailure::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(HttpResponse { status, body })
    }
}
