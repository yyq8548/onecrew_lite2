//! Evidence rules for resuming an already-claimed intent. A bounded read that
//! misses a receipt is never permission to repeat the write.

use onecrew_creator_runtime::RelationshipMutation;
use onecrew_dm_contracts::{DmReadbackCoverage, DmReceipt, DmThread, DmThreadReadback};

use super::effects::{RelationshipReadback, ReplyReadback};

pub(super) fn relationship_satisfied(
    readback: &RelationshipReadback,
    action: RelationshipMutation,
) -> bool {
    match action {
        RelationshipMutation::Follow => readback.following == Some(true),
        RelationshipMutation::Unfollow => {
            readback.following == Some(false) && readback.pending_follow_request != Some(true)
        }
    }
}

/// Adoption requires exactly one matching outgoing message in the claimed
/// time window. Multiple matches cannot identify which write was ours.
pub(super) fn adopted_message(
    thread: &DmThread,
    target_id: &str,
    body: &str,
    claimed_at: u64,
) -> Option<DmReceipt> {
    if thread.peer_external_id != target_id || thread.external_thread_id.trim().is_empty() {
        return None;
    }
    let mut matches = thread.messages.iter().filter(|message| {
        message.outgoing
            && message.body == body
            && message.sent_at >= claimed_at
            && !message.external_message_id.trim().is_empty()
    });
    let message = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(DmReceipt {
        external_message_id: message.external_message_id.clone(),
        external_thread_id: Some(thread.external_thread_id.clone()),
    })
}

pub(super) fn message_verified(
    thread: &DmThread,
    receipt: &DmReceipt,
    target_id: &str,
    body: &str,
) -> bool {
    !receipt.external_message_id.trim().is_empty()
        && !thread.external_thread_id.trim().is_empty()
        && thread.peer_external_id == target_id
        && receipt
            .external_thread_id
            .as_deref()
            .is_none_or(|thread_id| thread.external_thread_id == thread_id)
        && thread.messages.iter().any(|message| {
            message.external_message_id == receipt.external_message_id
                && message.outgoing
                && message.body == body
        })
}

pub(super) fn message_absence_proven(
    readback: &DmThreadReadback,
    target_id: &str,
    body: &str,
    claimed_at: u64,
) -> bool {
    if readback.coverage != DmReadbackCoverage::Complete {
        return false;
    }
    match &readback.thread {
        None => true,
        Some(thread) => {
            thread.peer_external_id == target_id
                && !thread.external_thread_id.trim().is_empty()
                && !thread.messages.iter().any(|message| {
                    message.outgoing && message.body == body && message.sent_at >= claimed_at
                })
        }
    }
}

pub(super) fn reply_verified(
    reply: &ReplyReadback,
    receipt_id: &str,
    parent_id: &str,
    body: &str,
) -> bool {
    !receipt_id.trim().is_empty()
        && reply.id == receipt_id
        && reply.body == body
        && reply.reply_to_id.as_deref() == Some(parent_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_dm_contracts::DmMessage;

    fn thread() -> DmThread {
        DmThread {
            external_thread_id: "thread-1".into(),
            peer_external_id: "target-1".into(),
            messages: vec![DmMessage {
                external_message_id: "message-1".into(),
                body: "reviewed text".into(),
                sent_at: 100,
                outgoing: true,
                provider: serde_json::json!({}),
            }],
        }
    }

    #[test]
    fn adoption_needs_exact_outgoing_body_and_claim_window() {
        assert!(adopted_message(&thread(), "target-1", "reviewed text", 100).is_some());
        assert!(adopted_message(&thread(), "target-1", "reviewed text", 101).is_none());
        assert!(adopted_message(&thread(), "other-target", "reviewed text", 100).is_none());
        assert!(adopted_message(&thread(), "target-1", "changed text", 100).is_none());
        let mut incoming = thread();
        incoming.messages[0].outgoing = false;
        assert!(adopted_message(&incoming, "target-1", "reviewed text", 100).is_none());
        let mut duplicate = thread();
        duplicate.messages.push(duplicate.messages[0].clone());
        assert!(adopted_message(&duplicate, "target-1", "reviewed text", 100).is_none());
    }

    #[test]
    fn accepted_message_receipt_does_not_verify_the_wrong_thread_or_body() {
        let receipt = DmReceipt {
            external_message_id: "message-1".into(),
            external_thread_id: Some("thread-1".into()),
        };
        assert!(message_verified(
            &thread(),
            &receipt,
            "target-1",
            "reviewed text"
        ));
        assert!(!message_verified(
            &thread(),
            &receipt,
            "target-1",
            "other text"
        ));
        let mut wrong = thread();
        wrong.external_thread_id = "thread-2".into();
        assert!(!message_verified(
            &wrong,
            &receipt,
            "target-1",
            "reviewed text"
        ));
    }

    #[test]
    fn unknown_relationship_is_never_the_requested_unfollow_state() {
        let unknown = RelationshipReadback {
            following: None,
            can_message: None,
            pending_follow_request: None,
        };
        assert!(!relationship_satisfied(
            &unknown,
            RelationshipMutation::Unfollow
        ));
    }

    #[test]
    fn a_bounded_empty_or_short_read_never_proves_absence() {
        for thread in [None, Some(thread())] {
            assert!(!message_absence_proven(
                &DmThreadReadback {
                    thread,
                    coverage: DmReadbackCoverage::Incomplete
                },
                "target-1",
                "new body",
                101
            ));
        }
        assert!(message_absence_proven(
            &DmThreadReadback {
                thread: Some(thread()),
                coverage: DmReadbackCoverage::Complete
            },
            "target-1",
            "new body",
            101
        ));
    }
}
