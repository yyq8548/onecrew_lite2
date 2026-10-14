use std::{fmt, time::Duration};

use axum::http::StatusCode;
use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    model::ProtocolVersion,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};

use super::McpAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpHealthReport {
    pub tool_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpHealthErrorKind {
    InvalidConfiguration,
    Unreachable,
    Unauthorized,
    Protocol,
    MissingRequiredTool,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct McpHealthError {
    pub kind: McpHealthErrorKind,
}

impl McpHealthError {
    fn new(kind: McpHealthErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for McpHealthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            McpHealthErrorKind::InvalidConfiguration => "invalid MCP health probe configuration",
            McpHealthErrorKind::Unreachable => "MCP endpoint is unreachable",
            McpHealthErrorKind::Unauthorized => "MCP endpoint rejected authorization",
            McpHealthErrorKind::Protocol => "MCP protocol health check failed",
            McpHealthErrorKind::MissingRequiredTool => {
                "MCP tool discovery is missing a required stateless meta tool"
            }
            McpHealthErrorKind::Timeout => "MCP health check timed out",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for McpHealthError {}

impl McpAdapter {
    pub async fn probe(
        endpoint: &str,
        access_token: &str,
        timeout: Duration,
    ) -> Result<McpHealthReport, McpHealthError> {
        if endpoint.trim().is_empty() || access_token.trim().is_empty() || timeout.is_zero() {
            return Err(McpHealthError::new(
                McpHealthErrorKind::InvalidConfiguration,
            ));
        }
        tokio::time::timeout(timeout, probe_protocol(endpoint, access_token))
            .await
            .map_err(|_| McpHealthError::new(McpHealthErrorKind::Timeout))?
    }
}

async fn probe_protocol(
    endpoint: &str,
    access_token: &str,
) -> Result<McpHealthReport, McpHealthError> {
    let http_client = health_http_client()?;

    // An authenticated OPTIONS request gives us transport/auth classification
    // without sending malformed protocol input or relying on unstable dependency
    // error strings. The actual probe below performs the complete MCP handshake.
    let reachability = http_client
        .request(reqwest::Method::OPTIONS, endpoint)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| McpHealthError::new(McpHealthErrorKind::Unreachable))?;
    if matches!(
        reachability.status(),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
    ) {
        return Err(McpHealthError::new(McpHealthErrorKind::Unauthorized));
    }

    let transport = StreamableHttpClientTransport::with_client(
        http_client,
        StreamableHttpClientTransportConfig::with_uri(endpoint).auth_header(access_token),
    );
    // Health must prove the modern stateless lifecycle. Discover with one
    // pinned version either negotiates 2026-07-28 or fails; it never falls
    // back to the legacy initialize handshake, which stays covered by its
    // own compatibility test instead of masking a broken modern path.
    let client = ()
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .map_err(|_| McpHealthError::new(McpHealthErrorKind::Protocol))?;
    let tools = client
        .list_all_tools()
        .await
        .map_err(|_| McpHealthError::new(McpHealthErrorKind::Protocol))?;
    let has_required_tool = [
        "onecrew_connection_list",
        "onecrew_tool_search",
        "onecrew_tool_get",
        "onecrew_tool_execute",
        "onecrew_result_present",
    ]
    .iter()
    .all(|required| tools.iter().any(|tool| tool.name.as_ref() == *required));
    let tool_count = tools.len();
    if let Err(error) = client.cancel().await {
        log::debug!("health probe client cancel failed: {error}");
    }
    if !has_required_tool {
        return Err(McpHealthError::new(McpHealthErrorKind::MissingRequiredTool));
    }
    Ok(McpHealthReport { tool_count })
}

pub(super) fn health_http_client() -> Result<reqwest::Client, McpHealthError> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| McpHealthError::new(McpHealthErrorKind::InvalidConfiguration))
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[tokio::test]
    async fn health_probe_has_stable_timeout_classification_without_secrets() {
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("test listener");
        let address = listener.local_addr().unwrap();
        let stalled = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.expect("accept probe");
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        let secret = "timeout-secret-must-not-leak";
        let error = McpAdapter::probe(
            &format!("http://{address}/mcp"),
            secret,
            Duration::from_millis(50),
        )
        .await
        .expect_err("stalled endpoint must time out");
        assert_eq!(error.kind, McpHealthErrorKind::Timeout);
        assert!(!error.to_string().contains(secret));
        stalled.abort();
    }
}
