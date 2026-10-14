//! High-level operator-account workflows shared by the desktop peer and MCP.
//!
//! Connection input DTOs may contain the credentials explicitly supplied by an
//! operator. They are consumed into private, short-lived runtime state. Every
//! status/result DTO is a deliberately secret-free projection: adapters only
//! receive opaque operation IDs, public phases and sanitized account metadata.

use onecrew_local_domain::{Platform, ProfileSecret};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::ProxyConfig;
const CONNECTION_FLOW_TTL: Duration = Duration::from_secs(30 * 60);
const MAX_PENDING_CONNECTION_FLOWS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountPlatform {
    X,
    Instagram,
    Telegram,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionIntent {
    Connect,
    Reconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionMethod {
    Browser,
    TelegramQr,
    TelegramPhone,
    ExistingAccount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionPhase {
    AwaitingSecretInApp,
    ReadyToBegin,
    Starting,
    AwaitingUserInApp,
    Finalizing,
    Completed,
    Canceled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionAction {
    OpenDesktop,
    Begin,
    Status,
    Finalize,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionHandoffKind {
    ConfigureConnectionInDesktop,
    CompleteBrowserLoginInDesktop,
    ScanTelegramQrInDesktop,
    EnterTelegramOtpInDesktop,
    EnterTelegramTwoFactorInDesktop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountConnectionKind {
    BrowserConnect,
    BrowserReconnect,
    TelegramQrConnect,
    TelegramPhoneConnect,
    TelegramReconnect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAccountCredentialAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum XChatConnectionPreference {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum XChatPasscodeStatus {
    Configured,
    NotConfigured,
    Unavailable,
    NotApplicable,
}

/// Secret-free account projection suitable for MCP and desktop callers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountProjection {
    pub operator_account_id: String,
    pub platform: OperatorAccountPlatform,
    /// Stable provider-side account identifier; safe metadata, never a secret.
    pub provider_account_id: String,
    pub username: String,
    pub label: String,
    /// A vault-backed operator identity is configured. This does not claim its
    /// remote session has been freshly validated; reconnect performs that check.
    pub configured: bool,
    /// Whether the encrypted vault record can actually be read. Callers must
    /// not infer availability merely from the public profile row existing.
    pub credential_availability: OperatorAccountCredentialAvailability,
    /// Whether this X profile has a non-empty X Chat passcode in the encrypted
    /// vault. Other platforms report `not_applicable`; unreadable X credentials
    /// report `unavailable`. The passcode itself is never projected.
    pub x_chat_passcode_status: XChatPasscodeStatus,
    pub updated_at: String,
}

/// Secret-bearing local X Chat update input. It is deserialize-only so the raw
/// passcode cannot be accidentally projected into adapter results or logs.
#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetXChatPasscodeInput {
    pub operator_account_id: String,
    pub x_chat_passcode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearXChatPasscodeInput {
    pub operator_account_id: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionHandoff {
    pub kind: OperatorAccountConnectionHandoffKind,
    pub message: String,
}

/// Prepare is intentionally metadata-only. Account credentials, proxy
/// passwords, phone numbers, passcodes, OTPs, and 2FA values are not fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionPrepareInput {
    pub platform: OperatorAccountPlatform,
    pub intent: OperatorAccountConnectionIntent,
    pub method: OperatorAccountConnectionMethod,
    #[serde(default)]
    pub operator_account_id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionBeginInput {
    pub flow_id: String,
    pub confirmed: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionStatusInput {
    pub flow_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionFinalizeInput {
    pub flow_id: String,
    pub confirmed: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionCancelInput {
    pub flow_id: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountConnectionStatus {
    pub flow_id: String,
    pub platform: OperatorAccountPlatform,
    pub intent: OperatorAccountConnectionIntent,
    pub method: OperatorAccountConnectionMethod,
    pub kind: OperatorAccountConnectionKind,
    pub phase: OperatorAccountConnectionPhase,
    /// Owner-derived Unix timestamp in milliseconds. The adapter must not
    /// recreate connection-flow TTL policy.
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_account: Option<OperatorAccountProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff: Option<OperatorAccountConnectionHandoff>,
    pub allowed_actions: Vec<OperatorAccountConnectionAction>,
    /// Fresh browser capability for browser-backed flows. Non-browser flows do
    /// not project this field, keeping adapters from inferring OS behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_capability: Option<crate::BrowserStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Public phase used by the platform-specific direct connection facade. It
/// intentionally does not expose the legacy desktop-handoff terminology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DirectAccountConnectionPhase {
    Starting,
    AwaitingBrowserLogin,
    AwaitingTelegramQrScan,
    AwaitingTelegramOtp,
    AwaitingTelegramTwoFactor,
    Completed,
    Canceled,
    Failed,
}

#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserAccountConnectionStartInput {
    pub platform: OperatorAccountPlatform,
    pub profile_name: String,
    pub proxy: ProxyConfig,
    pub x_chat_preference: XChatConnectionPreference,
    #[serde(default)]
    pub x_chat_passcode: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserAccountConnectionCompleteInput {
    pub operation_id: String,
    pub login_completed: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DirectAccountConnectionCancelInput {
    pub operation_id: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountConnectionOperationInput {
    pub operation_id: String,
}

/// Secret-free X/Instagram connection result returned to either adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserAccountConnectionStatus {
    pub operation_id: String,
    pub platform: OperatorAccountPlatform,
    pub phase: DirectAccountConnectionPhase,
    pub expires_at: u64,
    /// Fresh end-to-end browser capability. This is runtime-owned so desktop
    /// and MCP adapters never infer OS/provider behavior independently.
    pub browser_capability: crate::BrowserStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_account: Option<OperatorAccountProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TelegramAccountConnectionMethod {
    #[default]
    Qr,
    Phone,
}

#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramAccountConnectionStartInput {
    pub api_id: i32,
    pub api_hash: String,
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub method: TelegramAccountConnectionMethod,
    #[serde(default)]
    pub phone: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramAccountConnectionOtpInput {
    pub operation_id: String,
    pub otp: String,
}

#[derive(Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramAccountConnectionTwoFactorInput {
    pub operation_id: String,
    pub password: String,
}

/// Public QR rendering. The raw Telegram login URI/token remains in the
/// transport; GUI and terminal clients receive renderings of one challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramQrCode {
    /// Login-token QR as a data URL. Consumed in-process (the MCP adapter
    /// materializes it to a private file); it encodes a live login token, so
    /// it never serializes out of the runtime.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub image_data_url: String,
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub terminal_qr: String,
    pub expires_at: i32,
}

/// Non-secret route metadata Telegram selected for one login code. The MCP
/// connection status and the in-app handoff both carry this runtime-owned
/// value; the flow keeps the transport's value and maps it only when projecting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TelegramLoginCodeDelivery {
    pub method: TelegramLoginCodeMethod,
    pub code_length: Option<i32>,
    pub destination_hint: Option<String>,
    pub timeout_seconds: Option<i32>,
    pub next_method: Option<TelegramLoginCodeMethod>,
    pub action_url: Option<String>,
    pub can_submit_code: bool,
}

/// Route Telegram uses to deliver a login code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TelegramLoginCodeMethod {
    TelegramApp,
    Sms,
    VoiceCall,
    FlashCall,
    MissedCall,
    Email,
    EmailSetupRequired,
    Fragment,
    FirebaseSms,
    SmsWord,
    SmsPhrase,
}

/// QR challenge for the in-app login page. Unlike the MCP [`TelegramQrCode`],
/// it serializes the rendered PNG data URL (with its expiry) to the desktop
/// webview; terminal art and the raw login token are not fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TelegramInAppQrChallenge {
    pub image_data_url: String,
    pub expires_at: i32,
}

impl From<onecrew_tg::LoginCodeDelivery> for TelegramLoginCodeDelivery {
    fn from(delivery: onecrew_tg::LoginCodeDelivery) -> Self {
        let onecrew_tg::LoginCodeDelivery {
            method,
            code_length,
            destination_hint,
            timeout_seconds,
            next_method,
            action_url,
            can_submit_code,
        } = delivery;
        Self {
            method: method.into(),
            code_length,
            destination_hint,
            timeout_seconds,
            next_method: next_method.map(Into::into),
            action_url,
            can_submit_code,
        }
    }
}

impl From<onecrew_tg::LoginCodeMethod> for TelegramLoginCodeMethod {
    fn from(method: onecrew_tg::LoginCodeMethod) -> Self {
        use onecrew_tg::LoginCodeMethod as Transport;
        match method {
            Transport::TelegramApp => Self::TelegramApp,
            Transport::Sms => Self::Sms,
            Transport::VoiceCall => Self::VoiceCall,
            Transport::FlashCall => Self::FlashCall,
            Transport::MissedCall => Self::MissedCall,
            Transport::Email => Self::Email,
            Transport::EmailSetupRequired => Self::EmailSetupRequired,
            Transport::Fragment => Self::Fragment,
            Transport::FirebaseSms => Self::FirebaseSms,
            Transport::SmsWord => Self::SmsWord,
            Transport::SmsPhrase => Self::SmsPhrase,
        }
    }
}

impl From<onecrew_tg::QrLoginChallenge> for TelegramInAppQrChallenge {
    fn from(challenge: onecrew_tg::QrLoginChallenge) -> Self {
        let onecrew_tg::QrLoginChallenge {
            image_data_url,
            terminal_qr: _,
            expires_at,
        } = challenge;
        Self {
            image_data_url,
            expires_at,
        }
    }
}

/// Secret-free Telegram connection result. It never contains API hash, proxy
/// credentials, phone, OTP, 2FA value, session bytes, or the raw QR token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramAccountConnectionStatus {
    pub operation_id: String,
    pub phase: DirectAccountConnectionPhase,
    pub expires_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qr_code: Option<TelegramQrCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<serde_json::Value>")]
    pub code_delivery: Option<TelegramLoginCodeDelivery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator_account: Option<OperatorAccountProjection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

/// Legacy desktop handoff detail for the in-app login page, carried by the
/// runtime QR and code-delivery carriers. The MCP status projects its own QR
/// type without the PNG data URL; the raw login URI/token is never a field.
#[derive(Debug, Clone, Serialize)]
pub struct OperatorAccountConnectionInAppHandoff {
    pub flow_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_qr_challenge: Option<TelegramInAppQrChallenge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram_code_delivery: Option<TelegramLoginCodeDelivery>,
}

mod browser;
mod details;
mod disconnect;
mod flow_state;
mod lifecycle;
mod management;
mod telegram;

pub use details::{
    InstagramPlatformDetail, InstagramPlatformGetInput, InstagramPlatformGetOutcome,
    TelegramPlatformDetail, TelegramPlatformGetInput, TelegramPlatformGetOutcome, XPlatformDetail,
    XPlatformGetInput, XPlatformGetOutcome,
};
pub use disconnect::{
    OperatorAccountDisconnectError, OperatorAccountDisconnectInput,
    OperatorAccountDisconnectOutcome,
};

pub(crate) use flow_state::PendingOperatorAccountConnection;

#[cfg(test)]
use browser::browser_connection_request_fingerprint;
pub(crate) use management::operator_account_projection;
#[cfg(test)]
use management::project_operator_account;
#[cfg(test)]
use telegram::{ensure_telegram_operation, telegram_connection_request_fingerprint};

fn validate_prepare(input: &OperatorAccountConnectionPrepareInput) -> Result<(), String> {
    if input
        .label
        .as_deref()
        .is_some_and(|label| label.len() > 120)
    {
        return Err("label must be at most 120 characters".into());
    }
    match (input.intent, input.platform, input.method) {
        (
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionMethod::Browser,
        )
        | (
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionMethod::TelegramQr
            | OperatorAccountConnectionMethod::TelegramPhone,
        )
        | (
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionMethod::Browser,
        )
        | (
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionMethod::ExistingAccount,
        ) => {}
        _ => return Err("connection intent, platform, and method are incompatible".into()),
    }
    match input.intent {
        OperatorAccountConnectionIntent::Connect if input.operator_account_id.is_some() => {
            Err("operator_account_id is not accepted for a new connection".into())
        }
        OperatorAccountConnectionIntent::Reconnect if input.operator_account_id.is_none() => {
            Err("operator_account_id is required for reconnect".into())
        }
        _ => Ok(()),
    }
}

fn required_trimmed<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str, String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| message.to_string())
}

pub(crate) fn project_x_chat_passcode_status(
    platform: Platform,
    secret: Option<&ProfileSecret>,
) -> XChatPasscodeStatus {
    if platform != Platform::X {
        return XChatPasscodeStatus::NotApplicable;
    }
    match secret {
        None => XChatPasscodeStatus::Unavailable,
        Some(secret)
            if secret
                .x_chat_passcode
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()) =>
        {
            XChatPasscodeStatus::Configured
        }
        Some(_) => XChatPasscodeStatus::NotConfigured,
    }
}

fn require_confirmation(confirmed: bool) -> Result<(), String> {
    confirmed
        .then_some(())
        .ok_or_else(|| "confirmed=true is required".to_string())
}

fn validate_idempotency_key(key: &str) -> Result<(), String> {
    let len = key.trim().len();
    if len == 0 || len > 128 {
        return Err("idempotency_key must contain 1 to 128 characters".into());
    }
    Ok(())
}

pub(crate) fn domain_platform(platform: OperatorAccountPlatform) -> Platform {
    match platform {
        OperatorAccountPlatform::X => Platform::X,
        OperatorAccountPlatform::Instagram => Platform::Instagram,
        OperatorAccountPlatform::Telegram => Platform::Telegram,
    }
}

#[cfg(test)]
mod tests;
