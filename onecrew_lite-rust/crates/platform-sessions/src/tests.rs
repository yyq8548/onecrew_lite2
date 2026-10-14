use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::thread::ThreadId;

use onecrew_ig::IgAccountSource;
use onecrew_local_domain::{BrowserCookie, Platform, ProfileSecret};
use onecrew_local_vault::LocalVault;
use onecrew_proxy::{ProxyConfig, ProxyKind};
use onecrew_tg::TgAccountSource;
use onecrew_x::{XAccount, XAccountSource, XSession};
use uuid::Uuid;

use super::{
    PlatformSessions, SessionMaterialError, TelegramPreparedSession, TelegramProxyError,
    cached_x_account, instagram_account_with_proxy, telegram_session_material,
    telegram_session_with_proxy, telegram_socks5_proxy_url, x_account_with_proxy,
    x_session_fingerprint,
};

const COOKIE_SECRET: &str = "COOKIE_VALUE_SENTINEL";
const SESSION_SECRET: &str = "SESSION_VALUE_SENTINEL";
const API_HASH_SECRET: &str = "API_HASH_SENTINEL";
const PROXY_USER: &str = "PROXY_USER_SENTINEL";
const PROXY_SECRET: &str = "PROXY_PASSWORD_SENTINEL";
/// Not a URL any proxy parser accepts, so a facade that receives it must fail.
const UNUSABLE_PROXY_URL: &str = "http://[unusable proxy";

/// Every vault read the adapter performs, with the thread it ran on, keyed by
/// profile id so parallel tests never read each other's entries.
static VAULT_READS: Mutex<Vec<(Uuid, ThreadId)>> = Mutex::new(Vec::new());

pub(super) fn observe_vault_read(id: Uuid) {
    VAULT_READS
        .lock()
        .expect("vault reads")
        .push((id, std::thread::current().id()));
}

fn cookie(name: &str, value: &str) -> BrowserCookie {
    BrowserCookie {
        name: name.into(),
        value: value.into(),
        domain: ".x.com".into(),
        path: "/".into(),
        secure: true,
        http_only: true,
    }
}

fn x_secret(cookies: Vec<BrowserCookie>, proxy: Option<ProxyConfig>) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: "x-1".into(),
        cookies,
        x_chat_passcode: None,
        instagram_session_settings: None,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_session: None,
        profile_name: String::new(),
        browser_profile_dir: String::new(),
        fingerprint_seed: String::new(),
        proxy,
        proxy_egress: None,
    }
}

fn proxy(host: &str) -> ProxyConfig {
    ProxyConfig {
        kind: ProxyKind::Http,
        host: host.into(),
        port: 8080,
        username: None,
        password: None,
    }
}

fn credentialed_proxy(kind: ProxyKind) -> ProxyConfig {
    ProxyConfig {
        kind,
        host: "198.51.100.2".into(),
        port: 1080,
        username: Some(PROXY_USER.into()),
        password: Some(PROXY_SECRET.into()),
    }
}

fn account() -> XAccount {
    let session = XSession::new("auth", "csrf", BTreeMap::new()).expect("session");
    XAccount::new(session, None).expect("account")
}

/// A session JSON the Instagram facade accepts; its `sessionid` is a sentinel.
fn instagram_session_json() -> String {
    format!(
        r#"{{"uuids":{{"phone_id":"p","uuid":"u","client_session_id":"c","advertising_id":"a","android_device_id":"d","request_id":"r","tray_session_id":"t"}},"device_settings":{{"app_version":"1","android_version":30,"android_release":"11","dpi":"480dpi","resolution":"1080x1920","manufacturer":"m","device":"d","model":"m","cpu":"c","version_code":"1"}},"user_agent":"ua","authorization_data":{{"ds_user_id":"1","sessionid":"{SESSION_SECRET}","should_use_header_over_cookies":true}}}}"#
    )
}

fn assert_secret_free(error: SessionMaterialError) {
    let message = error.to_string();
    for forbidden in [
        COOKIE_SECRET,
        SESSION_SECRET,
        API_HASH_SECRET,
        PROXY_USER,
        PROXY_SECRET,
        "198.51.100.2",
        "unusable",
    ] {
        assert!(
            !message.contains(forbidden),
            "{forbidden} leaked into {message}"
        );
    }
}

fn prepared_error(
    result: Result<TelegramPreparedSession, SessionMaterialError>,
) -> SessionMaterialError {
    match result {
        Ok(_) => panic!("the Telegram material was accepted"),
        Err(error) => error,
    }
}

#[test]
fn the_fingerprint_covers_every_input_the_session_is_built_from() {
    let base = x_secret(vec![cookie("auth_token", "a"), cookie("ct0", "b")], None);
    // Cookie order is storage detail, not identity.
    let reordered = x_secret(vec![cookie("ct0", "b"), cookie("auth_token", "a")], None);
    assert_eq!(
        x_session_fingerprint(&base),
        x_session_fingerprint(&reordered)
    );

    // A re-login rotates the cookie value: that must be a new session.
    let relogged = x_secret(vec![cookie("auth_token", "a2"), cookie("ct0", "b")], None);
    assert_ne!(
        x_session_fingerprint(&base),
        x_session_fingerprint(&relogged)
    );

    // So must switching or changing the egress proxy.
    let proxied = x_secret(
        vec![cookie("auth_token", "a"), cookie("ct0", "b")],
        Some(proxy("gate.example")),
    );
    let reproxied = x_secret(
        vec![cookie("auth_token", "a"), cookie("ct0", "b")],
        Some(proxy("other.example")),
    );
    assert_ne!(
        x_session_fingerprint(&base),
        x_session_fingerprint(&proxied)
    );
    assert_ne!(
        x_session_fingerprint(&proxied),
        x_session_fingerprint(&reproxied)
    );

    // Concatenation must not alias: {"ab","c"} and {"a","bc"} differ.
    let glued = x_secret(vec![cookie("ab", "c")], None);
    let split = x_secret(vec![cookie("a", "bc")], None);
    assert_ne!(x_session_fingerprint(&glued), x_session_fingerprint(&split));
}

#[test]
fn an_unchanged_profile_reuses_its_live_session_and_a_changed_one_rebuilds() {
    let cache = Mutex::new(HashMap::new());
    let id = Uuid::new_v4();
    let builds = std::cell::Cell::new(0_u32);
    let build = |fingerprint: u64| {
        cached_x_account(
            &cache,
            id,
            fingerprint,
            || true,
            || {
                builds.set(builds.get() + 1);
                Ok(account())
            },
        )
        .expect("account")
    };

    build(1);
    build(1);
    assert_eq!(builds.get(), 1, "the second call reuses the cached session");

    // A new fingerprint (re-login, proxy change) forces a rebuild and
    // replaces the entry rather than accreting one.
    build(2);
    assert_eq!(builds.get(), 2);
    assert_eq!(cache.lock().expect("cache").len(), 1);
    build(2);
    assert_eq!(builds.get(), 2);
}

fn telegram_secret(proxy: Option<ProxyConfig>) -> ProfileSecret {
    ProfileSecret {
        provider_account_id: "tg-1".into(),
        cookies: Vec::new(),
        x_chat_passcode: None,
        instagram_session_settings: None,
        telegram_api_id: Some(1),
        telegram_api_hash: Some("hash".into()),
        telegram_session: Some("session".into()),
        profile_name: String::new(),
        browser_profile_dir: String::new(),
        fingerprint_seed: String::new(),
        proxy,
        proxy_egress: None,
    }
}

#[test]
fn telegram_session_adapter_never_falls_back_to_direct() {
    assert_eq!(
        telegram_session_material(Platform::Telegram, &telegram_secret(None))
            .err()
            .as_deref(),
        Some("Telegram profile is missing its required SOCKS5 proxy")
    );
    assert_eq!(
        telegram_session_material(
            Platform::Telegram,
            &telegram_secret(Some(ProxyConfig {
                kind: ProxyKind::Http,
                host: "proxy.example".into(),
                port: 8080,
                username: None,
                password: None,
            }))
        )
        .err()
        .as_deref(),
        Some("Telegram profile requires a SOCKS5 proxy")
    );
}

#[test]
fn the_x_constructor_rejects_missing_and_empty_cookies_without_leaking_them() {
    for (cookies, expected) in [
        (Vec::new(), SessionMaterialError::MissingXAuthToken),
        (
            vec![cookie("auth_token", COOKIE_SECRET)],
            SessionMaterialError::MissingXCsrfToken,
        ),
        (
            vec![cookie("ct0", COOKIE_SECRET)],
            SessionMaterialError::MissingXAuthToken,
        ),
        // Present but empty: the session itself refuses it.
        (
            vec![cookie("auth_token", ""), cookie("ct0", COOKIE_SECRET)],
            SessionMaterialError::XSessionRejected,
        ),
    ] {
        let error = x_account_with_proxy(&cookies, None)
            .err()
            .expect("the cookies were accepted");
        assert_eq!(error, expected);
        assert_secret_free(error);
    }
}

#[test]
fn the_x_constructor_builds_through_the_explicit_proxy() {
    let cookies = [
        cookie("auth_token", COOKIE_SECRET),
        cookie("ct0", COOKIE_SECRET),
    ];
    assert!(x_account_with_proxy(&cookies, None).is_ok());
    let proxy_url = credentialed_proxy(ProxyKind::Http).to_url();
    assert!(x_account_with_proxy(&cookies, Some(&proxy_url)).is_ok());

    // The explicit proxy reaches the facade: one it cannot use fails the build.
    let error = x_account_with_proxy(&cookies, Some(UNUSABLE_PROXY_URL))
        .err()
        .expect("an unusable proxy was accepted");
    assert_eq!(error, SessionMaterialError::XSessionRejected);
    assert_secret_free(error);
}

#[test]
fn the_instagram_constructor_rejects_missing_empty_and_invalid_sessions_without_leaking() {
    for (session, proxy_url, expected) in [
        (None, None, SessionMaterialError::MissingInstagramSession),
        (
            Some(String::new()),
            None,
            SessionMaterialError::MissingInstagramSession,
        ),
        (
            Some(format!("{{\"sessionid\":\"{SESSION_SECRET}\"")),
            None,
            SessionMaterialError::InstagramSessionRejected,
        ),
        // The explicit proxy reaches the facade: one it cannot use fails.
        (
            Some(instagram_session_json()),
            Some(UNUSABLE_PROXY_URL.to_owned()),
            SessionMaterialError::InstagramSessionRejected,
        ),
    ] {
        let error = instagram_account_with_proxy(session.as_deref(), proxy_url.as_deref())
            .err()
            .expect("the session was accepted");
        assert_eq!(error, expected);
        assert_secret_free(error);
    }

    let session = instagram_session_json();
    assert!(instagram_account_with_proxy(Some(&session), None).is_ok());
    let proxy_url = credentialed_proxy(ProxyKind::Http).to_url();
    assert!(instagram_account_with_proxy(Some(&session), Some(&proxy_url)).is_ok());
}

#[test]
fn the_telegram_constructor_rejects_missing_material_and_non_socks5_egress_without_leaking() {
    let socks5 = credentialed_proxy(ProxyKind::Socks5);
    let http = credentialed_proxy(ProxyKind::Http);
    let https = credentialed_proxy(ProxyKind::Https);
    for (api_id, session, proxy, expected) in [
        (
            None,
            Some(SESSION_SECRET),
            Some(&socks5),
            SessionMaterialError::MissingTelegramApiId,
        ),
        (
            Some(1),
            None,
            Some(&socks5),
            SessionMaterialError::MissingTelegramSession,
        ),
        (
            Some(1),
            Some(""),
            Some(&socks5),
            SessionMaterialError::MissingTelegramSession,
        ),
        (
            Some(1),
            Some(SESSION_SECRET),
            None,
            SessionMaterialError::TelegramProxy(TelegramProxyError::Missing),
        ),
        (
            Some(1),
            Some(SESSION_SECRET),
            Some(&http),
            SessionMaterialError::TelegramProxy(TelegramProxyError::NotSocks5),
        ),
        (
            Some(1),
            Some(SESSION_SECRET),
            Some(&https),
            SessionMaterialError::TelegramProxy(TelegramProxyError::NotSocks5),
        ),
    ] {
        let error = prepared_error(telegram_session_with_proxy(
            api_id,
            Some(API_HASH_SECRET),
            session,
            proxy,
        ));
        assert_eq!(error, expected);
        assert_secret_free(error);
    }

    let error = prepared_error(telegram_session_with_proxy(
        Some(1),
        None,
        Some(SESSION_SECRET),
        Some(&socks5),
    ));
    assert_eq!(error, SessionMaterialError::MissingTelegramApiHash);
    assert_secret_free(error);
}

#[test]
fn the_telegram_constructor_connects_through_the_explicit_socks5_proxy() {
    let socks5 = credentialed_proxy(ProxyKind::Socks5);
    let prepared = match telegram_session_with_proxy(
        Some(12_345),
        Some(API_HASH_SECRET),
        Some(SESSION_SECRET),
        Some(&socks5),
    ) {
        Ok(prepared) => prepared,
        Err(error) => panic!("the Telegram material was rejected: {error}"),
    };
    assert_eq!(prepared.api_id(), 12_345);
    assert_eq!(prepared.api_hash(), API_HASH_SECRET);
    assert_eq!(prepared.session, SESSION_SECRET);
    assert_eq!(prepared.proxy, socks5.to_url());
    assert_eq!(
        telegram_socks5_proxy_url(Some(&socks5)),
        Ok(socks5.to_url())
    );
}

#[test]
fn the_socks5_rule_never_yields_a_direct_or_http_route() {
    assert_eq!(
        telegram_socks5_proxy_url(None),
        Err(TelegramProxyError::Missing)
    );
    for kind in [ProxyKind::Http, ProxyKind::Https] {
        let proxy = credentialed_proxy(kind);
        let error = telegram_socks5_proxy_url(Some(&proxy)).unwrap_err();
        assert_eq!(error, TelegramProxyError::NotSocks5);
        for forbidden in [PROXY_USER, PROXY_SECRET] {
            assert!(!error.to_string().contains(forbidden));
        }
    }
}

fn open_sessions() -> (PlatformSessions, std::path::PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("onecrew-platform-sessions-{}", Uuid::new_v4()));
    let vault = LocalVault::open(directory.join("vault.db")).expect("vault");
    (PlatformSessions::new(vault), directory)
}

#[test]
fn forgetting_an_x_account_drops_only_its_live_session() {
    let (sessions, directory) = open_sessions();
    let disconnected = Uuid::new_v4();
    let kept = Uuid::new_v4();
    for id in [disconnected, kept] {
        cached_x_account(&sessions.x_sessions, id, 1, || true, || Ok(account())).expect("account");
    }

    sessions.forget_x_account(&disconnected.to_string());
    sessions.forget_x_account("not-a-profile-id");

    let cache = sessions.x_sessions.lock().expect("cache");
    assert!(!cache.contains_key(&disconnected));
    assert!(cache.contains_key(&kept));
    drop(cache);
    drop(sessions);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[test]
fn a_forget_during_a_build_keeps_that_session_out_of_the_cache() {
    use std::sync::atomic::Ordering::SeqCst;
    let (sessions, directory) = open_sessions();
    let id = Uuid::new_v4();
    let current = |observed: u64| sessions.x_session_forgets.load(SeqCst) == observed;

    // `x()` observes the forget count before its vault read; the account is
    // disconnected while the session is still being built.
    let observed = sessions.x_session_forgets.load(SeqCst);
    let built = cached_x_account(
        &sessions.x_sessions,
        id,
        1,
        || current(observed),
        || {
            sessions.forget_x_account(&id.to_string());
            Ok(account())
        },
    );
    assert!(built.is_ok(), "the caller still receives its session");
    assert!(!sessions.x_sessions.lock().expect("cache").contains_key(&id));

    // Without a forget during the build, the same session is cached.
    let observed = sessions.x_session_forgets.load(SeqCst);
    cached_x_account(
        &sessions.x_sessions,
        id,
        1,
        || current(observed),
        || Ok(account()),
    )
    .expect("account");
    assert!(sessions.x_sessions.lock().expect("cache").contains_key(&id));
    drop(sessions);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}

#[tokio::test]
async fn account_sources_read_the_vault_off_the_async_worker() {
    let (sessions, directory) = open_sessions();
    let worker = std::thread::current().id();
    let x = Uuid::new_v4();
    let instagram = Uuid::new_v4();
    let telegram = Uuid::new_v4();

    // None of these profiles exists, so every source fails after its read.
    assert!(sessions.open_x_account(&x.to_string()).await.is_err());
    assert!(
        sessions
            .open_ig_account(&instagram.to_string())
            .await
            .is_err()
    );
    assert!(
        sessions
            .open_tg_account(&telegram.to_string())
            .await
            .is_err()
    );

    let reads = VAULT_READS.lock().expect("vault reads").clone();
    for id in [x, instagram, telegram] {
        let thread = reads
            .iter()
            .find(|(read, _)| *read == id)
            .map(|(_, thread)| *thread)
            .expect("the source read the vault");
        assert_ne!(thread, worker, "the vault was read on the async worker");
    }
    drop(sessions);
    std::fs::remove_dir_all(directory).expect("remove test directory");
}
