use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::threads::{EMAIL_PREVIEW_MAX_LENGTH, EmailMessageView, EmailThread};
use crate::models::{
    AuditEvent, CreatorContactRouteId, CreatorId, EmailDraftId, EmailDraftState, EmailId,
    EmailTemplateId, MailboxRefId, Touchpoint,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailDraft {
    pub id: EmailDraftId,
    pub mailbox_ref_id: MailboxRefId,
    pub to: String,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub reply_to_email_id: Option<EmailId>,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub template_id: Option<EmailTemplateId>,
    pub template_variables: std::collections::BTreeMap<String, String>,
    pub state: EmailDraftState,
    pub sending: bool,
    pub pending_send_idempotency_key: Option<String>,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListEmailDraftsInput {
    pub mailbox_ref_id: Option<MailboxRefId>,
    pub state: Option<EmailDraftState>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
}

/// List-row projection of one draft: state and a bounded text preview
/// without the bodies, template variables, or the pending send key.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailDraftSummary {
    pub id: EmailDraftId,
    pub mailbox_ref_id: MailboxRefId,
    pub to: String,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub subject: String,
    pub preview_text: String,
    pub has_html: bool,
    pub state: EmailDraftState,
    pub sending: bool,
    pub revision: u64,
    pub created_at: u64,
    pub updated_at: u64,
}

impl EmailDraft {
    pub fn summary(&self) -> EmailDraftSummary {
        EmailDraftSummary {
            id: self.id.clone(),
            mailbox_ref_id: self.mailbox_ref_id.clone(),
            to: self.to.clone(),
            creator_id: self.creator_id.clone(),
            contact_route_id: self.contact_route_id.clone(),
            subject: self.subject.clone(),
            preview_text: self
                .text
                .trim()
                .chars()
                .take(EMAIL_PREVIEW_MAX_LENGTH)
                .collect(),
            has_html: self.html.is_some(),
            state: self.state,
            sending: self.sending,
            revision: self.revision,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailDraftGetInput {
    pub draft_id: EmailDraftId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveEmailDraftInput {
    pub id: Option<EmailDraftId>,
    pub mailbox_ref_id: MailboxRefId,
    pub to: String,
    pub creator_id: Option<CreatorId>,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub reply_to_email_id: Option<EmailId>,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub template_id: Option<EmailTemplateId>,
    #[serde(default)]
    pub template_variables: std::collections::BTreeMap<String, String>,
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailDraftMutationInput {
    pub draft_id: EmailDraftId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendEmailDraftInput {
    pub draft_id: EmailDraftId,
    pub expected_revision: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailDraftSendResult {
    pub draft_id: EmailDraftId,
    pub thread: EmailThread,
    pub email: EmailMessageView,
    pub touchpoint: Option<Touchpoint>,
    pub audit_event: AuditEvent,
    pub replayed: bool,
}
