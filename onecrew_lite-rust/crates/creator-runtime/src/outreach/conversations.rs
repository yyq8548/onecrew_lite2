use onecrew_dm_contracts::{DmTarget, DmThread};
use std::sync::Arc;

use crate::{
    AccountRef, ContactChannel, Conversation, ConversationGetInput, ConversationMessageInput,
    ConversationPlatformError, ConversationSummary, ConversationSyncInput, ConversationView,
    CreatorId, CreatorRepositoryCallError, ListConversationsInput, Page,
    PersistConversationSnapshotInput, Platform, ResolvedTarget,
};

use crate::outreach::shared::{contact_route_outreach_context, dm_target, message_channel};

impl crate::CreatorRuntime {
    /// How many rows in each inbox are waiting on a human. Pure read — the
    /// sidebar badge must never be able to trigger a provider round trip.
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn inbox_attention_async(
        &self,
    ) -> Result<crate::InboxAttention, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.inbox_attention())
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::inbox_attention_async`] to own admission and dispatch.
    pub fn inbox_attention(&self) -> Result<crate::InboxAttention, String> {
        self.repository.inbox_attention()
    }

    /// The DM inbox listing: a bounded, filterable view over locally persisted
    /// conversations across creators and operator accounts, newest activity
    /// first. Local read only — refreshing an individual thread stays an
    /// explicit sync.
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn list_dm_conversations_async(
        &self,
        input: ListConversationsInput,
    ) -> Result<Page<ConversationSummary>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.list_dm_conversations(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::list_dm_conversations_async`] to own admission and dispatch.
    pub fn list_dm_conversations(
        &self,
        input: ListConversationsInput,
    ) -> Result<Page<ConversationSummary>, String> {
        self.repository.list_conversations(input)
    }

    /// Fetch one provider conversation into the local Creator store.
    ///
    /// `expected_platform` is the toolkit scope of the caller: a
    /// toolkit-prefixed operation names its own platform and refuses another
    /// one before any provider call, while the platform-neutral transport
    /// names none.
    pub async fn sync_conversation(
        &self,
        input: ConversationSyncInput,
        expected_platform: Option<Platform>,
    ) -> Result<Option<ConversationView>, ConversationPlatformError> {
        let read_runtime = self.clone();
        let read_input = input.clone();
        let context = self
            .repository_read_segment(move || {
                let (_, _, operator, target) = contact_route_outreach_context(
                    &read_runtime,
                    &read_input.creator_id,
                    &read_input.contact_route_id,
                    &read_input.resolved_target_id,
                    &read_input.operator_account_ref_id,
                    true,
                )?;
                let dm_target = dm_target(&target)?;
                let channel = message_channel(operator.platform)?;
                let local_conversation =
                    read_runtime
                        .repository
                        .get_conversation(&operator.id, &target.id, channel)?;
                Ok(ConversationSyncContext {
                    operator,
                    target,
                    dm_target,
                    channel,
                    local_conversation,
                })
            })
            .await
            .map_err(|error| conversation_read_segment_error("context", error))?;
        if expected_platform.is_some_and(|expected| context.operator.platform != expected) {
            return Err(ConversationPlatformError::WrongPlatform);
        }
        self.sync_conversation_thread(input, context).await
    }

    async fn sync_conversation_thread(
        &self,
        input: ConversationSyncInput,
        context: ConversationSyncContext,
    ) -> Result<Option<ConversationView>, ConversationPlatformError> {
        let ConversationSyncContext {
            operator,
            target,
            dm_target,
            channel,
            local_conversation,
        } = context;
        let explicit_thread_id = input
            .external_thread_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let external_thread_id = explicit_thread_id.or_else(|| {
            local_conversation
                .as_ref()
                .map(|conversation| conversation.provider_thread_id.clone())
        });
        let remote = self
            .gateways
            .direct_messages
            .as_ref()
            .ok_or(ConversationPlatformError::GatewayUnavailable)?
            .thread(
                &operator,
                &dm_target,
                external_thread_id.as_deref(),
                input.limit.clamp(1, 200),
            )
            .await
            .map_err(|error| {
                ConversationPlatformError::ProviderRead(onecrew_dm_contracts::DmFailure {
                    stage: "thread_sync",
                    diagnostic: error.diagnostic,
                })
            })?;
        let Some(remote) = remote else {
            let Some(conversation) = local_conversation else {
                return Ok(None);
            };
            let repository = Arc::clone(&self.repository);
            let conversation_id = conversation.id.clone();
            let (messages, pending_triage) = self
                .repository_read_segment(move || {
                    let messages = repository.list_messages(&conversation_id)?;
                    let pending_triage = repository.pending_conversation_triage(
                        &conversation_id,
                        crate::PENDING_REPLY_TRIAGE_LIMIT,
                    )?;
                    Ok((messages, pending_triage))
                })
                .await
                .map_err(|error| conversation_read_segment_error("local fallback", error))?;
            return Ok(Some(ConversationView {
                creator_id: input.creator_id,
                operator_account: operator,
                target,
                conversation,
                messages,
                pending_triage,
            }));
        };
        let snapshot = conversation_snapshot(
            input.creator_id.clone(),
            input.contact_route_id.clone(),
            operator.id.clone(),
            target.id.clone(),
            channel,
            &target,
            remote,
        )
        .map_err(|_| ConversationPlatformError::InvalidSnapshot)?;
        let write_runtime = self.clone();
        let (conversation, messages) = self
            .repository_write_segment(move || {
                let result = write_runtime
                    .repository
                    .persist_conversation_snapshot(snapshot)?;
                // Publish inside the supervised blocking closure after the
                // commit, so caller cancellation cannot strand a durable
                // conversation behind a stale process-local view.
                write_runtime.publish_changes([crate::CreatorChangeScope::Conversations]);
                Ok(result)
            })
            .await
            .map_err(conversation_write_segment_error)?;
        // Only the conversation scope is published above. Outreach would also
        // invalidate capability probes (a Provider round trip plus credential
        // recovery) and could turn a read refresh into platform rate limiting.
        let repository = Arc::clone(&self.repository);
        let conversation_id = conversation.id.clone();
        let pending_triage = self
            .repository_read_segment(move || {
                repository.pending_conversation_triage(
                    &conversation_id,
                    crate::PENDING_REPLY_TRIAGE_LIMIT,
                )
            })
            .await
            .map_err(|error| conversation_read_segment_error("triage readback", error))?;
        Ok(Some(ConversationView {
            creator_id: input.creator_id,
            operator_account: operator,
            target,
            conversation,
            messages,
            pending_triage,
        }))
    }

    /// One locally recorded conversation and its messages, by conversation ID.
    ///
    /// The conversation row is the whole address: it names the account, the
    /// peer and the provider thread, and its first-linked contact route names
    /// the Creator. Reads stay open while that Creator is archived, because
    /// the history is already local.
    /// Run this use case under bounded read admission before blocking dispatch.
    pub async fn get_conversation_async(
        &self,
        input: ConversationGetInput,
    ) -> Result<Option<ConversationView>, crate::CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_read_segment(move || owner.get_conversation(input))
            .await
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::get_conversation_async`] to own admission and dispatch.
    pub fn get_conversation(
        &self,
        input: ConversationGetInput,
    ) -> Result<Option<ConversationView>, String> {
        let Some(context) = self
            .repository
            .conversation_route_context(&input.conversation_id)?
        else {
            return Ok(None);
        };
        let route = context.route.ok_or_else(|| {
            "conversation is not linked to a contact route, so it has no Creator".to_string()
        })?;
        let conversation = context.conversation;
        let operator = self
            .repository
            .get_account_ref(&conversation.account_ref_id)?;
        let target = self
            .repository
            .get_resolved_target(&conversation.resolved_target_id)?;
        let messages = self.repository.list_messages(&conversation.id)?;
        let pending_triage = self
            .repository
            .pending_conversation_triage(&conversation.id, crate::PENDING_REPLY_TRIAGE_LIMIT)?;
        Ok(Some(ConversationView {
            creator_id: route.creator_id,
            operator_account: operator,
            target,
            conversation,
            messages,
            pending_triage,
        }))
    }
}

struct ConversationSyncContext {
    operator: AccountRef,
    target: ResolvedTarget,
    dm_target: DmTarget,
    channel: ContactChannel,
    local_conversation: Option<Conversation>,
}

fn conversation_read_segment_error(
    stage: &'static str,
    _error: CreatorRepositoryCallError<String>,
) -> ConversationPlatformError {
    ConversationPlatformError::LocalRead(stage)
}

fn conversation_write_segment_error(
    error: CreatorRepositoryCallError<String>,
) -> ConversationPlatformError {
    match error {
        CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            ConversationPlatformError::RecordNotStarted
        }
        CreatorRepositoryCallError::Owner(_) | CreatorRepositoryCallError::Dispatch(_) => {
            ConversationPlatformError::RecordUnknown
        }
    }
}

pub(super) fn conversation_snapshot(
    creator_id: CreatorId,
    contact_route_id: crate::CreatorContactRouteId,
    account_ref_id: crate::AccountRefId,
    resolved_target_id: crate::ResolvedTargetId,
    channel: ContactChannel,
    target: &ResolvedTarget,
    remote: DmThread,
) -> Result<PersistConversationSnapshotInput, String> {
    if remote.peer_external_id != target.external_target_id {
        return Err("conversation peer stable ID does not match the resolved target".into());
    }
    if remote.external_thread_id.trim().is_empty() {
        return Err("platform conversation returned no thread id".into());
    }
    Ok(PersistConversationSnapshotInput {
        creator_id,
        contact_route_id,
        account_ref_id,
        resolved_target_id,
        channel,
        provider_thread_id: remote.external_thread_id,
        cursor: None,
        unread_count: 0,
        messages: remote
            .messages
            .into_iter()
            .map(|message| ConversationMessageInput {
                external_message_id: message.external_message_id,
                sender_target_id: (!message.outgoing).then(|| target.id.clone()),
                direction: if message.outgoing {
                    "outbound".into()
                } else {
                    "inbound".into()
                },
                body: message.body,
                status: if message.outgoing {
                    "sent".into()
                } else {
                    "received".into()
                },
                // The platform's own message, kept as it was returned. Rows
                // written before this carry no `provider` key, which reads as
                // an explicit absence; nothing backfills them.
                public_data: serde_json::json!({ "provider": message.provider }),
                sent_at: message.sent_at,
            })
            .collect(),
    })
}
