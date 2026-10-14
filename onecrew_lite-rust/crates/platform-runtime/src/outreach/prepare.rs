use std::time::{Duration, Instant};

use onecrew_creator_runtime::{
    Platform, PrepareContentReplyInput, PrepareDirectMessageInput, PrepareRelationshipActionInput,
    PreparedCreatorOutreachIntent, SendContentReplyInput, SendConversationReplyInput,
    SendDirectMessageInput, UpdateRelationshipInput,
};
use onecrew_dm_contracts::DmCapabilityState;

use super::{
    OutreachError, PreparedOutreachAction,
    effects::{OutreachEffects, unfollowed_confirmation_required},
};
use crate::{
    LocalPlatform, OwnerReviewContext, PreparedOwnerReview, PresentationField,
    PresentationProjection, PresentationSection, ResultPresentationKind,
};

const PREPARE_LIFETIME: Duration = Duration::from_secs(15 * 60);

impl LocalPlatform {
    /// Freeze one relationship mutation after a fresh identity-checked read.
    /// No claim or remote write occurs until submitted owner authority is consumed.
    ///
    /// # Errors
    /// Rejects unavailable or mismatched Creator context, failed live preflight,
    /// conflicting intent identity, and invalid or oversized review metadata.
    pub async fn prepare_outreach_relationship(
        &self,
        input: UpdateRelationshipInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, OutreachError> {
        let context = self
            .creators
            .creator_relationship_context(PrepareRelationshipActionInput {
                creator_id: input.creator_id,
                contact_route_id: input.contact_route_id,
                resolved_target_id: input.resolved_target_id,
                operator_account_ref_id: input.operator_account_ref_id,
                action: input.action,
            })
            .await
            .map_err(OutreachError::Creator)?;
        let intent = PreparedCreatorOutreachIntent::relationship(context.clone(), input.action)
            .map_err(OutreachError::Creator)?;
        if self
            .creators
            .creator_outreach_read(&input.idempotency_key, &intent)
            .await
            .map_err(OutreachError::Creator)?
            .is_none()
        {
            self.creators
                .validate_new_outreach_intent(&intent)
                .await
                .map_err(OutreachError::Creator)?;
            self.relationship_read(context.operator_account.id.as_str(), &context.target)
                .await?;
        }
        let fields = vec![
            field("Creator", context.creator_display_name),
            field("Operator", context.operator_account.label),
            field("Platform", context.target.platform.as_str()),
            field("Stable target ID", context.target.external_target_id),
            field("Action", input.action.as_str()),
        ];
        seal(
            intent,
            input.idempotency_key,
            review_context,
            "Review relationship change",
            fields,
        )
    }

    /// Freeze the normalized public reply and its canonical Creator context.
    ///
    /// # Errors
    /// Rejects invalid text or Creator ownership, unavailable or mismatched
    /// content/parent reads, conflicting intent identity, and invalid review data.
    pub async fn prepare_outreach_content_reply(
        &self,
        input: SendContentReplyInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, OutreachError> {
        let context = self
            .creators
            .creator_content_reply_context(PrepareContentReplyInput {
                creator_id: input.creator_id,
                target_platform_account_id: input.target_platform_account_id,
                operator_account_ref_id: input.operator_account_ref_id,
                content_id: input.content_id,
                reply_to_id: input.reply_to_id,
            })
            .await
            .map_err(OutreachError::Creator)?;
        let intent = PreparedCreatorOutreachIntent::content_reply(context.clone(), input.body)
            .map_err(OutreachError::Creator)?;
        if self
            .creators
            .creator_outreach_read(&input.idempotency_key, &intent)
            .await
            .map_err(OutreachError::Creator)?
            .is_none()
        {
            self.creators
                .validate_new_outreach_intent(&intent)
                .await
                .map_err(OutreachError::Creator)?;
            let content = self
                .reply_read(context.operator_account.id.as_str(), &context.content_id)
                .await?
                .ok_or(OutreachError::InvalidInput("reply content is unavailable"))?;
            if content.id != context.content_id
                || content.author_external_id != context.target.external_target_id
            {
                return Err(OutreachError::IdentityMismatch);
            }
            if let Some(reply_to_id) = &context.reply_to_id {
                let parent = self
                    .reply_read(context.operator_account.id.as_str(), reply_to_id)
                    .await?
                    .ok_or(OutreachError::InvalidInput("reply parent is unavailable"))?;
                if parent.id != *reply_to_id
                    || (parent.id != content.id
                        && parent.conversation_id.as_deref()
                            != Some(content.conversation_id.as_deref().unwrap_or(&content.id)))
                {
                    return Err(OutreachError::IdentityMismatch);
                }
            }
        }
        let fields = vec![
            field("Creator", context.creator_display_name),
            field("Operator", context.operator_account.label),
            field("Content ID", context.content_id.clone()),
            field(
                "Reply to",
                context.reply_to_id.unwrap_or(context.content_id),
            ),
            field(
                "Reply",
                intent
                    .body()
                    .ok_or(OutreachError::InvalidInput("reply body is required"))?,
            ),
        ];
        seal(
            intent,
            input.idempotency_key,
            review_context,
            "Review public reply",
            fields,
        )
    }

    /// Freeze the first-contact permission together with the exact message.
    ///
    /// # Errors
    /// Rejects invalid context/text, an unavailable capability, missing explicit
    /// permission for an unfollowed target, or invalid/oversized review data.
    pub async fn prepare_outreach_direct_message(
        &self,
        input: SendDirectMessageInput,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, OutreachError> {
        let context = self
            .creators
            .creator_direct_message_context(PrepareDirectMessageInput {
                creator_id: input.creator_id,
                contact_route_id: input.contact_route_id,
                resolved_target_id: input.resolved_target_id,
                operator_account_ref_id: input.operator_account_ref_id,
            })
            .await
            .map_err(OutreachError::Creator)?;
        let intent = PreparedCreatorOutreachIntent::direct_message(
            context.clone(),
            input.body,
            input.allow_unfollowed_recipient,
        )
        .map_err(OutreachError::Creator)?;
        if self
            .creators
            .creator_outreach_read(&input.idempotency_key, &intent)
            .await
            .map_err(OutreachError::Creator)?
            .is_none()
        {
            self.creators
                .validate_new_outreach_intent(&intent)
                .await
                .map_err(OutreachError::Creator)?;
            let capability = self.message_capability(&context).await?;
            if capability.state == DmCapabilityState::Unavailable {
                return Err(OutreachError::CapabilityUnavailable);
            }
            if capability.follow_relevant {
                let relationship = self
                    .relationship_read(context.operator_account.id.as_str(), &context.target)
                    .await?;
                if unfollowed_confirmation_required(
                    capability.follow_relevant,
                    relationship_value(relationship.following),
                    input.allow_unfollowed_recipient,
                ) {
                    return Err(OutreachError::UnfollowedConfirmationRequired);
                }
            }
        }
        let fields = vec![
            field("Creator", context.creator_display_name),
            field("Operator", context.operator_account.label),
            field("Platform", context.target.platform.as_str()),
            field("Stable target ID", context.target.external_target_id),
            field(
                "Message",
                intent
                    .body()
                    .ok_or(OutreachError::InvalidInput("message body is required"))?,
            ),
            field(
                "Allow unfollowed recipient",
                input.allow_unfollowed_recipient.to_string(),
            ),
        ];
        seal(
            intent,
            input.idempotency_key,
            review_context,
            "Review direct message",
            fields,
        )
    }

    /// A conversation reply derives its entire address from the saved thread.
    /// It has the same Creator record semantics as a first-contact message.
    ///
    /// # Errors
    /// Rejects absent or wrongly scoped conversations, invalid text, unavailable
    /// capability, conflicting intent identity, and invalid review data.
    pub async fn prepare_outreach_conversation_reply(
        &self,
        input: SendConversationReplyInput,
        expected_platform: Option<Platform>,
        review_context: OwnerReviewContext,
    ) -> Result<PreparedOwnerReview, OutreachError> {
        let context = self
            .creators
            .creator_conversation_reply_context(input.conversation_id)
            .await
            .map_err(OutreachError::Creator)?;
        if expected_platform.is_some_and(|expected| context.target.platform != expected) {
            return Err(OutreachError::WrongPlatform);
        }
        let intent =
            PreparedCreatorOutreachIntent::direct_message(context.clone(), input.body, false)
                .map_err(OutreachError::Creator)?;
        if self
            .creators
            .creator_outreach_read(&input.idempotency_key, &intent)
            .await
            .map_err(OutreachError::Creator)?
            .is_none()
        {
            self.creators
                .validate_new_outreach_intent(&intent)
                .await
                .map_err(OutreachError::Creator)?;
            if self.message_capability(&context).await?.state == DmCapabilityState::Unavailable {
                return Err(OutreachError::CapabilityUnavailable);
            }
        }
        let conversation = context
            .conversation
            .as_ref()
            .ok_or(OutreachError::InvalidInput(
                "conversation context is required",
            ))?;
        let fields = vec![
            field("Creator", context.creator_display_name),
            field("Operator", context.operator_account.label),
            field("Conversation", conversation.conversation_id.as_str()),
            field("Provider thread", &conversation.provider_thread_id),
            field(
                "Message",
                intent
                    .body()
                    .ok_or(OutreachError::InvalidInput("message body is required"))?,
            ),
        ];
        seal(
            intent,
            input.idempotency_key,
            review_context,
            "Review conversation reply",
            fields,
        )
    }
}

fn relationship_value(value: Option<bool>) -> onecrew_creator_runtime::RelationshipValue {
    match value {
        Some(true) => onecrew_creator_runtime::RelationshipValue::Yes,
        Some(false) => onecrew_creator_runtime::RelationshipValue::No,
        None => onecrew_creator_runtime::RelationshipValue::Unknown,
    }
}

fn field(label: &str, value: impl Into<String>) -> PresentationField {
    PresentationField {
        label: label.to_owned(),
        value: value.into(),
    }
}

fn seal(
    intent: PreparedCreatorOutreachIntent,
    idempotency_key: String,
    context: OwnerReviewContext,
    title: &str,
    fields: Vec<PresentationField>,
) -> Result<PreparedOwnerReview, OutreachError> {
    if idempotency_key.trim().is_empty() || idempotency_key.len() > 256 {
        return Err(OutreachError::InvalidInput(
            "idempotency_key must contain 1 to 256 bytes",
        ));
    }
    let fingerprint = intent.fingerprint().to_owned();
    Ok(PreparedOwnerReview::outreach(
        PreparedOutreachAction {
            intent,
            fingerprint,
            idempotency_key,
            expires: Instant::now() + PREPARE_LIFETIME,
        },
        context,
        PresentationProjection {
            kind: ResultPresentationKind::new("outreach_confirmation")?,
            title: title.to_owned(),
            summary: Some(
                "Confirm the exact operator, stable target and content shown below.".into(),
            ),
            sections: vec![PresentationSection::Fields {
                heading: None,
                fields: fields.into_iter().flat_map(review_fields).collect(),
            }],
        },
        "Confirm this external action?".into(),
        "Confirm action".into(),
    )?)
}

fn review_fields(field: PresentationField) -> Vec<PresentationField> {
    // A long body is reviewed in full. Splitting at UTF-8 boundaries preserves
    // every byte while retaining the result owner's per-field display budget.
    const FIELD_BYTES: usize = 8 * 1024;
    if field.value.len() <= FIELD_BYTES {
        return vec![field];
    }
    let mut remaining = field.value.as_str();
    let mut result = Vec::new();
    while !remaining.is_empty() {
        let mut split = remaining.len().min(FIELD_BYTES);
        while !remaining.is_char_boundary(split) {
            split -= 1;
        }
        result.push(PresentationField {
            label: format!("{} (part {})", field.label, result.len() + 1),
            value: remaining[..split].to_owned(),
        });
        remaining = &remaining[split..];
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_review_text_preserves_every_unicode_byte() {
        let body = "消息正文🙂".repeat(1400);
        let fields = review_fields(field("Message", &body));
        assert!(fields.len() > 1);
        assert!(fields.iter().all(|field| field.value.len() <= 8192));
        assert_eq!(
            fields
                .iter()
                .map(|field| field.value.as_str())
                .collect::<String>(),
            body
        );
    }
}
