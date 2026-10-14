// Every repository method carries exactly one never-enabled `cfg_attr` marker
// beside its trait declaration. That private source record is the execution-
// class source of truth without entering rustdoc: the test-only syn contract
// discovers every port and SQLite trait impl, rejects missing/unknown markers,
// and verifies the path-aware connection topology. Adding a method without
// classifying it therefore fails closed; there is no separately maintained
// method-name registry.
mod accounts;
mod contacts;
mod content;
mod conversations;
mod creators;
mod discovery;
mod idempotency;
mod mail;
mod gmail_bulk;
pub use gmail_bulk::GmailBulkRepository;
mod relationships;
mod targets;

pub use accounts::OperatorAccountRepository;
pub use contacts::ContactRepository;
pub use content::ContentRepository;
pub use conversations::{AppendMessageInput, ConversationRepository};
pub use creators::{
    ConfirmImportWrite, ConfirmImportWriteOutcome, CreatorRepository, ImportBasis,
    ImportOperationSnapshot, PrepareImportWrite, PrepareImportWriteOutcome,
    ValidatedImportArtifact,
};
pub use discovery::DiscoveryCampaignRepository;
pub use idempotency::IdempotencyRepository;
pub use mail::{
    EmailDraftRepository, EmailRepository, EmailTemplateRepository, MailboxRepository,
    ReplyTriageRepository,
};
pub use relationships::RelationshipRepository;
pub use targets::{OperableAccountRefError, TargetRepository};

pub trait CreatorRuntimeRepository:
    CreatorRepository
    + DiscoveryCampaignRepository
    + ContactRepository
    + TargetRepository
    + OperatorAccountRepository
    + RelationshipRepository
    + ConversationRepository
    + ContentRepository
    + MailboxRepository
    + EmailRepository
    + EmailTemplateRepository
    + EmailDraftRepository
    + IdempotencyRepository
    + GmailBulkRepository
    + ReplyTriageRepository
{
}

impl<T> CreatorRuntimeRepository for T where
    T: CreatorRepository
        + DiscoveryCampaignRepository
        + ContactRepository
        + TargetRepository
        + OperatorAccountRepository
        + RelationshipRepository
        + ConversationRepository
        + ContentRepository
        + MailboxRepository
        + EmailRepository
        + EmailTemplateRepository
        + EmailDraftRepository
        + IdempotencyRepository
        + GmailBulkRepository
    + ReplyTriageRepository
{
}
