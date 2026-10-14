//! Frozen proxy decoding for every runtime entry that accepts a proxy: the MCP
//! browser and Telegram connection inputs, and the Web entries that set or
//! probe a profile proxy, configure an in-app browser or Telegram flow, and
//! open the Telegram credentials browser. The fixture is the wire contract and
//! must pass unchanged whichever type carries the proxy.

use serde::Deserialize;
use serde_json::{Value, json};

use super::{ProxyConfig, ProxyKind};
use crate::{BrowserAccountConnectionStartInput, TelegramAccountConnectionStartInput};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DecodeCase {
    name: String,
    input: Value,
    decoded: Option<Value>,
    #[serde(default)]
    debug: Option<String>,
}

fn decoded(proxy: &ProxyConfig) -> Value {
    json!({
        "kind": match proxy.kind {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5",
        },
        "host": proxy.host,
        "port": proxy.port,
        "username": proxy.username,
        "password": proxy.password,
    })
}

#[test]
fn proxy_decoding_matches_the_frozen_wire_contract() {
    let cases: Vec<DecodeCase> =
        serde_json::from_str(include_str!("fixtures/proxy_config_decode.json")).unwrap();
    assert_eq!(cases.len(), 18);
    for case in cases {
        let name = case.name.as_str();
        let direct = serde_json::from_value::<ProxyConfig>(case.input.clone()).ok();
        assert_eq!(
            direct.as_ref().map(decoded),
            case.decoded,
            "{name}: proxy parameter"
        );

        let optional = serde_json::from_value::<Option<ProxyConfig>>(case.input.clone());
        if case.input.is_null() {
            assert!(
                matches!(optional, Ok(None)),
                "{name}: absent optional proxy"
            );
        } else {
            assert_eq!(
                optional.ok().flatten().as_ref().map(decoded),
                case.decoded,
                "{name}: optional proxy parameter"
            );
        }

        let browser = serde_json::from_value::<BrowserAccountConnectionStartInput>(json!({
            "platform": "x",
            "profile_name": "operator",
            "proxy": case.input.clone(),
            "x_chat_preference": "disabled",
            "idempotency_key": "proxy-decode",
        }))
        .ok();
        assert_eq!(
            browser.as_ref().map(|input| decoded(&input.proxy)),
            case.decoded,
            "{name}: browser connection input"
        );

        let telegram = serde_json::from_value::<TelegramAccountConnectionStartInput>(json!({
            "api_id": 12345,
            "api_hash": "api-hash",
            "proxy": case.input.clone(),
            "idempotency_key": "proxy-decode",
        }))
        .ok();
        assert_eq!(
            telegram.as_ref().map(|input| decoded(&input.proxy)),
            case.decoded,
            "{name}: Telegram connection input"
        );

        match (direct, case.debug) {
            (Some(proxy), Some(expected)) => {
                let rendered = format!("{proxy:?}");
                assert_eq!(rendered, expected, "{name}: debug rendering");
                if let Some(password) = proxy.password.as_deref() {
                    assert!(
                        !rendered.contains(password),
                        "{name}: debug leaks the password"
                    );
                }
            }
            (None, None) => {}
            _ => panic!("{name}: accepted cases freeze a debug rendering, rejected cases none"),
        }
    }
}
