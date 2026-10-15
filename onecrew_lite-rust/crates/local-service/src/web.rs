//! Same-origin local webpage, typed human command adapter and runtime events.
//! Browser authentication is independent of the private MCP/control transport.
mod auth;
pub(crate) use auth::BrowserSessions;

use axum::{
    Json, Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{
        Html, IntoResponse, Response, Sse,
        sse::{Event, KeepAlive},
    },
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{convert::Infallible, sync::Arc, time::Duration};

use crate::{ServiceState, state::ServiceEvent};

// Independent transport ceiling for up to 50 direct candidates, including
// notes and source context. It stays deliberately above the business batch's
// own budgets so an oversized request is rejected by the runtime's typed
// validation with a business reason, not truncated by the transport.
// Other JSON commands retain Axum's 2 MiB default.
const CREATOR_SAVE_PREPARE_HTTP_MAX_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn router(state: Arc<ServiceState>) -> Router {
    Router::new()
        .route("/api/session", get(session))
        .route("/access/account", get(access_account))
        .route("/api/events", get(events))
        .route("/api/commands/{name}", post(command))
        .route(
            "/api/creator-import",
            post(import_workbook).layer(DefaultBodyLimit::max(
                onecrew_creator_runtime::CREATOR_BUNDLE_XLSX_MAX_BYTES,
            )),
        )
        .fallback(get(static_file))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            protect_origin,
        ))
        .with_state(state)
}

async fn protect_origin(
    State(state): State<Arc<ServiceState>>,
    mut request: Request,
    next: Next,
) -> Response {
    let write =
        request.method() != axum::http::Method::GET && request.method() != axum::http::Method::HEAD;
    if state
        .browser
        .validate_origin(request.headers(), write)
        .is_err()
    {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"ok":false,"error":"invalid_local_origin"})),
        )
            .into_response();
    }
    if request.method() == axum::http::Method::POST
        && request.uri().path() == "/api/commands/onecrew_creator_save_prepare"
    {
        DefaultBodyLimit::max(CREATOR_SAVE_PREPARE_HTTP_MAX_BYTES).apply(&mut request);
    }
    // Reaching this origin is the whole admission, so a read that arrives
    // without a session is given one before the route sees it: the first
    // request a browser makes is the page itself, which cannot ask for a
    // session first. A write still needs the CSRF value of a session its
    // caller already holds, so it is never admitted this way.
    let opened = (!write && state.browser.csrf(request.headers()).is_err())
        .then(|| state.browser.open_session().ok())
        .flatten()
        .and_then(|(cookie, _)| {
            let value = HeaderValue::from_str(&cookie).ok()?;
            request.headers_mut().insert(header::COOKIE, {
                let pair = cookie.split(';').next()?;
                HeaderValue::from_str(pair).ok()?
            });
            Some(value)
        });
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if let Some(cookie) = opened {
        headers.insert(header::SET_COOKIE, cookie);
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}

/// `protect_origin` has already given a session to a read that arrived without
/// one, so this answers from the session cookie alone.
async fn access_account() -> axum::response::Redirect {
    axum::response::Redirect::temporary(&format!(
        "{}/account?product=onecrew_lite",
        onecrew_creator_sync::production_access_origin()
    ))
}

async fn session(State(state): State<Arc<ServiceState>>, headers: HeaderMap) -> Response {
    match state.browser.csrf(&headers) {
        Ok(csrf) => Json(json!({"ok":true,"result":{"csrf":csrf,"version":env!("CARGO_PKG_VERSION"),"transport":"local_http"}})).into_response(),
        Err(()) => unauthorized(),
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"ok":false,"error":"web_session_required: run onecrew web"})),
    )
        .into_response()
}

fn result_response(result: Result<Value, Value>) -> Response {
    Json(match result {
        Ok(result) => json!({"ok":true,"result":result}),
        Err(error) => json!({"ok":false,"error":error}),
    })
    .into_response()
}

async fn command(
    State(state): State<Arc<ServiceState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Json(arguments): Json<Value>,
) -> Response {
    let result_origin = match state.browser.result_origin(&headers) {
        Ok(origin) => origin,
        Err(()) => return unauthorized(),
    };
    if !arguments.is_object() {
        return result_response(Err(Value::from("command_arguments_must_be_object")));
    }
    let Some(command) = crate::commands::CommandName::lookup(&name) else {
        return result_response(Err(Value::from("unknown_command")));
    };
    use crate::commands::ExecutionClass;
    let owner = state.clone();
    let future = async move { command.dispatch(&owner, arguments, &result_origin).await };
    let result = match command.execution_class() {
        ExecutionClass::Stop => {
            // Stop drains controls, so it owns its future outside that count.
            tokio::spawn(future)
                .await
                .unwrap_or_else(|_| Err(Value::from("service_stop_failed")))
        }
        ExecutionClass::ControlOwnerAsync => state
            .controls
            .run(future)
            .await
            .unwrap_or_else(|error| Err(Value::from(error))),
        ExecutionClass::BusinessOwnerAsync
        | ExecutionClass::BusinessBlockingSegment
        | ExecutionClass::CreatorRead
        | ExecutionClass::CreatorWrite
        | ExecutionClass::CreatorSegments => state
            .work
            .run(future)
            .await
            .unwrap_or_else(|error| Err(Value::from(error))),
    };
    result_response(result)
}

pub(crate) async fn creator_sync_pairing_status(state: &ServiceState) -> Result<Value, Value> {
    serde_json::to_value(state.validate_login().await.map_err(Value::from)?)
        .map_err(|_| Value::from("status_unavailable"))
}

pub(crate) async fn creator_sync_pairing_begin(state: &ServiceState) -> Result<Value, Value> {
    state.begin_login().await.map_err(Value::from)
}

pub(crate) async fn creator_sync_pairing_reopen(state: &ServiceState) -> Result<Value, Value> {
    Ok(
        json!({"authorize_url":state.login.reopen().map_err(Value::from)?,
        "projection":state.login_status().await.map_err(Value::from)?}),
    )
}

pub(crate) async fn creator_sync_pairing_cancel(state: &ServiceState) -> Result<Value, Value> {
    state
        .login
        .cancel(state.sync().await.map_err(Value::from)?)
        .await
        .map_err(Value::from)?;
    serde_json::to_value(state.login_status().await.map_err(Value::from)?)
        .map_err(|_| Value::from("status_unavailable"))
}

pub(crate) async fn onecrew_logout(state: &ServiceState) -> Result<Value, Value> {
    state.logout().await.map_err(Value::from)
}

pub(crate) async fn local_data_status(state: &ServiceState) -> Result<Value, Value> {
    state.data_status().await.map_err(Value::from)
}

pub(crate) async fn set_application_locale(
    state: &ServiceState,
    locale: onecrew_local_runtime::PresentationLocale,
) -> Result<(), Value> {
    state.presentation_preferences.set_locale(locale);
    Ok(())
}

pub(crate) async fn service_status(state: &ServiceState) -> Result<Value, Value> {
    serde_json::to_value(
        state
            .service_status(crate::status::StatusContext::Human)
            .await
            .map_err(Value::from)?,
    )
    .map_err(|_| Value::from("status_unavailable"))
}

pub(crate) async fn service_stop(state: &ServiceState) -> Result<Value, Value> {
    state.stop().await.map_err(Value::from)?;
    Ok(json!({"stopping":true}))
}

async fn import_workbook(
    State(state): State<Arc<ServiceState>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    if state.browser.authorize(&headers, true).is_err() {
        return unauthorized();
    }
    let Some(encoded_name) = headers
        .get("x-onecrew-file-name")
        .and_then(|header| header.to_str().ok())
        .filter(|name| name.len() <= 768)
    else {
        return result_response(Err(Value::from("missing_workbook_file_name")));
    };
    let encoded = format!("name={encoded_name}");
    let Some((_, file_name)) = url::form_urlencoded::parse(encoded.as_bytes()).next() else {
        return result_response(Err(Value::from("invalid_file_name")));
    };
    let file_name = file_name.into_owned();
    let owner = state.clone();
    let result = state
        .work
        .run(async move {
            crate::onecrew_creators::onecrew_creator_import_file_prepare(
                &owner,
                file_name,
                bytes.to_vec(),
            )
            .await
        })
        .await;
    result_response(match result {
        Ok(Ok(value)) => {
            serde_json::to_value(value).map_err(|_| Value::from("import_result_unavailable"))
        }
        Ok(Err(error)) | Err(error) => Err(Value::from(error)),
    })
}

async fn events(State(state): State<Arc<ServiceState>>, headers: HeaderMap) -> Response {
    if state.browser.authorize(&headers, false).is_err() {
        return unauthorized();
    }
    let receiver = state.events.subscribe();
    let shutdown = state.shutdown.clone();
    let stream = futures::stream::unfold(
        (receiver, shutdown),
        |(mut receiver, shutdown)| async move {
            let event = tokio::select! {
                _ = shutdown.cancelled() => return None,
                result = receiver.recv() => match result {
                    Ok(event) => event,
                    // Zero forces refetch without claiming an order relative
                    // to the older events still buffered for this connection.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ServiceEvent { event: "runtime://changed", payload: json!({"sequence":0,"scopes":["all"]}) },
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            };
            Some((
                Ok::<Event, Infallible>(
                    Event::default().data(serde_json::to_string(&event).unwrap_or_default()),
                ),
                (receiver, shutdown),
            ))
        },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

async fn static_file(State(state): State<Arc<ServiceState>>, request: Request) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if state.browser.authorize(request.headers(), false).is_err() {
        return (StatusCode::UNAUTHORIZED, Html("<h1>OneCrew</h1><p>This page must be opened from this machine's own address. Run <code>onecrew web</code> to open it, or <code>onecrew web --no-browser</code> to print the address.</p>")).into_response();
    }
    let selected = if path.is_empty() { "index.html" } else { path };
    let asset = state
        .web_assets
        .iter()
        .find(|(name, _)| *name == selected)
        .or_else(|| {
            (!selected.contains('.'))
                .then(|| {
                    state
                        .web_assets
                        .iter()
                        .find(|(name, _)| *name == "index.html")
                })
                .flatten()
        });
    let Some((name, bytes)) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "json" => "application/json",
        _ => "application/octet-stream",
    };
    ([(header::CONTENT_TYPE, mime)], Body::from(*bytes)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;

    #[tokio::test]
    async fn account_link_uses_canonical_lite_destination_and_origin_guard() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew_lite".into(),
            "test".into(),
        );
        state.browser.set_port(23458).unwrap();
        let app = router(state);
        for (host, expected) in [
            ("127.0.0.1:23458", StatusCode::TEMPORARY_REDIRECT),
            ("untrusted.example", StatusCode::FORBIDDEN),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/access/account?next=https://untrusted.example&product=pro")
                        .header("host", host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::TEMPORARY_REDIRECT {
                assert_eq!(
                    response.headers()[header::LOCATION],
                    format!(
                        "{}/account?product=onecrew_lite",
                        onecrew_creator_sync::production_access_origin(),
                    )
                );
            } else {
                assert!(!response.headers().contains_key(header::LOCATION));
            }
        }
    }

    #[tokio::test]
    async fn the_page_opens_without_a_session_while_a_write_still_needs_one() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23458).unwrap();
        let app = router(state.clone());
        // The first request a browser makes is the page itself, so it cannot
        // ask for a session first: reaching the origin has to be enough.
        let page = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("host", "127.0.0.1:23458")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(page.status(), StatusCode::OK);
        let issued = page.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(issued.contains("HttpOnly; SameSite=Strict"));
        assert_eq!(
            axum::body::to_bytes(page.into_body(), 4096)
                .await
                .unwrap()
                .as_ref(),
            b"test"
        );
        // A write is never admitted this way: it carries no CSRF value, and
        // this admission mints one the caller has never seen.
        let write = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/commands/toolkit_directory")
                    .header("host", "127.0.0.1:23458")
                    .header("origin", "http://127.0.0.1:23458")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(write.status(), StatusCode::UNAUTHORIZED);
        assert!(!write.headers().contains_key(header::SET_COOKIE));
    }

    #[tokio::test]
    async fn review_http_commands_require_session_and_active_account() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23459).unwrap();
        let app = router(state.clone());
        let body = json!({"input":{"campaign_id":"campaign","request_id":"request"}}).to_string();
        let anonymous = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/commands/onecrew_discovery_review_get")
                    .header("host", "127.0.0.1:23459")
                    .header("origin", "http://127.0.0.1:23459")
                    .header("content-type", "application/json")
                    .body(Body::from(body.clone()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
        let (set_cookie, csrf) = state.browser.open_session().unwrap();
        let cookie = set_cookie.split(';').next().unwrap();
        let authenticated = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/commands/onecrew_discovery_review_get")
                    .header("host", "127.0.0.1:23459")
                    .header("origin", "http://127.0.0.1:23459")
                    .header("cookie", cookie)
                    .header("x-onecrew-csrf", csrf)
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
        let wire: Value = serde_json::from_slice(
            &axum::body::to_bytes(authenticated.into_body(), 4096)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            wire,
            json!({"ok":false,"error":"login_required: run onecrew login or use onecrew_login"})
        );
    }

    #[tokio::test]
    async fn identity_review_http_commands_require_session() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23459).unwrap();
        let app = router(state);
        for command in [
            "onecrew_discovery_identity_material_get",
            "onecrew_discovery_identity_groups_get",
            "onecrew_discovery_identity_review_get",
            "onecrew_discovery_identity_review_submit",
            "onecrew_discovery_identity_review_revoke",
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/commands/{command}"))
                        .header("host", "127.0.0.1:23459")
                        .header("origin", "http://127.0.0.1:23459")
                        .header("content-type", "application/json")
                        .body(Body::from("{}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{command}");
            assert!(!response.headers().contains_key(header::SET_COOKIE));
        }
    }

    #[tokio::test]
    async fn draft_prepare_requires_session_csrf_and_active_account() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23460).unwrap();
        let app = router(state.clone());
        let (set_cookie, csrf) = state.browser.open_session().unwrap();
        let cookie = set_cookie.split(';').next().unwrap();
        let body = json!({"input":{"id":"draft-test","brief":"我要卖鞋子", "profile_ref":"browser",
            "opencli_version":"1.8.8","target_count":30,"max_calls":200}})
        .to_string();
        let request = |cookie: &str, csrf: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/commands/onecrew_discovery_draft_prepare")
                .header("host", "127.0.0.1:23460")
                .header("origin", "http://127.0.0.1:23460")
                .header("cookie", cookie)
                .header("x-onecrew-csrf", csrf)
                .header("content-type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request("", "")).await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            app.clone()
                .oneshot(request(cookie, "wrong"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app.oneshot(request(cookie, &csrf)).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let wire: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            wire,
            json!({"ok":false,"error":"login_required: run onecrew login or use onecrew_login"})
        );
    }

    #[tokio::test]
    async fn direct_save_has_a_bounded_body_budget_without_widening_other_commands() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[("index.html", b"test")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23457).unwrap();
        let app = router(state.clone());
        let (set_cookie, csrf) = state.browser.open_session().unwrap();
        let cookie = set_cookie.split(';').next().unwrap().to_owned();
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(&cookie).unwrap());
        let candidates = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS;
        let input = json!({"candidates": (0..candidates).map(|index| json!({
            "intent":"save_or_update",
            "target": {"platform":"x", "user_id":(index + 1).to_string(), "handle":format!("creator{index}")},
            "connection": {"platform":"x", "account_ref_id":"00000000-0000-4000-8000-000000000001"},
            "provenance": {"kind":"direct_selection", "source_url":format!("https://x.com/creator{index}"), "description":null},
            "notes":"n".repeat(45 * 1024), "tags":null,
            "quality":{"score":null,"assessment":null,"follower_counts":{}}
        })).collect::<Vec<_>>()});
        let typed: onecrew_local_runtime::PrepareCreatorSaveInput =
            serde_json::from_value(input.clone()).unwrap();
        assert_eq!(typed.candidates.len(), candidates);
        let body = serde_json::to_string(&json!({"input":input})).unwrap();
        assert!(body.len() > 2 * 1024 * 1024);
        assert!(body.len() < CREATOR_SAVE_PREPARE_HTTP_MAX_BYTES);
        let request = |name: &str, body: String| {
            Request::builder()
                .method("POST")
                .uri(format!("/api/commands/{name}"))
                .header("host", "127.0.0.1:23457")
                .header("origin", "http://127.0.0.1:23457")
                .header("cookie", &cookie)
                .header("x-onecrew-csrf", &csrf)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(request("onecrew_creator_save_prepare", body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap(),
        )
        .unwrap();
        // The real command decoded the typed input and reached its owner/login
        // gate. This fixture deliberately has no connected provider account.
        assert!(value["error"].as_str().unwrap().contains("login_required"));
        let response = app
            .clone()
            .oneshot(request("toolkit_directory", body.clone()))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let oversized = format!("{body}{}", " ".repeat(CREATOR_SAVE_PREPARE_HTTP_MAX_BYTES));
        let response = app
            .oneshot(request("onecrew_creator_save_prepare", oversized))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn browser_cookie_is_not_access_login_and_origin_csrf_protect_every_write() {
        let data = tempfile::tempdir().unwrap();
        let state = ServiceState::new(
            data.path().into(),
            &[("index.html", b"real test asset")],
            "/test/onecrew".into(),
            "test".into(),
        );
        state.startup.start(data.path().into()).unwrap();
        state.browser.set_port(23456).unwrap();
        let app = router(state.clone());
        let launch = Request::builder()
            .uri("/api/session")
            .header("host", "127.0.0.1:23456")
            .body(Body::empty())
            .unwrap();
        // Reaching the origin is the admission now, so the first handshake
        // both mints the session and hands back its CSRF value.
        let response = app.clone().oneshot(launch).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap(),
        )
        .unwrap();
        let csrf = value["result"]["csrf"].as_str().unwrap();
        let request = |origin: &str, csrf: &str| {
            Request::builder()
                .method("POST")
                .uri("/api/commands/toolkit_directory")
                .header("host", "127.0.0.1:23456")
                .header("origin", origin)
                .header("cookie", &cookie)
                .header("x-onecrew-csrf", csrf)
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request("https://attacker.example", csrf))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.clone()
                .oneshot(request("http://127.0.0.1:23456", "invalid"))
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let response = app
            .oneshot(request("http://127.0.0.1:23456", csrf))
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["error"].as_str().unwrap().contains("login_required"));
    }
}
#[cfg(test)]
#[tokio::test]
async fn unknown_web_commands_fail_closed_before_host_admission() {
    use tower::ServiceExt;
    let directory = tempfile::tempdir().unwrap();
    let state = ServiceState::new(
        directory.path().into(),
        &[],
        "/test/onecrew".into(),
        "test".into(),
    );
    state.browser.set_port(23456).unwrap();
    let app = router(state.clone());
    let (set_cookie, csrf) = state.browser.open_session().unwrap();
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    let mut headers = HeaderMap::new();
    headers.insert(header::COOKIE, cookie.parse().unwrap());
    let _work_paused = state.work.pause(Duration::from_millis(100)).await.unwrap();
    let _controls_paused = state
        .controls
        .pause(Duration::from_millis(100))
        .await
        .unwrap();
    for name in [
        "unknown",
        "onecrew.creator.get",
        "local_data_upgrade",
        "onecrew_posts_list",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/commands/{name}"))
                    .header("host", "127.0.0.1:23456")
                    .header("origin", "http://127.0.0.1:23456")
                    .header("cookie", &cookie)
                    .header("x-onecrew-csrf", &csrf)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        let wire: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 8192)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(wire, json!({"ok":false,"error":"unknown_command"}));
    }
}
