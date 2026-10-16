use std::{fmt, time::Instant};

use onecrew_creator_runtime::{
    EmailActionResult, EmailBatchActionResult, EmailDraftSendResult, ManualEmailActionResult,
    ReconcileEmailBatchInput, SendEmailDraftInput, SendEmailInput, SendManualEmailInput,
    SendManyEmailInput,
};
use schemars::JsonSchema;
use serde::Serialize;

#[cfg(test)]
mod retirement_tests {
    use super::*;

    #[test]
    fn retired_email_owner_cannot_submit_and_history_stays_readable() {
        let directory = tempfile::tempdir().unwrap();
        let platform = crate::LocalPlatform::open(directory.path()).unwrap();
        let origin = crate::ResultOrigin::new("web:retired-mail").unwrap();
        let context = crate::OwnerReviewContext::new(origin.clone(), "retired-call", "email.draft.send", None).unwrap();
        let review = crate::PreparedOwnerReview::email(
            PreparedEmailAction {
                intent: EmailSendIntent::Draft(SendEmailDraftInput {
                    draft_id: onecrew_creator_runtime::EmailDraftId::new("retained-draft"),
                    expected_revision: 1,
                    idempotency_key: "retained-send-intent".into(),
                }),
                idempotency_key: "retained-send-intent".into(),
                expires: Instant::now() + std::time::Duration::from_secs(60),
            },
            context,
            crate::PresentationProjection {
                kind: crate::ResultPresentationKind::new("email_confirmation").unwrap(),
                title: "Retained email review".into(), summary: None, sections: vec![],
            },
            "Retained confirmation".into(), "Confirm".into(),
        ).unwrap();
        let reference = platform.mint_owner_review(review, crate::ExecutedBatch {
            outcomes: vec![crate::ExecutedCallOutcome {
                call_id: "retired-call".into(), operation_id: "email.draft.send".into(),
                disposition: crate::CallDisposition::NotRun { reason: "confirmation_required".into() },
                summary: "Retained email review".into(), resource_refs: vec![],
            }],
        }, None, vec![]).unwrap();
        let available = platform.present_result(&reference, &origin).unwrap();
        assert!(matches!(platform.submit_owner_review(&available.binding, crate::OwnerReviewSelection::Confirm),
            Err(crate::ResultStoreError::InvalidState(crate::RETIRED_OPERATION_CODE))));
        assert_eq!(platform.read_current_result(&available.binding).unwrap(), available);
    }
}

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
