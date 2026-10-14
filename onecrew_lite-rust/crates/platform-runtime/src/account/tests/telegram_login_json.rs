//! Frozen JSON for the Telegram login material the two peers read: the MCP
//! connection status (`code_delivery`, `qr_code`) and the Web in-app handoff
//! (`telegram_code_delivery`, `telegram_qr_challenge`). The flow holds the
//! Telegram transport values it received; the fixture is the wire contract and
//! must pass unchanged whichever type carries them across the public surface.

use onecrew_tg::{LoginCodeDelivery, LoginCodeMethod, QrLoginChallenge};
use serde::Deserialize;
use serde_json::Value;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    code_delivery: Vec<DeliveryCase>,
    qr_challenge: Projection,
    no_login_material: Projection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeliveryCase {
    name: String,
    status: Value,
    handoff: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Projection {
    status: Value,
    handoff: Value,
}

const METHODS: [LoginCodeMethod; 11] = [
    LoginCodeMethod::TelegramApp,
    LoginCodeMethod::Sms,
    LoginCodeMethod::VoiceCall,
    LoginCodeMethod::FlashCall,
    LoginCodeMethod::MissedCall,
    LoginCodeMethod::Email,
    LoginCodeMethod::EmailSetupRequired,
    LoginCodeMethod::Fragment,
    LoginCodeMethod::FirebaseSms,
    LoginCodeMethod::SmsWord,
    LoginCodeMethod::SmsPhrase,
];

/// Every method appears once as `method` and once as `next_method`; the last
/// delivery leaves every optional route detail empty.
fn deliveries() -> Vec<LoginCodeDelivery> {
    let mut deliveries: Vec<_> = METHODS
        .into_iter()
        .zip(METHODS.into_iter().cycle().skip(1))
        .zip(0_i32..)
        .map(|((method, next_method), index)| LoginCodeDelivery {
            method,
            code_length: Some(5 + index),
            destination_hint: Some(format!("hint-{index}")),
            timeout_seconds: Some(60 + index),
            next_method: Some(next_method),
            action_url: Some(format!("https://fragment.example/login/{index}")),
            can_submit_code: true,
        })
        .collect();
    deliveries.push(LoginCodeDelivery {
        method: LoginCodeMethod::EmailSetupRequired,
        code_length: None,
        destination_hint: None,
        timeout_seconds: None,
        next_method: None,
        action_url: None,
        can_submit_code: false,
    });
    deliveries
}

async fn prepared(platform: &LocalPlatform, method: OperatorAccountConnectionMethod) -> String {
    platform
        .operator_account_connection_prepare(OperatorAccountConnectionPrepareInput {
            platform: OperatorAccountPlatform::Telegram,
            intent: OperatorAccountConnectionIntent::Connect,
            method,
            operator_account_id: None,
            label: None,
        })
        .await
        .unwrap()
        .flow_id
}

/// MCP status JSON with the opaque operation ID and wall-clock expiry replaced
/// by placeholders after checking their shape.
fn status_json(platform: &LocalPlatform, flow_id: &str) -> Value {
    let status = platform
        .telegram_account_connection_snapshot(flow_id)
        .unwrap();
    let mut status = serde_json::to_value(status).unwrap();
    let object = status.as_object_mut().unwrap();
    assert_eq!(
        object.insert("operation_id".into(), "<operation_id>".into()),
        Some(Value::from(flow_id))
    );
    assert!(
        object
            .insert("expires_at".into(), "<expires_at>".into())
            .is_some_and(|expires_at| expires_at.is_u64())
    );
    status
}

/// Web in-app handoff JSON, encoded as the local-service command encodes it,
/// with the opaque flow ID replaced by a placeholder.
async fn handoff_json(platform: &LocalPlatform, flow_id: &str) -> Value {
    let handoff = platform
        .operator_account_connection_in_app_handoff(flow_id)
        .await
        .unwrap();
    let mut handoff = serde_json::to_value(handoff).unwrap();
    assert_eq!(
        handoff
            .as_object_mut()
            .unwrap()
            .insert("flow_id".into(), "<flow_id>".into()),
        Some(Value::from(flow_id))
    );
    handoff
}

#[tokio::test]
async fn telegram_login_projections_keep_the_frozen_json() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/telegram_login_json.json")).unwrap();
    let (platform, directory) = platform();

    let phone = prepared(&platform, OperatorAccountConnectionMethod::TelegramPhone).await;
    assert_eq!(
        status_json(&platform, &phone),
        fixture.no_login_material.status,
        "status without login material"
    );
    assert_eq!(
        handoff_json(&platform, &phone).await,
        fixture.no_login_material.handoff,
        "handoff without login material"
    );

    let deliveries = deliveries();
    assert_eq!(deliveries.len(), fixture.code_delivery.len());
    for (delivery, case) in deliveries.into_iter().zip(fixture.code_delivery) {
        {
            let mut flows = platform.pending_account_connections.lock().unwrap();
            let flow = flows.get_mut(&phone).unwrap();
            flow.live = Some(LiveConnection::TelegramPhone {
                token: "phone-login-token".into(),
            });
            flow.code_delivery = Some(delivery);
            flow.phase = OperatorAccountConnectionPhase::AwaitingSecretInApp;
        }
        assert_eq!(
            status_json(&platform, &phone),
            case.status,
            "{}: MCP status",
            case.name
        );
        assert_eq!(
            handoff_json(&platform, &phone).await,
            case.handoff,
            "{}: in-app handoff",
            case.name
        );
    }

    let qr = prepared(&platform, OperatorAccountConnectionMethod::TelegramQr).await;
    {
        let mut flows = platform.pending_account_connections.lock().unwrap();
        let flow = flows.get_mut(&qr).unwrap();
        flow.live = Some(LiveConnection::TelegramQr {
            token: "qr-login-token".into(),
        });
        flow.qr_challenge = Some(QrLoginChallenge {
            image_data_url: "data:image/png;base64,iVBORw0KGgo=".into(),
            terminal_qr: "  █  ".into(),
            expires_at: 1_800_000_000,
        });
        flow.phase = OperatorAccountConnectionPhase::AwaitingUserInApp;
    }
    assert_eq!(
        status_json(&platform, &qr),
        fixture.qr_challenge.status,
        "QR status"
    );
    assert_eq!(
        handoff_json(&platform, &qr).await,
        fixture.qr_challenge.handoff,
        "QR handoff"
    );

    drop(platform);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Both peers read login material through runtime-owned carriers, never the
/// Telegram transport types the flow holds internally.
#[test]
fn login_material_fields_are_runtime_carriers() {
    let status = TelegramAccountConnectionStatus {
        operation_id: "operation".into(),
        phase: DirectAccountConnectionPhase::AwaitingTelegramOtp,
        expires_at: 0,
        qr_code: None,
        code_delivery: Some(crate::TelegramLoginCodeDelivery {
            method: crate::TelegramLoginCodeMethod::Sms,
            code_length: None,
            destination_hint: None,
            timeout_seconds: None,
            next_method: None,
            action_url: None,
            can_submit_code: true,
        }),
        operator_account: None,
        error_code: None,
    };
    let handoff = OperatorAccountConnectionInAppHandoff {
        flow_id: "flow".into(),
        telegram_qr_challenge: Some(crate::TelegramInAppQrChallenge {
            image_data_url: "data:image/png;base64,iVBORw0KGgo=".into(),
            expires_at: 1_800_000_000,
        }),
        telegram_code_delivery: status.code_delivery.clone(),
    };
    assert_eq!(
        serde_json::to_value(&handoff).unwrap(),
        serde_json::json!({
            "flow_id": "flow",
            "telegram_qr_challenge": {
                "image_data_url": "data:image/png;base64,iVBORw0KGgo=",
                "expires_at": 1_800_000_000,
            },
            "telegram_code_delivery": {
                "method": "sms",
                "code_length": null,
                "destination_hint": null,
                "timeout_seconds": null,
                "next_method": null,
                "action_url": null,
                "can_submit_code": true,
            },
        })
    );
}
