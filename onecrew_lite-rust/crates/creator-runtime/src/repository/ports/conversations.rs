use crate::{
    AccountRefId, CommitMessageActionResult, ContactRouteLastContactAccount, Conversation,
    CreatorContactRouteId, CreatorId, InboxAttention, Message, Page,
    PersistConversationSnapshotInput, ResolvedTargetId,
};

pub struct AppendMessageInput<'a> {
    pub conversation_id: &'a crate::ConversationId,
    pub external_message_id: Option<&'a str>,
    pub sender_target_id: Option<&'a ResolvedTargetId>,
    pub direction: &'a str,
    pub body: &'a str,
    pub status: &'a str,
    pub public_data: serde_json::Value,
    pub sent_at: u64,
}

pub trait ConversationRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn contact_route_last_contact_account(
        &self,
        creator_id: &CreatorId,
        contact_route_id: &CreatorContactRouteId,
    ) -> Result<Option<ContactRouteLastContactAccount>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn inbox_attention(&self) -> Result<InboxAttention, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_conversation(
        &self,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
        channel: crate::ContactChannel,
    ) -> Result<Option<Conversation>, String>;
    /// One conversation by ID, with the first-linked contact route that owns
    /// it. The inbox list resolves a conversation to its Creator the same
    /// way; this is the by-ID read of that same rule.
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn conversation_route_context(
        &self,
        conversation_id: &crate::ConversationId,
    ) -> Result<Option<crate::ConversationRouteContext>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_conversations(
        &self,
        input: crate::ListConversationsInput,
    ) -> Result<Page<crate::ConversationSummary>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn list_messages(
        &self,
        conversation_id: &crate::ConversationId,
    ) -> Result<Vec<Message>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_conversation(
        &self,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
        channel: crate::ContactChannel,
        provider_thread_id: &str,
        cursor: Option<&str>,
        unread_count: u32,
    ) -> Result<Conversation, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn append_message(&self, input: AppendMessageInput<'_>) -> Result<Message, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn persist_conversation_snapshot(
        &self,
        input: PersistConversationSnapshotInput,
    ) -> Result<(Conversation, Vec<Message>), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn record_creator_message(
        &self,
        key: &str,
        message: crate::CreatorOutreachMessageRecord,
    ) -> Result<CommitMessageActionResult, crate::CreatorOutreachError>;
}
