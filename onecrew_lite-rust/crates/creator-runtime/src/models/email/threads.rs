use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::mailboxes::MailboxDetail;
use crate::models::{
    CreatorContactRouteId, CreatorId, EmailDirection, EmailId, EmailThreadFolder, EmailThreadId,
    MailboxRefId,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailThread {
    pub id: EmailThreadId,
    pub mailbox_ref_id: MailboxRefId,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub provider_thread_key: String,
    pub subject: String,
    pub unread_count: u32,
    pub folder: EmailThreadFolder,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailMessage {
    pub id: EmailId,
    pub thread_id: EmailThreadId,
    pub provider_message_id: String,
    pub internet_message_id: Option<String>,
    pub direction: EmailDirection,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub received_for: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub delivery_status: String,
    pub headers: serde_json::Value,
    pub occurred_at: u64,
}

/// Public projection of one email message: the canonical message without
/// BCC, received-for, or raw provider headers.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailMessageView {
    pub id: EmailId,
    pub thread_id: EmailThreadId,
    pub provider_message_id: String,
    pub internet_message_id: Option<String>,
    pub direction: EmailDirection,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub delivery_status: String,
    pub occurred_at: u64,
}

/// List-row projection of one email message: routing facts plus a bounded
/// text preview instead of the full body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailMessagePreview {
    pub id: EmailId,
    pub direction: EmailDirection,
    pub from: String,
    pub to: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub preview_text: Option<String>,
    pub has_html: bool,
    pub delivery_status: String,
    pub occurred_at: u64,
}

pub const EMAIL_PREVIEW_MAX_LENGTH: usize = 200;

impl EmailMessage {
    pub fn view(self) -> EmailMessageView {
        EmailMessageView {
            id: self.id,
            thread_id: self.thread_id,
            provider_message_id: self.provider_message_id,
            internet_message_id: self.internet_message_id,
            direction: self.direction,
            from: self.from,
            to: self.to,
            cc: self.cc,
            reply_to: self.reply_to,
            in_reply_to: self.in_reply_to,
            references: self.references,
            subject: self.subject,
            text: self.text,
            html: self.html,
            delivery_status: self.delivery_status,
            occurred_at: self.occurred_at,
        }
    }

    pub fn preview(&self) -> EmailMessagePreview {
        EmailMessagePreview {
            id: self.id.clone(),
            direction: self.direction,
            from: self.from.clone(),
            to: self.to.clone(),
            reply_to: self.reply_to.clone(),
            subject: self.subject.clone(),
            preview_text: self
                .text
                .as_deref()
                .map(|text| text.trim().chars().take(EMAIL_PREVIEW_MAX_LENGTH).collect()),
            has_html: self.html.is_some(),
            delivery_status: self.delivery_status.clone(),
            occurred_at: self.occurred_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailThreadSummary {
    pub thread: EmailThread,
    pub latest_message: Option<EmailMessagePreview>,
    pub message_count: u64,
    pub has_outbound: bool,
    pub creator_display_name: Option<String>,
    /// The newest verdict on any inbound message of this thread — newest by
    /// when it was judged, not by which message it judges. A later inbound
    /// that nobody has read yet leaves the earlier verdict standing here;
    /// what is still owed is `pending_triage` on the sync result, not the
    /// absence of this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage: Option<crate::models::ReplyTriageBrief>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailThreadView {
    pub mailbox: MailboxDetail,
    pub thread: EmailThread,
    /// One bounded page of messages, newest first; pass next_message_cursor
    /// back through EmailThreadGetInput.cursor to read older messages.
    pub messages: Vec<EmailMessageView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_message_cursor: Option<String>,
    pub creator_display_name: Option<String>,
}
