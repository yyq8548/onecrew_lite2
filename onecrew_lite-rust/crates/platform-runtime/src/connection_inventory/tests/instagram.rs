use crate::{
    OperatorAccountCredentialAvailability, OperatorAccountPlatform, OperatorAccountProjection,
    XChatPasscodeStatus,
};

use super::super::{LocalConnectionAvailability, instagram};

fn account(id: &str, label: &str, configured: bool) -> OperatorAccountProjection {
    OperatorAccountProjection {
        operator_account_id: id.into(),
        platform: OperatorAccountPlatform::Instagram,
        provider_account_id: format!("provider-{id}"),
        username: format!("user-{id}"),
        label: label.into(),
        configured,
        credential_availability: if configured {
            OperatorAccountCredentialAvailability::Available
        } else {
            OperatorAccountCredentialAvailability::Unavailable
        },
        x_chat_passcode_status: XChatPasscodeStatus::NotApplicable,
        updated_at: "2026-08-26T00:00:00Z".into(),
    }
}

#[test]
fn instagram_mapper_keeps_unavailable_real_account_and_orders_by_label() {
    let summaries = instagram::summaries(&[
        account("ig-zulu", "Zulu", false),
        account("ig-alpha", "Alpha", true),
    ]);

    assert_eq!(summaries[0].operator_account_id, "ig-alpha");
    assert_eq!(summaries[1].operator_account_id, "ig-zulu");
    assert_eq!(
        summaries[1].availability,
        LocalConnectionAvailability::Unavailable
    );
}
