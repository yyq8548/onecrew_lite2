use crate::{
    OperatorAccountCredentialAvailability, OperatorAccountPlatform, OperatorAccountProjection,
    XChatPasscodeStatus,
};

use super::super::{LocalConnectionAvailability, telegram};

fn account(id: &str, label: &str, username: &str, available: bool) -> OperatorAccountProjection {
    OperatorAccountProjection {
        operator_account_id: id.into(),
        platform: OperatorAccountPlatform::Telegram,
        provider_account_id: format!("provider-{id}"),
        username: username.into(),
        label: label.into(),
        configured: available,
        credential_availability: if available {
            OperatorAccountCredentialAvailability::Available
        } else {
            OperatorAccountCredentialAvailability::Unavailable
        },
        x_chat_passcode_status: XChatPasscodeStatus::NotApplicable,
        updated_at: "2026-08-26T00:00:00Z".into(),
    }
}

#[test]
fn telegram_mapper_keeps_public_username_and_real_account_id() {
    let summaries = telegram::summaries(&[
        account("tg-zulu", "Zulu", "@zulu", true),
        account("tg-same-b", "Same", "@same_b", true),
        account("tg-same-a", "Same", "@same_a", false),
    ]);

    assert_eq!(
        summaries
            .iter()
            .map(|summary| summary.operator_account_id.as_str())
            .collect::<Vec<_>>(),
        vec!["tg-same-a", "tg-same-b", "tg-zulu"]
    );
    assert_eq!(summaries[0].username, "@same_a");
    assert_eq!(
        summaries[0].availability,
        LocalConnectionAvailability::Unavailable
    );
}
