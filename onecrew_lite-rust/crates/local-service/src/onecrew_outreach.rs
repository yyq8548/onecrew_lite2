use onecrew_creator_runtime::{
    ContactRouteLastContactAccount, ContactRouteLastContactAccountInput, ConversationGetInput,
    ConversationSummary, ConversationSyncInput, ConversationView, CreatorOutreachStatus,
    DirectMessagePreparation, InboxAttention, ListConversationsInput, OutreachActionStatusInput,
    Page, PrepareDirectMessageInput, SendDirectMessageInput,
};
use onecrew_local_runtime::{ResultOrigin, ResultSnapshot};

use crate::ServiceState;

/// The existing command encoder serializes this narrow DM failure unchanged.
/// Only typed owner diagnostics or fixed local copy may populate it.
#[derive(Debug, serde::Serialize)]
pub(super) struct WebDirectMessageError {
    pub(super) code: &'static str,
    pub(super) message: String,
    /// Boxed like the owner's `OwnerExecutionFailure::dm_failure` so this
    /// error stays small in `Result`; serde serializes the box transparently.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) failure: Option<Box<onecrew_local_runtime::DmFailure>>,
}

impl WebDirectMessageError {
    pub(super) fn local(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message: message.into(),
            failure: None,
        }
    }

    pub(super) fn runtime_unavailable() -> Self {
        Self::local("runtime_unavailable", "The local runtime is unavailable")
    }

    pub(super) fn from_outreach(error: onecrew_local_runtime::OutreachError) -> Self {
        use onecrew_local_runtime::OutreachError;
        if let OutreachError::DirectMessage(failure) = error {
            return Self {
                code: "direct_message_failed",
                message: failure.diagnostic.message.into(),
                failure: Some(Box::new(failure)),
            };
        }
        let code = match &error {
            OutreachError::InvalidInput(_) => "invalid_input",
            OutreachError::Creator(error) => match error {
                onecrew_creator_runtime::CreatorOutreachError::InvalidInput => "invalid_input",
                onecrew_creator_runtime::CreatorOutreachError::ContextUnavailable => {
                    "context_unavailable"
                }
                onecrew_creator_runtime::CreatorOutreachError::ScopeMismatch => "scope_mismatch",
                onecrew_creator_runtime::CreatorOutreachError::IntentConflict => "intent_conflict",
                onecrew_creator_runtime::CreatorOutreachError::ActionUnavailable => {
                    "action_unavailable"
                }
                onecrew_creator_runtime::CreatorOutreachError::ActionInProgress => {
                    "action_in_progress"
                }
                onecrew_creator_runtime::CreatorOutreachError::Storage => "storage_unavailable",
                onecrew_creator_runtime::CreatorOutreachError::CommitUnknown => "commit_unknown",
                onecrew_creator_runtime::CreatorOutreachError::RecordUnavailable => {
                    "record_unavailable"
                }
                onecrew_creator_runtime::CreatorOutreachError::RetiredActionUnresolved => {
                    "retired_action_unresolved"
                }
            },
            OutreachError::Target(_) => "target_unavailable",
            OutreachError::ProviderRead(_) => "provider_read_unavailable",
            OutreachError::ProviderStateCommitUnknown => "provider_state_commit_unknown",
            OutreachError::IdentityMismatch => "identity_mismatch",
            OutreachError::CapabilityUnavailable => "capability_unavailable",
            OutreachError::UnfollowedConfirmationRequired => "unfollowed_confirmation_required",
            OutreachError::WrongPlatform => "wrong_platform",
            OutreachError::Encoding => "encoding_failed",
            OutreachError::Result(_) => {
                return Self::local(
                    "review_unavailable",
                    "The outreach review could not be completed",
                );
            }
            OutreachError::DirectMessage(_) => unreachable!("DM diagnostic handled above"),
        };
        // Every remaining variant formats only its typed, owner-authored copy.
        Self {
            code,
            message: error.to_string(),
            failure: None,
        }
    }

    fn from_conversation(error: onecrew_creator_runtime::ConversationPlatformError) -> Self {
        use onecrew_creator_runtime::ConversationPlatformError;
        if let ConversationPlatformError::ProviderRead(failure) = error {
            return Self {
                code: "conversation_read_failed",
                message: failure.diagnostic.message.into(),
                failure: Some(Box::new(failure)),
            };
        }
        let code = match &error {
            ConversationPlatformError::WrongPlatform => "wrong_platform",
            ConversationPlatformError::LocalRead(_) => "conversation_local_read_unavailable",
            ConversationPlatformError::GatewayUnavailable => "gateway_unavailable",
            ConversationPlatformError::InvalidSnapshot => "invalid_snapshot",
            ConversationPlatformError::RecordNotStarted => "record_not_started",
            ConversationPlatformError::RecordUnknown => "record_unknown",
            ConversationPlatformError::ProviderRead(_) => {
                unreachable!("DM diagnostic handled above")
            }
        };
        Self {
            code,
            message: error.to_string(),
            failure: None,
        }
    }
}

pub(super) async fn onecrew_outreach_message_prepare(
    state: &ServiceState,
    input: PrepareDirectMessageInput,
) -> Result<DirectMessagePreparation, WebDirectMessageError> {
    state
        .platform()
        .await
        .map_err(|_| WebDirectMessageError::runtime_unavailable())?
        .prepare_direct_message(input)
        .await
        .map_err(WebDirectMessageError::from_outreach)
}

pub(super) async fn onecrew_outreach_message_send_prepare(
    state: &ServiceState,
    input: SendDirectMessageInput,
    origin: &ResultOrigin,
) -> Result<ResultSnapshot, WebDirectMessageError> {
    let platform = state
        .platform()
        .await
        .map_err(|_| WebDirectMessageError::runtime_unavailable())?;
    let review = platform
        .prepare_outreach_direct_message(
            input,
            crate::outreach_review::context(origin, "onecrew.outreach.message.send").map_err(
                |_| {
                    WebDirectMessageError::local(
                        "review_unavailable",
                        "The outreach review could not be prepared",
                    )
                },
            )?,
        )
        .await
        .map_err(WebDirectMessageError::from_outreach)?;
    crate::outreach_review::present(&platform, review).map_err(|_| {
        WebDirectMessageError::local(
            "review_unavailable",
            "The outreach review could not be presented",
        )
    })
}

pub(super) async fn onecrew_outreach_action_status(
    state: &ServiceState,
    input: OutreachActionStatusInput,
) -> Result<CreatorOutreachStatus, String> {
    state
        .platform()
        .await?
        .outreach_action_status(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_contact_route_last_contact_account(
    state: &ServiceState,
    input: ContactRouteLastContactAccountInput,
) -> Result<Option<ContactRouteLastContactAccount>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .contact_route_last_contact_account_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_conversation_sync(
    state: &ServiceState,
    input: ConversationSyncInput,
) -> Result<Option<ConversationView>, WebDirectMessageError> {
    state
        .platform()
        .await
        .map_err(|_| WebDirectMessageError::runtime_unavailable())?
        .creators()
        .map_err(|_| WebDirectMessageError::runtime_unavailable())?
        // The Web is not scoped to one toolkit: it syncs whatever thread the
        // human opened.
        .sync_conversation(input, None)
        .await
        .map_err(WebDirectMessageError::from_conversation)
}

pub(super) async fn onecrew_conversation_get(
    state: &ServiceState,
    input: ConversationGetInput,
) -> Result<Option<ConversationView>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .get_conversation_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(super) async fn onecrew_conversation_list(
    state: &ServiceState,
    input: ListConversationsInput,
) -> Result<Page<ConversationSummary>, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .list_dm_conversations_async(input)
        .await
        .map_err(|error| error.to_string())
}

/// Sidebar badge source: how many rows in each inbox are waiting on a
/// human. A pure counting read — the badge must never be able to start a
/// provider round trip.
pub(super) async fn onecrew_inbox_attention(
    state: &ServiceState,
) -> Result<InboxAttention, String> {
    let creators = state.platform().await?.creators()?;
    creators
        .inbox_attention_async()
        .await
        .map_err(|error| error.to_string())
}
