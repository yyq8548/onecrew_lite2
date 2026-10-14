use crate::{
    CommitEmailActionInput, CommitEmailDraftActionInput, CommitManualEmailActionInput,
    EmailActionResult, EmailDraftSendResult, ManualEmailActionResult,
};

#[cfg(test)]
use crate::{
    ContactChannel, ContactLocatorInput, CreatorChangeScope, EmailCapability, EmailFolder,
    EmailThreadGetInput, EmailThreadMutationInput, ListEmailFoldersInput, LocatorKind,
    MailboxDetail, MailboxRefInput,
};

mod attachments;
mod batch;
mod failure;
mod mailboxes;
mod message;
mod recovery;
mod send;
mod sent_sync;
mod sync;
mod sync_full;
mod threads;
mod validation;

pub(crate) use mailboxes::mailbox_detail;

pub(crate) use crate::persisted_mailbox_snapshot;
#[cfg(test)]
use message::{header_message_ids, parse_provider_time};
use message::{
    inbound_thread_key, isolated_inbound_thread_key, message_reference_ids, safe_inbound_thread,
    validate_inbound, verify_sent_readback,
};
pub(crate) use message::{
    normalize_email_address, provider_message_input, quote_original, reply_headers,
};
use validation::{
    canonical_email_body, validate_remote_accepted_email_action,
    validate_remote_started_email_action,
};
pub(crate) use validation::{
    canonical_email_request_fingerprint, ensure_email_route, ensure_mailbox, failed_replay,
    validate_idempotency_key,
};

pub(crate) use sync_full::EmailSyncRegistry;

pub(crate) const EMAIL_ACTION: &str = "email_send";
pub(crate) const MANUAL_EMAIL_ACTION: &str = "manual_email_send";
pub(crate) const EMAIL_BATCH_ACTION: &str = "email_batch_send";
pub(crate) enum RecoveredEmailAction {
    Creator(Box<EmailActionResult>),
    Manual(ManualEmailActionResult),
    Draft(EmailDraftSendResult),
}

pub(crate) enum VerifiedEmailAction {
    Creator(Box<CommitEmailActionInput>),
    Manual(CommitManualEmailActionInput),
    Draft(CommitEmailDraftActionInput),
}

pub use batch::{
    EmailBatchActionResult, EmailBatchChunkResult, EmailBatchCommitInput, EmailBatchContentInput,
    EmailBatchItemInput, EmailBatchItemResult, EmailBatchOutcome, EmailBatchPreparation,
    EmailBatchRecipient, EmailBatchReconciliationItem, EmailBatchReconciliationOutcome,
    EmailBatchValidation, ReconcileEmailBatchInput, SendEmailManyInput, SendManyEmailInput,
};
pub(crate) use batch::{
    EmailBatchCreatorSnapshot, EmailBatchPreflightSnapshot, PREFLIGHT_CREATOR_CHUNK_SIZE,
};
pub use failure::{EmailSendEffect, EmailSendFailure};

#[cfg(test)]
pub(crate) mod tests;
