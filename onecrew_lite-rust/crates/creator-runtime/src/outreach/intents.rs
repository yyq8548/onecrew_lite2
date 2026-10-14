//! Canonical business intent identity. Provider request DTOs never enter this owner.

use crate::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorOutreachOperation {
    RelationshipUpdate,
    ContentReplySend,
    DirectMessageSend,
}

impl CreatorOutreachOperation {
    pub fn action_name(self) -> &'static str {
        match self {
            Self::RelationshipUpdate => "relationship_update_v1",
            Self::ContentReplySend => "content_reply_send_v1",
            Self::DirectMessageSend => "direct_message_send_v1",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatorOutreachTarget {
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_target_id: String,
    pub platform_account_id: PlatformAccountId,
    pub handle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreatorOutreachConversation {
    pub conversation_id: ConversationId,
    pub provider_thread_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatorRouteOutreachContext {
    pub creator_id: CreatorId,
    pub creator_display_name: String,
    pub contact_route_id: CreatorContactRouteId,
    pub operator_account: AccountRef,
    pub resolved_target_id: ResolvedTargetId,
    pub target: CreatorOutreachTarget,
    pub conversation: Option<CreatorOutreachConversation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatorContentOutreachContext {
    pub creator_id: CreatorId,
    pub creator_display_name: String,
    pub operator_account: AccountRef,
    pub target: CreatorOutreachTarget,
    pub content_id: String,
    pub reply_to_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum CreatorOutreachContext {
    Route(CreatorRouteOutreachContext),
    Content(CreatorContentOutreachContext),
}

/// Stable, body-free ownership retained by every durable claim. Optional
/// fields are encoded explicitly; constructors enforce each operation's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorOutreachIdentity {
    pub operation: CreatorOutreachOperation,
    pub creator_id: CreatorId,
    pub operator_account_ref_id: AccountRefId,
    pub platform: Platform,
    pub target_platform_account_id: PlatformAccountId,
    pub target_kind: TargetKind,
    pub external_target_id: String,
    pub contact_route_id: Option<CreatorContactRouteId>,
    pub resolved_target_id: Option<ResolvedTargetId>,
    pub conversation: Option<CreatorOutreachConversation>,
    pub content_id: Option<String>,
    pub reply_to_id: Option<String>,
    pub relationship_action: Option<RelationshipMutation>,
    pub allow_unfollowed_recipient: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorOutreachError {
    InvalidInput,
    ContextUnavailable,
    ScopeMismatch,
    IntentConflict,
    ActionUnavailable,
    ActionInProgress,
    Storage,
    CommitUnknown,
    RecordUnavailable,
    RetiredActionUnresolved,
}

impl std::fmt::Display for CreatorOutreachError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "outreach input is invalid",
            Self::ContextUnavailable => "outreach context is unavailable",
            Self::ScopeMismatch => "outreach stable ownership does not match",
            Self::IntentConflict => "idempotency key belongs to another outreach intent",
            Self::ActionUnavailable => "outreach action is unavailable",
            Self::ActionInProgress => "outreach action is already in progress",
            Self::Storage => "outreach durable state is unavailable",
            Self::CommitUnknown => "outreach local commit outcome is unknown",
            Self::RecordUnavailable => {
                "outreach record is unavailable; the durable phase is retained"
            }
            Self::RetiredActionUnresolved => {
                "retired outreach action has an unresolved remote effect"
            }
        })
    }
}
impl std::error::Error for CreatorOutreachError {}

/// Memory-only prepared authority. Serialize is for the trusted runtime result
/// store; there is deliberately no Deserialize or mutable field access.
#[derive(Clone, Serialize)]
pub struct PreparedCreatorOutreachIntent {
    pub(crate) identity: CreatorOutreachIdentity,
    context: CreatorOutreachContext,
    body: Option<String>,
    fingerprint: String,
}

impl std::fmt::Debug for PreparedCreatorOutreachIntent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedCreatorOutreachIntent")
            .field("identity", &self.identity)
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

impl PreparedCreatorOutreachIntent {
    pub fn relationship(
        context: CreatorRouteOutreachContext,
        action: RelationshipMutation,
    ) -> Result<Self, CreatorOutreachError> {
        let mut identity = route_identity(&context, CreatorOutreachOperation::RelationshipUpdate);
        identity.relationship_action = Some(action);
        Self::build(identity, CreatorOutreachContext::Route(context), None)
    }
    pub fn direct_message(
        context: CreatorRouteOutreachContext,
        body: String,
        allow_unfollowed_recipient: bool,
    ) -> Result<Self, CreatorOutreachError> {
        let mut identity = route_identity(&context, CreatorOutreachOperation::DirectMessageSend);
        identity.conversation = context.conversation.clone();
        identity.allow_unfollowed_recipient = allow_unfollowed_recipient;
        Self::build(identity, CreatorOutreachContext::Route(context), Some(body))
    }
    pub fn content_reply(
        mut context: CreatorContentOutreachContext,
        body: String,
    ) -> Result<Self, CreatorOutreachError> {
        context.content_id = context.content_id.trim().to_owned();
        context.reply_to_id = context
            .reply_to_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let identity = CreatorOutreachIdentity {
            operation: CreatorOutreachOperation::ContentReplySend,
            creator_id: context.creator_id.clone(),
            operator_account_ref_id: context.operator_account.id.clone(),
            platform: context.target.platform,
            target_platform_account_id: context.target.platform_account_id.clone(),
            target_kind: context.target.target_kind,
            external_target_id: context.target.external_target_id.clone(),
            contact_route_id: None,
            resolved_target_id: None,
            conversation: None,
            content_id: Some(context.content_id.trim().to_owned()),
            reply_to_id: context
                .reply_to_id
                .as_deref()
                .map(str::trim)
                .map(str::to_owned),
            relationship_action: None,
            allow_unfollowed_recipient: false,
        };
        Self::build(
            identity,
            CreatorOutreachContext::Content(context),
            Some(body),
        )
    }
    fn build(
        identity: CreatorOutreachIdentity,
        context: CreatorOutreachContext,
        body: Option<String>,
    ) -> Result<Self, CreatorOutreachError> {
        let body = body.map(|body| body.trim().to_owned());
        if body.as_deref().is_some_and(str::is_empty)
            || identity.external_target_id.trim().is_empty()
            || identity.content_id.as_deref().is_some_and(str::is_empty)
            || identity.reply_to_id.as_deref().is_some_and(str::is_empty)
        {
            return Err(CreatorOutreachError::InvalidInput);
        }
        let fingerprint = fingerprint_v1(&identity, body.as_deref())?;
        Ok(Self {
            identity,
            context,
            body,
            fingerprint,
        })
    }
    pub fn identity(&self) -> &CreatorOutreachIdentity {
        &self.identity
    }
    pub fn context(&self) -> &CreatorOutreachContext {
        &self.context
    }
    pub fn body(&self) -> Option<&str> {
        self.body.as_deref()
    }
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn operation(&self) -> CreatorOutreachOperation {
        self.identity.operation
    }
}

fn route_identity(
    context: &CreatorRouteOutreachContext,
    operation: CreatorOutreachOperation,
) -> CreatorOutreachIdentity {
    CreatorOutreachIdentity {
        operation,
        creator_id: context.creator_id.clone(),
        operator_account_ref_id: context.operator_account.id.clone(),
        platform: context.target.platform,
        target_platform_account_id: context.target.platform_account_id.clone(),
        target_kind: context.target.target_kind,
        external_target_id: context.target.external_target_id.clone(),
        contact_route_id: Some(context.contact_route_id.clone()),
        resolved_target_id: Some(context.resolved_target_id.clone()),
        conversation: None,
        content_id: None,
        reply_to_id: None,
        relationship_action: None,
        allow_unfollowed_recipient: false,
    }
}

pub(crate) fn fingerprint_v1(
    identity: &CreatorOutreachIdentity,
    body: Option<&str>,
) -> Result<String, CreatorOutreachError> {
    #[derive(Serialize)]
    struct Encoding<'a> {
        version: u8,
        identity: &'a CreatorOutreachIdentity,
        actual_body: Option<&'a str>,
    }
    let bytes = serde_json::to_vec(&Encoding {
        version: 1,
        identity,
        actual_body: body,
    })
    .map_err(|_| CreatorOutreachError::InvalidInput)?;
    Ok(format!("sha256:v1:{:x}", Sha256::digest(bytes)))
}
