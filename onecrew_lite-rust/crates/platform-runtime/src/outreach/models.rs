use std::{fmt, time::Instant};

use onecrew_creator_runtime::CreatorOutreachRecord;
use schemars::JsonSchema;
use serde::Serialize;

/// The precise recoverable state of a Creator outreach composition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutreachPhase {
    RemoteEffectUnknown,
    RemoteAccepted,
    RecordPending,
    Completed,
    Rejected,
}

/// A receipt reference is evidence of acceptance, not proof of Creator recording.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutreachReceiptReference {
    Relationship {
        target_external_id: String,
    },
    ContentReply {
        external_reply_id: String,
    },
    Message {
        external_message_id: String,
        external_thread_id: Option<String>,
    },
}

/// Completion preserves accepted or verified effects when a later record fails.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct OutreachCompletion {
    pub idempotency_key: String,
    pub intent_fingerprint: String,
    pub phase: OutreachPhase,
    pub creator_recorded: bool,
    pub replayed: bool,
    pub receipt: Option<OutreachReceiptReference>,
    pub recorded: Option<CreatorOutreachRecord>,
    pub guidance: String,
    /// This invocation's safe diagnostics, never used as effect evidence or persisted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<onecrew_dm_contracts::DmFailure>,
}

/// Failures before an effect begins. Once a claim exists, execution returns a
/// typed completion so an adapter cannot mistake a lost result for no send.
#[derive(Debug)]
pub enum OutreachError {
    InvalidInput(&'static str),
    Creator(onecrew_creator_runtime::CreatorOutreachError),
    Target(onecrew_creator_runtime::CreatorTargetResolutionError),
    ProviderRead(&'static str),
    DirectMessage(onecrew_dm_contracts::DmFailure),
    ProviderStateCommitUnknown,
    IdentityMismatch,
    CapabilityUnavailable,
    UnfollowedConfirmationRequired,
    WrongPlatform,
    Encoding,
    Result(crate::ResultStoreError),
}

impl fmt::Display for OutreachError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => formatter.write_str(message),
            Self::Creator(error) => error.fmt(formatter),
            Self::Target(error) => error.fmt(formatter),
            Self::ProviderRead(stage) => write!(formatter, "outreach {stage} read is unavailable"),
            Self::DirectMessage(failure) => write!(formatter, "{}: {}", failure.stage, failure.diagnostic.message),
            Self::ProviderStateCommitUnknown => formatter.write_str("Local provider connection state could not be confirmed; inspect connection status."),
            Self::IdentityMismatch => {
                formatter.write_str("outreach stable target identity changed")
            }
            Self::CapabilityUnavailable => {
                formatter.write_str("direct messaging is unavailable for this target")
            }
            Self::UnfollowedConfirmationRequired => formatter
                .write_str("allow_unfollowed_recipient must be confirmed in a new preparation"),
            Self::WrongPlatform => formatter.write_str("this conversation is on another platform"),
            Self::Encoding => formatter.write_str("outreach intent could not be encoded"),
            Self::Result(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for OutreachError {}

impl From<crate::ResultStoreError> for OutreachError {
    fn from(error: crate::ResultStoreError) -> Self {
        Self::Result(error)
    }
}

impl From<serde_json::Error> for OutreachError {
    fn from(_: serde_json::Error) -> Self {
        Self::Encoding
    }
}

/// Only runtime preparation constructs this authority. Its fields stay private
/// to the outreach owner, and no wire decoder or Clone can mint a second action.
pub(crate) struct PreparedOutreachAction {
    pub(super) intent: onecrew_creator_runtime::PreparedCreatorOutreachIntent,
    pub(super) fingerprint: String,
    pub(super) idempotency_key: String,
    pub(super) expires: Instant,
}

impl PreparedOutreachAction {
    pub(crate) fn encoded_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&(
            "outreach_authority_v1",
            &self.idempotency_key,
            &self.fingerprint,
            &self.intent,
        ))
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.expires
    }
}

impl fmt::Debug for PreparedOutreachAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedOutreachAction")
            .field("fingerprint", &self.fingerprint)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}
