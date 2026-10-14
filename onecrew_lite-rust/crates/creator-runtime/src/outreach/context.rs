//! Stable, read-only ownership loading. Admission for a new remote effect is
//! deliberately separate from historical context and ledger recovery.

use crate::*;

impl CreatorRuntime {
    pub async fn creator_relationship_read_context(
        &self,
        input: RelationshipContextInput,
    ) -> Result<(AccountRef, CreatorOutreachTarget), CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            let creator = repository
                .get_creator(&input.creator_id)
                .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
            let operator = repository
                .get_account_ref(&input.operator_account_ref_id)
                .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
            let target = repository
                .get_resolved_target(&input.resolved_target_id)
                .map_err(|_| CreatorOutreachError::ContextUnavailable)?;
            let account_id = target
                .platform_account_id
                .clone()
                .ok_or(CreatorOutreachError::ScopeMismatch)?;
            if target.account_ref_id != operator.id
                || target.platform != operator.platform
                || !creator.account_links.iter().any(|link| {
                    link.platform_account_id == account_id && link.state == LinkState::Confirmed
                })
            {
                return Err(CreatorOutreachError::ScopeMismatch);
            }
            let handle = target
                .metadata
                .get("handle")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            Ok((
                operator,
                CreatorOutreachTarget {
                    platform: target.platform,
                    target_kind: target.target_kind,
                    external_target_id: target.external_target_id,
                    platform_account_id: account_id,
                    handle,
                },
            ))
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_cached_relationship_snapshot(
        &self,
        operator: &AccountRefId,
        target: &PlatformAccountId,
    ) -> Result<Option<RelationshipSnapshot>, CreatorOutreachError> {
        let repository = self.repository.clone();
        let operator = operator.clone();
        let target = target.clone();
        self.repository_read_segment(move || {
            let following = repository
                .get_account_relationship(&operator, &target, RelationshipKind::Following)
                .map_err(|_| CreatorOutreachError::Storage)?;
            let can_message = repository
                .get_account_relationship(&operator, &target, RelationshipKind::CanMessage)
                .map_err(|_| CreatorOutreachError::Storage)?;
            Ok(match (following, can_message) {
                (Some(following), Some(can_message)) => Some(RelationshipSnapshot {
                    following,
                    can_message,
                }),
                _ => None,
            })
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_record_relationship_observation(
        &self,
        operator: AccountRefId,
        target: PlatformAccountId,
        following: RelationshipValue,
        can_message: RelationshipValue,
        checked_at: u64,
    ) -> Result<RelationshipSnapshot, CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_write_segment(move || {
            let following = repository
                .upsert_account_relationship(
                    &operator,
                    &target,
                    RelationshipKind::Following,
                    following,
                    checked_at,
                )
                .map_err(|_| CreatorOutreachError::Storage)?;
            let can_message = repository
                .upsert_account_relationship(
                    &operator,
                    &target,
                    RelationshipKind::CanMessage,
                    can_message,
                    checked_at,
                )
                .map_err(|_| CreatorOutreachError::Storage)?;
            Ok(RelationshipSnapshot {
                following,
                can_message,
            })
        })
        .await
        .map_err(write_error)
    }
    pub async fn creator_outreach_resolved_target(
        &self,
        target_id: &ResolvedTargetId,
    ) -> Result<ResolvedTarget, CreatorOutreachError> {
        let repository = self.repository.clone();
        let target_id = target_id.clone();
        self.repository_read_segment(move || {
            repository
                .get_resolved_target(&target_id)
                .map_err(|_| CreatorOutreachError::ContextUnavailable)
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_relationship_context(
        &self,
        input: PrepareRelationshipActionInput,
    ) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            repository.outreach_route_context(
                &input.creator_id,
                &input.contact_route_id,
                &input.resolved_target_id,
                &input.operator_account_ref_id,
            )
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_direct_message_context(
        &self,
        input: PrepareDirectMessageInput,
    ) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            repository.outreach_route_context(
                &input.creator_id,
                &input.contact_route_id,
                &input.resolved_target_id,
                &input.operator_account_ref_id,
            )
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_conversation_reply_context(
        &self,
        conversation_id: ConversationId,
    ) -> Result<CreatorRouteOutreachContext, CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || {
            repository.outreach_conversation_context(&conversation_id)
        })
        .await
        .map_err(read_error)
    }
    pub async fn creator_content_reply_context(
        &self,
        input: PrepareContentReplyInput,
    ) -> Result<CreatorContentOutreachContext, CreatorOutreachError> {
        let repository = self.repository.clone();
        self.repository_read_segment(move || repository.outreach_content_context(input))
            .await
            .map_err(read_error)
    }

    /// Read the existing public-reply preview without provider I/O or a claim.
    ///
    /// # Errors
    /// Rejects invalid content or Creator ownership, archived Creators,
    /// unavailable operator accounts, and failed admitted repository reads.
    pub async fn creator_content_reply_preview(
        &self,
        input: PrepareContentReplyInput,
    ) -> Result<ContentReplyPreparation, CreatorOutreachError> {
        let runtime = self.clone();
        self.repository_read_segment(move || {
            let context = runtime.repository.outreach_content_context(input)?;
            let creator = runtime
                .require_active_creator(&context.creator_id)
                .map_err(|_| CreatorOutreachError::ActionUnavailable)?;
            let operator_account = runtime
                .repository
                .require_operable_account_ref(&context.operator_account.id)
                .map_err(|_| CreatorOutreachError::ActionUnavailable)?;
            let target_platform_account = creator
                .platform_accounts
                .iter()
                .find(|account| account.id == context.target.platform_account_id)
                .cloned()
                .ok_or(CreatorOutreachError::ScopeMismatch)?;
            Ok(ContentReplyPreparation {
                creator,
                target_platform_account,
                operator_account,
                content_id: context.content_id,
                reply_to_id: context.reply_to_id,
                confirmation_required: true,
                confirmation_reason: "publishing a public reply is an external write".into(),
            })
        })
        .await
        .map_err(read_error)
    }
}

pub(super) fn read_error(
    error: CreatorRepositoryCallError<CreatorOutreachError>,
) -> CreatorOutreachError {
    match error {
        CreatorRepositoryCallError::Owner(error) => error,
        CreatorRepositoryCallError::Dispatch(_) => CreatorOutreachError::Storage,
    }
}

pub(super) fn write_error(
    error: CreatorRepositoryCallError<CreatorOutreachError>,
) -> CreatorOutreachError {
    match error {
        CreatorRepositoryCallError::Owner(error) => error,
        CreatorRepositoryCallError::Dispatch(error) if error.known_no_effect() => {
            CreatorOutreachError::Storage
        }
        CreatorRepositoryCallError::Dispatch(_) => CreatorOutreachError::CommitUnknown,
    }
}
