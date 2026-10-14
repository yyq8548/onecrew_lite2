use onecrew_creator_runtime::AccountRefId;
use onecrew_local_domain::{BrowserCookie, LocalProfile, ProfileSecret};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::{
    LocalPlatform, OperatorAccountDisconnectError, OperatorAccountDisconnectInput, ProxyKind,
};

use super::flow_state::{DirectRequestReservation, LiveConnection, complete_flow};
use super::*;

fn platform() -> (LocalPlatform, std::path::PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("onecrew-account-workflow-test-{}", Uuid::new_v4()));
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

fn dummy_secret() -> ProfileSecret {
    ProfileSecret {
        provider_account_id: "remote-operator-1".into(),
        cookies: Vec::new(),
        x_chat_passcode: Some("must-not-leak".into()),
        instagram_session_settings: None,
        telegram_api_id: None,
        telegram_api_hash: None,
        telegram_session: None,
        profile_name: "Public label".into(),
        browser_profile_dir: "/tmp/nonexistent".into(),
        fingerprint_seed: "private fingerprint".into(),
        proxy: Some(onecrew_proxy::ProxyConfig {
            kind: onecrew_proxy::ProxyKind::Socks5,
            host: "proxy.internal".into(),
            port: 1080,
            username: Some("proxy-user".into()),
            password: Some("proxy-password".into()),
        }),
        proxy_egress: Some("203.0.113.1".into()),
    }
}

mod account_projection;
mod browser_direct;
mod direct_requests;
mod disconnect;
mod flow_commands;
mod flow_lifecycle;
mod reconnect;
mod telegram_direct;
mod telegram_login_json;
mod x_chat_passcode;
