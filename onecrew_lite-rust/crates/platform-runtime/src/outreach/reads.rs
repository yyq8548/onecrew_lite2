//! Existing read-only UI preflight projections. These do not mint authority,
//! claim an action, or authorize a remote write.

use std::time::{SystemTime, UNIX_EPOCH};

use onecrew_creator_runtime::{
    DirectMessageCapability, DirectMessageCapabilityState, DirectMessagePreparation, Platform,
    PrepareDirectMessageInput, PrepareRelationshipActionInput, PreparedCreatorOutreachIntent,
    RelationshipActionPreparation, RelationshipContextInput, RelationshipSnapshot,
    RelationshipValue, TargetKind,
};

use super::{
    OutreachError,
    effects::{
        OutreachEffects, RelationshipReadback,
        unfollowed_confirmation_required as requires_unfollowed_confirmation,
    },
};
use crate::LocalPlatform;

impl LocalPlatform {
    /// Read the existing public-reply preview without provider I/O or authority.
    ///
    /// # Errors
    /// Rejects invalid ownership/content, unavailable accounts or Creators,
    /// and failed admitted Creator reads.
    pub async fn prepare_content_reply(
        &self,
        input: onecrew_creator_runtime::PrepareContentReplyInput,
    ) -> Result<onecrew_creator_runtime::ContentReplyPreparation, OutreachError> {
        let mut preview = self
            .creators
            .creator_content_reply_preview(input)
            .await
            .map_err(OutreachError::Creator)?;
        preview.confirmation_reason = super::copy::text(
            self.presentation_preferences.locale(),
            "Publishing a public reply requires review of the exact frozen action.",
        )
        .into();
        Ok(preview)
    }

    /// Read body-free current or retired outreach facts without provider I/O.
    ///
    /// # Errors
    /// Returns a typed Creator error for absent, invalid, or unreadable durable state.
    pub async fn outreach_action_status(
        &self,
        input: onecrew_creator_runtime::OutreachActionStatusInput,
    ) -> Result<onecrew_creator_runtime::CreatorOutreachStatus, OutreachError> {
        self.creators
            .creator_outreach_status(&input.idempotency_key)
            .await
            .map_err(OutreachError::Creator)?
            .ok_or(OutreachError::Creator(
                onecrew_creator_runtime::CreatorOutreachError::ActionUnavailable,
            ))
    }

    /// Refresh or read the Creator-owned relationship observation for one
    /// saved target. Telegram user relationships are explicitly not applicable.
    ///
    /// # Errors
    /// Rejects unavailable or mismatched context, unsupported targets, failed
    /// provider reads, and failed Creator observation reads or writes.
    pub async fn relationship_get(
        &self,
        input: RelationshipContextInput,
    ) -> Result<RelationshipSnapshot, OutreachError> {
        let (operator, target) = self
            .creators
            .creator_relationship_read_context(input.clone())
            .await
            .map_err(OutreachError::Creator)?;
        if !input.refresh
            && let Some(snapshot) = self
                .creators
                .creator_cached_relationship_snapshot(&operator.id, &target.platform_account_id)
                .await
                .map_err(OutreachError::Creator)?
        {
            return Ok(snapshot);
        }
        let observed = match (target.platform, target.target_kind) {
            (Platform::X | Platform::Instagram, _) => {
                self.relationship_read(operator.id.as_str(), &target)
                    .await?
            }
            (Platform::Telegram, TargetKind::User) => RelationshipReadback {
                following: None,
                can_message: None,
                pending_follow_request: None,
            },
            _ => return Err(OutreachError::WrongPlatform),
        };
        let checked_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| OutreachError::InvalidInput("relationship readback time is unavailable"))?
            .as_secs();
        self.creators
            .creator_record_relationship_observation(
                operator.id,
                target.platform_account_id,
                relationship_value(observed.following),
                relationship_value(observed.can_message),
                checked_at,
            )
            .await
            .map_err(OutreachError::Creator)
    }

    /// Read the existing relationship preview without granting a mutation.
    ///
    /// # Errors
    /// Rejects unavailable context, an inadmissible action, or failed relationship reads.
    pub async fn prepare_relationship_action(
        &self,
        input: PrepareRelationshipActionInput,
    ) -> Result<RelationshipActionPreparation, OutreachError> {
        let context = self
            .creators
            .creator_relationship_context(input.clone())
            .await
            .map_err(OutreachError::Creator)?;
        let intent = PreparedCreatorOutreachIntent::relationship(context.clone(), input.action)
            .map_err(OutreachError::Creator)?;
        self.creators
            .validate_new_outreach_intent(&intent)
            .await
            .map_err(OutreachError::Creator)?;
        let relationship = self
            .relationship_get(RelationshipContextInput {
                creator_id: input.creator_id.clone(),
                resolved_target_id: input.resolved_target_id,
                operator_account_ref_id: input.operator_account_ref_id,
                refresh: true,
            })
            .await?;
        let target = self
            .creators
            .creator_outreach_resolved_target(&context.resolved_target_id)
            .await
            .map_err(OutreachError::Creator)?;
        Ok(RelationshipActionPreparation {
            creator_id: input.creator_id,
            operator_account: context.operator_account,
            target,
            action: input.action,
            relationship,
            confirmation_required: true,
            confirmation_reason: super::copy::text(
                self.presentation_preferences.locale(),
                "Relationship changes require review of the exact frozen action.",
            )
            .into(),
        })
    }

    /// Preserve capability and unfollowed-recipient UX before a body exists.
    /// A subsequent send still prepares and confirms its own immutable intent.
    ///
    /// # Errors
    /// Rejects unavailable Creator context, unsupported targets, failed provider
    /// probes, or unavailable relationship observations.
    pub async fn prepare_direct_message(
        &self,
        input: PrepareDirectMessageInput,
    ) -> Result<DirectMessagePreparation, OutreachError> {
        let context = self
            .creators
            .creator_direct_message_context(input.clone())
            .await
            .map_err(OutreachError::Creator)?;
        let probe = self.message_capability(&context).await?;
        let follow_relevant = probe.follow_relevant;
        let capability = DirectMessageCapability::from(probe);
        let relationship = self
            .relationship_get(RelationshipContextInput {
                creator_id: input.creator_id.clone(),
                resolved_target_id: input.resolved_target_id,
                operator_account_ref_id: input.operator_account_ref_id,
                refresh: true,
            })
            .await?;
        let unavailable = capability.state == DirectMessageCapabilityState::Unavailable;
        let locale = self.presentation_preferences.locale();
        let unfollowed_confirmation_required = !unavailable
            && requires_unfollowed_confirmation(
                follow_relevant,
                relationship.following.value,
                false,
            );
        let mut warnings = Vec::new();
        if capability.state == DirectMessageCapabilityState::Unknown {
            warnings.push(
                super::copy::text(
                    locale,
                    "The platform has not confirmed direct-message capability.",
                )
                .into(),
            );
        }
        if unfollowed_confirmation_required {
            warnings.push(
                super::copy::text(
                    locale,
                    "The operator is not confirmed as following this target.",
                )
                .into(),
            );
        }
        let target = self
            .creators
            .creator_outreach_resolved_target(&context.resolved_target_id)
            .await
            .map_err(OutreachError::Creator)?;
        Ok(DirectMessagePreparation {
            creator_id: input.creator_id,
            operator_account: context.operator_account,
            target,
            relationship,
            capability,
            warnings,
            confirmation_required: !unavailable,
            unfollowed_confirmation_required,
            confirmation_reason: super::copy::text(
                locale,
                if unavailable {
                    "Direct messaging is unavailable for this operator and target."
                } else if unfollowed_confirmation_required {
                    "Sending to an unfollowed recipient requires explicit review."
                } else {
                    "Sending a direct message requires review of the exact frozen message."
                },
            )
            .into(),
        })
    }
}

fn relationship_value(value: Option<bool>) -> RelationshipValue {
    match value {
        Some(true) => RelationshipValue::Yes,
        Some(false) => RelationshipValue::No,
        None => RelationshipValue::Unknown,
    }
}
