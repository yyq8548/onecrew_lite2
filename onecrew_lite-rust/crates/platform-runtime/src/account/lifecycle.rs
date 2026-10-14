use std::time::Instant;
use uuid::Uuid;

use crate::LocalPlatform;

use super::browser::browser_failure_code;
use super::flow_state::{
    DirectRequestReservation, LiveConnection, PendingOperatorAccountConnection, TERMINAL_FLOW_TTL,
    complete_flow, is_terminal_phase, mark_flow_canceled,
};
use super::{
    CONNECTION_FLOW_TTL, MAX_PENDING_CONNECTION_FLOWS, OperatorAccountConnectionBeginInput,
    OperatorAccountConnectionCancelInput, OperatorAccountConnectionFinalizeInput,
    OperatorAccountConnectionInAppHandoff, OperatorAccountConnectionIntent,
    OperatorAccountConnectionMethod, OperatorAccountConnectionPhase,
    OperatorAccountConnectionPrepareInput, OperatorAccountConnectionStatus,
    OperatorAccountConnectionStatusInput, OperatorAccountPlatform, require_confirmation,
    required_trimmed, validate_idempotency_key, validate_prepare,
};

mod begin;

impl LocalPlatform {
    pub(super) async fn cleanup_connection_flows_async(&self) -> Result<(), String> {
        let expired_live = {
            let now = Instant::now();
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let expired_ids = flows
                .iter()
                .filter(|(_, flow)| {
                    flow.expires_at <= now
                        && !matches!(
                            flow.phase,
                            OperatorAccountConnectionPhase::Starting
                                | OperatorAccountConnectionPhase::Finalizing
                        )
                        && flow.operation_token.is_none()
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            expired_ids
                .into_iter()
                .filter_map(|id| flows.remove(&id).and_then(|mut flow| flow.live.take()))
                .collect::<Vec<_>>()
        };
        for live in expired_live {
            self.cleanup_live_connection(live).await;
        }
        Ok(())
    }

    pub(super) async fn cleanup_live_connection(&self, live: LiveConnection) {
        match live {
            LiveConnection::Browser { token } => {
                if let Err(error) = self.browser_cancel(&token) {
                    log::warn!("browser connection cleanup failed: {error}");
                }
            }
            LiveConnection::TelegramQr { token } => self.tg_cancel_qr_login(&token).await,
            LiveConnection::TelegramPhone { token }
            | LiveConnection::TelegramPassword { token } => self.tg_cancel_login(&token).await,
        }
    }

    pub async fn operator_account_connection_prepare(
        &self,
        input: OperatorAccountConnectionPrepareInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.operator_account_connection_prepare_internal(input, None)
            .await
            .map(|(status, _created)| status)
    }

    pub(super) async fn operator_account_connection_prepare_internal(
        &self,
        input: OperatorAccountConnectionPrepareInput,
        request_reservation: Option<DirectRequestReservation>,
    ) -> Result<(OperatorAccountConnectionStatus, bool), String> {
        self.cleanup_connection_flows_async().await?;
        validate_prepare(&input)?;
        let target = match input.intent {
            OperatorAccountConnectionIntent::Connect => None,
            OperatorAccountConnectionIntent::Reconnect => {
                let account_id = required_trimmed(
                    input.operator_account_id.as_deref(),
                    "operator_account_id is required for reconnect",
                )?;
                let projection = self.operator_account_by_id(account_id)?;
                if projection.platform != input.platform {
                    return Err("operator_account_id does not match platform".into());
                }
                Some(account_id.to_string())
            }
        };

        let (phase, error_code) = if input.intent == OperatorAccountConnectionIntent::Connect {
            (OperatorAccountConnectionPhase::AwaitingSecretInApp, None)
        } else {
            self.reconnect_initial_phase(&input, target.as_deref().expect("validated target"))?
        };
        let flow_id = Uuid::new_v4().to_string();
        let now = Instant::now();
        let expires_at = now
            + if is_terminal_phase(phase) {
                TERMINAL_FLOW_TTL
            } else {
                CONNECTION_FLOW_TTL
            };
        let pending = PendingOperatorAccountConnection {
            created_at: now,
            expires_at,
            platform: input.platform,
            intent: input.intent,
            method: input.method,
            phase,
            target_account_id: target,
            label: input
                .label
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            secret: None,
            live: None,
            completed_account_id: None,
            request_idempotency_key: request_reservation
                .as_ref()
                .map(|reservation| reservation.idempotency_key.clone()),
            request_fingerprint: request_reservation
                .as_ref()
                .map(|reservation| reservation.fingerprint),
            begin_idempotency_key: None,
            finalize_idempotency_key: None,
            qr_challenge: None,
            code_delivery: None,
            error_code,
            operation_token: None,
            cancel_requested: false,
        };
        let mut flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        if let Some(reservation) = request_reservation.as_ref()
            && let Some((existing_id, existing)) = flows.iter().find(|(_, flow)| {
                flow.request_idempotency_key.as_deref()
                    == Some(reservation.idempotency_key.as_str())
            })
        {
            if existing.request_fingerprint.as_ref() != Some(&reservation.fingerprint) {
                return Err("idempotency_key is already used by another connection request".into());
            }
            let existing_id = existing_id.clone();
            drop(flows);
            return self
                .operator_account_connection_snapshot(&existing_id)
                .map(|status| (status, false));
        }
        if flows.len() >= MAX_PENDING_CONNECTION_FLOWS {
            let oldest_terminal = flows
                .iter()
                .filter(|(_, flow)| is_terminal_phase(flow.phase))
                .min_by_key(|(_, flow)| flow.created_at)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest_terminal {
                flows.remove(&id);
            } else {
                return Err("too many active operator account connection flows".into());
            }
        }
        flows.insert(flow_id.clone(), pending);
        drop(flows);
        self.operator_account_connection_snapshot(&flow_id)
            .map(|status| (status, true))
    }

    pub async fn operator_account_connection_begin(
        &self,
        input: OperatorAccountConnectionBeginInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.operator_account_connection_begin_internal(input).await
    }

    pub async fn operator_account_connection_status(
        &self,
        input: OperatorAccountConnectionStatusInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        let qr_token = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(&input.flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            let token = match flow.live.as_ref() {
                Some(LiveConnection::TelegramQr { token })
                    if flow.phase == OperatorAccountConnectionPhase::AwaitingUserInApp =>
                {
                    Some(token.clone())
                }
                _ => None,
            };
            if token.is_some() {
                flow.phase = OperatorAccountConnectionPhase::Starting;
            }
            token
        };

        let cleanup_live = if let Some(token) = qr_token {
            match self.tg_poll_qr_login(&token).await {
                Ok(result) => {
                    let mut flows = self
                        .pending_account_connections
                        .lock()
                        .map_err(|_| "operator account connection lock poisoned".to_string())?;
                    let flow = flows
                        .get_mut(&input.flow_id)
                        .ok_or_else(|| "operator account connection flow not found".to_string())?;
                    let completed_profile = result.profile;
                    if completed_profile.is_none() && result.needs_password {
                        flow.live = Some(LiveConnection::TelegramPassword { token });
                        flow.qr_challenge = None;
                        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
                    } else if completed_profile.is_none() {
                        flow.live = Some(LiveConnection::TelegramQr { token });
                        flow.qr_challenge = result.challenge;
                        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
                    }
                    if flow.cancel_requested {
                        let live = flow.live.take();
                        mark_flow_canceled(flow);
                        live
                    } else {
                        if let Some(profile) = completed_profile {
                            complete_flow(flow, &profile);
                        }
                        flow.error_code = None;
                        None
                    }
                }
                Err(_) => {
                    let mut flows = self
                        .pending_account_connections
                        .lock()
                        .map_err(|_| "operator account connection lock poisoned".to_string())?;
                    let flow = flows
                        .get_mut(&input.flow_id)
                        .ok_or_else(|| "operator account connection flow not found".to_string())?;
                    if flow.cancel_requested {
                        let live = flow.live.take();
                        mark_flow_canceled(flow);
                        live
                    } else {
                        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
                        flow.error_code = Some("telegram_qr_status_temporarily_unavailable".into());
                        None
                    }
                }
            }
        } else {
            None
        };
        if let Some(live) = cleanup_live {
            self.cleanup_live_connection(live).await;
        }
        self.operator_account_connection_snapshot(&input.flow_id)
    }

    pub async fn operator_account_connection_finalize(
        &self,
        input: OperatorAccountConnectionFinalizeInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        require_confirmation(input.confirmed)?;
        validate_idempotency_key(&input.idempotency_key)?;

        let current = self.operator_account_connection_snapshot(&input.flow_id)?;
        if current.method == OperatorAccountConnectionMethod::Browser
            && current.phase == OperatorAccountConnectionPhase::AwaitingUserInApp
        {
            self.require_browser_completion_capability()?;
        }

        let (browser_token, expected_account_id) = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(&input.flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if flow.phase == OperatorAccountConnectionPhase::Completed {
                if let Some(existing) = flow.finalize_idempotency_key.as_deref()
                    && existing != input.idempotency_key
                {
                    return Err(
                        "connection finalize already used a different idempotency_key".into(),
                    );
                }
                flow.finalize_idempotency_key = Some(input.idempotency_key);
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            if let Some(existing) = flow.finalize_idempotency_key.as_deref() {
                if existing != input.idempotency_key {
                    return Err(
                        "connection finalize already used a different idempotency_key".into(),
                    );
                }
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            let token = match flow.live.as_ref() {
                Some(LiveConnection::Browser { token })
                    if flow.phase == OperatorAccountConnectionPhase::AwaitingUserInApp =>
                {
                    token.clone()
                }
                _ => {
                    drop(flows);
                    return self.operator_account_connection_snapshot(&input.flow_id);
                }
            };
            flow.finalize_idempotency_key = Some(input.idempotency_key);
            flow.phase = OperatorAccountConnectionPhase::Finalizing;
            let expected_account_id = (flow.intent == OperatorAccountConnectionIntent::Reconnect)
                .then(|| flow.target_account_id.clone())
                .flatten();
            (token, expected_account_id)
        };

        match self
            .browser_extract_for_account(&browser_token, &input.flow_id)
            .await
        {
            Ok(profile) => {
                if expected_account_id
                    .as_deref()
                    .is_some_and(|expected| expected != profile.id.to_string())
                {
                    return self
                        .fail_connection(
                            &input.flow_id,
                            "reconnect_returned_a_different_operator_account",
                        )
                        .await;
                }
                let mut flows = self
                    .pending_account_connections
                    .lock()
                    .map_err(|_| "operator account connection lock poisoned".to_string())?;
                let flow = flows
                    .get_mut(&input.flow_id)
                    .ok_or_else(|| "operator account connection flow not found".to_string())?;
                if flow.cancel_requested {
                    mark_flow_canceled(flow);
                } else {
                    complete_flow(flow, &profile);
                }
                drop(flows);
                self.operator_account_connection_snapshot(&input.flow_id)
            }
            Err(_) if self.connection_cancel_requested(&input.flow_id)? => {
                self.cancel_requested_connection(&input.flow_id).await
            }
            Err(error) => {
                let error_code = browser_failure_code(&error);
                self.fail_connection(&input.flow_id, error_code).await
            }
        }
    }

    pub async fn operator_account_connection_cancel(
        &self,
        input: OperatorAccountConnectionCancelInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        require_confirmation(input.confirmed)?;
        let live = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(&input.flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if flow.phase == OperatorAccountConnectionPhase::Canceled {
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            if flow.phase == OperatorAccountConnectionPhase::Completed {
                return Err("completed operator account connections cannot be canceled".into());
            }
            if matches!(
                flow.phase,
                OperatorAccountConnectionPhase::Starting
                    | OperatorAccountConnectionPhase::Finalizing
            ) {
                flow.cancel_requested = true;
                flow.error_code = Some("cancellation_requested".into());
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            let live = flow.live.take();
            flow.secret = None;
            flow.qr_challenge = None;
            flow.code_delivery = None;
            flow.phase = OperatorAccountConnectionPhase::Canceled;
            flow.expires_at = Instant::now() + TERMINAL_FLOW_TTL;
            flow.error_code = None;
            flow.operation_token = None;
            flow.cancel_requested = true;
            live
        };

        if let Some(live) = live {
            self.cleanup_live_connection(live).await;
        }
        self.operator_account_connection_snapshot(&input.flow_id)
    }

    /// Desktop-only access to QR/code-delivery display material.
    pub async fn operator_account_connection_in_app_handoff(
        &self,
        flow_id: &str,
    ) -> Result<OperatorAccountConnectionInAppHandoff, String> {
        self.cleanup_connection_flows_async().await?;
        let flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let flow = flows
            .get(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?;
        Ok(OperatorAccountConnectionInAppHandoff {
            flow_id: flow_id.to_string(),
            telegram_qr_challenge: flow.qr_challenge.clone().map(Into::into),
            telegram_code_delivery: flow.code_delivery.clone().map(Into::into),
        })
    }

    fn reconnect_initial_phase(
        &self,
        input: &OperatorAccountConnectionPrepareInput,
        account_id: &str,
    ) -> Result<(OperatorAccountConnectionPhase, Option<String>), String> {
        let id = Uuid::parse_str(account_id).map_err(|_| "invalid operator_account_id")?;
        let (profile, secret) = match self.vault.get(id) {
            Ok(value) => value,
            Err(_) if input.platform == OperatorAccountPlatform::Telegram => {
                return Ok((
                    OperatorAccountConnectionPhase::Failed,
                    Some("telegram_reconnect_unavailable_start_new_connection".into()),
                ));
            }
            Err(_) => {
                return Ok((
                    OperatorAccountConnectionPhase::Failed,
                    Some("browser_reconnect_unavailable_start_new_connection".into()),
                ));
            }
        };
        if profile.provider_account_id.trim().is_empty()
            || secret.provider_account_id.trim().is_empty()
            || profile.provider_account_id != secret.provider_account_id
        {
            let error_code = match input.platform {
                OperatorAccountPlatform::Telegram => {
                    "telegram_reconnect_unavailable_start_new_connection"
                }
                OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram => {
                    "browser_reconnect_unavailable_start_new_connection"
                }
            };
            return Ok((
                OperatorAccountConnectionPhase::Failed,
                Some(error_code.into()),
            ));
        }
        let ready = match input.platform {
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram => {
                !secret.browser_profile_dir.trim().is_empty()
                    && std::path::Path::new(&secret.browser_profile_dir).exists()
                    && secret.proxy.is_some()
            }
            OperatorAccountPlatform::Telegram => {
                secret.telegram_api_id.is_some()
                    && secret
                        .telegram_api_hash
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
                    && secret
                        .telegram_session
                        .as_deref()
                        .is_some_and(|value| !value.is_empty())
            }
        };
        Ok(if ready {
            (OperatorAccountConnectionPhase::ReadyToBegin, None)
        } else {
            let error_code = match input.platform {
                OperatorAccountPlatform::Telegram => {
                    "telegram_reconnect_unavailable_start_new_connection"
                }
                OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram => {
                    "browser_reconnect_unavailable_start_new_connection"
                }
            };
            (
                OperatorAccountConnectionPhase::Failed,
                Some(error_code.into()),
            )
        })
    }

    pub(super) async fn cancel_requested_connection(
        &self,
        flow_id: &str,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        let live = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            let live = flow.live.take();
            mark_flow_canceled(flow);
            live
        };
        if let Some(live) = live {
            self.cleanup_live_connection(live).await;
        }
        self.operator_account_connection_snapshot(flow_id)
    }

    pub(super) async fn fail_connection(
        &self,
        flow_id: &str,
        error_code: &str,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        let live = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            let live = flow.live.take();
            flow.secret = None;
            flow.qr_challenge = None;
            flow.code_delivery = None;
            flow.phase = if flow.cancel_requested {
                OperatorAccountConnectionPhase::Canceled
            } else {
                OperatorAccountConnectionPhase::Failed
            };
            flow.expires_at = Instant::now() + TERMINAL_FLOW_TTL;
            flow.error_code = (!flow.cancel_requested).then(|| error_code.to_string());
            flow.operation_token = None;
            live
        };
        if let Some(live) = live {
            self.cleanup_live_connection(live).await;
        }
        self.operator_account_connection_snapshot(flow_id)
    }
}
