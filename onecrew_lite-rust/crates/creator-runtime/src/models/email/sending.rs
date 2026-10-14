use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    mailboxes::MailboxDetail,
    threads::{EmailMessageView, EmailThread},
};
use crate::models::{
    AuditEvent, CreatorContactRouteId, CreatorContactRouteRecord, CreatorId, CreatorRecord,
    EmailId, MailboxRefId, Touchpoint,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareEmailInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub mailbox_ref_id: MailboxRefId,
    pub reply_to_email_id: Option<EmailId>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailPreparation {
    pub creator: CreatorRecord,
    pub route: CreatorContactRouteRecord,
    pub mailbox: MailboxDetail,
    pub thread: Option<EmailThread>,
    pub reply_to: Option<EmailMessageView>,
    /// Advance notice from this prepare step that the matching send is gated by
    /// an owner review. It is never an input to a send: confirmed intent is
    /// carried only by the sealed grant that review mints, so no field an Agent
    /// can set decides whether mail leaves.
    pub confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SendEmailInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub mailbox_ref_id: MailboxRefId,
    pub reply_to_email_id: Option<EmailId>,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailActionResult {
    pub preparation: EmailPreparation,
    pub thread: EmailThread,
    pub email: EmailMessageView,
    pub touchpoint: Touchpoint,
    pub audit_event: AuditEvent,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareManualEmailInput {
    pub mailbox_ref_id: MailboxRefId,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManualEmailPreparation {
    pub mailbox: MailboxDetail,
    pub to: String,
    /// See [`EmailPreparation::confirmation_required`]: advance notice of the
    /// owner review, not an input that authorizes the send.
    pub confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SendManualEmailInput {
    pub mailbox_ref_id: MailboxRefId,
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ManualEmailActionResult {
    pub preparation: ManualEmailPreparation,
    pub thread: EmailThread,
    pub email: EmailMessageView,
    pub audit_event: AuditEvent,
    pub replayed: bool,
}
