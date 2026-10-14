mod ports;
mod sqlite;

pub use ports::{
    AppendMessageInput, ConfirmImportWrite, ConfirmImportWriteOutcome, ContactRepository,
    ContentRepository, ConversationRepository, CreatorRepository, CreatorRuntimeRepository,
    DiscoveryCampaignRepository, EmailDraftRepository, EmailRepository, EmailTemplateRepository,
    IdempotencyRepository, ImportBasis, ImportOperationSnapshot, MailboxRepository,
    OperableAccountRefError, OperatorAccountRepository, PrepareImportWrite,
    PrepareImportWriteOutcome, RelationshipRepository, ReplyTriageRepository, TargetRepository,
    ValidatedImportArtifact,
};
pub use ports::GmailBulkRepository;
pub use sqlite::CURRENT_SCHEMA_VERSION;

pub(crate) use sqlite::{SQLITE_READ_POOL_SIZE, SqliteRepository};

pub(crate) fn inspect_database(path: &std::path::Path) -> crate::CreatorDatabaseStatus {
    sqlite::inspect_database(path)
}

pub(crate) fn upgrade_database(
    path: &std::path::Path,
) -> Result<crate::CreatorDatabaseUpgradeResult, crate::CreatorDatabaseUpgradeError> {
    sqlite::upgrade_database(path)
}

#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v27_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v27_for_test(path)
}

#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v28_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v28_for_test(path)
}

#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v29_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v29_for_test(path)
}

#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v31_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v31_for_test(path)
}

#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v35_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v35_for_test(path)
}

/// Structure-only predecessor fixture reconstruction; never a production downgrade.
#[cfg(feature = "test-support")]
pub(crate) fn downgrade_database_to_v36_for_test(path: &std::path::Path) -> Result<(), String> {
    sqlite::downgrade_database_to_v36_for_test(path)
}
