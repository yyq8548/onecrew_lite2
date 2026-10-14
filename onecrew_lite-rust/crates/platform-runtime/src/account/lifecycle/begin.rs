use onecrew_local_domain::LocalProfile;
use onecrew_tg::{LoginCodeDelivery, QrLoginChallenge};
use std::time::Instant;

use crate::LocalPlatform;
use crate::account::browser::browser_failure_code;

use super::super::flow_state::{
    InAppConnectionSecret, LiveConnection, PendingOperatorAccountConnection, TERMINAL_FLOW_TTL,
    complete_flow, mark_flow_canceled,
};
use super::super::{
    OperatorAccountConnectionBeginInput, OperatorAccountConnectionIntent,
    OperatorAccountConnectionMethod, OperatorAccountConnectionPhase,
    OperatorAccountConnectionStatus, OperatorAccountPlatform, domain_platform,
    require_confirmation, validate_idempotency_key,
};

enum BeginPlan {
    BrowserNew {
        platform: OperatorAccountPlatform,
        profile_name: String,
        proxy: onecrew_proxy::ProxyConfig,
        x_chat_passcode: Option<String>,
    },
    BrowserReconnect {
        account_id: String,
    },
    TelegramNew {
        method: OperatorAccountConnectionMethod,
        api_id: i32,
        api_hash: String,
        phone: Option<String>,
        proxy: Option<onecrew_proxy::ProxyConfig>,
    },
    TelegramReconnect {
        account_id: String,
    },
}

enum BeginOutcome {
    Browser {
        token: String,
    },
    TelegramQr {
        token: String,
        challenge: QrLoginChallenge,
    },
    TelegramPhone {
        token: String,
        delivery: LoginCodeDelivery,
    },
    TelegramPassword {
        token: String,
    },
    Completed(LocalProfile),
}

fn begin_plan(flow: &PendingOperatorAccountConnection) -> Result<BeginPlan, String> {
    match (&flow.secret, flow.intent, flow.platform, flow.method) {
        (
            Some(InAppConnectionSecret::Browser {
                profile_name,
                proxy,
                x_chat_passcode,
            }),
            OperatorAccountConnectionIntent::Connect,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionMethod::Browser,
        ) => Ok(BeginPlan::BrowserNew {
            platform: flow.platform,
            profile_name: profile_name.clone(),
            proxy: proxy.clone(),
            x_chat_passcode: x_chat_passcode.clone(),
        }),
        (
            None,
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountPlatform::X | OperatorAccountPlatform::Instagram,
            OperatorAccountConnectionMethod::Browser,
        ) => Ok(BeginPlan::BrowserReconnect {
            account_id: flow
                .target_account_id
                .clone()
                .ok_or_else(|| "reconnect target is missing".to_string())?,
        }),
        (
            Some(InAppConnectionSecret::Telegram {
                api_id,
                api_hash,
                phone,
                proxy,
            }),
            _,
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionMethod::TelegramQr
            | OperatorAccountConnectionMethod::TelegramPhone,
        ) => Ok(BeginPlan::TelegramNew {
            method: flow.method,
            api_id: *api_id,
            api_hash: api_hash.clone(),
            phone: phone.clone(),
            proxy: proxy.clone(),
        }),
        (
            None,
            OperatorAccountConnectionIntent::Reconnect,
            OperatorAccountPlatform::Telegram,
            OperatorAccountConnectionMethod::ExistingAccount,
        ) => Ok(BeginPlan::TelegramReconnect {
            account_id: flow
                .target_account_id
                .clone()
                .ok_or_else(|| "reconnect target is missing".to_string())?,
        }),
        _ => Err("operator account connection is awaiting secret input in the desktop app".into()),
    }
}

fn begin_outcome_live(outcome: BeginOutcome) -> Option<LiveConnection> {
    match outcome {
        BeginOutcome::Browser { token } => Some(LiveConnection::Browser { token }),
        BeginOutcome::TelegramQr { token, .. } => Some(LiveConnection::TelegramQr { token }),
        BeginOutcome::TelegramPhone { token, .. } => Some(LiveConnection::TelegramPhone { token }),
        BeginOutcome::TelegramPassword { token } => {
            Some(LiveConnection::TelegramPassword { token })
        }
        BeginOutcome::Completed(_) => None,
    }
}

impl LocalPlatform {
    pub(super) async fn operator_account_connection_begin_internal(
        &self,
        input: OperatorAccountConnectionBeginInput,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        self.cleanup_connection_flows_async().await?;
        require_confirmation(input.confirmed)?;
        validate_idempotency_key(&input.idempotency_key)?;

        let current = self.operator_account_connection_snapshot(&input.flow_id)?;
        if current.method == OperatorAccountConnectionMethod::Browser
            && current.phase == OperatorAccountConnectionPhase::ReadyToBegin
        {
            self.require_browser_start_capability()?;
        }

        let plan = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(&input.flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if let Some(existing) = flow.begin_idempotency_key.as_deref() {
                if existing != input.idempotency_key {
                    return Err("connection begin already used a different idempotency_key".into());
                }
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            if flow.phase == OperatorAccountConnectionPhase::AwaitingSecretInApp {
                drop(flows);
                return self.operator_account_connection_snapshot(&input.flow_id);
            }
            if flow.phase != OperatorAccountConnectionPhase::ReadyToBegin {
                return Err("operator account connection is not ready to begin".into());
            }
            let plan = begin_plan(flow)?;
            flow.begin_idempotency_key = Some(input.idempotency_key.clone());
            flow.phase = OperatorAccountConnectionPhase::Starting;
            plan
        };

        let outcome = self.execute_begin_plan(plan).await;
        match outcome {
            Ok(outcome) => self.apply_begin_outcome(&input.flow_id, outcome).await,
            Err(error_code) => self.fail_connection(&input.flow_id, error_code).await,
        }
    }

    async fn execute_begin_plan(&self, plan: BeginPlan) -> Result<BeginOutcome, &'static str> {
        match plan {
            BeginPlan::BrowserNew {
                platform,
                profile_name,
                proxy,
                x_chat_passcode,
            } => {
                let probe = self
                    .proxy_validate(proxy.clone())
                    .await
                    .map_err(|_| "proxy_validation_failed")?;
                let created = self
                    .browser_create(domain_platform(platform), profile_name, proxy)
                    .map_err(|error| browser_failure_code(&error))?;
                if self
                    .browser_set_passcode(&created.token, x_chat_passcode)
                    .is_err()
                {
                    if let Err(cancel) = self.browser_cancel(&created.token) {
                        log::warn!("browser cancel after a configuration failure failed: {cancel}");
                    }
                    return Err("browser_configuration_failed");
                }
                if let Err(error) = self
                    .browser_open_login(&created.token, probe.timezone, probe.accept_language)
                    .await
                {
                    if let Err(cancel) = self.browser_cancel(&created.token) {
                        log::warn!("browser cancel after a failed login open failed: {cancel}");
                    }
                    return Err(browser_failure_code(&error));
                }
                Ok(BeginOutcome::Browser {
                    token: created.token,
                })
            }
            BeginPlan::BrowserReconnect { account_id } => self
                .browser_relogin(&account_id)
                .await
                .map(|created| BeginOutcome::Browser {
                    token: created.token,
                })
                .map_err(|error| browser_failure_code(&error)),
            BeginPlan::TelegramNew {
                method,
                api_id,
                api_hash,
                phone,
                proxy,
            } => {
                if let Err(error) = self.tg_close_credentials() {
                    log::warn!("closing the previous telegram credentials window failed: {error}");
                }
                let proxy = proxy
                    .filter(|proxy| {
                        onecrew_sessions::telegram_socks5_proxy_url(Some(proxy)).is_ok()
                    })
                    .ok_or("connection_begin_failed")?;
                // Never trust a previous validation call: prove the proxy is
                // still usable immediately before creating the MTProto client.
                self.proxy_validate(proxy.clone())
                    .await
                    .map_err(|_| "connection_begin_failed")?;
                match method {
                    OperatorAccountConnectionMethod::TelegramQr => {
                        let result = self
                            .tg_start_qr_login(api_id, api_hash, Some(proxy))
                            .await
                            .map_err(|_| "connection_begin_failed")?;
                        if let Some(profile) = result.profile {
                            Ok(BeginOutcome::Completed(profile))
                        } else if result.needs_password {
                            Ok(BeginOutcome::TelegramPassword {
                                token: result.token.ok_or("connection_begin_failed")?,
                            })
                        } else {
                            Ok(BeginOutcome::TelegramQr {
                                token: result.token.ok_or("connection_begin_failed")?,
                                challenge: result.challenge.ok_or("connection_begin_failed")?,
                            })
                        }
                    }
                    OperatorAccountConnectionMethod::TelegramPhone => {
                        let result = self
                            .tg_start_login(
                                api_id,
                                api_hash,
                                phone.ok_or("connection_begin_failed")?,
                                Some(proxy),
                            )
                            .await
                            .map_err(|_| "connection_begin_failed")?;
                        Ok(BeginOutcome::TelegramPhone {
                            token: result.token,
                            delivery: result.delivery,
                        })
                    }
                    _ => Err("connection_begin_failed"),
                }
            }
            BeginPlan::TelegramReconnect { account_id } => self
                .refresh_profile(&account_id)
                .await
                .map(BeginOutcome::Completed)
                .map_err(|_| "connection_begin_failed"),
        }
    }

    async fn apply_begin_outcome(
        &self,
        flow_id: &str,
        outcome: BeginOutcome,
    ) -> Result<OperatorAccountConnectionStatus, String> {
        let cleanup_live = {
            let mut flows = self
                .pending_account_connections
                .lock()
                .map_err(|_| "operator account connection lock poisoned".to_string())?;
            let flow = flows
                .get_mut(flow_id)
                .ok_or_else(|| "operator account connection flow not found".to_string())?;
            if flow.cancel_requested {
                let live = begin_outcome_live(outcome);
                mark_flow_canceled(flow);
                live
            } else {
                // The downstream browser/Telegram session now owns only what it needs;
                // do not retain a second plaintext copy in the public workflow map.
                flow.secret = None;
                match outcome {
                    BeginOutcome::Browser { token } => {
                        flow.live = Some(LiveConnection::Browser { token });
                        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
                    }
                    BeginOutcome::TelegramQr { token, challenge } => {
                        flow.live = Some(LiveConnection::TelegramQr { token });
                        flow.qr_challenge = Some(challenge);
                        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
                    }
                    BeginOutcome::TelegramPhone { token, delivery } => {
                        flow.live = Some(LiveConnection::TelegramPhone { token });
                        flow.code_delivery = Some(delivery);
                        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
                    }
                    BeginOutcome::TelegramPassword { token } => {
                        flow.live = Some(LiveConnection::TelegramPassword { token });
                        flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
                    }
                    BeginOutcome::Completed(profile) => {
                        let profile_id = profile.id.to_string();
                        if flow.intent == OperatorAccountConnectionIntent::Reconnect
                            && flow.target_account_id.as_deref() != Some(profile_id.as_str())
                        {
                            flow.live = None;
                            flow.qr_challenge = None;
                            flow.code_delivery = None;
                            flow.phase = OperatorAccountConnectionPhase::Failed;
                            flow.expires_at = Instant::now() + TERMINAL_FLOW_TTL;
                            flow.error_code = Some("reconnect_identity_mismatch".into());
                        } else {
                            complete_flow(flow, &profile);
                        }
                    }
                }
                None
            }
        };
        if let Some(live) = cleanup_live {
            self.cleanup_live_connection(live).await;
        }
        self.operator_account_connection_snapshot(flow_id)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::account::{
        OperatorAccountConnectionCancelInput, OperatorAccountConnectionPrepareInput,
        OperatorAccountConnectionStatusInput,
    };

    fn platform() -> (LocalPlatform, std::path::PathBuf) {
        let directory =
            std::env::temp_dir().join(format!("onecrew-account-begin-test-{}", Uuid::new_v4()));
        // 探测结果由测试决定,不继承跑测试那台机器装没装 Chrome。
        (
            LocalPlatform::open_with_browser_host(
                &directory,
                std::sync::Arc::new(crate::browser::FixedBrowserHost::ready()),
            )
            .unwrap(),
            directory,
        )
    }

    async fn prepare_browser_flow(platform: &LocalPlatform) -> String {
        platform
            .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
                platform: OperatorAccountPlatform::X,
                intent: OperatorAccountConnectionIntent::Connect,
                method: OperatorAccountConnectionMethod::Browser,
                operator_account_id: None,
                label: None,
            })
            .await
            .unwrap()
            .flow_id
    }

    #[tokio::test]
    async fn repeated_begin_is_read_only_and_starting_status_remains_queryable() {
        let (platform, directory) = platform();
        let flow_id = prepare_browser_flow(&platform).await;
        {
            let mut flows = platform.pending_account_connections.lock().unwrap();
            let flow = flows.get_mut(&flow_id).unwrap();
            flow.begin_idempotency_key = Some("begin-once".into());
            flow.phase = OperatorAccountConnectionPhase::Starting;
        }

        let replay = platform
            .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
                flow_id: flow_id.clone(),
                confirmed: true,
                idempotency_key: "begin-once".into(),
            })
            .await
            .unwrap();
        assert_eq!(replay.phase, OperatorAccountConnectionPhase::Starting);

        let status = platform
            .operator_account_connection_status(OperatorAccountConnectionStatusInput {
                flow_id: flow_id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(status.phase, OperatorAccountConnectionPhase::Starting);

        let error = platform
            .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
                flow_id,
                confirmed: true,
                idempotency_key: "different-key".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "connection begin already used a different idempotency_key"
        );

        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn begin_execution_failure_is_terminal_and_erases_attached_secret() {
        let (platform, directory) = platform();
        let flow_id = prepare_browser_flow(&platform).await;
        {
            let mut flows = platform.pending_account_connections.lock().unwrap();
            let flow = flows.get_mut(&flow_id).unwrap();
            flow.secret = Some(InAppConnectionSecret::Browser {
                profile_name: "operator".into(),
                proxy: onecrew_proxy::ProxyConfig {
                    kind: onecrew_proxy::ProxyKind::Http,
                    host: String::new(),
                    port: 0,
                    username: Some("must-be-erased".into()),
                    password: Some("must-be-erased".into()),
                },
                x_chat_passcode: Some("must-be-erased".into()),
            });
            flow.phase = OperatorAccountConnectionPhase::ReadyToBegin;
        }

        let failed = platform
            .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
                flow_id: flow_id.clone(),
                confirmed: true,
                idempotency_key: "failing-begin".into(),
            })
            .await
            .unwrap();
        assert_eq!(failed.phase, OperatorAccountConnectionPhase::Failed);
        assert_eq!(
            failed.error_code.as_deref(),
            Some("proxy_validation_failed")
        );
        let flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get(&flow_id).unwrap();
        assert!(flow.secret.is_none());
        assert!(flow.live.is_none());
        assert_eq!(flow.begin_idempotency_key.as_deref(), Some("failing-begin"));
        drop(flows);

        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn canceled_begin_discards_every_late_outcome_and_cannot_restart() {
        let (platform, directory) = platform();
        let flow_id = prepare_browser_flow(&platform).await;
        {
            let mut flows = platform.pending_account_connections.lock().unwrap();
            let flow = flows.get_mut(&flow_id).unwrap();
            flow.begin_idempotency_key = Some("in-flight-begin".into());
            flow.phase = OperatorAccountConnectionPhase::Starting;
        }

        let cancellation = platform
            .operator_account_connection_cancel(OperatorAccountConnectionCancelInput {
                flow_id: flow_id.clone(),
                confirmed: true,
            })
            .await
            .unwrap();
        assert_eq!(
            cancellation.error_code.as_deref(),
            Some("cancellation_requested")
        );

        for token in ["first-late-handle", "older-late-handle"] {
            let canceled = platform
                .apply_begin_outcome(
                    &flow_id,
                    BeginOutcome::Browser {
                        token: token.into(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(canceled.phase, OperatorAccountConnectionPhase::Canceled);
            assert!(canceled.error_code.is_none());
        }
        {
            let flows = platform.pending_account_connections.lock().unwrap();
            let flow = flows.get(&flow_id).unwrap();
            assert!(flow.cancel_requested);
            assert!(flow.secret.is_none());
            assert!(flow.live.is_none());
            assert!(flow.qr_challenge.is_none());
            assert!(flow.code_delivery.is_none());
        }

        let replay = platform
            .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
                flow_id: flow_id.clone(),
                confirmed: true,
                idempotency_key: "in-flight-begin".into(),
            })
            .await
            .unwrap();
        assert_eq!(replay.phase, OperatorAccountConnectionPhase::Canceled);

        let restart_error = platform
            .operator_account_connection_begin(OperatorAccountConnectionBeginInput {
                flow_id,
                confirmed: true,
                idempotency_key: "restart-key".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(
            restart_error,
            "connection begin already used a different idempotency_key"
        );

        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
