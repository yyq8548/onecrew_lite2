use onecrew_local_domain::LocalProfile;
use onecrew_local_runtime::{
    BrowserStatus, ClearXChatPasscodeInput, OperatorAccountConnectionBeginInput,
    OperatorAccountConnectionCancelInput, OperatorAccountConnectionFinalizeInput,
    OperatorAccountConnectionInAppHandoff, OperatorAccountConnectionPrepareInput,
    OperatorAccountConnectionStatus, OperatorAccountConnectionStatusInput,
    OperatorAccountDisconnectInput, OperatorAccountDisconnectOutcome, OperatorAccountProjection,
    ProfileRow, ProxyConfig, SetXChatPasscodeInput, XChatConnectionPreference,
};

use crate::{ServiceState, blocking::run_web_blocking};

/// Whether the user's Chrome is installed (连接 page detection card).
pub(super) async fn onecrew_operator_account_browser_status_in_app(
    state: &ServiceState,
) -> Result<BrowserStatus, String> {
    let platform = state.platform().await?;
    run_web_blocking(
        "onecrew_operator_account_browser_status_in_app",
        move || Ok(platform.browser_status()),
    )
    .await?
}

pub(super) async fn refresh_profile(
    state: &ServiceState,
    profile_id: String,
) -> Result<LocalProfile, String> {
    state.platform().await?.refresh_profile(&profile_id).await
}

pub(super) async fn list_profiles(state: &ServiceState) -> Result<Vec<ProfileRow>, String> {
    let platform = state.platform().await?;
    run_web_blocking("list_profiles", move || {
        // Open the local vault eagerly so already-imported profiles show on load.
        // Its key is a restricted local file; the macOS Chrome Safe Storage
        // Keychain prompt happens later, while extracting Chrome cookies.
        platform.list_profiles()
    })
    .await?
}

// ---- High-level operator-account workflow (shared service with MCP) ----

pub(super) async fn onecrew_operator_account_disconnect(
    state: &ServiceState,
    input: OperatorAccountDisconnectInput,
) -> Result<OperatorAccountDisconnectOutcome, String> {
    let platform = state.platform().await?;
    run_web_blocking("onecrew_operator_account_disconnect", move || {
        platform
            .operator_account_disconnect(input)
            .map_err(|error| error.safe_message().to_string())
    })
    .await?
}

pub(super) async fn onecrew_operator_account_connection_prepare(
    state: &ServiceState,
    input: OperatorAccountConnectionPrepareInput,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_prepare(input)
        .await
}

pub(super) async fn onecrew_operator_account_connection_begin(
    state: &ServiceState,
    input: OperatorAccountConnectionBeginInput,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_begin(input)
        .await
}

pub(super) async fn onecrew_operator_account_connection_status(
    state: &ServiceState,
    input: OperatorAccountConnectionStatusInput,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_status(input)
        .await
}

pub(super) async fn onecrew_operator_account_connection_finalize(
    state: &ServiceState,
    input: OperatorAccountConnectionFinalizeInput,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_finalize(input)
        .await
}

pub(super) async fn onecrew_operator_account_connection_cancel(
    state: &ServiceState,
    input: OperatorAccountConnectionCancelInput,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_cancel(input)
        .await
}

/// Desktop-only credential handoff for the guided GUI flow. MCP has a peer
/// adapter over the same runtime contract; neither adapter persists or
/// projects the proxy password or X Chat passcode itself.
pub(super) async fn onecrew_operator_account_connection_configure_browser_in_app(
    state: &ServiceState,
    flow_id: String,
    profile_name: String,
    proxy: ProxyConfig,
    x_chat_preference: XChatConnectionPreference,
    x_chat_passcode: Option<String>,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_configure_browser_in_app(
            &flow_id,
            profile_name,
            proxy,
            x_chat_preference,
            x_chat_passcode,
        )
        .await
}

/// Desktop X Chat secret handoff. This adapter forwards the passcode directly
/// to the runtime and returns only its secret-free account projection.
pub(super) async fn onecrew_operator_account_x_chat_passcode_set_in_app(
    state: &ServiceState,
    input: SetXChatPasscodeInput,
) -> Result<OperatorAccountProjection, String> {
    let platform = state.platform().await?;
    run_web_blocking(
        "onecrew_operator_account_x_chat_passcode_set_in_app",
        move || platform.operator_account_x_chat_passcode_set(input),
    )
    .await?
}

pub(super) async fn onecrew_operator_account_x_chat_passcode_clear_in_app(
    state: &ServiceState,
    input: ClearXChatPasscodeInput,
) -> Result<OperatorAccountProjection, String> {
    let platform = state.platform().await?;
    run_web_blocking(
        "onecrew_operator_account_x_chat_passcode_clear_in_app",
        move || platform.operator_account_x_chat_passcode_clear(input),
    )
    .await?
}

/// Desktop-only Telegram credential handoff. api_hash and phone never enter an
/// MCP request schema.
pub(super) async fn onecrew_operator_account_connection_configure_telegram_in_app(
    state: &ServiceState,
    flow_id: String,
    api_id: i32,
    api_hash: String,
    phone: Option<String>,
    proxy: Option<ProxyConfig>,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_configure_telegram_in_app(
            &flow_id, api_id, api_hash, phone, proxy,
        )
        .await
}

pub(super) async fn onecrew_operator_account_connection_open_telegram_credentials_in_app(
    state: &ServiceState,
    flow_id: String,
    proxy: ProxyConfig,
) -> Result<(), String> {
    state
        .platform()
        .await?
        .operator_account_connection_open_telegram_credentials_in_app(&flow_id, proxy)
        .await
}

/// Desktop-only QR/code-delivery rendering material. The corresponding MCP
/// status contains only a typed handoff instruction.
pub(super) async fn onecrew_operator_account_connection_handoff_in_app(
    state: &ServiceState,
    flow_id: String,
) -> Result<OperatorAccountConnectionInAppHandoff, String> {
    state
        .platform()
        .await?
        .operator_account_connection_in_app_handoff(&flow_id)
        .await
}

pub(super) async fn onecrew_operator_account_connection_submit_telegram_code_in_app(
    state: &ServiceState,
    flow_id: String,
    code: String,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_submit_telegram_code_in_app(&flow_id, &code)
        .await
}

pub(super) async fn onecrew_operator_account_connection_submit_telegram_password_in_app(
    state: &ServiceState,
    flow_id: String,
    password: String,
) -> Result<OperatorAccountConnectionStatus, String> {
    state
        .platform()
        .await?
        .operator_account_connection_submit_telegram_password_in_app(&flow_id, &password)
        .await
}
