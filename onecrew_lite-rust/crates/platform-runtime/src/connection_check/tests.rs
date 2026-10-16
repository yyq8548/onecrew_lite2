use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

const PROXY_SECRET: &str = "PROXY_PASSWORD_SENTINEL";

fn egress() -> EgressInfo {
    EgressInfo {
        ip: "203.0.113.7".into(),
        country: "TR".into(),
        org: "Residential ISP".into(),
        timezone: "Europe/Istanbul".into(),
    }
}

fn proxy_url() -> Option<String> {
    Some(format!(
        "socks5://operator:{PROXY_SECRET}@198.51.100.2:1080"
    ))
}

fn secret(proxy: Option<onecrew_proxy::ProxyConfig>) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: "provider-account".into(),
        cookies: Vec::new(),
        x_chat_passcode: None,
        instagram_session_settings: None,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_session: None,
        profile_name: "connection check fixture".into(),
        browser_profile_dir: String::new(),
        fingerprint_seed: String::new(),
        proxy,
        proxy_egress: None,
    }
}

fn socks5_proxy() -> onecrew_proxy::ProxyConfig {
    onecrew_proxy::ProxyConfig {
        kind: onecrew_proxy::ProxyKind::Socks5,
        host: "198.51.100.2".into(),
        port: 1080,
        username: Some("operator".into()),
        password: Some(PROXY_SECRET.into()),
    }
}

fn http_proxy() -> onecrew_proxy::ProxyConfig {
    onecrew_proxy::ProxyConfig {
        kind: onecrew_proxy::ProxyKind::Http,
        host: "198.51.100.2".into(),
        port: 8080,
        username: Some("operator".into()),
        password: Some(PROXY_SECRET.into()),
    }
}

fn assert_secret_free(check: &ConnectionCheck) {
    let wire = serde_json::to_string(check).unwrap();
    for forbidden in [
        PROXY_SECRET,
        "198.51.100.2",
        "operator:",
        "refused",
        "cookie",
    ] {
        assert!(!wire.contains(forbidden), "{forbidden} leaked into {wire}");
    }
}

#[tokio::test]
async fn a_reachable_proxy_and_accepted_session_report_ok_with_the_egress() {
    let check = project_operator_probe(
        probe_route(
            proxy_url(),
            |_| async { Ok(egress()) },
            |route| async move {
                assert_eq!(route, proxy_url(), "the account probe uses the proxy route");
                Ok("riverfjs".to_owned())
            },
        )
        .await,
    );
    assert_eq!(check.status, ConnectionCheckStatus::Ok);
    assert!(check.proxy_configured);
    assert_eq!(
        check.egress,
        Some(ConnectionCheckEgress {
            ip: "203.0.113.7".into(),
            country: "TR".into(),
        })
    );
    assert_eq!(check.account_handle.as_deref(), Some("riverfjs"));
    assert!(check.checked_at > 0);
    assert_secret_free(&check);
}

#[tokio::test]
async fn a_down_proxy_stops_before_the_provider_is_called() {
    let provider_called = AtomicBool::new(false);
    let check = project_operator_probe(
        probe_route(
            proxy_url(),
            |url| async move {
                Err(ProxyError::Unreachable(format!(
                    "{url} → connection refused"
                )))
            },
            |_| async {
                provider_called.store(true, Ordering::SeqCst);
                Ok("unreachable".to_owned())
            },
        )
        .await,
    );
    assert!(!provider_called.load(Ordering::SeqCst));
    assert_eq!(check.status, ConnectionCheckStatus::ProxyUnreachable);
    assert!(check.proxy_configured);
    assert_eq!(check.egress, None);
    assert_eq!(check.account_handle, None);
    assert_secret_free(&check);
}

#[tokio::test]
async fn without_a_proxy_the_provider_is_called_directly_and_network_failure_is_reported() {
    let egress_called = AtomicBool::new(false);
    let check = project_operator_probe(
        probe_route(
            None,
            |_| async {
                egress_called.store(true, Ordering::SeqCst);
                Ok(egress())
            },
            |route| async move {
                assert_eq!(route, None, "the account probe is direct");
                Err(AccountFailure::new(
                    ConnectionCheckStatus::NetworkUnreachable,
                ))
            },
        )
        .await,
    );
    assert!(!egress_called.load(Ordering::SeqCst));
    assert_eq!(check.status, ConnectionCheckStatus::NetworkUnreachable);
    assert!(!check.proxy_configured);
    assert_eq!(check.egress, None);
    assert_eq!(check.account_handle, None);
    assert_secret_free(&check);
}

#[tokio::test]
async fn a_rejected_session_behind_a_working_proxy_keeps_the_egress() {
    let check = project_operator_probe(
        probe_route(
            proxy_url(),
            |_| async { Ok(egress()) },
            |_| async {
                Err(AccountFailure::new(
                    ConnectionCheckStatus::CredentialRejected,
                ))
            },
        )
        .await,
    );
    assert_eq!(check.status, ConnectionCheckStatus::CredentialRejected);
    assert!(check.proxy_configured);
    assert!(check.egress.is_some());
    assert_eq!(check.account_handle, None);
    assert_secret_free(&check);
}

#[tokio::test]
async fn missing_local_sessions_are_rejected_per_kind_without_network() {
    for (platform, proxy) in [
        (Platform::X, None),
        (Platform::Instagram, None),
        (Platform::Telegram, Some(socks5_proxy())),
    ] {
        let failure = account_probe(platform, &secret(proxy), None)
            .await
            .unwrap_err();
        assert_eq!(
            failure.status,
            ConnectionCheckStatus::CredentialRejected,
            "{platform:?}"
        );
    }

    let telegram_over_http = account_probe(Platform::Telegram, &secret(Some(http_proxy())), None)
        .await
        .unwrap_err();
    assert_eq!(
        telegram_over_http.status,
        ConnectionCheckStatus::ProxyUnreachable
    );
}

#[tokio::test]
async fn a_telegram_profile_without_its_socks5_proxy_never_reaches_telegram() {
    // A complete stored session: only the SOCKS5 rule keeps it from dialing
    // Telegram directly from this computer.
    let mut stored = secret(None);
    stored.telegram_api_id = Some(12_345);
    stored.telegram_api_hash = Some("api-hash".into());
    stored.telegram_session = Some("{}".into());
    let failure = account_probe(Platform::Telegram, &stored, None)
        .await
        .unwrap_err();
    assert_eq!(failure.status, ConnectionCheckStatus::ProxyUnreachable);

    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    let profile = platform
        .vault
        .upsert(Platform::Telegram, "legacy-telegram", &stored)
        .unwrap();
    let check = platform
        .operator_connection_check(OperatorAccountPlatform::Telegram, &profile.id.to_string())
        .await
        .unwrap();
    assert_eq!(check.status, ConnectionCheckStatus::ProxyUnreachable);
    assert!(!check.proxy_configured);
    assert_eq!(check.egress, None);
    assert_eq!(check.account_handle, None);
    assert_secret_free(&check);
}

#[test]
fn provider_failures_split_network_credential_and_provider_per_kind() {
    let http = |status, code| TransportError::Http {
        status,
        code,
        message: format!("provider body via {}", proxy_url().unwrap()),
    };
    let network = || TransportError::Network(format!("connect via {}", proxy_url().unwrap()));
    for (error, expected) in [
        (
            XError::Transport(network()),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            XError::Client(XClientError::Transport(network())),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            XError::Chat(XChatError::Client(XClientError::Transport(network()))),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            XError::Transport(http(503, None)),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Transport(http(401, Some(32))),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            XError::Client(XClientError::Transport(http(403, None))),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            XError::Chat(XChatError::Transport(http(401, None))),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            XError::Transport(TransportError::MissingCredential),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            XError::Transport(http(429, Some(88))),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Transport(http(404, None)),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Transport(TransportError::InvalidHeader),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Transport(TransportError::ApiResponse {
                status: 401,
                code: Some(32),
            }),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Client(XClientError::Response("unexpected".into())),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Client(XClientError::Input("rejected".into())),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            XError::Chat(XChatError::Protocol("unexpected".into())),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
    ] {
        assert_eq!(x_failure_status(&error), expected, "{error:?}");
    }

    for (error, expected) in [
        (
            IgError::Network("connect".into()),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            IgError::Session("expired".into()),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            IgError::Challenge("checkpoint".into()),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            IgError::Api("HTTP 500".into()),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
    ] {
        assert_eq!(instagram_failure_status(&error), expected, "{error:?}");
    }

    for (error, expected) in [
        (
            TgError::Network("dc unreachable".into()),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            TgError::Session("invalid".into()),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            TgError::Auth("unauthorized".into()),
            ConnectionCheckStatus::CredentialRejected,
        ),
        (
            TgError::Other("flood".into()),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
    ] {
        assert_eq!(telegram_failure_status(&error), expected, "{error:?}");
    }
}

#[test]
fn youtube_test_outcomes_project_to_the_typed_check() {
    let tested = |ok| YoutubePlatformTestResult {
        credential_id: "youtube-1".into(),
        ok,
        checked_at: 1,
        channel_id: None,
        issue: None,
        message: "provider message".into(),
    };
    for (result, expected) in [
        (Ok(tested(true)), ConnectionCheckStatus::Ok),
        (Ok(tested(false)), ConnectionCheckStatus::CredentialRejected),
        (
            Err(YoutubePlatformTestError::NetworkUnreachable),
            ConnectionCheckStatus::NetworkUnreachable,
        ),
        (
            Err(YoutubePlatformTestError::Transient),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
        (
            Err(YoutubePlatformTestError::RateLimited),
            ConnectionCheckStatus::ProviderUnavailable,
        ),
    ] {
        let check = project_youtube_test(result).unwrap();
        assert_eq!(check.status, expected);
        assert!(!check.proxy_configured);
        assert_eq!(check.egress, None);
        assert_eq!(check.account_handle, None);
        assert!(
            !serde_json::to_string(&check)
                .unwrap()
                .contains("provider message")
        );
    }
    assert_eq!(
        project_youtube_test(Err(YoutubePlatformTestError::CredentialUnavailable)),
        Err(ConnectionCheckError::ConnectionUnavailable)
    );
    assert_eq!(
        project_youtube_test(Err(YoutubePlatformTestError::Infrastructure)),
        Err(ConnectionCheckError::Internal)
    );
}

#[tokio::test]
async fn the_owner_check_resolves_the_exact_local_connection_before_any_network() {
    let directory = tempfile::tempdir().unwrap();
    let platform = LocalPlatform::open(directory.path()).unwrap();
    for id in ["not-a-uuid".to_owned(), Uuid::new_v4().to_string()] {
        assert_eq!(
            platform
                .operator_connection_check(OperatorAccountPlatform::X, &id)
                .await,
            Err(ConnectionCheckError::ConnectionUnavailable)
        );
    }

    let profile = platform
        .vault
        .upsert(Platform::X, "riverfjs", &secret(None))
        .unwrap();
    let profile_id = profile.id.to_string();
    assert_eq!(
        platform
            .operator_connection_check(OperatorAccountPlatform::Instagram, &profile_id)
            .await,
        Err(ConnectionCheckError::ConnectionUnavailable),
        "a profile of another platform is not selectable"
    );

    // No proxy and no stored cookies: the direct path fails closed locally.
    let check = platform
        .operator_connection_check(OperatorAccountPlatform::X, &profile_id)
        .await
        .unwrap();
    assert_eq!(check.status, ConnectionCheckStatus::CredentialRejected);
    assert!(!check.proxy_configured);
    assert_eq!(check.egress, None);
    assert_eq!(check.account_handle, None);

    assert_eq!(
        platform.mailbox_connection_check("missing-mailbox").await,
        Err(ConnectionCheckError::ConnectionUnavailable)
    );
    assert_eq!(
        platform.youtube_connection_check("not-a-uuid").await,
        Err(ConnectionCheckError::ConnectionUnavailable)
    );
}

#[test]
fn the_public_contract_is_closed_and_secret_free() {
    let schema = serde_json::to_value(schemars::schema_for!(ConnectionCheck)).unwrap();
    let properties = schema["properties"].as_object().unwrap();
    let mut keys = properties.keys().map(String::as_str).collect::<Vec<_>>();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "account_handle",
            "checked_at",
            "egress",
            "proxy_configured",
            "status"
        ]
    );
    let text = serde_json::to_string(&schema).unwrap();
    for status in [
        "ok",
        "proxy_unreachable",
        "network_unreachable",
        "credential_rejected",
        "provider_unavailable",
    ] {
        assert!(text.contains(status), "{status}");
    }
    for forbidden in ["password", "api_key", "token", "cookie", "endpoint"] {
        assert!(!text.contains(forbidden), "{forbidden}");
    }
}
