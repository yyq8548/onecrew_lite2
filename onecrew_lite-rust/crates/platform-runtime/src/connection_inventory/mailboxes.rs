use onecrew_creator_runtime::{MailboxDetail, MailboxProvider};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{LocalConnectionAvailability, shared};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailboxProviderKind {
    Resend,
    Gmail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxAccountSummary {
    pub mailbox_ref_id: String,
    pub email_address: String,
    pub display_label: String,
    pub provider: MailboxProviderKind,
    pub availability: LocalConnectionAvailability,
}

pub(super) fn summaries(mailboxes: Vec<MailboxDetail>) -> Vec<MailboxAccountSummary> {
    let mut summaries = mailboxes
        .into_iter()
        .filter_map(summary)
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        left.email_address
            .cmp(&right.email_address)
            .then_with(|| left.mailbox_ref_id.cmp(&right.mailbox_ref_id))
    });
    summaries
}

fn summary(mailbox: MailboxDetail) -> Option<MailboxAccountSummary> {
    let provider = match mailbox.provider {
        MailboxProvider::Resend => MailboxProviderKind::Resend,
        MailboxProvider::Gmail => MailboxProviderKind::Gmail,
    };
    let mailbox_ref_id = mailbox.mailbox_ref_id.as_str().trim().to_owned();
    if mailbox_ref_id.is_empty() {
        return None;
    }
    let display_label = mailbox
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let label = mailbox.label.trim();
            (!label.is_empty()).then_some(label)
        })
        .unwrap_or(mailbox.email_address.as_str())
        .to_owned();
    Some(MailboxAccountSummary {
        mailbox_ref_id,
        email_address: mailbox.email_address,
        display_label,
        provider,
        availability: shared::local_availability(mailbox.available),
    })
}
