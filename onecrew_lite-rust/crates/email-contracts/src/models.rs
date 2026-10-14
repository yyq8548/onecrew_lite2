use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundEmail {
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub idempotency_key: String,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailCapability {
    Send,
    SentReadback,
    Inbox,
    Reply,
}

/// What a sent email's `last_event` says about whether the provider actually
/// put the message on the wire.
///
/// `last_event` is not a verdict, it is a position in a sequence that moves
/// over time: `queued` or `scheduled`, then `sent`, then `delivered`, and
/// afterwards `opened`, `clicked`, `bounced`, or `complained`. Reading one
/// single value as the only success would stall every send that has merely
/// not advanced yet, so each documented value carries exactly one of these
/// three meanings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentDelivery {
    /// The provider handed the message to the receiving side. This is the
    /// only class that proves the send happened, and it begins at `sent`:
    /// waiting for `delivered` would stall a message already on its way,
    /// because the receiving server confirms on its own schedule, or never.
    Sent,
    /// The send is still moving and the provider has not said where it
    /// landed. Read the status again later — this is a wait, not a fault.
    Advancing,
    /// The provider will not deliver this message, and no later read of the
    /// same message will say otherwise.
    Failed,
}

/// Every `last_event` value Resend documents, with what it says about the
/// send.
///
/// Two placements are worth stating outright. `complained` is
/// [`SentDelivery::Sent`]: a spam complaint can only come from someone who
/// received the message. `canceled` is [`SentDelivery::Failed`]: a scheduled
/// send stopped before it ever went out.
///
/// An event outside this table reads as [`SentDelivery::Advancing`] — the
/// only reading that neither invents a delivery nor condemns one, and the
/// same tolerance the plain-string wire contract keeps so that a new
/// provider event degrades instead of failing a whole page.
pub const DOCUMENTED_SENT_EVENTS: [(&str, SentDelivery); 11] = [
    ("bounced", SentDelivery::Failed),
    ("canceled", SentDelivery::Failed),
    ("clicked", SentDelivery::Sent),
    ("complained", SentDelivery::Sent),
    ("delivered", SentDelivery::Sent),
    ("delivery_delayed", SentDelivery::Advancing),
    ("failed", SentDelivery::Failed),
    ("opened", SentDelivery::Sent),
    ("queued", SentDelivery::Advancing),
    ("scheduled", SentDelivery::Advancing),
    ("sent", SentDelivery::Sent),
];

/// Read one `last_event` value. An undocumented event is
/// [`SentDelivery::Advancing`].
pub fn sent_delivery(last_event: &str) -> SentDelivery {
    DOCUMENTED_SENT_EVENTS
        .iter()
        .find(|(event, _)| *event == last_event)
        .map_or(SentDelivery::Advancing, |(_, delivery)| *delivery)
}

/// Whether the provider reported an event this contract documents.
pub fn is_documented_sent_event(last_event: &str) -> bool {
    DOCUMENTED_SENT_EVENTS
        .iter()
        .any(|(event, _)| *event == last_event)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSentEmail {
    pub provider_message_id: String,
    /// RFC Message-ID (`<...@...>`). `None` only for emails without an SMTP
    /// message yet (scheduled sends).
    pub internet_message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub last_event: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInboundEmail {
    pub provider_message_id: String,
    pub internet_message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    /// Provider envelope recipients. Resend's Receiving API is team-scoped,
    /// so these addresses are required to route messages to one local mailbox
    /// even when the visible To/Cc/Bcc headers do not contain that address.
    pub received_for: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub headers: serde_json::Value,
    pub created_at: String,
    pub attachments: Vec<ProviderAttachmentMetadata>,
}

/// One received list item. Carries every list-sufficient fact — RFC
/// identity, sender, subject, timestamps, and attachment metadata — so
/// callers can decide detail necessity and mailbox routing without a detail
/// request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInboundEmailSummary {
    pub provider_message_id: String,
    pub internet_message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub received_for: Vec<String>,
    pub subject: String,
    pub created_at: String,
    pub attachments: Vec<ProviderAttachmentMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInboundEmailPage {
    pub messages: Vec<ProviderInboundEmailSummary>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// One `GET /emails` list item mapped to the provider-neutral shape. Carries
/// every list-sufficient sent fact: delivery status (`last_event`) and RFC
/// identity reconcile from here without a detail request; the body does not.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSentEmailSummary {
    pub provider_message_id: String,
    /// RFC Message-ID (`<...@...>`). `None` only for emails that have no SMTP
    /// message yet (scheduled sends).
    pub internet_message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub last_event: String,
    pub created_at: String,
    pub scheduled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSentEmailPage {
    pub messages: Vec<ProviderSentEmailSummary>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

/// Durable-safe attachment metadata: provider identity and content facts,
/// never an access URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAttachmentMetadata {
    pub provider_attachment_id: String,
    pub filename: String,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size: Option<u64>,
}

/// A short-lived signed download capability for one attachment.
/// Intentionally not serializable and redacted in `Debug`: the URL expires
/// within an hour and must never reach SQLite, logs, audits, or any public
/// projection.
#[derive(Clone)]
pub struct ProviderAttachmentAccess {
    pub metadata: ProviderAttachmentMetadata,
    pub download_url: String,
    pub expires_at: String,
}

impl std::fmt::Debug for ProviderAttachmentAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderAttachmentAccess")
            .field("metadata", &self.metadata)
            .field("download_url", &"<redacted signed url>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// The four documented rate-limit headers as parsed from one response. All
/// optional: absence selects conservative fallback backoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderRateLimitStatus {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    pub reset_seconds: Option<u64>,
    pub retry_after_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEmailDomain {
    pub name: String,
    pub verified: bool,
    pub sending_enabled: bool,
    pub receiving_enabled: bool,
}

impl OutboundEmail {
    pub fn validate(&self) -> Result<(), crate::EmailError> {
        if self.from_email.trim().is_empty() {
            return Err(crate::EmailError::InvalidMessage(
                "from_email is required".into(),
            ));
        }
        if self.to.is_empty() || self.to.iter().any(|value| value.trim().is_empty()) {
            return Err(crate::EmailError::InvalidMessage(
                "at least one recipient is required".into(),
            ));
        }
        if self.subject.trim().is_empty() {
            return Err(crate::EmailError::InvalidMessage(
                "subject is required".into(),
            ));
        }
        if self.idempotency_key.trim().is_empty() {
            return Err(crate::EmailError::InvalidMessage(
                "idempotency_key is required".into(),
            ));
        }
        if self.text.as_deref().is_none_or(str::is_empty)
            && self.html.as_deref().is_none_or(str::is_empty)
        {
            return Err(crate::EmailError::InvalidMessage(
                "text or html body is required".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailReceipt {
    pub provider: String,
    pub message_id: Option<String>,
    pub accepted: bool,
}

#[cfg(test)]
mod sent_delivery_tests {
    use super::{DOCUMENTED_SENT_EVENTS, SentDelivery, is_documented_sent_event, sent_delivery};

    #[test]
    fn a_send_counts_from_the_moment_it_leaves_the_provider() {
        // Not `delivered` alone: a send that had to wait for the receiving
        // server's confirmation would stall while already on its way.
        for event in ["sent", "delivered", "opened", "clicked"] {
            assert_eq!(sent_delivery(event), SentDelivery::Sent, "{event}");
        }
        // A spam complaint can only come from someone who received it.
        assert_eq!(sent_delivery("complained"), SentDelivery::Sent);
    }

    #[test]
    fn a_status_still_in_motion_is_a_wait_not_a_verdict() {
        for event in ["queued", "scheduled", "delivery_delayed"] {
            assert_eq!(sent_delivery(event), SentDelivery::Advancing, "{event}");
        }
    }

    #[test]
    fn a_send_the_provider_will_not_deliver_is_final() {
        // `canceled` belongs here: a scheduled send stopped before it went
        // out, so no later read turns it into a delivery.
        for event in ["bounced", "failed", "canceled"] {
            assert_eq!(sent_delivery(event), SentDelivery::Failed, "{event}");
        }
    }

    #[test]
    fn an_undocumented_event_waits_instead_of_inventing_an_outcome() {
        assert!(!is_documented_sent_event("brand_new_event"));
        assert_eq!(sent_delivery("brand_new_event"), SentDelivery::Advancing);
        assert_eq!(sent_delivery(""), SentDelivery::Advancing);
        for (event, _) in DOCUMENTED_SENT_EVENTS {
            assert!(is_documented_sent_event(event), "{event}");
        }
    }
}
