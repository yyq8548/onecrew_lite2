use onecrew_creator_runtime::{
    EmailCapability, MailboxDetail, MailboxIssue, MailboxProvider, MailboxRefId, MailboxRefInput,
};

use super::{cleanup, legacy_mailbox, platform};
use crate::{LocalConnectionAvailability, MailboxProviderKind, connection_inventory::mailboxes};

#[test]
fn mailbox_mapper_uses_mailbox_identity_and_recorded_local_availability() {
    let (platform, directory) = platform();
    let legacy_id = legacy_mailbox(
            &platform,
            "Zulu mailbox",
            "zulu@example.com",
            Some("Zulu Sender".into()),
            "resend-secret",
        );
    let unavailable_id = MailboxRefId::new("33333333-3333-4333-8333-333333333333");
    platform
        .creators
        .sync_mailbox_ref(MailboxRefInput {
            id: unavailable_id.clone(),
            provider: "resend".into(),
            label: "Alpha mailbox".into(),
            email_address: "alpha@example.com".into(),
            display_name: None,
            capabilities: vec![EmailCapability::Inbox],
            available: false,
            last_error: Some("owner-only failure".into()),
        })
        .unwrap();

    let summaries = platform.platform_connection_list().unwrap().mailboxes;
    assert_eq!(summaries[0].mailbox_ref_id, unavailable_id.as_str());
    assert_eq!(summaries[0].display_label, "Alpha mailbox");
    assert_eq!(summaries[0].provider, MailboxProviderKind::Resend);
    assert_eq!(
        summaries[0].availability,
        LocalConnectionAvailability::Unavailable
    );
    assert_eq!(summaries[1].mailbox_ref_id, legacy_id.to_string());
    assert_eq!(summaries[1].display_label, "Zulu Sender");
    assert_eq!(
        summaries[1].availability,
        LocalConnectionAvailability::Unavailable
    );

    cleanup(platform, directory);
}

#[test]
fn mailbox_mapper_omits_a_record_without_a_detail_route_id() {
    let summaries = mailboxes::summaries(vec![MailboxDetail {
        mailbox_ref_id: MailboxRefId::new(" "),
        provider: MailboxProvider::Gmail,
        label: "No route".into(),
        email_address: "missing-id@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        issues: Vec::new(),
        created_at: 0,
        updated_at: 0,
    }]);

    assert!(summaries.is_empty());
}

#[test]
fn mailbox_mapper_uses_authoritative_availability_without_projecting_owner_error() {
    const OWNER_ERROR: &str = "owner-only diagnostic";
    let summaries = mailboxes::summaries(vec![MailboxDetail {
        mailbox_ref_id: MailboxRefId::new("44444444-4444-4444-8444-444444444444"),
        provider: MailboxProvider::Gmail,
        label: "Available mailbox".into(),
        email_address: "available@example.com".into(),
        display_name: None,
        capabilities: vec![EmailCapability::Inbox],
        available: true,
        issues: vec![MailboxIssue::RecordedFailure],
        created_at: 0,
        updated_at: 0,
    }]);

    assert_eq!(
        summaries[0].availability,
        LocalConnectionAvailability::Available
    );
    assert!(
        !serde_json::to_string(&summaries)
            .unwrap()
            .contains(OWNER_ERROR)
    );
}
