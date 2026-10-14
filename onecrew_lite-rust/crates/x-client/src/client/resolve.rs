use onecrew_x_transport::JsonTransport;
use url::Url;

use crate::graphql::{OPENAPI_URL, bundle_urls, openapi_query_id, scan_js_query_ids};

use super::XClient;

impl<T: JsonTransport> XClient<T> {
    /// Resolve a queryId, mirroring twitter-cli's `_resolve_query_id`: cache →
    /// hardcoded fallback (when `prefer_fallback`) → community openapi → x.com JS
    /// bundle scan → fallback. `prefer_fallback=false` is the recovery path used
    /// after a fallback queryId returned 404/422.
    pub(super) async fn resolve_query_id(
        &self,
        operation: &str,
        prefer_fallback: bool,
    ) -> Option<String> {
        if let Some(id) = self.cached(operation) {
            return Some(id);
        }
        let fallback = self.query_ids.get(operation).copied();
        if prefer_fallback && let Some(id) = fallback {
            self.cache(operation, id);
            return Some(id.to_string());
        }
        // Live source 1: community twitter-openapi (actively maintained).
        if let Some(id) = self.resolve_openapi(operation).await {
            self.cache(operation, &id);
            return Some(id);
        }
        // Live source 2: scan the current x.com JS bundles (caches all it finds).
        self.scan_bundles().await;
        if let Some(id) = self.cached(operation) {
            return Some(id);
        }
        fallback.map(|id| {
            self.cache(operation, id);
            id.to_string()
        })
    }

    fn cached(&self, operation: &str) -> Option<String> {
        self.resolved
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(operation)
            .cloned()
    }

    fn cache(&self, operation: &str, query_id: &str) {
        self.resolved
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(operation.to_string(), query_id.to_string());
    }

    pub(super) fn invalidate(&self, operation: &str) {
        self.resolved
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(operation);
    }

    async fn resolve_openapi(&self, operation: &str) -> Option<String> {
        let url = Url::parse(OPENAPI_URL).ok()?;
        let text = self.transport.fetch_text(url).await.ok()?;
        openapi_query_id(&text, operation)
    }

    /// Fetch x.com, then each client-web JS bundle, mining `operationName→queryId`
    /// into the cache. One-shot per session (a rare recovery path).
    async fn scan_bundles(&self) {
        if *self
            .scanned
            .lock()
            .unwrap_or_else(|error| error.into_inner())
        {
            return;
        }
        let Ok(home) = Url::parse("https://x.com/") else {
            return;
        };
        let Ok(html) = self.transport.fetch_text(home).await else {
            return; // transient — allow a later retry (don't mark scanned)
        };
        *self
            .scanned
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = true;
        for bundle in bundle_urls(&html) {
            let Ok(url) = Url::parse(&bundle) else {
                continue;
            };
            if let Ok(js) = self.transport.fetch_text(url).await {
                let mut cache = self
                    .resolved
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                for (operation, query_id) in scan_js_query_ids(&js) {
                    cache.entry(operation).or_insert(query_id);
                }
            }
        }
    }
}
