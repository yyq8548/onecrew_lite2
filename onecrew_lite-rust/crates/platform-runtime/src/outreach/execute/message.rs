use onecrew_creator_runtime::{CreatorOutreachMessageRecord, CreatorOutreachVerification};
use onecrew_dm_contracts::DmReceipt;

use super::*;
use crate::outreach::recovery::{adopted_message, message_absence_proven, message_verified};

pub(super) async fn execute(
    effects: &dyn OutreachEffects,
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    mut state: CreatorOutreachActionState,
    replayed: bool,
) -> OutreachCompletion {
    let CreatorOutreachContext::Route(context) = action.intent.context() else {
        return completion(
            action,
            &state,
            OutreachPhase::RemoteEffectUnknown,
            replayed,
            "Stored message context is unavailable; no send was attempted.",
        );
    };
    let Some(body) = action.intent.body() else {
        return completion(
            action,
            &state,
            phase_for(&state),
            replayed,
            "The exact original message must be supplied again with the same intent and key before recovery can continue.",
        );
    };
    if state.phase == CreatorOutreachPhase::RecordPending {
        // The newly supplied identical intent is hash-checked before this
        // branch. Its exact body plus durable verified IDs/time are sufficient
        // to finish the local record, even if the account is now disabled.
        let Some(CreatorOutreachVerification::DirectMessage {
            external_message_id,
            provider_thread_id,
            sent_at,
        }) = &state.verification
        else {
            return completion(
                action,
                &state,
                OutreachPhase::RecordPending,
                replayed,
                "Verified message facts are unavailable; recording remains pending and no send was attempted.",
            );
        };
        return record_message(
            creators,
            lease,
            action,
            &state,
            replayed,
            CreatorOutreachMessageRecord {
                external_message_id: external_message_id.clone(),
                provider_thread_id: provider_thread_id.clone(),
                body: body.to_owned(),
                sent_at: *sent_at,
            },
        )
        .await;
    }
    let original_thread = context
        .conversation
        .as_ref()
        .map(|conversation| conversation.provider_thread_id.as_str());
    if state.phase == CreatorOutreachPhase::RemoteStarted {
        let mut adopted = None;
        if replayed {
            let readback = match effects.message_read(context, original_thread).await {
                Ok(readback) => readback,
                Err(error) => {
                    let mut result = completion(
                        action,
                        &state,
                        OutreachPhase::RemoteEffectUnknown,
                        true,
                        "The earlier message has no durable receipt and its thread could not be read. No send was repeated.",
                    );
                    if let OutreachError::DirectMessage(failure) = error {
                        result.failure = Some(failure);
                    }
                    return result;
                }
            };
            if let Some(thread) = &readback.thread {
                adopted = adopted_message(
                    thread,
                    &context.target.external_target_id,
                    body,
                    state.claimed_at,
                );
            }
            if adopted.is_none()
                && !message_absence_proven(
                    &readback,
                    &context.target.external_target_id,
                    body,
                    state.claimed_at,
                )
            {
                return completion(
                    action,
                    &state,
                    OutreachPhase::RemoteEffectUnknown,
                    true,
                    "Thread readback cannot prove absence across the complete claimed window. The earlier message remains unknown and no send was repeated.",
                );
            }
        }
        let receipt = match adopted {
            Some(receipt) => receipt,
            None => match effects.message_write(context, body).await {
                Ok(receipt)
                    if !receipt.external_message_id.trim().is_empty()
                        && receipt
                            .external_thread_id
                            .as_deref()
                            .is_none_or(|id| !id.trim().is_empty()) =>
                {
                    receipt
                }
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
                    let effect = match error.kind() {
                        onecrew_dm_contracts::DmSendFailureKind::Refused => EffectFailure::Refused,
                        onecrew_dm_contracts::DmSendFailureKind::Unknown => EffectFailure::Unknown,
                    };
                    let mut result =
                        effect_failure(creators, lease, action, &state, replayed, effect).await;
                    result.failure = Some(onecrew_dm_contracts::DmFailure {
                        stage: "message_send",
                        diagnostic: error.diagnostic,
                    });
                    return result;
                }
            },
        };
        let durable_receipt = CreatorOutreachReceipt::DirectMessage {
            external_message_id: receipt.external_message_id.clone(),
            provider_thread_id: receipt.external_thread_id.clone(),
        };
        match creators
            .creator_outreach_stage_accepted(lease, &action.intent, durable_receipt)
            .await
        {
            Ok(accepted) => state = accepted,
            Err(_) => {
                let mut result = completion(
                    action,
                    &state,
                    OutreachPhase::RemoteAccepted,
                    replayed,
                    "The message was accepted, but durable receipt staging failed. Keep this key and receipt; readback must precede any recovery.",
                );
                result.receipt = Some(OutreachReceiptReference::Message {
                    external_message_id: receipt.external_message_id,
                    external_thread_id: receipt.external_thread_id,
                });
                return result;
            }
        }
    }
    let Some(CreatorOutreachReceipt::DirectMessage {
        external_message_id,
        provider_thread_id,
    }) = &state.receipt
    else {
        return completion(
            action,
            &state,
            phase_for(&state),
            replayed,
            "The durable message receipt is unavailable; no send was attempted.",
        );
    };
    let receipt = DmReceipt {
        external_message_id: external_message_id.clone(),
        external_thread_id: provider_thread_id.clone(),
    };
    // Accepted receipt IDs authorize readback and a local record. A missing
    // receipt/thread never sends again.
    let thread_id = receipt.external_thread_id.as_deref().or(original_thread);
    let thread = match effects.message_read(context, thread_id).await {
        Ok(readback) => match readback.thread {
            Some(thread) => thread,
            None => {
                return completion(
                    action,
                    &state,
                    phase_for(&state),
                    replayed,
                    "The accepted message thread is unavailable. Resume this same key for readback or recording without sending again.",
                );
            }
        },
        Err(error) => {
            let mut result = completion(
                action,
                &state,
                phase_for(&state),
                replayed,
                "The accepted message could not be read back. Keep the same key; no additional send was attempted.",
            );
            if let OutreachError::DirectMessage(failure) = error {
                result.failure = Some(failure);
            }
            return result;
        }
    };
    if !message_verified(&thread, &receipt, &context.target.external_target_id, body)
        || original_thread.is_some_and(|expected| thread.external_thread_id != expected)
    {
        return completion(
            action,
            &state,
            phase_for(&state),
            replayed,
            "Message readback did not match the stable target, thread, outgoing receipt and exact body. No additional send was attempted.",
        );
    }
    let Some(message) = thread
        .messages
        .iter()
        .find(|message| message.external_message_id == receipt.external_message_id)
    else {
        return completion(
            action,
            &state,
            phase_for(&state),
            replayed,
            "The verified message is unavailable for recording.",
        );
    };
    let record = CreatorOutreachMessageRecord {
        external_message_id: message.external_message_id.clone(),
        provider_thread_id: thread.external_thread_id.clone(),
        body: message.body.clone(),
        sent_at: message.sent_at,
    };
    if state.phase != CreatorOutreachPhase::RecordPending {
        match creators
            .creator_outreach_stage_verified(
                lease,
                &action.intent,
                CreatorOutreachVerification::DirectMessage {
                    external_message_id: record.external_message_id.clone(),
                    provider_thread_id: record.provider_thread_id.clone(),
                    sent_at: record.sent_at,
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
                    "The message is verified, but its durable verification could not be staged. Resume this same key without sending again.",
                );
            }
        }
    }
    record_message(creators, lease, action, &state, replayed, record).await
}

async fn record_message(
    creators: &CreatorRuntime,
    lease: &CreatorOutreachLease,
    action: &PreparedOutreachAction,
    state: &CreatorOutreachActionState,
    replayed: bool,
    record: CreatorOutreachMessageRecord,
) -> OutreachCompletion {
    match creators
        .creator_outreach_record_message(lease, record)
        .await
    {
        Ok(record) => committed(
            action,
            state,
            replayed,
            CreatorOutreachRecord::DirectMessage(record),
        ),
        Err(_) => record_failure(creators, lease, action, state, replayed).await,
    }
}

fn phase_for(state: &CreatorOutreachActionState) -> OutreachPhase {
    if state.phase == CreatorOutreachPhase::RecordPending {
        OutreachPhase::RecordPending
    } else {
        OutreachPhase::RemoteAccepted
    }
}
