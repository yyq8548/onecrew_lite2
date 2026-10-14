use crate::{
    OperatorAccountCredentialAvailability, OperatorAccountPlatform, OperatorAccountProjection,
    XChatPasscodeStatus,
};

use super::super::{LocalConnectionAvailability, x};

fn account(id: &str, label: &str, platform: OperatorAccountPlatform) -> OperatorAccountProjection {
    OperatorAccountProjection {
        operator_account_id: id.into(),
        platform,
        provider_account_id: format!("provider-{id}"),
        username: format!("user-{id}"),
        label: label.into(),
        configured: true,
        credential_availability: OperatorAccountCredentialAvailability::Available,
        x_chat_passcode_status: XChatPasscodeStatus::Configured,
        updated_at: "2026-08-26T00:00:00Z".into(),
    }
}

#[test]
fn x_mapper_filters_preserves_ids_and_sorts() {
    let mut missing_id = account("", "Missing", OperatorAccountPlatform::X);
    missing_id.configured = false;
    missing_id.credential_availability = OperatorAccountCredentialAvailability::Unavailable;
    let mut unavailable = account("x-unavailable", "Unavailable", OperatorAccountPlatform::X);
    unavailable.configured = false;
    unavailable.credential_availability = OperatorAccountCredentialAvailability::Unavailable;
    let summaries = x::summaries(&[
        account("x-zulu", "Zulu", OperatorAccountPlatform::X),
        account("ig", "Instagram", OperatorAccountPlatform::Instagram),
        missing_id,
        unavailable,
        account("x-alpha", "Alpha", OperatorAccountPlatform::X),
    ]);

    assert_eq!(
        summaries
            .iter()
            .map(|summary| summary.operator_account_id.as_str())
            .collect::<Vec<_>>(),
        vec!["x-alpha", "x-unavailable", "x-zulu"]
    );
    assert_eq!(
        summaries[1].availability,
        LocalConnectionAvailability::Unavailable
    );
}
