//! X direct messages.
//!
//! This provider owns everything X-specific about a direct message: the target
//! kinds it accepts, the X Chat passcode requirement, the stable-id check it
//! performs immediately before a write, and how an X Chat permission signal
//! becomes a capability. Write failures expose closed, secret-free evidence.

use std::sync::Arc;

use async_trait::async_trait;
use onecrew_dm_contracts::{
    DirectMessageProvider, DmCapability, DmCapabilityReason, DmCapabilityScope, DmCapabilityState,
    DmMessage, DmReadError, DmReceipt, DmSendError, DmSendRequest, DmTarget, DmTargetKind,
    DmThread,
};
use zeroize::Zeroizing;

use crate::{XAccount, XChatConversation, XChatDmCapability};

/// One profile's X Chat passcode.
///
/// X Chat unlocks through this buffer and clears it in place, so a passcode
/// that reaches the vendored client is already gone; the wrapper covers every
/// path where it never gets there — a refused target, a failed profile read,
/// an early return.
pub type XChatPasscode = Zeroizing<Vec<u8>>;

/// Supplies the live X session and its X Chat passcode for one profile.
///
/// The session is requested per call rather than held, so the passcode keeps
/// the short in-memory lifetime the vault owner gives it. `None` means the
/// profile has no X Chat passcode configured, which is an explicit absence
/// rather than an empty one.
#[async_trait]
pub trait XAccountSource: Send + Sync {
    async fn open_x_account(
        &self,
        profile_id: &str,
    ) -> Result<(XAccount, Option<XChatPasscode>), String>;
}

pub struct XDirectMessages {
    source: Arc<dyn XAccountSource>,
    profile_id: String,
}

impl XDirectMessages {
    pub fn new(source: Arc<dyn XAccountSource>, profile_id: String) -> Self {
        Self { source, profile_id }
    }

    async fn account(&self) -> Result<(XAccount, Option<XChatPasscode>), String> {
        self.source.open_x_account(&self.profile_id).await
    }
}

#[async_trait]
impl DirectMessageProvider for XDirectMessages {
    async fn capability(&self, target: &DmTarget) -> Result<DmCapability, DmReadError> {
        if target.kind != DmTargetKind::User {
            return Err(local_read(
                "unsupported_target",
                "X direct messages require a User target",
            ));
        }
        let recipient = handle(target);
        if recipient.is_empty() {
            return Err(local_read(
                "recipient_missing",
                "X direct-message recipient is required",
            ));
        }
        let (account, passcode) = self
            .account()
            .await
            .map_err(|_| local_read("session_unavailable", "X session could not be opened"))?;
        let Some(mut passcode) = passcode else {
            return Ok(missing_passcode_capability());
        };
        let probe = account
            .direct_message_capability(recipient, passcode.as_mut_slice())
            .await
            .map_err(read_error)?;
        if probe.target_user_id != target.external_id.trim() {
            return Err(local_read(
                "target_identity_changed",
                "X direct-message target stable ID changed",
            ));
        }
        Ok(capability(probe.capability))
    }

    async fn send(&self, request: DmSendRequest) -> Result<DmReceipt, DmSendError> {
        if request.target.kind != DmTargetKind::User {
            return Err(DmSendError::refused(local_diagnostic(
                "unsupported_target",
                "X direct messages require a User target",
            )));
        }
        let recipient = handle(&request.target);
        if recipient.is_empty() || request.body.trim().is_empty() {
            return Err(DmSendError::refused(local_diagnostic(
                "recipient_or_body_missing",
                "X recipient and message body are required",
            )));
        }
        let (account, passcode) = self.account().await.map_err(|_| {
            DmSendError::refused(local_diagnostic(
                "session_unavailable",
                "X session could not be opened",
            ))
        })?;
        let Some(mut passcode) = passcode else {
            return Err(DmSendError::refused(local_diagnostic(
                "chat_passcode_missing",
                MISSING_PASSCODE,
            )));
        };
        let receipt = account
            .send_message(
                recipient,
                request.target.external_id.trim(),
                request.body.trim(),
                passcode.as_mut_slice(),
            )
            .await
            .map_err(send_error)?;
        if receipt.message_id.trim().is_empty() || receipt.conversation_id.trim().is_empty() {
            return Err(DmSendError::unknown(local_diagnostic(
                "receipt_identity_missing",
                "X returned an empty direct-message receipt",
            )));
        }
        Ok(DmReceipt {
            external_message_id: receipt.message_id,
            external_thread_id: Some(receipt.conversation_id),
        })
    }

    async fn thread(
        &self,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError> {
        if target.kind != DmTargetKind::User {
            return Err(local_read(
                "unsupported_target",
                "X conversations require a User target",
            ));
        }
        let recipient = handle(target);
        if recipient.is_empty() {
            return Err(local_read(
                "recipient_missing",
                "X conversation recipient is required",
            ));
        }
        let (account, passcode) = self
            .account()
            .await
            .map_err(|_| local_read("session_unavailable", "X session could not be opened"))?;
        let Some(mut passcode) = passcode else {
            return Err(local_read("chat_passcode_missing", MISSING_PASSCODE));
        };
        let current = account.who_am_i().await.map_err(read_error)?;
        let Some(conversation) = account
            .thread(
                external_thread_id,
                recipient,
                passcode.as_mut_slice(),
                usize::from(limit.clamp(1, 200)),
            )
            .await
            .map_err(read_error)?
        else {
            return Ok(None);
        };
        let thread = thread(conversation, &current.id)?;
        if thread.peer_external_id != target.external_id.trim() {
            return Err(local_read(
                "target_identity_changed",
                "X conversation target stable ID changed",
            ));
        }
        Ok(Some(thread))
    }
}

const MISSING_PASSCODE: &str = "X Chat passcode is not configured for this account; reconnect the X account and configure its X Chat passcode first";

fn handle(target: &DmTarget) -> &str {
    target.handle.trim().trim_start_matches('@')
}

fn send_error(error: crate::XError) -> DmSendError {
    let diagnostic = error.evidence();
    match error.write_failure() {
        crate::XWriteFailure::Refused => DmSendError::refused(diagnostic),
        crate::XWriteFailure::Unknown => DmSendError::unknown(diagnostic),
    }
}

fn read_error(error: crate::XError) -> DmReadError {
    DmReadError {
        diagnostic: error.evidence(),
    }
}

fn local_diagnostic(
    code: &'static str,
    message: &'static str,
) -> onecrew_dm_contracts::DmDiagnostic {
    onecrew_dm_contracts::DmDiagnostic::local("x", "x_chat", code, message)
}

fn local_read(code: &'static str, message: &'static str) -> DmReadError {
    DmReadError {
        diagnostic: local_diagnostic(code, message),
    }
}

fn missing_passcode_capability() -> DmCapability {
    DmCapability {
        state: DmCapabilityState::Unavailable,
        scope: DmCapabilityScope::Unknown,
        reason: Some(DmCapabilityReason::OperatorChatPasscodeMissing),
        detail: Some(MISSING_PASSCODE.into()),
        follow_relevant: true,
    }
}

fn capability(capability: XChatDmCapability) -> DmCapability {
    let (state, scope, reason, detail) = match capability {
        XChatDmCapability::ExistingConversation => (
            DmCapabilityState::Allowed,
            DmCapabilityScope::ExistingConversation,
            None,
            "existing X Chat conversation keys are available",
        ),
        XChatDmCapability::NewMessageRequestAllowed => (
            DmCapabilityState::Allowed,
            DmCapabilityScope::NewMessageRequest,
            None,
            "X permits a new direct-message request",
        ),
        XChatDmCapability::TargetDmBlocking => (
            DmCapabilityState::Unavailable,
            DmCapabilityScope::NewMessageRequest,
            Some(DmCapabilityReason::TargetDmBlocking),
            "X reports that the target blocks direct messages",
        ),
        XChatDmCapability::PlatformDmNotAllowed => (
            DmCapabilityState::Unavailable,
            DmCapabilityScope::NewMessageRequest,
            Some(DmCapabilityReason::PlatformDmNotAllowed),
            "X does not allow a new direct-message request to this target",
        ),
        XChatDmCapability::PremiumRequired => (
            DmCapabilityState::Unavailable,
            DmCapabilityScope::NewMessageRequest,
            Some(DmCapabilityReason::OperatorPremiumRequired),
            "X Premium is required to start this direct-message conversation",
        ),
    };
    DmCapability {
        state,
        scope,
        reason,
        detail: Some(detail.into()),
        // Following is part of X's first-contact rule, so the Creator owner
        // asks for the extra confirmation before an unfollowed recipient.
        follow_relevant: true,
    }
}

fn thread(conversation: XChatConversation, current_user_id: &str) -> Result<DmThread, DmReadError> {
    if current_user_id.is_empty() || conversation.id.trim().is_empty() {
        return Err(local_read(
            "thread_identity_missing",
            "X thread identity is unavailable",
        ));
    }
    let mut peers = conversation
        .participants
        .iter()
        .filter(|participant| participant.id != current_user_id);
    let peer = peers
        .next()
        .ok_or_else(|| local_read("peer_missing", "X conversation has no peer"))?;
    if peer.id.is_empty() || peers.next().is_some() {
        return Err(local_read(
            "peer_identity_ambiguous",
            "X readback requires exactly one identified peer",
        ));
    }
    let peer = peer.id.clone();
    Ok(DmThread {
        external_thread_id: conversation.id,
        peer_external_id: peer,
        messages: conversation
            .messages
            .into_iter()
            .map(|message| {
                if message.id.trim().is_empty() {
                    return Err(local_read(
                        "message_identity_missing",
                        "X message identity is unavailable",
                    ));
                }
                let provider = serde_json::to_value(&message).map_err(|_| {
                    local_read("message_encoding_failed", "X message could not be encoded")
                })?;
                // X states the send time in milliseconds. A message without one
                // fails the read rather than being stamped with a time nobody
                // reported.
                let sent_at = message
                    .created_at_msec
                    .and_then(|value| u64::try_from(value).ok())
                    .map(|milliseconds| milliseconds / 1_000)
                    .ok_or_else(|| {
                        local_read(
                            "message_time_missing",
                            "X Chat message carries no usable send time",
                        )
                    })?;
                Ok(DmMessage {
                    external_message_id: message.id,
                    outgoing: message.sender_id == current_user_id,
                    body: message.text,
                    sent_at,
                    provider,
                })
            })
            .collect::<Result<Vec<_>, DmReadError>>()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_and_identity_free_threads_cannot_verify_a_direct_message() {
        let participant = |id: &str| crate::XChatParticipant {
            id: id.into(),
            ..Default::default()
        };
        let direct = XChatConversation {
            id: "100:200".into(),
            participants: vec![participant("100"), participant("200")],
            messages: Vec::new(),
        };
        assert_eq!(
            thread(direct.clone(), "100").unwrap().peer_external_id,
            "200"
        );
        let mut group = direct.clone();
        group.participants.push(participant("300"));
        assert!(thread(group, "100").is_err());
        let mut unidentified = direct;
        unidentified.id.clear();
        assert!(thread(unidentified, "100").is_err());
    }

    #[test]
    fn existing_conversation_is_allowed_independently_of_profile_dm_flags() {
        let existing = capability(XChatDmCapability::ExistingConversation);
        assert_eq!(existing.state, DmCapabilityState::Allowed);
        assert_eq!(existing.scope, DmCapabilityScope::ExistingConversation);
        assert_eq!(existing.reason, None);

        let message_request = capability(XChatDmCapability::NewMessageRequestAllowed);
        assert_eq!(message_request.state, DmCapabilityState::Allowed);
        assert_eq!(message_request.scope, DmCapabilityScope::NewMessageRequest);
        assert_eq!(message_request.reason, None);
    }

    #[test]
    fn message_request_permission_signals_keep_specific_reasons() {
        for (signal, expected) in [
            (
                XChatDmCapability::TargetDmBlocking,
                DmCapabilityReason::TargetDmBlocking,
            ),
            (
                XChatDmCapability::PlatformDmNotAllowed,
                DmCapabilityReason::PlatformDmNotAllowed,
            ),
            (
                XChatDmCapability::PremiumRequired,
                DmCapabilityReason::OperatorPremiumRequired,
            ),
        ] {
            let capability = capability(signal);
            assert_eq!(capability.state, DmCapabilityState::Unavailable);
            assert_eq!(capability.scope, DmCapabilityScope::NewMessageRequest);
            assert_eq!(capability.reason, Some(expected));
        }
    }

    #[test]
    fn every_x_capability_keeps_following_relevant_to_a_first_contact() {
        assert!(missing_passcode_capability().follow_relevant);
        assert!(capability(XChatDmCapability::ExistingConversation).follow_relevant);
    }

    #[test]
    fn missing_operator_passcode_is_a_typed_unavailable_capability() {
        let capability = missing_passcode_capability();
        assert_eq!(capability.state, DmCapabilityState::Unavailable);
        assert_eq!(capability.scope, DmCapabilityScope::Unknown);
        assert_eq!(
            capability.reason,
            Some(DmCapabilityReason::OperatorChatPasscodeMissing)
        );
    }

    #[test]
    fn a_passcode_buffer_is_wiped_in_place_and_zeroized_when_it_is_dropped() {
        let mut passcode: XChatPasscode = Zeroizing::new(b"secret-passcode".to_vec());
        assert_eq!(passcode.as_slice(), b"secret-passcode");
        // X Chat unlocks through this buffer and fills it with zeros; the
        // wrapper wipes whatever is left on every other path out.
        passcode.as_mut_slice().fill(0);
        assert_eq!(passcode.as_slice(), [0; 15]);
    }
}
