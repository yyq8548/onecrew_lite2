use std::{fmt, time::Instant};

use onecrew_creator_runtime::{
    EmailActionResult, EmailBatchActionResult, EmailDraftSendResult, ManualEmailActionResult,
    ReconcileEmailBatchInput, SendEmailDraftInput, SendEmailInput, SendManualEmailInput,
    SendManyEmailInput,
};
use schemars::JsonSchema;
use serde::Serialize;

/// The exact Creator email send one review froze. It is serialized only inside
/// the sealed authority bytes and is never decoded back from a wire payload,
/// so no caller can substitute a different recipient, body, or draft revision.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EmailSendIntent {
    Direct(Box<SendEmailInput>),
    Manual(Box<SendManualEmailInput>),
    Draft(SendEmailDraftInput),
    Batch(Box<SendManyEmailInput>),
    Reconcile(Box<ReconcileEmailBatchInput>),
}

/// Only runtime preparation constructs this authority. Its fields stay private
/// to the email owner, and no wire decoder or `Clone` can mint a second send.
pub(crate) struct PreparedEmailAction {
    pub(super) intent: EmailSendIntent,
    pub(super) idempotency_key: String,
    pub(super) expires: Instant,
}

impl PreparedEmailAction {
    /// Sealed bytes for the store's payload budget and digest. The tag keeps
    /// this authority distinct from every other owner's encoding.
    pub(crate) fn encoded_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&("email_authority_v1", &self.idempotency_key, &self.intent))
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.expires
    }
}

impl fmt::Debug for PreparedEmailAction {
    /// Recipients, subjects, and bodies never reach a log through this type.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEmailAction")
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

/// The finished send, exactly as its own registered operation contract declares
/// it. Untagged, so each operation's registered output schema still validates
/// the result and no second wrapper appears on the wire.
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum EmailCallOutput {
    Direct(Box<EmailActionResult>),
    Manual(Box<ManualEmailActionResult>),
    Draft(Box<EmailDraftSendResult>),
    Batch(Box<EmailBatchActionResult>),
}

impl EmailCallOutput {
    pub(super) fn replayed(&self) -> bool {
        match self {
            Self::Direct(result) => result.replayed,
            Self::Manual(result) => result.replayed,
            Self::Draft(result) => result.replayed,
            Self::Batch(result) => result.replayed,
        }
    }
}
