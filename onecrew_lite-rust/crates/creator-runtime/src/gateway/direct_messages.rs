use crate::AccountRef;
use async_trait::async_trait;

use onecrew_dm_contracts::{
    DmCapability, DmCapabilityReason, DmCapabilityScope, DmCapabilityState, DmReadError, DmTarget,
    DmThread,
};

/// Reads provider conversation snapshots for Creator synchronization.
///
/// Runtime owns provider selection; this port exposes the thread read needed
/// by Creator without capability probes, message sends, or write readback.
#[async_trait]
pub trait DirectMessageAccess: Send + Sync {
    /// Read one bounded thread for this operator and stable target.
    ///
    /// # Errors
    /// Reports unavailable access or a failed read; failure is not evidence
    /// that the conversation does not exist.
    async fn thread(
        &self,
        account: &AccountRef,
        target: &DmTarget,
        external_thread_id: Option<&str>,
        limit: u16,
    ) -> Result<Option<DmThread>, DmReadError>;
}

/// The provider's answer projected onto the Creator's published capability.
/// Runtime retains `follow_relevant` for write admission; it is not part of
/// the published Creator capability shape.
impl From<DmCapability> for crate::DirectMessageCapability {
    fn from(capability: DmCapability) -> Self {
        Self {
            state: match capability.state {
                DmCapabilityState::Allowed => crate::DirectMessageCapabilityState::Allowed,
                DmCapabilityState::Unavailable => crate::DirectMessageCapabilityState::Unavailable,
                DmCapabilityState::Unknown => crate::DirectMessageCapabilityState::Unknown,
            },
            scope: match capability.scope {
                DmCapabilityScope::ExistingConversation => {
                    crate::DirectMessageCapabilityScope::ExistingConversation
                }
                DmCapabilityScope::NewMessageRequest => {
                    crate::DirectMessageCapabilityScope::NewMessageRequest
                }
                DmCapabilityScope::Unknown => crate::DirectMessageCapabilityScope::Unknown,
            },
            reason_code: capability.reason.map(|reason| match reason {
                DmCapabilityReason::TargetDmBlocking => {
                    crate::DirectMessageCapabilityReasonCode::TargetDmBlocking
                }
                DmCapabilityReason::PlatformDmNotAllowed => {
                    crate::DirectMessageCapabilityReasonCode::PlatformDmNotAllowed
                }
                DmCapabilityReason::OperatorPremiumRequired => {
                    crate::DirectMessageCapabilityReasonCode::OperatorPremiumRequired
                }
                DmCapabilityReason::OperatorChatPasscodeMissing => {
                    crate::DirectMessageCapabilityReasonCode::OperatorChatPasscodeMissing
                }
            }),
            detail: capability.detail,
        }
    }
}
