//! Narrow SDK transport/admission seam for the Resend provider.
//!
//! `resend-rs` owns HTTP request construction, endpoint serialization, and
//! response decoding. This crate only owns the shared reqwest client seam,
//! bounded admission, pacing, and provider-directed pauses. Read compatibility
//! decoding lives in `resend-platform`, not here.

use std::sync::Arc;

use onecrew_email_contracts::{EmailError, ProviderRateLimitStatus};
use reqwest::Client;

mod quota;

pub use quota::ResendOpPriority;
use quota::{ResendQuota, ResendQuotaPermit};

const LIVE_BASE_URL: &str = "https://api.resend.com";
pub const MAX_ATTEMPTS: u32 = 4;
pub const BACKOFF_BASE: tokio::time::Duration = tokio::time::Duration::from_millis(500);

#[derive(Debug)]
pub struct ResendTransport {
    http: Client,
    quota: ResendQuota,
    base_url: reqwest::Url,
}

impl ResendTransport {
    pub fn new() -> Result<Self, EmailError> {
        Self::with_base_url(LIVE_BASE_URL)
    }

    pub fn with_base_url(base_url: impl AsRef<str>) -> Result<Self, EmailError> {
        let http = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| EmailError::InvalidConfig(error.to_string()))?;
        let base_url = reqwest::Url::parse(base_url.as_ref()).map_err(|error| {
            EmailError::InvalidConfig(format!("invalid Resend base URL: {error}"))
        })?;
        Ok(Self {
            http,
            quota: ResendQuota::default(),
            base_url,
        })
    }

    pub fn sdk_http_client(&self) -> Client {
        self.http.clone()
    }

    pub fn sdk_base_url(&self) -> reqwest::Url {
        self.base_url.clone()
    }

    pub async fn acquire(&self, priority: ResendOpPriority) -> ResendQuotaPermit<'_> {
        self.quota.acquire(priority).await
    }

    pub fn observe(&self, status: ProviderRateLimitStatus) {
        self.quota.observe(status);
    }

    pub fn report_rate_limited(&self, status: ProviderRateLimitStatus) -> tokio::time::Duration {
        self.quota.report_rate_limited(status)
    }

    pub fn base_url(&self) -> &reqwest::Url {
        &self.base_url
    }

    pub fn http_client(&self) -> &Client {
        &self.http
    }
}

pub type SharedResendTransport = Arc<ResendTransport>;
