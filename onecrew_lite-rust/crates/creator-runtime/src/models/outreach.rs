use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::ConversationId;
use super::{
    AccountRefId, CreatorContactRouteId, CreatorId, PlatformAccountId, ResolvedTargetId,
    accounts_and_targets::{AccountRef, ResolvedTarget},
    core::{ActionFailureKind, ContactChannel, DirectMessageCapability, RelationshipMutation},
    creator::CreatorRecord,
    creator_identity::PlatformAccount,
    default_conversation_limit,
    engagement_records::{AccountRelationship, AuditEvent, Conversation, Message, Touchpoint},
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipSnapshot {
    pub following: AccountRelationship,
    pub can_message: AccountRelationship,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipContextInput {
    pub creator_id: CreatorId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareRelationshipActionInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
    pub action: RelationshipMutation,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RelationshipActionPreparation {
    pub creator_id: CreatorId,
    pub operator_account: AccountRef,
    pub target: ResolvedTarget,
    pub action: RelationshipMutation,
    pub relationship: RelationshipSnapshot,
    pub confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateRelationshipInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
    pub action: RelationshipMutation,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareDirectMessageInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectMessagePreparation {
    pub creator_id: CreatorId,
    pub operator_account: AccountRef,
    pub target: ResolvedTarget,
    pub relationship: RelationshipSnapshot,
    pub capability: DirectMessageCapability,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub confirmation_required: bool,
    pub unfollowed_confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactRouteLastContactAccountInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ContactRouteLastContactAccount {
    pub contact_route_id: CreatorContactRouteId,
    pub account_ref: AccountRef,
    pub channel: ContactChannel,
    pub last_contact_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendDirectMessageInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
    pub body: String,
    #[serde(default)]
    pub allow_unfollowed_recipient: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationSyncInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub operator_account_ref_id: AccountRefId,
    pub external_thread_id: Option<String>,
    #[serde(default = "default_conversation_limit")]
    pub limit: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConversationGetInput {
    pub conversation_id: ConversationId,
}

/// Reply inside one open conversation. The conversation is the whole address:
/// it already names the account, the peer and the provider thread, and its
/// first-linked contact route names the Creator the touchpoint lands under.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SendConversationReplyInput {
    pub conversation_id: ConversationId,
    pub body: String,
    pub idempotency_key: String,
}

/// Safe conversation-sync failures. Remote reads retain protocol evidence;
/// local read and commit failures never expose repository Display strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConversationPlatformError {
    /// The conversation is not on the platform this caller is scoped to.
    WrongPlatform,
    ProviderRead(onecrew_dm_contracts::DmFailure),
    LocalRead(&'static str),
    GatewayUnavailable,
    InvalidSnapshot,
    RecordNotStarted,
    RecordUnknown,
}

impl std::fmt::Display for ConversationPlatformError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WrongPlatform => formatter.write_str("this conversation is on another platform"),
            Self::ProviderRead(failure) => formatter.write_str(failure.diagnostic.message),
            Self::LocalRead(stage) => write!(formatter, "conversation sync {stage} local read is unavailable"),
            Self::GatewayUnavailable => formatter.write_str("direct message gateway is not configured"),
            Self::InvalidSnapshot => formatter.write_str("conversation readback identity is invalid"),
            Self::RecordNotStarted => formatter.write_str("conversation snapshot commit did not start"),
            Self::RecordUnknown => formatter.write_str("conversation snapshot commit outcome is unknown; read the conversation before retrying"),
        }
    }
}

impl std::error::Error for ConversationPlatformError {}

/// One conversation resolved by ID together with the contact route that owns
/// it. A conversation resolves to a Creator through its first-linked contact
/// route, exactly as the inbox list does; the resolved target only names the
/// peer account.
#[derive(Debug, Clone)]
pub struct ConversationRouteContext {
    pub conversation: Conversation,
    /// Absent when nothing links this conversation to a contact route.
    pub route: Option<ConversationRoute>,
}

#[derive(Debug, Clone)]
pub struct ConversationRoute {
    pub contact_route_id: CreatorContactRouteId,
    pub creator_id: CreatorId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConversationView {
    pub creator_id: CreatorId,
    pub operator_account: AccountRef,
    pub target: ResolvedTarget,
    pub conversation: Conversation,
    pub messages: Vec<Message>,
    /// 这条会话里还欠着判断的入站消息。与邮箱同步同一笔账:收进来是
    /// 一半,说清楚每条是什么意思是另一半。
    pub pending_triage: crate::PendingReplyTriageDebt,
}

/// Prepare an explicitly requested public reply through one operating account.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrepareContentReplyInput {
    pub creator_id: CreatorId,
    pub target_platform_account_id: PlatformAccountId,
    pub operator_account_ref_id: AccountRefId,
    pub content_id: String,
    pub reply_to_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContentReplyPreparation {
    pub creator: CreatorRecord,
    pub target_platform_account: PlatformAccount,
    pub operator_account: AccountRef,
    pub content_id: String,
    pub reply_to_id: Option<String>,
    pub confirmation_required: bool,
    pub confirmation_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SendContentReplyInput {
    pub creator_id: CreatorId,
    pub target_platform_account_id: PlatformAccountId,
    pub operator_account_ref_id: AccountRefId,
    pub content_id: String,
    pub reply_to_id: Option<String>,
    pub body: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActionFailure {
    pub kind: ActionFailureKind,
    pub message: String,
    pub remote_receipt: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessageInput {
    pub external_message_id: String,
    pub sender_target_id: Option<ResolvedTargetId>,
    pub direction: String,
    pub body: String,
    pub status: String,
    pub public_data: serde_json::Value,
    pub sent_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistConversationSnapshotInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub account_ref_id: AccountRefId,
    pub resolved_target_id: ResolvedTargetId,
    pub channel: ContactChannel,
    pub provider_thread_id: String,
    pub cursor: Option<String>,
    pub unread_count: u32,
    pub messages: Vec<ConversationMessageInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitRelationshipActionResult {
    pub relationship: AccountRelationship,
    pub can_message: AccountRelationship,
    pub touchpoint: Touchpoint,
    pub audit_event: AuditEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitMessageActionResult {
    pub conversation: Conversation,
    pub message: Message,
    pub touchpoint: Touchpoint,
    pub audit_event: AuditEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitContentReplyActionResult {
    pub external_reply_id: String,
    pub touchpoint: Touchpoint,
    pub audit_event: AuditEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum IdempotencyClaim {
    Acquired,
    Completed(serde_json::Value),
    Failed(serde_json::Value),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum RecoverableActionClaim {
    RemoteStarted(serde_json::Value),
    RemoteAccepted(serde_json::Value),
    RemoteVerified(serde_json::Value),
    Completed(serde_json::Value),
    Failed(serde_json::Value),
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn assert_rejects_unknown_field<T: DeserializeOwned>(mut value: serde_json::Value) {
        assert!(
            serde_json::from_value::<T>(value.clone()).is_ok(),
            "base input must be valid"
        );
        value
            .as_object_mut()
            .expect("test input must be an object")
            .insert("unexpected".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<T>(value).is_err());
    }

    #[test]
    fn route_attributed_outreach_inputs_reject_unknown_fields() {
        assert_rejects_unknown_field::<PrepareRelationshipActionInput>(serde_json::json!({
            "creator_id": "creator-1",
            "contact_route_id": "route-1",
            "resolved_target_id": "target-1",
            "operator_account_ref_id": "operator-1",
            "action": "follow"
        }));
        assert_rejects_unknown_field::<UpdateRelationshipInput>(serde_json::json!({
            "creator_id": "creator-1",
            "contact_route_id": "route-1",
            "resolved_target_id": "target-1",
            "operator_account_ref_id": "operator-1",
            "action": "follow",
            "idempotency_key": "relationship-1"
        }));
        assert_rejects_unknown_field::<PrepareDirectMessageInput>(serde_json::json!({
            "creator_id": "creator-1",
            "contact_route_id": "route-1",
            "resolved_target_id": "target-1",
            "operator_account_ref_id": "operator-1"
        }));
        assert_rejects_unknown_field::<SendDirectMessageInput>(serde_json::json!({
            "creator_id": "creator-1",
            "contact_route_id": "route-1",
            "resolved_target_id": "target-1",
            "operator_account_ref_id": "operator-1",
            "body": "hello",
            "allow_unfollowed_recipient": false,
            "idempotency_key": "message-1"
        }));
        assert_rejects_unknown_field::<ConversationSyncInput>(serde_json::json!({
            "creator_id": "creator-1",
            "contact_route_id": "route-1",
            "resolved_target_id": "target-1",
            "operator_account_ref_id": "operator-1",
            "external_thread_id": null,
            "limit": 25
        }));
        assert_rejects_unknown_field::<ConversationGetInput>(serde_json::json!({
            "conversation_id": "conversation-1"
        }));
        assert_rejects_unknown_field::<SendConversationReplyInput>(serde_json::json!({
            "conversation_id": "conversation-1",
            "body": "hello",
            "idempotency_key": "reply-1"
        }));
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutreachActionStatusInput {
    pub idempotency_key: String,
}

/// One DM-inbox row: the persisted conversation plus the full four-id route
/// context the desktop needs to open its thread directly, and the fields the
/// list line renders. Local data only.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationSummary {
    pub conversation_id: ConversationId,
    pub creator_id: CreatorId,
    pub creator_display_name: String,
    /// The creator's display avatar (first-captured account first, falling
    /// back to the next account that has one) — the same face everywhere
    /// this person appears, not a per-thread one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_avatar_url: Option<String>,
    /// Latest observed handle of that peer account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer_handle: Option<String>,
    pub channel: ContactChannel,
    pub operator_account_ref_id: AccountRefId,
    pub operator_account_label: String,
    pub contact_route_id: CreatorContactRouteId,
    pub resolved_target_id: ResolvedTargetId,
    pub provider_thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<ConversationLastMessage>,
    /// The newest verdict on any inbound message of this conversation —
    /// newest by when it was judged, not by which message it judges. A later
    /// unread inbound leaves the earlier verdict standing; what is still owed
    /// is `pending_triage`. It belongs on the row because that is where a
    /// human decides what to open next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triage: Option<crate::models::ReplyTriageBrief>,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct ConversationLastMessage {
    pub direction: String,
    /// First 120 characters of the body.
    pub preview: String,
    pub sent_at: u64,
}

/// Bounded, filterable inbox page over locally persisted DM conversations,
/// newest activity first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConversationListStatus {
    WaitingReply,
    Replied,
}

impl ConversationListStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WaitingReply => "waiting_reply",
            Self::Replied => "replied",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListConversationsInput {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    /// Case-insensitive substring match on the Creator display name, the
    /// peer's latest observed handle, or the operator-account label. A leading
    /// '@' is ignored.
    #[serde(default)]
    pub query: Option<String>,
    /// Reply state derived from the latest persisted message direction. DM
    /// providers do not expose a reliable read/unread state.
    #[serde(default)]
    pub status: Option<ConversationListStatus>,
    #[serde(default)]
    pub channel: Option<ContactChannel>,
}

/// 两个收件箱各有多少条在等人。侧边栏徽标要的就是这个数,所以它是
/// 一次专门的计数读,不是把整页会话拉下来自己数。
///
/// 两侧语义**不同**,而且是故意的:
///
/// * 邮件有真正的未读状态(`emails.delivery_status`,快照时重算,打开
///   线程清零),所以邮件数的是未读。
/// * 私信没有。provider 不给未读数,`conversations.unread_count` 从来
///   就是 0。对外联来说更有用的问题本来也不是「我读没读」,而是
///   「球在谁手里」——所以私信数的是「最后一条是对方发的」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InboxAttention {
    /// 对方最后发言、还没回的私信会话数。
    pub conversations: u32,
    /// 有未读邮件的线程数。
    pub mail: u32,
}
