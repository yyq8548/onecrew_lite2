use onecrew_creator_runtime::CreatorOutreachVerification;

use super::*;
use crate::outreach::recovery::reply_verified;

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
    let CreatorOutreachContext::Content(context) = action.intent.context() else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "Stored reply context is unavailable; no write was attempted.",
        );
    };
    let Some(body) = action.intent.body() else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "The frozen reply body is unavailable; no write was attempted.",
        );
    };
    let parent_id = context
        .reply_to_id
        .as_deref()
        .unwrap_or(&context.content_id);
    if state.phase == CreatorOutreachPhase::RemoteStarted {
        if replayed {
            return completion(
                action,
                &state,
                OutreachPhase::RemoteEffectUnknown,
                true,
                "An earlier public reply started without a durable receipt. Its effect is unknown; publishing again is forbidden.",
            );
        }
        let receipt_id = match effects
            .reply_write(context.operator_account.id.as_str(), parent_id, body)
            .await
        {
            Ok(id) if !id.trim().is_empty() => id,
            Ok(_) => {
                return effect_failure(
                    creators,
                    lease,
                    action,
                    &state,
                    replayed,
                    EffectFailure::Unknown,
                )
                .await;
            }
            Err(error) => {
                return effect_failure(creators, lease, action, &state, replayed, error).await;
            }
        };
        match creators
            .creator_outreach_stage_accepted(
                lease,
                &action.intent,
                CreatorOutreachReceipt::ContentReply {
                    external_reply_id: receipt_id.clone(),
                },
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
                    "The public reply was accepted, but its receipt could not be stored. Keep this key and receipt; do not publish it again.",
                );
                result.receipt = Some(OutreachReceiptReference::ContentReply {
                    external_reply_id: receipt_id,
                });
                return result;
            }
        }
    }
    let Some(CreatorOutreachReceipt::ContentReply { external_reply_id }) = &state.receipt else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "The durable reply receipt is unavailable. No publish was attempted.",
        );
    };
    let external_reply_id = external_reply_id.clone();
    match effects
        .reply_read(context.operator_account.id.as_str(), &external_reply_id)
        .await
    {
        Ok(Some(reply)) if reply_verified(&reply, &external_reply_id, parent_id, body) => {}
        _ => {
            return completion(
                action,
                &state,
                OutreachPhase::RemoteAccepted,
                replayed,
                "The accepted reply's ID, parent and exact text could not be verified. Resume this key for readback without publishing again.",
            );
        }
    }
    match creators
        .creator_outreach_stage_verified(
            lease,
            &action.intent,
            CreatorOutreachVerification::ContentReply { external_reply_id },
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
                "The public reply is verified, but durable verification staging failed. Resume this key without publishing again.",
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
    match creators.creator_outreach_record_content_reply(lease).await {
        Ok(record) => committed(
            action,
            state,
            replayed,
            CreatorOutreachRecord::ContentReply(record),
        ),
        Err(_) => record_failure(creators, lease, action, state, replayed).await,
    }
}
