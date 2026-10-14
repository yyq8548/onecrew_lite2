use onecrew_x_transport::{JsonTransport, Method, TransportError};
use serde_json::{Value, json};
use url::Url;

use crate::graphql::compact_features;

use super::{XClient, XClientError};

const GRAPHQL_BASE: &str = "https://x.com/i/api/graphql";

impl<T: JsonTransport> XClient<T> {
    pub async fn get(
        &self,
        operation: &str,
        variables: Value,
        features: Value,
    ) -> Result<Value, XClientError> {
        let fallback = self.query_ids.get(operation).copied();
        let query_id = self
            .resolve_query_id(operation, true)
            .await
            .ok_or_else(|| XClientError::UnknownOperation(operation.into()))?;
        let using_fallback = fallback.is_some_and(|f| query_id == f);
        let url = build_get_url(&query_id, operation, &variables, &features)?;
        match self.transport.request(Method::GET, url, None).await {
            Ok(value) => Ok(value),
            // A stale fallback queryId 404/422s → refresh from live sources + retry.
            Err(error) if using_fallback && is_stale_query_id(&error) => {
                self.invalidate(operation);
                let refreshed = self
                    .resolve_query_id(operation, false)
                    .await
                    .ok_or_else(|| XClientError::UnknownOperation(operation.into()))?;
                let url = build_get_url(&refreshed, operation, &variables, &features)?;
                Ok(self.transport.request(Method::GET, url, None).await?)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub async fn post(
        &self,
        operation: &str,
        variables: Value,
        features: Option<Value>,
    ) -> Result<Value, XClientError> {
        let fallback = self.query_ids.get(operation).copied();
        let query_id = self
            .resolve_query_id(operation, true)
            .await
            .ok_or_else(|| XClientError::UnknownOperation(operation.into()))?;
        let using_fallback = fallback.is_some_and(|f| query_id == f);
        match self
            .post_once(operation, &query_id, &variables, features.as_ref())
            .await
        {
            Ok(value) => Ok(value),
            Err(error) if using_fallback && is_stale_query_id(&error) => {
                self.invalidate(operation);
                let refreshed = self
                    .resolve_query_id(operation, false)
                    .await
                    .ok_or_else(|| XClientError::UnknownOperation(operation.into()))?;
                Ok(self
                    .post_once(operation, &refreshed, &variables, features.as_ref())
                    .await?)
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn post_once(
        &self,
        operation: &str,
        query_id: &str,
        variables: &Value,
        features: Option<&Value>,
    ) -> Result<Value, TransportError> {
        let url = Url::parse(&format!("{GRAPHQL_BASE}/{query_id}/{operation}"))
            .map_err(|error| TransportError::Network(error.to_string()))?;
        let mut body = json!({ "variables": variables, "queryId": query_id });
        if let Some(features) = features {
            body["features"] = features.clone();
        }
        self.transport.request(Method::POST, url, Some(body)).await
    }
}

pub(super) fn add_cursor(variables: &mut Value, cursor: Option<&str>) {
    if let Some(cursor) = cursor {
        variables["cursor"] = Value::String(cursor.into());
    }
}

/// GraphQL GET URL with `false`-valued features dropped (twitter-cli parity —
/// keeps the URL short enough to avoid 414).
fn build_get_url(
    query_id: &str,
    operation: &str,
    variables: &Value,
    features: &Value,
) -> Result<Url, XClientError> {
    let mut url = Url::parse(&format!("{GRAPHQL_BASE}/{query_id}/{operation}"))
        .map_err(|error| XClientError::Url(error.to_string()))?;
    url.query_pairs_mut()
        .append_pair("variables", &variables.to_string())
        .append_pair("features", &compact_features(features).to_string());
    Ok(url)
}

/// A queryId that Twitter no longer recognises comes back as 404/422; that's the
/// signal to refresh it from live sources and retry.
fn is_stale_query_id(error: &TransportError) -> bool {
    matches!(error, TransportError::Http { status, .. } if *status == 404 || *status == 422)
}
