use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AccountRefId, AccountRelationshipId, AuditEventId, ContactLocatorId, ConversationId, CreatorId,
    EmailId, MailboxRefId, MessageId, PlatformAccountId, ResolvedTargetId, TouchpointId,
    core::{ContactChannel, RelationshipKind, RelationshipValue},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AccountRelationship {
    pub id: AccountRelationshipId,
    pub actor_account_ref_id: AccountRefId,
    pub target_platform_account_id: PlatformAccountId,
    pub kind: RelationshipKind,
    pub value: RelationshipValue,
    pub checked_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Conversation {
    pub id: ConversationId,
    pub account_ref_id: AccountRefId,
    pub resolved_target_id: ResolvedTargetId,
    pub channel: ContactChannel,
    pub provider_thread_id: String,
    pub cursor: Option<String>,
    pub unread_count: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Message {
    pub id: MessageId,
    pub conversation_id: ConversationId,
    pub external_message_id: Option<String>,
    pub sender_target_id: Option<ResolvedTargetId>,
    pub direction: String,
    pub body: String,
    pub status: String,
    pub public_data: serde_json::Value,
    pub sent_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Touchpoint {
    pub id: TouchpointId,
    pub creator_id: CreatorId,
    pub kind: String,
    pub message_id: Option<MessageId>,
    pub email_id: Option<EmailId>,
    pub account_relationship_id: Option<AccountRelationshipId>,
    pub occurred_at: u64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub kind: String,
    pub creator_id: Option<CreatorId>,
    pub account_ref_id: Option<AccountRefId>,
    pub resolved_target_id: Option<ResolvedTargetId>,
    pub mailbox_ref_id: Option<MailboxRefId>,
    pub contact_locator_id: Option<ContactLocatorId>,
    pub email_id: Option<EmailId>,
    pub payload: serde_json::Value,
    pub created_at: u64,
}
