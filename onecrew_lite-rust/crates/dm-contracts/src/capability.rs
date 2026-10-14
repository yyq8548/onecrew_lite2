use serde::{Deserialize, Serialize};

/// Whether this operator account may direct-message this target right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmCapabilityState {
    Allowed,
    Unavailable,
    Unknown,
}

/// Which contact the state describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmCapabilityScope {
    ExistingConversation,
    NewMessageRequest,
    Unknown,
}

/// Why a capability is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DmCapabilityReason {
    TargetDmBlocking,
    PlatformDmNotAllowed,
    OperatorPremiumRequired,
    OperatorChatPasscodeMissing,
}

/// One provider's answer about one target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmCapability {
    pub state: DmCapabilityState,
    pub scope: DmCapabilityScope,
    pub reason: Option<DmCapabilityReason>,
    pub detail: Option<String>,
    /// Whether the follow relationship is part of this platform's first-contact
    /// rule. The Creator owner asks for the extra confirmation; the provider is
    /// the one that knows whether following means anything here.
    pub follow_relevant: bool,
}

impl DmCapability {
    /// The provider exposes no capability probe, so nothing is known before the
    /// send is attempted. This is an explicit absence, not a permissive default.
    pub const fn unknown(follow_relevant: bool) -> Self {
        Self {
            state: DmCapabilityState::Unknown,
            scope: DmCapabilityScope::Unknown,
            reason: None,
            detail: None,
            follow_relevant,
        }
    }
}
