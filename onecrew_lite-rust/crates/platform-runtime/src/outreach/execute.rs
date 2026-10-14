use std::sync::Arc;

use onecrew_creator_runtime::{
    CreatorOutreachActionState, CreatorOutreachContext, CreatorOutreachLease,
    CreatorOutreachOperation, CreatorOutreachPhase, CreatorOutreachReceipt, CreatorOutreachRecord,
    CreatorRuntime,
};
use onecrew_dm_contracts::DmCapabilityState;

use super::{
    OutreachCompletion, OutreachError, OutreachPhase, OutreachReceiptReference,
    PreparedOutreachAction,
    effects::{EffectFailure, OutreachEffects, unfollowed_confirmation_required},
};
use crate::{LocalPlatform, SubmittedOwnerGrant};

mod message;
mod relationship;
mod reply;
#[cfg(test)]
mod tests;

impl LocalPlatform {
    /// Consume the single-use grant and await the complete write/readback/record
    /// composition inside the caller's admitted Host work future.
    ///
    /// The Host owns this future through its protocol terminal update. This
    /// method creates no detached task and promises no rescue of an arbitrary
    /// directly aborted runtime future.
    ///
    /// # Errors
    /// Rejects a grant for another owner, invalid or conflicting claims, and
    /// unavailable fresh preflight. Failures after a claim is acquired return
    /// the typed completion with the known remote facts and recovery guidance.
    pub async fn execute_outreach_submitted(
        self: &Arc<Self>,
        grant: SubmittedOwnerGrant,
    ) -> Result<OutreachCompletion, OutreachError> {
        let action = grant.into_outreach()?;
        let owner = Arc::clone(self);
        execute_action(owner.as_ref(), Arc::clone(&owner.creators), action).await
    }
}

/// One process's lease covers the entire composition. Creator's blocking
/// segments retain clones of the same lease until the real repository work ends.
pub(super) async fn execute_action(
    effects: &dyn OutreachEffects,
    creators: Arc<CreatorRuntime>,
    action: PreparedOutreachAction,
) -> Result<OutreachCompletion, OutreachError> {
    let lease = creators
        .acquire_creator_outreach(&action.idempotency_key)
        .map_err(OutreachError::Creator)?;
    if creators
        .creator_outreach_read(&action.idempotency_key, &action.intent)
        .await
        .map_err(OutreachError::Creator)?
        .is_none()
    {
        creators
            .validate_new_outreach_intent(&action.intent)
            .await
            .map_err(OutreachError::Creator)?;
        preflight(effects, &action).await?;
    }
    let claim = creators
        .creator_outreach_claim(&lease, &action.intent)
        .await
        .map_err(OutreachError::Creator)?;
    let state = claim.state;
    if state.phase == CreatorOutreachPhase::Completed {
        return Ok(recorded_completion(&creators, &lease, &action, &state, true).await);
    }
    if state.phase == CreatorOutreachPhase::Refused {
        return Ok(completion(
            &action,
            &state,
            OutreachPhase::Rejected,
            true,
            "The provider refused this action; this idempotency key will not send again.",
        ));
    }
    let replayed = !claim.is_new;
    Ok(match action.intent.operation() {
        CreatorOutreachOperation::RelationshipUpdate => {
            relationship::execute(effects, &creators, &lease, &action, state, replayed).await
        }
        CreatorOutreachOperation::ContentReplySend => {
            reply::execute(effects, &creators, &lease, &action, state, replayed).await
        }
        CreatorOutreachOperation::DirectMessageSend => {
            message::execute(effects, &creators, &lease, &action, state, replayed).await
        }
    })
}

async fn preflight(
    effects: &dyn OutreachEffects,
    action: &PreparedOutreachAction,
) -> Result<(), OutreachError> {
    match action.intent.context() {
        CreatorOutreachContext::Route(context) => match action.intent.operation() {
            CreatorOutreachOperation::RelationshipUpdate => {
                effects
                    .relationship_read(context.operator_account.id.as_str(), &context.target)
                    .await?;
            }
            CreatorOutreachOperation::DirectMessageSend => {
                let capability = effects.message_capability(context).await?;
                if capability.state == DmCapabilityState::Unavailable {
                    return Err(OutreachError::CapabilityUnavailable);
                }
                if context.conversation.is_none() && capability.follow_relevant {
                    let read = effects
                        .relationship_read(context.operator_account.id.as_str(), &context.target)
                        .await?;
                    if unfollowed_confirmation_required(
                        capability.follow_relevant,
                        match read.following {
                            Some(true) => onecrew_creator_runtime::RelationshipValue::Yes,
                            Some(false) => onecrew_creator_runtime::RelationshipValue::No,
                            None => onecrew_creator_runtime::RelationshipValue::Unknown,
                        },
                        action.intent.identity().allow_unfollowed_recipient,
                    ) {
                        return Err(OutreachError::UnfollowedConfirmationRequired);
                    }
                }
            }
            CreatorOutreachOperation::ContentReplySend => {
                return Err(OutreachError::IdentityMismatch);
            }
        },
        CreatorOutreachContext::Content(context) => {
            let content = effects
                .reply_read(context.operator_account.id.as_str(), &context.content_id)
                .await?
                .ok_or(OutreachError::InvalidInput("reply content is unavailable"))?;
            if content.id != context.content_id
                || content.author_external_id != context.target.external_target_id
            {
                return Err(OutreachError::IdentityMismatch);
            }
        }
    }
    Ok(())
}

fn completion(
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    phase: OutreachPhase,
    replayed: bool,
    guidance: &str,
) -> OutreachCompletion {
    OutreachCompletion {
        idempotency_key: action.idempotency_key.clone(),
        intent_fingerprint: action.fingerprint.clone(),
        phase,
        creator_recorded: false,
        replayed,
        receipt: state
            .receipt
            .as_ref()
            .map(|receipt| receipt_reference(receipt, &state.identity.external_target_id)),
        recorded: None,
        guidance: guidance.into(),
        failure: None,
    }
}

fn committed(
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
    record: CreatorOutreachRecord,
) -> OutreachCompletion {
    let mut result = completion(
        action,
        state,
        OutreachPhase::Completed,
        replayed,
        "The remote effect is verified and Creator records are committed.",
    );
    result.creator_recorded = true;
    result.recorded = Some(record);
    result
}

fn receipt_reference(
    receipt: &CreatorOutreachReceipt,
    target_id: &str,
) -> OutreachReceiptReference {
    match receipt {
        CreatorOutreachReceipt::Relationship => OutreachReceiptReference::Relationship {
            target_external_id: target_id.to_owned(),
        },
        CreatorOutreachReceipt::ContentReply { external_reply_id } => {
            OutreachReceiptReference::ContentReply {
                external_reply_id: external_reply_id.clone(),
            }
        }
        CreatorOutreachReceipt::DirectMessage {
            external_message_id,
            provider_thread_id,
        } => OutreachReceiptReference::Message {
            external_message_id: external_message_id.clone(),
            external_thread_id: provider_thread_id.clone(),
        },
    }
}

async fn recorded_completion(
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
) -> OutreachCompletion {
    match creators.creator_outreach_recorded(lease).await {
        Ok(record) => {
            let mut result = completion(
                action,
                state,
                OutreachPhase::Completed,
                replayed,
                "The remote effect is verified and Creator records are committed.",
            );
            result.creator_recorded = true;
            result.recorded = Some(record);
            result
        }
        Err(_) => {
            let mut result = completion(
                action,
                state,
                OutreachPhase::Completed,
                replayed,
                "Creator recording completed; its safe record projection is temporarily unavailable. Read status with the same key.",
            );
            result.creator_recorded = true;
            result
        }
    }
}

async fn effect_failure(
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
    failure: EffectFailure,
) -> OutreachCompletion {
    match failure {
        EffectFailure::Unknown => completion(
            action,
            state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "The remote effect is unknown. Keep this key and inspect or resume this exact intent; do not send with a new key.",
        ),
        EffectFailure::Refused => {
            let stored = creators
                .creator_outreach_refused(lease, &action.intent)
                .await;
            completion(
                action,
                stored.as_ref().unwrap_or(state),
                OutreachPhase::Rejected,
                replayed,
                if stored.is_ok() {
                    "The provider refused this action before an accepted effect."
                } else {
                    "The provider refused this action, but recording the refusal failed. Keep this key and read its durable status."
                },
            )
        }
    }
}

async fn record_failure(
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
) -> OutreachCompletion {
    if let Ok(Some(durable)) = creators
        .creator_outreach_read(&action.idempotency_key, &action.intent)
        .await
        && durable.phase == CreatorOutreachPhase::Completed
    {
        return recorded_completion(creators, lease, action, &durable, replayed).await;
    }
    completion(
        action,
        state,
        OutreachPhase::RecordPending,
        replayed,
        "The remote effect is verified; Creator recording is pending. Retry this exact intent with the same key to finish recording without sending again.",
    )
}
