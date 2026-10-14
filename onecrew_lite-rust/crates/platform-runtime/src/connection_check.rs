//! Live connection self-check shared by the Web commands and the MCP
//! `*.connection.check` operations: can this computer, or the profile's
//! residential proxy, reach the provider, and does the provider still accept
//! the stored session or key?
//!
//! The typed [`ConnectionCheck`] is secret-free: it never carries proxy
//! credentials, keys, tokens, or provider error text. Provider, proxy, and
//! session material diagnostics decide the status and are then dropped.

use std::future::Future;
use std::time::{SystemTime, UNIX_EPOCH};

use onecrew_email::{EmailError, ProviderErrorClass};
use onecrew_ig::IgError;
use onecrew_local_domain::{Platform, ProfileSecret};
use onecrew_proxy::{EgressInfo, ProxyError, check_egress_ip};
use onecrew_sessions::{
    instagram_account_with_proxy, telegram_session_with_proxy, telegram_socks5_proxy_url,
    x_account_with_proxy,
};
use onecrew_tg::TgError;
use onecrew_x::{TransportError, XChatError, XClientError, XError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::credentials::services::{ResendCredentialTest, ResendCredentialTestError};
use crate::{
    LocalPlatform, OperatorAccountPlatform, YoutubePlatformTestError, YoutubePlatformTestInput,
    YoutubePlatformTestResult,
};

/// One live self-check of a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionCheck {
    pub status: ConnectionCheckStatus,
    /// Whether provider traffic for this connection routes through a configured proxy.
    pub proxy_configured: bool,
    /// The configured proxy's observed exit; present only when that proxy answered.
    pub egress: Option<ConnectionCheckEgress>,
    /// The signed-in account handle; present only when the provider accepted the session.
    pub account_handle: Option<String>,
    /// Unix seconds when the check finished.
    pub checked_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionCheckStatus {
    /// The network, the proxy when configured, and the provider all accepted the connection.
    Ok,
    /// The configured proxy did not answer or cannot carry this provider, or a provider that
    /// requires a proxy has none; the provider was not called.
    ProxyUnreachable,
    /// The provider could not be reached from this computer or through its proxy (no HTTP response).
    NetworkUnreachable,
    /// The provider rejected the stored session or key; reconnect the account or replace the key.
    CredentialRejected,
    /// The provider answered with a server error, a rate limit, or an unexpected response.
    ProviderUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConnectionCheckEgress {
    pub ip: String,
    /// Country reported by the IP-echo service; empty when unknown.
    pub country: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionCheckError {
    /// The selected connection does not exist for this toolkit or cannot be read locally.
    ConnectionUnavailable,
    /// The check could not finish locally, for example while recording credential health.
    Internal,
}

/// What the check's live probe of an operator account observed.
enum OperatorProbe {
    /// No proxy is configured. X and Instagram call the provider directly;
    /// Telegram, which requires its SOCKS5 proxy, stops before any call.
    Direct {
        account: Result<String, AccountFailure>,
    },
    /// The configured proxy did not answer; the provider was not called.
    ProxyUnreachable,
    /// The configured proxy answered; the provider was called through it.
    Proxied {
        egress: EgressInfo,
        account: Result<String, AccountFailure>,
    },
}

/// A failed account probe: only the status it decided survives.
struct AccountFailure {
    status: ConnectionCheckStatus,
}

impl AccountFailure {
    fn new(status: ConnectionCheckStatus) -> Self {
        Self { status }
    }
}

impl LocalPlatform {
    /// Live self-check of one X, Instagram, or Telegram operator account: the
    /// configured proxy's egress first, then who-am-i through the same route
    /// (directly for X and Instagram when no proxy is configured; never for
    /// Telegram). Reads the vault only.
    pub async fn operator_connection_check(
        &self,
        platform: OperatorAccountPlatform,
        operator_account_id: &str,
    ) -> Result<ConnectionCheck, ConnectionCheckError> {
        let id = Uuid::parse_str(operator_account_id)
            .map_err(|_| ConnectionCheckError::ConnectionUnavailable)?;
        let vault = self.vault.clone();
        let (profile, secret) = tokio::task::spawn_blocking(move || vault.get(id))
            .await
            .map_err(|_| ConnectionCheckError::Internal)?
            .map_err(|_| ConnectionCheckError::ConnectionUnavailable)?;
        if profile.platform != crate::account::domain_platform(platform) {
            return Err(ConnectionCheckError::ConnectionUnavailable);
        }
        Ok(project_operator_probe(
            probe_operator(profile.platform, &secret).await,
        ))
    }

    /// Live self-check of one YouTube credential. Reuses the credential test,
    /// which records credential health and reconciles its account reference.
    pub async fn youtube_connection_check(
        &self,
        credential_id: &str,
    ) -> Result<ConnectionCheck, ConnectionCheckError> {
        project_youtube_test(
            self.platform_youtube_test(YoutubePlatformTestInput {
                credential_id: credential_id.to_owned(),
            })
            .await,
        )
    }

    /// Probe Gmail through its own read path; other mailboxes retain their credential test.
    pub async fn mailbox_connection_check(
        &self,
        mailbox_ref_id: &str,
    ) -> Result<ConnectionCheck, ConnectionCheckError> {
        let probe = self.credential_services.gmail_connection_probe(
            &onecrew_creator_runtime::MailboxRefId::new(mailbox_ref_id.to_owned())
        ).await;
        let (status, account_handle) = match probe {
            Ok(None) => return project_resend_test(self.test_resend_credential_typed(mailbox_ref_id).await),
            Ok(Some(email)) => (ConnectionCheckStatus::Ok, Some(email)),
            Err(code) if code == "gmail_read_timeout" || code == "gmail_read_transport_failed" =>
                (ConnectionCheckStatus::NetworkUnreachable, None),
            Err(code) if code == "gmail_authorization_required" || code == "gmail_read_http_401" =>
                (ConnectionCheckStatus::CredentialRejected, None),
            Err(code) if code.starts_with("gmail_read_") =>
                (ConnectionCheckStatus::ProviderUnavailable, None),
            Err(_) => return Err(ConnectionCheckError::ConnectionUnavailable),
        };
        Ok(ConnectionCheck {
            status, account_handle, proxy_configured: false, egress: None,
            checked_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
        })
    }
}

async fn probe_operator(platform: Platform, secret: &ProfileSecret) -> OperatorProbe {
    probe_route(
        secret.proxy.as_ref().map(|proxy| proxy.to_url()),
        probe_egress,
        |route| account_probe(platform, secret, route),
    )
    .await
}

async fn probe_egress(proxy_url: String) -> Result<EgressInfo, ProxyError> {
    check_egress_ip(Some(&proxy_url)).await
}

async fn probe_through_proxy<E, EF, A, AF>(
    proxy_url: String,
    probe_egress: E,
    probe_account: A,
) -> Result<(EgressInfo, Result<String, AccountFailure>), ProxyError>
where
    E: FnOnce(String) -> EF,
    EF: Future<Output = Result<EgressInfo, ProxyError>>,
    A: FnOnce(Option<String>) -> AF,
    AF: Future<Output = Result<String, AccountFailure>>,
{
    let egress = probe_egress(proxy_url.clone()).await?;
    Ok((egress, probe_account(Some(proxy_url)).await))
}

/// A configured proxy must answer before the provider is called; without one
/// the provider is called directly.
async fn probe_route<E, EF, A, AF>(
    proxy_url: Option<String>,
    probe_egress: E,
    probe_account: A,
) -> OperatorProbe
where
    E: FnOnce(String) -> EF,
    EF: Future<Output = Result<EgressInfo, ProxyError>>,
    A: FnOnce(Option<String>) -> AF,
    AF: Future<Output = Result<String, AccountFailure>>,
{
    match proxy_url {
        None => OperatorProbe::Direct {
            account: probe_account(None).await,
        },
        Some(url) => match probe_through_proxy(url, probe_egress, probe_account).await {
            Ok((egress, account)) => OperatorProbe::Proxied { egress, account },
            Err(_) => OperatorProbe::ProxyUnreachable,
        },
    }
}

async fn account_probe(
    platform: Platform,
    secret: &ProfileSecret,
    proxy_url: Option<String>,
) -> Result<String, AccountFailure> {
    let rejected = |_| AccountFailure::new(ConnectionCheckStatus::CredentialRejected);
    match platform {
        Platform::X => {
            let account =
                x_account_with_proxy(&secret.cookies, proxy_url.as_deref()).map_err(rejected)?;
            let me = account
                .who_am_i()
                .await
                .map_err(|error| AccountFailure::new(x_failure_status(&error)))?;
            Ok(me.username)
        }
        Platform::Instagram => {
            let account = instagram_account_with_proxy(
                secret.instagram_session_settings.as_deref(),
                proxy_url.as_deref(),
            )
            .map_err(rejected)?;
            let me = account
                .who_am_i()
                .await
                .map_err(|error| AccountFailure::new(instagram_failure_status(&error)))?;
            Ok(me.username)
        }
        Platform::Telegram => {
            // Telegram always egresses through the profile's SOCKS5 proxy
            // (grammers is SOCKS5-only); a missing or http/https proxy stops
            // here, before the stored session is examined and before any
            // network call, with no direct-connect fallback.
            telegram_socks5_proxy_url(secret.proxy.as_ref())
                .map_err(|_| AccountFailure::new(ConnectionCheckStatus::ProxyUnreachable))?;
            // The egress already passed its rule, so what remains is missing
            // or empty session material.
            let prepared = telegram_session_with_proxy(
                secret.telegram_api_id,
                secret.telegram_api_hash.as_deref(),
                secret.telegram_session.as_deref(),
                secret.proxy.as_ref(),
            )
            .map_err(rejected)?;
            let telegram_failure =
                |error: TgError| AccountFailure::new(telegram_failure_status(&error));
            let account = prepared.connect().await.map_err(telegram_failure)?;
            let me = account.who_am_i().await;
            account.close().await;
            let me = me.map_err(telegram_failure)?;
            Ok(me.username.unwrap_or(me.id))
        }
    }
}

/// Without a response X was unreachable; a missing credential or an HTTP 401
/// or 403 rejects the stored session; every other failure, including a local
/// client or chat failure, leaves X unavailable for the check.
fn x_failure_status(error: &XError) -> ConnectionCheckStatus {
    let transport = match error {
        XError::Transport(transport)
        | XError::Client(XClientError::Transport(transport))
        | XError::Chat(
            XChatError::Transport(transport)
            | XChatError::Client(XClientError::Transport(transport)),
        ) => transport,
        XError::Client(_) | XError::Chat(_) => return ConnectionCheckStatus::ProviderUnavailable,
    };
    match transport {
        TransportError::Network(_) => ConnectionCheckStatus::NetworkUnreachable,
        TransportError::MissingCredential
        | TransportError::Http {
            status: 401 | 403, ..
        } => ConnectionCheckStatus::CredentialRejected,
        TransportError::InvalidHeader
        | TransportError::Http { .. }
        | TransportError::Api(_)
        | TransportError::ApiResponse { .. } => ConnectionCheckStatus::ProviderUnavailable,
    }
}

fn instagram_failure_status(error: &IgError) -> ConnectionCheckStatus {
    match error {
        IgError::Network(_) => ConnectionCheckStatus::NetworkUnreachable,
        IgError::Session(_) | IgError::Challenge(_) => ConnectionCheckStatus::CredentialRejected,
        IgError::Api(_) | IgError::InvalidHeader | IgError::Response { .. } => {
            ConnectionCheckStatus::ProviderUnavailable
        }
    }
}

fn telegram_failure_status(error: &TgError) -> ConnectionCheckStatus {
    match error {
        TgError::Network(_) => ConnectionCheckStatus::NetworkUnreachable,
        TgError::Session(_) | TgError::Auth(_) => ConnectionCheckStatus::CredentialRejected,
        TgError::Permission(_) | TgError::Other(_) => ConnectionCheckStatus::ProviderUnavailable,
        TgError::Rpc { code: 401, .. } => ConnectionCheckStatus::CredentialRejected,
        TgError::Rpc { .. } => ConnectionCheckStatus::ProviderUnavailable,
    }
}

fn project_operator_probe(probe: OperatorProbe) -> ConnectionCheck {
    let (proxy_configured, egress, account) = match probe {
        OperatorProbe::ProxyUnreachable => {
            return ConnectionCheck {
                status: ConnectionCheckStatus::ProxyUnreachable,
                proxy_configured: true,
                egress: None,
                account_handle: None,
                checked_at: now(),
            };
        }
        OperatorProbe::Direct { account } => (false, None, account),
        OperatorProbe::Proxied { egress, account } => (
            true,
            Some(ConnectionCheckEgress {
                ip: egress.ip,
                country: egress.country,
            }),
            account,
        ),
    };
    let (status, account_handle) = match account {
        Ok(handle) => (ConnectionCheckStatus::Ok, Some(handle)),
        Err(failure) => (failure.status, None),
    };
    ConnectionCheck {
        status,
        proxy_configured,
        egress,
        account_handle,
        checked_at: now(),
    }
}

fn project_youtube_test(
    result: Result<YoutubePlatformTestResult, YoutubePlatformTestError>,
) -> Result<ConnectionCheck, ConnectionCheckError> {
    let status = match result {
        Ok(test) if test.ok => ConnectionCheckStatus::Ok,
        Ok(_) => ConnectionCheckStatus::CredentialRejected,
        Err(YoutubePlatformTestError::NetworkUnreachable) => {
            ConnectionCheckStatus::NetworkUnreachable
        }
        Err(
            YoutubePlatformTestError::RateLimited
            | YoutubePlatformTestError::Transient
            | YoutubePlatformTestError::AmbiguousForbidden,
        ) => ConnectionCheckStatus::ProviderUnavailable,
        Err(
            YoutubePlatformTestError::InvalidCredentialId
            | YoutubePlatformTestError::CredentialUnavailable,
        ) => return Err(ConnectionCheckError::ConnectionUnavailable),
        Err(YoutubePlatformTestError::Infrastructure) => {
            return Err(ConnectionCheckError::Internal);
        }
    };
    Ok(service_check(status))
}

fn project_resend_test(
    result: Result<ResendCredentialTest, ResendCredentialTestError>,
) -> Result<ConnectionCheck, ConnectionCheckError> {
    let status = match result {
        Ok(_) => ConnectionCheckStatus::Ok,
        Err(ResendCredentialTestError::MailboxUnavailable(_)) => {
            return Err(ConnectionCheckError::ConnectionUnavailable);
        }
        Err(ResendCredentialTestError::Local(_)) => return Err(ConnectionCheckError::Internal),
        Err(ResendCredentialTestError::Provider(error)) => resend_failure_status(&error),
    };
    Ok(service_check(status))
}

fn resend_failure_status(error: &EmailError) -> ConnectionCheckStatus {
    match error {
        EmailError::Transport(_) => ConnectionCheckStatus::NetworkUnreachable,
        EmailError::Provider(failure) if failure.class == ProviderErrorClass::Auth => {
            ConnectionCheckStatus::CredentialRejected
        }
        // An unverified or unconfigured sender domain is fixed on the key's
        // Resend account, like a rejected key.
        EmailError::InvalidConfig(_) => ConnectionCheckStatus::CredentialRejected,
        EmailError::Provider(_)
        | EmailError::OversizedResponse { .. }
        | EmailError::InvalidMessage(_)
        | EmailError::UnsupportedCapability(_) => ConnectionCheckStatus::ProviderUnavailable,
    }
}

/// Service credentials call their provider directly; no proxy is configured.
fn service_check(status: ConnectionCheckStatus) -> ConnectionCheck {
    ConnectionCheck {
        status,
        proxy_configured: false,
        egress: None,
        account_handle: None,
        checked_at: now(),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests;
