use sha2::{Digest, Sha256};

use crate::{LocalPlatform, ProxyConfig};

use super::flow_state::{
    DirectRequestReservation, InAppConnectionSecret, PendingOperatorAccountConnection,
    hash_fingerprint_field, hash_optional_fingerprint_field, hash_proxy_fingerprint,
};
use super::{
    AccountConnectionOperationInput, BrowserAccountConnectionCompleteInput,
    BrowserAccountConnectionStartInput, BrowserAccountConnectionStatus,
    DirectAccountConnectionCancelInput, DirectAccountConnectionPhase,
    OperatorAccountConnectionBeginInput, OperatorAccountConnectionCancelInput,
    OperatorAccountConnectionFinalizeInput, OperatorAccountConnectionIntent,
    OperatorAccountConnectionMethod, OperatorAccountConnectionPhase,
    OperatorAccountConnectionPrepareInput, OperatorAccountConnectionStatus,
    OperatorAccountConnectionStatusInput, OperatorAccountPlatform, XChatConnectionPreference,
    validate_idempotency_key,
};

impl LocalPlatform {
    /// Start a complete X/Instagram login operation. The proxy is actively
    /// probed before any flow/browser state is created, and the lower-level
    /// begin path probes it again immediately before opening Chrome.
    pub async fn browser_account_connection_start(
        &self,
        input: BrowserAccountConnectionStartInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        if !matches!(
            input.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) {
            return Err("browser account connection only supports X or Instagram".into());
        }
        validate_x_chat_connection_input(
            input.platform,
            input.x_chat_preference,
            input.x_chat_passcode.as_deref(),
        )?;
        validate_idempotency_key(&input.idempotency_key)?;
        let profile_name = input.profile_name.trim();
        if profile_name.is_empty() || profile_name.len() > 120 {
            return Err("profile_name must contain 1 to 120 characters".into());
        }
        let fingerprint = browser_connection_request_fingerprint(
            input.platform,
            profile_name,
            &input.proxy,
            input.x_chat_preference,
            input
                .x_chat_passcode
                .as_deref()
                .filter(|value| !value.trim().is_empty()),
        );
        self.cleanup_connection_flows_async().await?;
        if let Some(operation_id) =
            self.direct_operation_for_request_key(&input.idempotency_key, &fingerprint)?
        {
            return self
                .browser_account_connection_status(AccountConnectionOperationInput { operation_id })
                .await;
        }
        // Capability is an end-to-end precondition, not an executable-only UI
        // hint. Reject before proxy probing or allocating any connection state.
        self.require_browser_start_capability()?;
        // A failed probe must not leave an operation, Chrome profile, or vault
        // record behind.
        self.proxy_validate(input.proxy.clone().into()).await?;
        let (prepared, created) = self
            .operator_account_connection_prepare_internal(
                OperatorAccountConnectionPrepareInput {
                    platform: input.platform,
                    intent: OperatorAccountConnectionIntent::Connect,
                    method: OperatorAccountConnectionMethod::Browser,
                    operator_account_id: None,
                    label: Some(profile_name.to_string()),
                },
                Some(DirectRequestReservation {
                    idempotency_key: input.idempotency_key.clone(),
                    fingerprint,
                }),
            )
            .await?;
        if !created {
            return Ok(self.project_browser_connection(prepared));
        }
        if self
            .operator_account_connection_configure_browser_in_app(
                &prepared.flow_id,
                profile_name.to_string(),
                input.proxy,
                input.x_chat_preference,
                input.x_chat_passcode,
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
            return Err("browser account connection configuration failed".into());
        }
        self.operator_account_connection_begin(OperatorAccountConnectionBeginInput {
            flow_id: prepared.flow_id.clone(),
            confirmed: true,
            idempotency_key: input.idempotency_key,
        })
        .await?;
        self.browser_account_connection_status(AccountConnectionOperationInput {
            operation_id: prepared.flow_id,
        })
        .await
    }

    pub async fn browser_account_connection_status(
        &self,
        input: AccountConnectionOperationInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        let current = self.operator_account_connection_snapshot(&input.operation_id)?;
        if !matches!(
            current.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) || current.method != OperatorAccountConnectionMethod::Browser
        {
            return Err("operation is not an X or Instagram browser connection".into());
        }
        let status = self
            .operator_account_connection_status(OperatorAccountConnectionStatusInput {
                flow_id: input.operation_id,
            })
            .await?;
        Ok(self.project_browser_connection(status))
    }

    /// Complete browser login after the user confirms the browser interaction
    /// is done. Extraction, remote identity verification, vault persistence and
    /// browser cleanup remain in the canonical finalize path.
    pub async fn browser_account_connection_complete(
        &self,
        input: BrowserAccountConnectionCompleteInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        if !input.login_completed {
            return Err("login_completed=true is required before extraction".into());
        }
        let current = self.operator_account_connection_snapshot(&input.operation_id)?;
        if !matches!(
            current.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) || current.method != OperatorAccountConnectionMethod::Browser
        {
            return Err("operation is not an X or Instagram browser connection".into());
        }
        let status = self
            .operator_account_connection_finalize(OperatorAccountConnectionFinalizeInput {
                flow_id: input.operation_id,
                confirmed: true,
                idempotency_key: input.idempotency_key,
            })
            .await?;
        if !matches!(
            status.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) || status.method != OperatorAccountConnectionMethod::Browser
        {
            return Err("operation is not an X or Instagram browser connection".into());
        }
        Ok(self.project_browser_connection(status))
    }

    pub async fn browser_account_connection_cancel(
        &self,
        input: DirectAccountConnectionCancelInput,
    ) -> Result<BrowserAccountConnectionStatus, String> {
        let current = self.operator_account_connection_snapshot(&input.operation_id)?;
        if !matches!(
            current.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) || current.method != OperatorAccountConnectionMethod::Browser
        {
            return Err("operation is not an X or Instagram browser connection".into());
        }
        let status = self
            .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
                flow_id: input.operation_id,
                confirmed: input.confirmed,
            })
            .await?;
        if !matches!(
            status.platform,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
        ) || status.method != OperatorAccountConnectionMethod::Browser
        {
            return Err("operation is not an X or Instagram browser connection".into());
        }
        Ok(self.project_browser_connection(status))
    }

    /// Desktop-only: attach browser/proxy credentials to a prepared X/IG flow.
    /// These values remain in private runtime memory until begin/finalize.
    pub async fn operator_account_connection_configure_browser_in_app(
        &self,
        flow_id: &str,
        profile_name: String,
        proxy: ProxyConfig,
        x_chat_preference: XChatConnectionPreference,
        x_chat_passcode: Option<String>,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        // Validate the caller-controlled X Chat contract before cleanup can
        // cancel an unrelated expired browser or touch any other live state.
        let platform = {
            let flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            validate_browser_configuration_flow(flow)?;
            flow.platform
        };
        validate_x_chat_connection_input(platform, x_chat_preference, x_chat_passcode.as_deref())?;
        self.cleanup_connection_flows_async().await?;
        let mut flows = self
            .pending_account_connections
            .lock()
            .map_err(|_| "operator account connection lock poisoned".to_string())?;
        let flow = flows
            .get_mut(flow_id)
            .ok_or_else(|| "operator account connection flow not found".to_string())?;
        validate_browser_configuration_flow(flow)?;
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
        let profile_name = if profile_name.trim().is_empty() {
            flow.label
                .clone()
                .unwrap_or_else(|| format!("{:?}", flow.platform))
        } else {
            profile_name.trim().to_string()
        };
        flow.secret = Some(InAppConnectionSecret::Browser {
            profile_name,
            proxy: proxy.into(),
            x_chat_passcode: match x_chat_preference {
                XChatConnectionPreference::Enabled => x_chat_passcode,
                XChatConnectionPreference::Disabled => None,
            },
        });
        flow.phase = OperatorAccountConnectionPhase::ReadyToBegin;
        flow.error_code = None;
        drop(flows);
        self.operator_account_connection_snapshot(flow_id)
    }
    fn project_browser_connection(
        &self,
        status: OperatorAccountConnectionStatus,
    ) -> BrowserAccountConnectionStatus {
        let phase = match status.phase {
            OperatorAccountConnectionPhase::AwaitingUserInApp => {
                DirectAccountConnectionPhase::AwaitingBrowserLogin
            }
            OperatorAccountConnectionPhase::Completed => DirectAccountConnectionPhase::Completed,
            OperatorAccountConnectionPhase::Canceled => DirectAccountConnectionPhase::Canceled,
            OperatorAccountConnectionPhase::Failed => DirectAccountConnectionPhase::Failed,
            OperatorAccountConnectionPhase::AwaitingSecretInApp
            | OperatorAccountConnectionPhase::ReadyToBegin
            | OperatorAccountConnectionPhase::Starting
            | OperatorAccountConnectionPhase::Finalizing => DirectAccountConnectionPhase::Starting,
        };
        BrowserAccountConnectionStatus {
            operation_id: status.flow_id,
            platform: status.platform,
            phase,
            expires_at: status.expires_at,
            browser_capability: self.browser_status(),
            operator_account: status.operator_account,
            error_code: status.error_code,
        }
    }
}

fn validate_x_chat_connection_input(
    platform: OperatorAccountPlatform,
    preference: XChatConnectionPreference,
    passcode: Option<&str>,
) -> Result<(), String> {
    match (platform, preference, passcode) {
        (OperatorAccountPlatform::X, XChatConnectionPreference::Enabled, Some(value))
            if !value.trim().is_empty() =>
        {
            Ok(())
        }
        (OperatorAccountPlatform::X, XChatConnectionPreference::Enabled, _) => {
            Err("X with x_chat_preference=enabled requires a non-empty x_chat_passcode".into())
        }
        (OperatorAccountPlatform::X, XChatConnectionPreference::Disabled, None) => Ok(()),
        (OperatorAccountPlatform::X, XChatConnectionPreference::Disabled, Some(_)) => {
            Err("X with x_chat_preference=disabled does not accept x_chat_passcode".into())
        }
        (OperatorAccountPlatform::Instagram, XChatConnectionPreference::Disabled, None) => Ok(()),
        (OperatorAccountPlatform::Instagram, _, _) => Err(
            "Instagram requires x_chat_preference=disabled and does not accept x_chat_passcode"
                .into(),
        ),
        (OperatorAccountPlatform::Telegram, _, _) => {
            Err("browser account connection only supports X or Instagram".into())
        }
    }
}

fn validate_browser_configuration_flow(
    flow: &PendingOperatorAccountConnection,
) -> Result<(), String> {
    if matches!(
        flow.platform,
        OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram
    ) && flow.method == OperatorAccountConnectionMethod::Browser
        && flow.intent == OperatorAccountConnectionIntent::Connect
    {
        Ok(())
    } else {
        Err("browser configuration does not match this flow".into())
    }
}

pub(super) fn browser_connection_request_fingerprint(
    platform: OperatorAccountPlatform,
    profile_name: &str,
    proxy: &ProxyConfig,
    x_chat_preference: XChatConnectionPreference,
    x_chat_passcode: Option<&str>,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    hash_fingerprint_field(&mut digest, b"direct-browser-account-v1");
    hash_fingerprint_field(
        &mut digest,
        match platform {
            OperatorAccountPlatform::X => b"x",
            OperatorAccountPlatform::Instagram => b"instagram",
            OperatorAccountPlatform::Telegram => b"telegram",
        },
    );
    hash_fingerprint_field(&mut digest, profile_name.as_bytes());
    hash_proxy_fingerprint(&mut digest, proxy);
    hash_fingerprint_field(
        &mut digest,
        match x_chat_preference {
            XChatConnectionPreference::Enabled => b"enabled",
            XChatConnectionPreference::Disabled => b"disabled",
        },
    );
    hash_optional_fingerprint_field(&mut digest, x_chat_passcode.map(str::as_bytes));
    digest.finalize().into()
}

/// Convert implementation detail into a stable adapter-safe failure code while
/// retaining the material root cause instead of collapsing every browser
/// failure into `connection_*_failed`.
pub(super) fn browser_failure_code(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("not detected") || lower.contains("未检测到 chrome") {
        "browser_not_installed"
    } else if lower.contains("cookie store was not found") {
        "browser_cookie_store_not_found"
    } else if lower.contains("cookie store is still locked") {
        "browser_cookie_store_locked"
    } else if lower.contains("local state was not found") {
        "browser_cookie_local_state_not_found"
    } else if lower.contains("no usable dpapi") || lower.contains("dpapi could not decrypt") {
        "browser_cookie_dpapi_failed"
    } else if lower.contains("app-bound (v20)") {
        "browser_cookie_app_bound_unsupported"
    } else if lower.contains("cookie ciphertext") || lower.contains("host integrity check") {
        "browser_cookie_decryption_failed"
    } else if lower.contains("keychain") {
        "browser_cookie_authorization_failed"
    } else if lower.contains("cookies db read failed") {
        "browser_cookie_store_read_failed"
    } else if lower.contains("graceful chrome shutdown")
        || lower.contains("graceful shutdown timeout")
    {
        "browser_close_failed"
    } else if lower.contains("启动 chrome") || lower.contains("launch") {
        "browser_launch_failed"
    } else if lower.contains("本地代理") || lower.contains("proxy") || lower.contains("家宽")
    {
        "proxy_validation_failed"
    } else {
        "browser_connection_failed"
    }
}

#[cfg(test)]
mod failure_code_tests {
    use super::browser_failure_code;

    #[test]
    fn preserves_material_windows_cookie_failure_categories() {
        assert_eq!(
            browser_failure_code(
                "读取 Cookie 失败:Chrome cookie store is still locked after browser shutdown: C:/profile/Cookies"
            ),
            "browser_cookie_store_locked"
        );
        assert_eq!(
            browser_failure_code("Chrome App-Bound (v20) cookies are not supported"),
            "browser_cookie_app_bound_unsupported"
        );
        assert_eq!(
            browser_failure_code("Windows DPAPI could not decrypt the app-owned Chrome cookie key"),
            "browser_cookie_dpapi_failed"
        );
    }
}
