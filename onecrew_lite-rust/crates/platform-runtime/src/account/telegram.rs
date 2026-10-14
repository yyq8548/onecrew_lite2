use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{LocalPlatform, ProxyConfig};

use super::flow_state::{
    DirectRequestReservation, InAppConnectionSecret, LiveConnection,
    PendingOperatorAccountConnection, complete_flow, hash_fingerprint_field,
    hash_optional_fingerprint_field, hash_proxy_fingerprint, mark_flow_canceled, project_expiry,
};
use super::{
    AccountConnectionOperationInput, DirectAccountConnectionCancelInput,
    DirectAccountConnectionPhase, OperatorAccountConnectionBeginInput,
    OperatorAccountConnectionCancelInput, OperatorAccountConnectionIntent,
    OperatorAccountConnectionMethod, OperatorAccountConnectionPhase,
    OperatorAccountConnectionPrepareInput, OperatorAccountConnectionStatus,
    OperatorAccountConnectionStatusInput, OperatorAccountPlatform, TelegramAccountConnectionMethod,
    TelegramAccountConnectionOtpInput, TelegramAccountConnectionStartInput,
    TelegramAccountConnectionStatus, TelegramAccountConnectionTwoFactorInput, TelegramQrCode,
    validate_idempotency_key,
};

impl LocalPlatform {
    /// Start Telegram using caller-supplied credentials. QR is the default and
    /// no phone fallback occurs. Telegram strictly requires a working SOCKS5
    /// residential proxy before any MTProto connection is created.
    pub async fn telegram_account_connection_start(
        &self,
        input: TelegramAccountConnectionStartInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        validate_idempotency_key(&input.idempotency_key)?;
        if input.api_id <= 0 || input.api_hash.trim().is_empty() {
            return Err("Telegram api_id and api_hash are required".into());
        }
        if onecrew_sessions::telegram_socks5_proxy_url(Some(&input.proxy.clone().into())).is_err() {
            return Err("Telegram account connection requires a SOCKS5 proxy".into());
        }
        let phone = input.phone.filter(|value| !value.is_empty());
        if input.method == TelegramAccountConnectionMethod::Phone && phone.is_none() {
            return Err("Telegram phone login requires a phone number".into());
        }
        if let Some(phone) = phone.as_deref() {
            validate_e164_phone(phone)?;
        }
        let method = match input.method {
            TelegramAccountConnectionMethod::Qr => OperatorAccountConnectionMethod::TelegramQr,
            TelegramAccountConnectionMethod::Phone => {
                OperatorAccountConnectionMethod::TelegramPhone
            }
        };
        let fingerprint = telegram_connection_request_fingerprint(
            input.api_id,
            &input.api_hash,
            &input.proxy,
            input.method,
            phone.as_deref(),
        );
        self.cleanup_connection_flows_async().await?;
        if let Some(operation_id) =
            self.direct_operation_for_request_key(&input.idempotency_key, &fingerprint)?
        {
            return self
                .telegram_account_connection_status(AccountConnectionOperationInput {
                    operation_id,
                })
                .await;
        }
        self.proxy_validate(input.proxy.clone().into()).await?;
        let (prepared, created) = self
            .operator_account_connection_prepare_internal(
                OperatorAccountConnectionPrepareInput {
                    platform: OperatorAccountPlatform::Telegram,
                    intent: OperatorAccountConnectionIntent::Connect,
                    method,
                    operator_account_id: None,
                    label: None,
                },
                Some(DirectRequestReservation {
                    idempotency_key: input.idempotency_key.clone(),
                    fingerprint,
                }),
            )
            .await?;
        if !created {
            return self
                .telegram_account_connection_status(AccountConnectionOperationInput {
                    operation_id: prepared.flow_id,
                })
                .await;
        }
        if self
            .operator_account_connection_configure_telegram_in_app(
                &prepared.flow_id,
                input.api_id,
                input.api_hash,
                phone,
                Some(input.proxy),
            )
            .await
            .is_err()
        {
            if let Err(error) = self
                .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
                    flow_id: prepared.flow_id,
                    confirmed: true,
                })
                .await
            {
                log::warn!("cancelling the failed operator connection flow failed: {error}");
            }
            return Err("Telegram account connection configuration failed".into());
        }
        self.operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
            idempotency_key: input.idempotency_key,
        })
        .await?;
        self.telegram_account_connection_status(AccountConnectionOperationInput {
            operation_id: prepared.flow_id,
        })
        .await
    }

    pub async fn telegram_account_connection_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        self.telegram_account_connection_snapshot(&input.operation_id)?;
        self.operator_account_connection_status(OperatorAccountConnectionStatusInput {
            flow_id: input.operation_id.clone(),
        })
        .await?;
        self.telegram_account_connection_snapshot(&input.operation_id)
    }

    pub async fn telegram_account_connection_submit_otp(
        &self,
        input: TelegramAccountConnectionOtpInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        if input.otp.trim().is_empty() {
            return Err("Telegram OTP is required".into());
        }
        self.operator_account_connection_submit_telegram_code_in_app(
            &input.operation_id,
            input.otp.trim(),
        )
        .await
        .map_err(|error| {
            if error.contains("invalid or expired login code") {
                "Telegram OTP is invalid or expired".to_string()
            } else {
                "Telegram OTP submission failed".to_string()
            }
        })?;
        self.telegram_account_connection_snapshot(&input.operation_id)
    }

    pub async fn telegram_account_connection_submit_two_factor(
        &self,
        input: TelegramAccountConnectionTwoFactorInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        if input.password.is_empty() {
            return Err("Telegram two-factor password is required".into());
        }
        self.operator_account_connection_submit_telegram_password_in_app(
            &input.operation_id,
            &input.password,
        )
        .await
        .map_err(|error| {
            if error.contains("invalid 2FA password") {
                "Telegram two-factor password is invalid".to_string()
            } else {
                "Telegram two-factor submission failed".to_string()
            }
        })?;
        self.telegram_account_connection_snapshot(&input.operation_id)
    }

    pub async fn telegram_account_connection_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        // Validate the typed facade before mutating the shared underlying flow.
        self.telegram_account_connection_snapshot(&input.operation_id)?;
        self.operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
            flow_id: input.operation_id.clone(),
            confirmed: input.confirmed,
        })
        .await?;
        self.telegram_account_connection_snapshot(&input.operation_id)
    }

    /// In-app credential handoff used by the GUI and the direct Telegram
    /// facade after it has validated the caller-supplied credentials.
    pub async fn operator_account_connection_configure_telegram_in_app(
        &self,
        flow_id: &str,
        api_id: i32,
        api_hash: String,
        phone: Option<String>,
        proxy: Option<ProxyConfig>,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        if api_id <= 0 || api_hash.trim().is_empty() {
            return Err("Telegram api_id and api_hash are required in the desktop app".into());
        }
        let proxy = proxy.map(onecrew_proxy::ProxyConfig::from);
        onecrew_sessions::telegram_socks5_proxy_url(proxy.as_ref())
            .map_err(|_| crate::telegram::TELEGRAM_SOCKS5_REQUIRED.to_string())?;
        let mut flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let flow = flows
            .get_mut(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?;
        if flow.platform != OperatorAccountPlatform::Telegram
            || flow.intent != OperatorAccountConnectionIntent::Connect
            || !matches!(
                flow.method,
                OperatorAccountConnectionMethod::TelegramQr
                    | OperatorAccountConnectionMethod::TelegramPhone
            )
        {
            return Err("Telegram configuration does not match this flow".into());
        }
        if !matches!(
            flow.phase,
            OperatorAccountConnectionPhase::AwaitingSecretInApp
                | OperatorAccountConnectionPhase::ReadyToBegin
        ) || flow.live.is_some()
        {
            return Err(
                "operator account connection cannot be configured in its current phase".into(),
            );
        }
        let phone = phone.filter(|value| !value.is_empty());
        if flow.method == OperatorAccountConnectionMethod::TelegramPhone && phone.is_none() {
            return Err("Telegram phone is required in the desktop app".into());
        }
        if let Some(phone) = phone.as_deref() {
            validate_e164_phone(phone)?;
        }
        flow.secret = Some(InAppConnectionSecret::Telegram {
            api_id,
            api_hash,
            phone,
            proxy,
        });
        flow.phase = OperatorAccountConnectionPhase::ReadyToBegin;
        flow.error_code = None;
        drop(flows);
        self.operator_account_connection_snapshot(flow_id)
    }

    /// Desktop-only convenience for obtaining Telegram API credentials. The
    /// browser primitive remains private behind the canonical flow boundary.
    pub async fn operator_account_connection_open_telegram_credentials_in_app(
        &self,
        flow_id: &str,
        proxy: ProxyConfig,
    ) -> Result<(), String> {
        self.cleanup_connection_flows_async().await?;
        {
            let flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if flow.platform != OperatorAccountPlatform::Telegram
                || flow.intent != OperatorAccountConnectionIntent::Connect
                || !matches!(
                    flow.phase,
                    OperatorAccountConnectionPhase::AwaitingSecretInApp
                        | OperatorAccountConnectionPhase::ReadyToBegin
                )
                || flow.live.is_some()
            {
                return Err("Telegram credential browser is unavailable for this flow".into());
            }
        }
        self.tg_open_credentials(proxy.into()).await
    }

    /// Lower-level OTP submission. The direct facade accepts OTP as a
    /// deserialize-only input and returns only a secret-free projection.
    pub async fn operator_account_connection_submit_telegram_code_in_app(
        &self,
        flow_id: &str,
        code: &str,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        let (operation_token, token) = self.begin_telegram_operation(flow_id, false)?;
        match self.tg_submit_code(&token, code).await {
            Ok(result) => {
                let cleanup_live = {
                    let mut flows = self
                        .pending_account_connections
                        .lock()
                        .map_err(|_| "operator account connection lock poisoned".to_string())?;
                    let flow = flows
                        .get_mut(flow_id)
                        .ok_or_else(|| "operator account connection flow not found".to_string())?;
                    ensure_telegram_operation(flow, &operation_token)?;
                    flow.operation_token = None;
                    let completed_profile = result.profile;
                    if completed_profile.is_none() && result.needs_password {
                        flow.live = Some(LiveConnection::TelegramPassword { token });
                        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
                    }
                    if flow.cancel_requested {
                        let live = flow.live.take();
                        mark_flow_canceled(flow);
                        live
                    } else {
                        if let Some(profile) = completed_profile {
                            complete_flow(flow, &profile);
                        }
                        None
                    }
                };
                if let Some(live) = cleanup_live {
                    self.cleanup_live_connection(live).await;
                }
                self.operator_account_connection_snapshot(flow_id)
            }
            Err(error) => {
                if self.connection_cancel_requested(flow_id)? {
                    self.cancel_requested_connection(flow_id).await?;
                    return Err("Telegram OTP submission was canceled".into());
                }
                self.restore_telegram_operation(flow_id, &operation_token)?;
                Err(error)
            }
        }
    }

    /// Lower-level Telegram 2FA submission. Like OTP, the value is consumed
    /// into the live login and never appears in a result.
    pub async fn operator_account_connection_submit_telegram_password_in_app(
        &self,
        flow_id: &str,
        password: &str,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        let (operation_token, token) = self.begin_telegram_operation(flow_id, true)?;
        match self.tg_submit_password(&token, password).await {
            Ok(profile) => {
                let cleanup_live = {
                    let mut flows = self
                        .pending_account_connections
                        .lock()
                        .map_err(|_| "operator account connection lock poisoned".to_string())?;
                    let flow = flows
                        .get_mut(flow_id)
                        .ok_or_else(|| "operator account connection flow not found".to_string())?;
                    ensure_telegram_operation(flow, &operation_token)?;
                    flow.operation_token = None;
                    if flow.cancel_requested {
                        let live = flow.live.take();
                        mark_flow_canceled(flow);
                        live
                    } else {
                        complete_flow(flow, &profile);
                        None
                    }
                };
                if let Some(live) = cleanup_live {
                    self.cleanup_live_connection(live).await;
                }
                self.operator_account_connection_snapshot(flow_id)
            }
            Err(error) => {
                if self.connection_cancel_requested(flow_id)? {
                    self.cancel_requested_connection(flow_id).await?;
                    return Err("Telegram two-factor submission was canceled".into());
                }
                self.restore_telegram_operation(flow_id, &operation_token)?;
                Err(error)
            }
        }
    }

    pub(super) fn begin_telegram_operation(
        &self,
        flow_id: &str,
        password: bool,
    ) -> Result<(String, String), String> {
        let mut flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let flow = flows
            .get_mut(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?;
        if flow.phase != OperatorAccountConnectionPhase::AwaitingSecretInApp
            || flow.operation_token.is_some()
        {
            return Err("operator account connection operation is in progress".into());
        }
        let token = match (&flow.live, password) {
            (Some(LiveConnection::TelegramPhone { token }), false) => token.clone(),
            (Some(LiveConnection::TelegramPassword { token }), true) => token.clone(),
            _ if password => {
                return Err("connection is not awaiting Telegram 2FA in the app".into());
            }
            _ => return Err("connection is not awaiting a Telegram OTP in the app".into()),
        };
        let operation_token = Uuid::new_v4().to_string();
        flow.operation_token = Some(operation_token.clone());
        flow.phase = OperatorAccountConnectionPhase::Starting;
        Ok((operation_token, token))
    }

    pub(super) fn restore_telegram_operation(
        &self,
        flow_id: &str,
        operation_token: &str,
    ) -> Result<(), String> {
        let mut flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let flow = flows
            .get_mut(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?;
        ensure_telegram_operation(flow, operation_token)?;
        flow.operation_token = None;
        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
        Ok(())
    }

    pub(super) fn telegram_account_connection_snapshot(
        &self,
        operation_id: &str,
    ) -> Result<TelegramAccountConnectionStatus, String> {
        let (phase, expires_at, qr_code, code_delivery, account_id, error_code) = {
            let flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get(operation_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if flow.platform != OperatorAccountPlatform::Telegram
                || !matches!(
                    flow.method,
                    OperatorAccountConnectionMethod::TelegramQr
                        | OperatorAccountConnectionMethod::TelegramPhone
                )
            {
                return Err("operation is not a Telegram account connection".into());
            }
            let phase = match (flow.phase, flow.live.as_ref()) {
                (OperatorAccountConnectionPhase::Completed, _) => {
                    DirectAccountConnectionPhase::Completed
                }
                (OperatorAccountConnectionPhase::Canceled, _) => {
                    DirectAccountConnectionPhase::Canceled
                }
                (OperatorAccountConnectionPhase::Failed, _) => DirectAccountConnectionPhase::Failed,
                (
                    OperatorAccountConnectionPhase::AwaitingUserInApp,
                    Some(LiveConnection::TelegramQr { .. }),
                ) => DirectAccountConnectionPhase::AwaitingTelegramQrScan,
                (
                    OperatorAccountConnectionPhase::AwaitingSecretInApp,
                    Some(LiveConnection::TelegramPhone { .. }),
                ) => DirectAccountConnectionPhase::AwaitingTelegramOtp,
                (
                    OperatorAccountConnectionPhase::AwaitingSecretInApp,
                    Some(LiveConnection::TelegramPassword { .. }),
                ) => DirectAccountConnectionPhase::AwaitingTelegramTwoFactor,
                _ => DirectAccountConnectionPhase::Starting,
            };
            let qr_code = flow.qr_challenge.as_ref().map(|challenge| TelegramQrCode {
                image_data_url: challenge.image_data_url.clone(),
                terminal_qr: challenge.terminal_qr.clone(),
                expires_at: challenge.expires_at,
            });
            (
                phase,
                project_expiry(flow.expires_at),
                qr_code,
                flow.code_delivery.clone().map(Into::into),
                flow.completed_account_id.clone(),
                flow.error_code.clone(),
            )
        };
        let operator_account = account_id
            .as_deref()
            .and_then(|id| self.operator_account_by_id(id).ok());
        Ok(TelegramAccountConnectionStatus {
            operation_id: operation_id.to_string(),
            phase,
            expires_at,
            qr_code,
            code_delivery,
            operator_account,
            error_code,
        })
    }
}

fn validate_e164_phone(phone: &str) -> Result<(), String> {
    let Some(digits) = phone.strip_prefix('+') else {
        return Err("Telegram phone must use strict E.164 format".into());
    };
    if !(8..=15).contains(&digits.len()) || !digits.chars().all(|value| value.is_ascii_digit()) {
        return Err("Telegram phone must use strict E.164 format".into());
    }
    Ok(())
}

pub(super) fn telegram_connection_request_fingerprint(
    api_id: i32,
    api_hash: &str,
    proxy: &ProxyConfig,
    method: TelegramAccountConnectionMethod,
    phone: Option<&str>,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    hash_fingerprint_field(&mut digest, b"direct-telegram-account-v1");
    hash_fingerprint_field(&mut digest, &api_id.to_be_bytes());
    hash_fingerprint_field(&mut digest, api_hash.as_bytes());
    hash_proxy_fingerprint(&mut digest, proxy);
    hash_fingerprint_field(
        &mut digest,
        match method {
            TelegramAccountConnectionMethod::Qr => b"qr",
            TelegramAccountConnectionMethod::Phone => b"phone",
        },
    );
    hash_optional_fingerprint_field(&mut digest, phone.map(str::as_bytes));
    digest.finalize().into()
}

pub(super) fn ensure_telegram_operation(
    flow: &PendingOperatorAccountConnection,
    operation_token: &str,
) -> Result<(), String> {
    if flow.phase == OperatorAccountConnectionPhase::Starting
        && flow.operation_token.as_deref() == Some(operation_token)
    {
        Ok(())
    } else {
        Err("Telegram connection operation was superseded or canceled".into())
    }
}
