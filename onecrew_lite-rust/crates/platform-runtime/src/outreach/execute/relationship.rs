use std::time::{SystemTime, UNIX_EPOCH};

use onecrew_creator_runtime::{CreatorOutreachVerification, RelationshipValue};

use super::*;
use crate::outreach::recovery::relationship_satisfied;

pub(super) async fn execute(
    effects: &dyn OutreachEffects,
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    mut state: CreatorOutreachActionState,
    replayed: bool,
) -> OutreachCompletion {
    if state.phase == CreatorOutreachPhase::RecordPending {
        return record(creators, lease, action, &state, replayed).await;
    }
    let CreatorOutreachContext::Route(context) = action.intent.context() else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "Stored relationship context is unavailable; no write was attempted.",
        );
    };
    let Some(mutation) = action.intent.identity().relationship_action else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "Stored relationship action is unavailable; no write was attempted.",
        );
    };
    let mut adopted = None;
    if state.phase == CreatorOutreachPhase::RemoteStarted {
        if replayed {
            // Neither facade currently proves a safe retry contract. Readback
            // may adopt the desired stable-ID state; it never guesses a resend.
            match effects
                .relationship_read(context.operator_account.id.as_str(), &context.target)
                .await
            {
                Ok(observed) if relationship_satisfied(&observed, mutation) => {
                    adopted = Some(observed)
                }
                _ => {
                    return completion(
                        action,
                        &state,
                        OutreachPhase::RemoteEffectUnknown,
                        true,
                        "The earlier relationship effect is unknown and readback does not prove the requested state. No relationship write was repeated.",
                    );
                }
            }
        } else if let Err(error) = effects.relationship_write(context, mutation).await {
            return effect_failure(creators, lease, action, &state, replayed, error).await;
        }
        match creators
            .creator_outreach_stage_accepted(
                lease,
                &action.intent,
                CreatorOutreachReceipt::Relationship,
            )
            .await
        {
            Ok(accepted) => state = accepted,
            Err(_) => {
                let mut result = completion(
                    action,
                    &state,
                    OutreachPhase::RemoteAccepted,
                    replayed,
                    "The relationship request was accepted, but durable receipt staging failed. Keep this key; a retry must read the stable target first.",
                );
                result.receipt = Some(OutreachReceiptReference::Relationship {
                    target_external_id: context.target.external_target_id.clone(),
                });
                return result;
            }
        }
    }
    let readback = match adopted {
        Some(readback) => readback,
        None => match effects
            .relationship_read(context.operator_account.id.as_str(), &context.target)
            .await
        {
            Ok(readback) => readback,
            Err(_) => {
                return completion(
                    action,
                    &state,
                    OutreachPhase::RemoteAccepted,
                    replayed,
                    "The accepted relationship change could not be read back. Resume this key for verification without writing again.",
                );
            }
        },
    };
    if !relationship_satisfied(&readback, mutation) {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteAccepted,
            replayed,
            "The accepted relationship change is not verified by the target's current state. No additional write was attempted.",
        );
    }
    let Ok(checked_at) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteAccepted,
            replayed,
            "Relationship readback cannot be dated; verification is pending.",
        );
    };
    match creators
        .creator_outreach_stage_verified(
            lease,
            &action.intent,
            CreatorOutreachVerification::Relationship {
                following: relationship_value(readback.following),
                can_message: relationship_value(readback.can_message),
                checked_at: checked_at.as_secs(),
            },
        )
        .await
    {
        Ok(verified) => state = verified,
        Err(_) => {
            return completion(
                action,
                &state,
                OutreachPhase::RecordPending,
                replayed,
                "The relationship state is verified, but its durable verification could not be stored. Resume this same key; no repeat write is allowed.",
            );
        }
    }
    record(creators, lease, action, &state, replayed).await
}

async fn record(
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
) -> OutreachCompletion {
    match creators.creator_outreach_record_relationship(lease).await {
        Ok(record) => committed(
            action,
            state,
            replayed,
            CreatorOutreachRecord::Relationship(record),
        ),
        Err(_) => record_failure(creators, lease, action, state, replayed).await,
    }
}

fn relationship_value(value: Option<bool>) -> RelationshipValue {
    match value {
        Some(true) => RelationshipValue::Yes,
        Some(false) => RelationshipValue::No,
        None => RelationshipValue::Unknown,
    }
}
