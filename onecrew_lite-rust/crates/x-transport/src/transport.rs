use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use url::Url;
use wreq::Method;

use crate::TransportError;

#[async_trait]
pub trait JsonTransport: Send + Sync {
    async fn request(
        &self,
        method: Method,
        url: Url,
        body: Option<Value>,
    ) -> Result<Value, TransportError>;

    async fn request_form(
        &self,
        method: Method,
        url: Url,
        form: BTreeMap<String, String>,
    ) -> Result<Value, TransportError>;

    /// GET a URL as plain text (browser emulation + session cookie, no GraphQL
    /// auth semantics). Used to resolve GraphQL queryIds from live sources — the
    /// community twitter-openapi file and x.com JS bundles — when a hardcoded
    /// fallback queryId goes stale.
    async fn fetch_text(&self, url: Url) -> Result<String, TransportError>;
}
