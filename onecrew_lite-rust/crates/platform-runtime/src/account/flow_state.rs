use onecrew_local_domain::LocalProfile;
use onecrew_tg::{LoginCodeDelivery, QrLoginChallenge};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{LocalPlatform, ProxyConfig, ProxyKind};

use super::{
    OperatorAccountConnectionAction, OperatorAccountConnectionHandoff,
    OperatorAccountConnectionHandoffKind, OperatorAccountConnectionIntent,
    OperatorAccountConnectionKind, OperatorAccountConnectionMethod, OperatorAccountConnectionPhase,
    OperatorAccountConnectionStatus, OperatorAccountPlatform,
};

pub(super) const TERMINAL_FLOW_TTL: Duration = Duration::from_secs(5 * 60);

pub(crate) struct PendingOperatorAccountConnection {
    pub(super) created_at: Instant,
    pub(super) expires_at: Instant,
    pub(super) platform: OperatorAccountPlatform,
    pub(super) intent: OperatorAccountConnectionIntent,
    pub(super) method: OperatorAccountConnectionMethod,
    pub(super) phase: OperatorAccountConnectionPhase,
    pub(super) target_account_id: Option<String>,
    pub(super) label: Option<String>,
    pub(super) secret: Option<InAppConnectionSecret>,
    pub(super) live: Option<LiveConnection>,
    pub(super) completed_account_id: Option<String>,
    pub(super) request_idempotency_key: Option<String>,
    /// SHA-256 over the canonical direct-start request. Secret-bearing request
    /// values are never retained in the workflow map after begin.
    pub(super) request_fingerprint: Option<[u8; 32]>,
    pub(super) begin_idempotency_key: Option<String>,
    pub(super) finalize_idempotency_key: Option<String>,
    pub(super) qr_challenge: Option<QrLoginChallenge>,
    pub(super) code_delivery: Option<LoginCodeDelivery>,
    pub(super) error_code: Option<String>,
    pub(super) operation_token: Option<String>,
    pub(super) cancel_requested: bool,
}

pub(super) struct DirectRequestReservation {
    pub(super) idempotency_key: String,
    pub(super) fingerprint: [u8; 32],
}

// In-app secrets hold the proxy value model the begin path consumes; the
// request fingerprints below hash the inbound runtime carrier.
pub(super) enum InAppConnectionSecret {
    Browser {
        profile_name: String,
        proxy: onecrew_proxy::ProxyConfig,
        x_chat_passcode: Option<String>,
    },
    Telegram {
        api_id: i32,
        api_hash: String,
        phone: Option<String>,
        proxy: Option<onecrew_proxy::ProxyConfig>,
    },
}

pub(super) enum LiveConnection {
    Browser { token: String },
    TelegramQr { token: String },
    TelegramPhone { token: String },
    TelegramPassword { token: String },
}

pub(super) fn hash_proxy_fingerprint(digest: &mut Sha256, proxy: &ProxyConfig) {
    hash_fingerprint_field(
        digest,
        match proxy.kind {
            ProxyKind::Http => b"http",
            ProxyKind::Https => b"https",
            ProxyKind::Socks5 => b"socks5",
        },
    );
    hash_fingerprint_field(digest, proxy.host.as_bytes());
    hash_fingerprint_field(digest, &proxy.port.to_be_bytes());
    hash_optional_fingerprint_field(digest, proxy.username.as_deref().map(str::as_bytes));
    hash_optional_fingerprint_field(digest, proxy.password.as_deref().map(str::as_bytes));
}

pub(super) fn hash_optional_fingerprint_field(digest: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            digest.update([1]);
            hash_fingerprint_field(digest, value);
        }
        None => digest.update([0]),
    }
}

pub(super) fn hash_fingerprint_field(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

pub(super) fn complete_flow(flow: &mut PendingOperatorAccountConnection, profile: &LocalProfile) {
    flow.live = None;
    flow.secret = None;
    flow.qr_challenge = None;
    flow.code_delivery = None;
    flow.completed_account_id = Some(profile.id.to_string());
    flow.phase = OperatorAccountConnectionPhase::Completed;
    flow.expires_at = Instant::now() + TERMINAL_FLOW_TTL;
    flow.error_code = None;
    flow.operation_token = None;
    flow.cancel_requested = false;
}

pub(super) fn mark_flow_canceled(flow: &mut PendingOperatorAccountConnection) {
    flow.live = None;
    flow.secret = None;
    flow.qr_challenge = None;
    flow.code_delivery = None;
    flow.phase = OperatorAccountConnectionPhase::Canceled;
    flow.expires_at = Instant::now() + TERMINAL_FLOW_TTL;
    flow.error_code = None;
    flow.operation_token = None;
    flow.cancel_requested = true;
}

pub(super) fn is_terminal_phase(phase: OperatorAccountConnectionPhase) -> bool {
    matches!(
        phase,
        OperatorAccountConnectionPhase::Completed
            | OperatorAccountConnectionPhase::Canceled
            | OperatorAccountConnectionPhase::Failed
    )
}

fn allowed_actions(phase: OperatorAccountConnectionPhase) -> Vec<OperatorAccountConnectionAction> {
    use OperatorAccountConnectionAction as Action;
    use OperatorAccountConnectionPhase as Phase;
    match phase {
        Phase::AwaitingSecretInApp => vec![Action::OpenDesktop, Action::Status, Action::Cancel],
        Phase::ReadyToBegin => vec![Action::Begin, Action::Status, Action::Cancel],
        Phase::Starting | Phase::Finalizing => vec![Action::Status, Action::Cancel],
        Phase::AwaitingUserInApp => vec![
            Action::OpenDesktop,
            Action::Status,
            Action::Finalize,
            Action::Cancel,
        ],
        Phase::Completed | Phase::Canceled => vec![Action::Status],
        Phase::Failed => vec![Action::Status, Action::Cancel],
    }
}

fn handoff(flow: &PendingOperatorAccountConnection) -> Option<OperatorAccountConnectionHandoff> {
    use OperatorAccountConnectionHandoffKind as Kind;
    let kind = match (&flow.live, flow.phase) {
        (_, OperatorAccountConnectionPhase::AwaitingSecretInApp)
            if matches!(
                flow.live.as_ref(),
                Some(LiveConnection::TelegramPhone { .. })
            ) =>
        {
            Kind::EnterTelegramOtpInDesktop
        }
        (_, OperatorAccountConnectionPhase::AwaitingSecretInApp)
            if matches!(
                flow.live.as_ref(),
                Some(LiveConnection::TelegramPassword { .. })
            ) =>
        {
            Kind::EnterTelegramTwoFactorInDesktop
        }
        (_, OperatorAccountConnectionPhase::AwaitingSecretInApp) => {
            Kind::ConfigureConnectionInDesktop
        }
        (
            Some(LiveConnection::Browser { .. }),
            OperatorAccountConnectionPhase::AwaitingUserInApp,
        ) => Kind::CompleteBrowserLoginInDesktop,
        (
            Some(LiveConnection::TelegramQr { .. }),
            OperatorAccountConnectionPhase::AwaitingUserInApp,
        ) => Kind::ScanTelegramQrInDesktop,
        _ => return None,
    };
    let message = match kind {
        Kind::ConfigureConnectionInDesktop => {
            "Open the desktop Accounts connection flow and enter credentials there; do not send secrets through MCP."
        }
        Kind::CompleteBrowserLoginInDesktop => {
            "Complete the X or Instagram login in the desktop-opened browser, then call finalize with explicit confirmation."
        }
        Kind::ScanTelegramQrInDesktop => {
            "Open the desktop connection flow and scan the displayed Telegram QR code; the QR payload is never returned through MCP."
        }
        Kind::EnterTelegramOtpInDesktop => {
            "Enter the Telegram one-time code in the desktop app; OTP values are never accepted through MCP."
        }
        Kind::EnterTelegramTwoFactorInDesktop => {
            "Enter the Telegram two-factor password in the desktop app; 2FA values are never accepted through MCP."
        }
    };
    Some(OperatorAccountConnectionHandoff {
        kind,
        message: message.into(),
    })
}

impl LocalPlatform {
    pub(crate) fn connection_cancel_requested(&self, flow_id: &str) -> Result<bool, String> {
        let flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        Ok(flows
            .get(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?
            .cancel_requested)
    }

    pub(super) fn operator_account_connection_snapshot(
        &self,
        flow_id: &str,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        let (
            platform,
            intent,
            method,
            phase,
            expires_at,
            account_id,
            handoff,
            allowed_actions,
            error_code,
        ) = {
            let flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            (
                flow.platform,
                flow.intent,
                flow.method,
                flow.phase,
                project_expiry(flow.expires_at),
                flow.completed_account_id
                    .clone()
                    .or_else(|| flow.target_account_id.clone()),
                handoff(flow),
                allowed_actions(flow.phase),
                flow.error_code.clone(),
            )
        };
        let operator_account = account_id
            .as_deref()
            .and_then(|id| self.operator_account_by_id(id).ok());
        let browser_capability =
            (method == OperatorAccountConnectionMethod::Browser).then(|| self.browser_status());
        Ok(OperatorAccountConnectionStatus {
            flow_id: flow_id.to_string(),
            platform,
            intent,
            method,
            kind: connection_kind(platform, intent, method),
            phase,
            expires_at,
            operator_account,
            handoff,
            allowed_actions,
            browser_capability,
            error_code,
        })
    }

    pub(super) fn direct_operation_for_request_key(
        &self,
        request_key: &str,
        request_fingerprint: &[u8; 32],
    ) -> Result<Option<String>, String> {
        let flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let Some((operation_id, flow)) = flows
            .iter()
            .find(|(_, flow)| flow.request_idempotency_key.as_deref() == Some(request_key))
        else {
            return Ok(None);
        };
        if flow.request_fingerprint.as_ref() != Some(request_fingerprint) {
            return Err("idempotency_key is already used by another connection request".into());
        }
        Ok(Some(operation_id.clone()))
    }
}

fn connection_kind(
    platform: OperatorAccountPlatform,
    intent: OperatorAccountConnectionIntent,
    method: OperatorAccountConnectionMethod,
) -> OperatorAccountConnectionKind {
    match (platform, intent, method) {
        (
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountConnectionMethod::Browser,
        ) => OperatorAccountConnectionKind::BrowserConnect,
        (
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountConnectionMethod::Browser,
        ) => OperatorAccountConnectionKind::BrowserReconnect,
        (
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountConnectionMethod::TelegramQr,
        ) => OperatorAccountConnectionKind::TelegramQrConnect,
        (
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountConnectionMethod::TelegramPhone,
        ) => OperatorAccountConnectionKind::TelegramPhoneConnect,
        (
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountConnectionMethod::ExistingAccount,
        ) => OperatorAccountConnectionKind::TelegramReconnect,
        _ => unreachable!("validated connection flows have a closed kind"),
    }
}

pub(super) fn project_expiry(expires_at: Instant) -> u64 {
    let remaining = expires_at.saturating_duration_since(Instant::now());
    SystemTime::now()
        .checked_add(remaining)
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(u64::MAX)
}
