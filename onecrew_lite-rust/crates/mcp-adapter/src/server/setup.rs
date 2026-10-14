use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};

pub struct McpSetupPrompt {
    pub prompt: String,
    pub expires_at: u64,
}

const SETUP_CAPABILITY_TTL: Duration = Duration::from_secs(5 * 60);

fn generate_setup_capability() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(super) async fn protect_setup(
    State(state): State<SetupState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let allowed = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| state.allowed_hosts.iter().any(|allowed| allowed == host));
    let mut response = if allowed {
        next.run(request).await
    } else {
        StatusCode::NOT_FOUND.into_response()
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=onecrew-setup.md"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'; sandbox"),
    );
    response
}

pub(super) async fn setup(
    State(state): State<SetupState>,
    Path(provided): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, StatusCode> {
    if !is_trusted_setup_request(&headers) {
        return Err(StatusCode::NOT_FOUND);
    }
    if !state.reserve(&provided) {
        return Err(StatusCode::NOT_FOUND);
    }
    let markdown = setup_document();
    Ok((
        [(header::CONTENT_TYPE, "text/markdown; charset=utf-8")],
        markdown,
    ))
}

fn setup_document() -> &'static str {
    r#"# onecrew_lite MCP connection

## Install and log in

Use the installation script provided by onecrew_lite Access. Follow its returned `next_action` or `next_actions` and preserve the installer-reported absolute executable path. In the commands below, `onecrew_lite` means that exact absolute path; do not assume it is on PATH.

Run `onecrew_lite login` with that path when the user chooses to log in. Complete the browser login and read the command's result and next actions before continuing. This guide describes available actions; reading it grants no permission to run commands or change client settings.

## Codex

When the user chooses Codex integration, explicitly run `onecrew_lite plugin install`. Read its result, then ask the user to fully quit and restart Codex. After the restart, run `onecrew_lite plugin status --refresh` to read back the installed plugin state. Installation and a live connection are separate states; verify the loaded onecrew_lite MCP tools in Codex.

Use the current conversation for follow-up; let the user choose any new conversation. Client configuration changes and plugin installation require the user's explicit choice.

## Other MCP clients

Run `onecrew_lite setup` to print the stdio MCP JSON. Add that returned configuration through the chosen client's configuration flow. The client starts the absolute onecrew_lite executable with `mcp --stdio`; `onecrew_lite mcp --stdio` is the client-managed server command.

## Updates

Run `onecrew_lite update` when the user chooses to update. Read its returned `next_action` or `next_actions`; follow a plugin refresh suggestion only when needed and explicitly chosen. A version or status check is read-only.

## Verify the connection

The loaded integration is onecrew_lite MCP. Its fixed business tools are `onecrew_tool_search`, `onecrew_tool_get`, `onecrew_tool_execute`, `onecrew_result_present`, and `onecrew_connection_list`. Read connection status with `onecrew_connection_list` before performing account-specific work.
"#
}

#[derive(Clone)]
pub(super) struct SetupState {
    pub(super) capability: Arc<Mutex<SetupCapabilityState>>,
    pub(super) allowed_hosts: Arc<[String; 2]>,
}

fn is_trusted_setup_request(headers: &HeaderMap) -> bool {
    let browser_context = headers.contains_key(header::ORIGIN)
        || headers
            .keys()
            .any(|name| name.as_str().starts_with("sec-fetch-"));
    !browser_context
}

pub(super) enum SetupCapabilityState {
    Unissued,
    Available {
        value: String,
        expires_at: Instant,
        expires_at_epoch_ms: u64,
    },
    Consumed,
}

impl SetupState {
    pub(super) fn issue(&self) -> McpSetupPrompt {
        let mut capability = self
            .capability
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let now = Instant::now();
        let (value, expires_at_epoch_ms) = match &*capability {
            SetupCapabilityState::Available {
                value,
                expires_at,
                expires_at_epoch_ms,
            } if now <= *expires_at => (value.clone(), *expires_at_epoch_ms),
            SetupCapabilityState::Unissued
            | SetupCapabilityState::Available { .. }
            | SetupCapabilityState::Consumed => {
                let value = generate_setup_capability();
                let expires_at_epoch_ms = epoch_millis_after(SETUP_CAPABILITY_TTL);
                *capability = SetupCapabilityState::Available {
                    value: value.clone(),
                    expires_at: now + SETUP_CAPABILITY_TTL,
                    expires_at_epoch_ms,
                };
                (value, expires_at_epoch_ms)
            }
        };
        let setup_url = format!("http://{}/setup/{value}", self.allowed_hosts[0]);
        McpSetupPrompt {
            prompt: bootstrap_prompt(&setup_url),
            expires_at: expires_at_epoch_ms,
        }
    }

    fn reserve(&self, provided: &str) -> bool {
        let mut capability = self
            .capability
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let matches = match &*capability {
            SetupCapabilityState::Available {
                value, expires_at, ..
            } => Instant::now() <= *expires_at && provided == value,
            SetupCapabilityState::Unissued | SetupCapabilityState::Consumed => false,
        };
        if matches {
            *capability = SetupCapabilityState::Consumed;
        }
        matches
    }
}

fn epoch_millis_after(duration: Duration) -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .saturating_add(duration)
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn bootstrap_prompt(setup_url: &str) -> String {
    format!(
        "Fetch and follow this local MCP setup link from a terminal running outside the sandbox with \
         host-network permission; sandboxed execution will fail, and only HTTP 404 requires a new \
         link: {setup_url}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_capability_is_random_256_bit_url_safe_material() {
        let first = generate_setup_capability();
        let second = generate_setup_capability();
        assert_ne!(first, second);
        assert_eq!(URL_SAFE_NO_PAD.decode(&first).unwrap().len(), 32);
        assert_eq!(URL_SAFE_NO_PAD.decode(&second).unwrap().len(), 32);
    }

    #[test]
    fn setup_prompt_is_issued_on_demand_and_renewed_after_consumption() {
        let state = SetupState {
            capability: Arc::new(Mutex::new(SetupCapabilityState::Unissued)),
            allowed_hosts: Arc::new(["127.0.0.1:39393".to_string(), "localhost:39393".to_string()]),
        };

        assert!(!state.reserve("anything-before-issue"));
        let first = state.issue();
        let duplicate = state.issue();
        assert_eq!(duplicate.prompt, first.prompt);
        assert_eq!(duplicate.expires_at, first.expires_at);

        let first_url = first.prompt.split_whitespace().next_back().unwrap();
        let first_capability = first_url.rsplit('/').next().unwrap();
        assert!(state.reserve(first_capability));
        assert!(!state.reserve(first_capability));

        let renewed = state.issue();
        assert_ne!(renewed.prompt, first.prompt);
        assert!(renewed.expires_at >= first.expires_at);
    }

    #[tokio::test]
    async fn trusted_cli_request_delivers_secret_free_guide_once_after_browser_rejection() {
        let state = SetupState {
            capability: Arc::new(Mutex::new(SetupCapabilityState::Available {
                value: "single-use".to_string(),
                expires_at: Instant::now() + Duration::from_secs(1),
                expires_at_epoch_ms: epoch_millis_after(Duration::from_secs(1)),
            })),
            allowed_hosts: Arc::new(["127.0.0.1:39393".to_string(), "localhost:39393".to_string()]),
        };
        let mut browser_headers = HeaderMap::new();
        browser_headers.insert(header::ORIGIN, HeaderValue::from_static("http://localhost"));
        assert!(
            setup(
                State(state.clone()),
                Path("single-use".to_string()),
                browser_headers
            )
            .await
            .is_err()
        );

        let cli_headers = HeaderMap::new();
        let response = setup(
            State(state.clone()),
            Path("single-use".to_string()),
            cli_headers.clone(),
        )
        .await
        .expect("trusted CLI install")
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let response_body = axum::body::to_bytes(response.into_body(), 32 * 1024)
            .await
            .expect("status body");
        let response_text = String::from_utf8(response_body.to_vec()).unwrap();
        assert!(!response_text.contains("Bearer"));
        assert!(!response_text.contains("http://"));
        assert!(!response_text.contains("single-use"));
        assert_eq!(response_text, setup_document());
        assert!(response_text.contains("# onecrew_lite MCP connection"));
        for required in [
            "onecrew_tool_search",
            "onecrew_tool_get",
            "onecrew_tool_execute",
            "onecrew_result_present",
            "onecrew_connection_list",
        ] {
            assert!(response_text.contains(required));
        }
        assert!(
            setup(State(state), Path("single-use".to_string()), cli_headers)
                .await
                .is_err()
        );
    }

    #[test]
    fn setup_capability_expires_and_consumes_atomically() {
        let expired = SetupState {
            capability: Arc::new(Mutex::new(SetupCapabilityState::Available {
                value: "expired".to_string(),
                expires_at: Instant::now() - Duration::from_millis(1),
                expires_at_epoch_ms: 0,
            })),
            allowed_hosts: Arc::new(["127.0.0.1:1".to_string(), "localhost:1".to_string()]),
        };
        assert!(!expired.reserve("expired"));

        let available = SetupState {
            capability: Arc::new(Mutex::new(SetupCapabilityState::Available {
                value: "once".to_string(),
                expires_at: Instant::now() + Duration::from_secs(1),
                expires_at_epoch_ms: epoch_millis_after(Duration::from_secs(1)),
            })),
            allowed_hosts: Arc::new(["127.0.0.1:1".to_string(), "localhost:1".to_string()]),
        };
        assert!(available.reserve("once"));
        assert!(!available.reserve("once"));

        let renewed = available.issue();
        assert!(renewed.expires_at > 0);
        assert!(!renewed.prompt.contains("Authorization: Bearer"));
        let renewed_url = renewed.prompt.split_whitespace().next_back().unwrap();
        assert_ne!(renewed_url, "http://127.0.0.1:1/setup/once");
    }

    #[test]
    fn setup_document_is_cli_first_and_requires_explicit_client_actions() {
        let document = setup_document();
        for required in [
            "installation script provided by onecrew_lite Access",
            "`next_action`",
            "`next_actions`",
            "absolute executable path",
            "`onecrew_lite login`",
            "`onecrew_lite plugin install`",
            "fully quit and restart Codex",
            "`onecrew_lite plugin status --refresh`",
            "`onecrew_lite setup`",
            "stdio MCP JSON",
            "`onecrew_lite mcp --stdio`",
            "`onecrew_lite update`",
            "explicitly chosen",
            "grants no permission",
            "onecrew_connection_list",
        ] {
            assert!(document.contains(required), "missing {required}");
        }
        for forbidden in [
            "Desktop",
            "Streamable HTTP",
            "Bearer",
            "Authorization:",
            "http://",
            "codex-marketplace",
            "marketplace add",
            "codex plugin add",
            "staged",
            "config.toml",
            "mcp_servers",
            "remove exactly",
            "automatically",
            "automatic",
            "reinstall",
            "create_thread",
            "send_message_to_thread",
            "NEW Codex session",
            "scheduled task",
            "authorizes exactly",
        ] {
            assert!(
                !document.contains(forbidden),
                "obsolete setup instruction: {forbidden}"
            );
        }
    }

    #[test]
    fn bootstrap_prompt_is_one_short_link_with_correct_retry_semantics() {
        let setup_url = "http://127.0.0.1:39393/setup/setup-secret";
        let prompt = bootstrap_prompt(setup_url);
        for required in [
            "terminal running outside the sandbox",
            "host-network permission",
            "sandboxed execution will fail",
            "only HTTP 404",
        ] {
            assert!(prompt.contains(required), "missing {required}: {prompt}");
        }
        assert_eq!(prompt.matches(setup_url).count(), 1);
        assert!(prompt.len() < 360, "bootstrap prompt is too long: {prompt}");
        assert!(!prompt.contains("Authorization: Bearer"));
        for forbidden in [
            "curl ",
            "PowerShell",
            "X-Agent-Platform-Setup",
            "crm_platform_connection_list",
            "retry the consumed URL",
            "installed for",
            "write_codex",
            "config.toml",
            "Set-Acl",
            "launchctl",
            "reg.exe",
        ] {
            assert!(
                !prompt.contains(forbidden),
                "installer side effect drift: {prompt}"
            );
        }
    }
}
