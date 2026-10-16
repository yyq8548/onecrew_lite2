//! Creator-centric Creator application boundary shared by the desktop and MCP
//! adapters. This crate owns the canonical domain, repository ports and the
//! current SQLite schema and the explicit, backup-first adjacent-chain upgrade
//! from the retained v27 baseline through the current version.

mod account_refs;
mod action_guard;
mod changes;
mod contact_routes;
pub use contact_routes::telegram_community::TelegramCommunityRouteContext;
mod creator_groups;
mod creator_import;
mod creator_transfer;
mod creator_transfer_xlsx;
mod creator_transfer_xlsx_security;
mod discovery;
mod discovery_query_rounds;
pub use discovery_query_rounds::AppendDiscoveryQueries;
pub use discovery::DiscoveryReadStop;
mod discovery_bluesky;
mod discovery_hackernews;
mod discovery_linkedin;
mod discovery_pinterest;
mod discovery_pixiv;
mod discovery_web_sources;
pub use discovery_web_sources::{
    DiscoveryWebSourceClaim, DiscoveryWebSourceRead, FinishDiscoveryWebSourceInput,
    web_source_twitter_target,
};
mod discovery_douyin;
pub use discovery_douyin::{
    DiscoveryFollowerEstimate, DiscoverySourceAuthorClaim, DiscoverySourceAuthorRead,
    FinishDiscoverySourceAuthorInput,
};
mod contact_navigation;
mod discovery_contact_lookup;
pub use contact_navigation::{contact_search_lead_matches, contact_search_lead_priority};
mod discovery_zhihu;
pub use discovery_contact_lookup::DiscoveryContactLookup;
mod discovery_contacts;
pub use discovery_contacts::{DiscoveryContact, DiscoveryContactAttribution, DiscoveryContactKind};
mod discovery_manual_approval;
pub use discovery_manual_approval::{ApproveDiscoveryCandidateInput, DiscoveryManualApproval};
mod discovery_intent;
#[cfg(test)]
mod discovery_target_tests;
#[cfg(test)]
mod discovery_unlimited_tests;
pub use discovery_intent::{
    DiscoveryIntentIssue, DiscoveryIntentProposal, DiscoveryIntentValidationResult,
    ProposedConstraint, QuotedHardCondition, QuotedPlatform, QuotedPreference,
    ValidateDiscoveryIntentInput, validate_discovery_intent_proposal,
};
mod discovery_review;
pub use discovery_review::{
    DiscoveryMatchLevel, DiscoveryReviewBasis, DiscoveryReviewCitation, DiscoveryReviewCriterion,
    DiscoveryReviewFinding, DiscoveryReviewJudgment, DiscoveryReviewMaterial,
    DiscoveryReviewMaterialInput, DiscoveryReviewOutcome, DiscoveryReviewQuote,
    DiscoveryReviewReceipt, DiscoveryReviewSubmissionBasis, DiscoveryReviewedTool,
    DiscoveryReviewer, DiscoveryReviewerKind, GetDiscoveryReviewInput, INITIAL_MATCH_REVIEWER,
    PreparedDiscoveryReview, StoredDiscoveryReview, SubmitDiscoveryReviewInput,
};
mod discovery_library;
pub use discovery_library::{
    DiscoveryLibraryBasis, DiscoveryLibraryMaterial, DiscoveryLibraryPublication,
    DiscoveryLibraryRow, DiscoveryLibrarySnapshot, DiscoveryLibraryTableName,
    DiscoveryLibraryVersions,
};
mod discovery_identity;
mod discovery_identity_merge;
pub use discovery_identity::{
    DiscoveryIdentityClaim, DiscoveryIdentityRead, DiscoveryIdentityReadInput,
    FinishDiscoveryIdentityInput,
};
pub use discovery_identity_merge::{
    DiscoveryIdentityDecision, DiscoveryIdentityFinding, DiscoveryIdentityGroup,
    DiscoveryIdentityGroupCounts, DiscoveryIdentityGroupMember, DiscoveryIdentityGroupReview,
    DiscoveryIdentityGroupReviewState, DiscoveryIdentityGroupsBasis,
    DiscoveryIdentityGroupsSnapshot, DiscoveryIdentityMergeMaterial,
    DiscoveryIdentityMergeMaterialInput, DiscoveryIdentityQuote, DiscoveryIdentityRelationship,
    DiscoveryIdentityReviewKind, DiscoveryIdentityReviewReceipt, DiscoveryIdentityRevocationBasis,
    DiscoveryIdentitySubmissionBasis, GetDiscoveryIdentityGroupsInput,
    GetDiscoveryIdentityReviewInput, PreparedDiscoveryIdentityReview,
    PreparedDiscoveryIdentityRevocation, RevokeDiscoveryIdentityReviewInput,
    StoredDiscoveryIdentityReview, SubmitDiscoveryIdentityReviewInput,
};
mod discovery_content;
mod discovery_facebook;
mod discovery_facebook_identity;
pub use discovery_facebook::DiscoveryFacebookTargetsRead;
pub use discovery_facebook_identity::{
    DiscoveryFacebookIdentityClaim, DiscoveryFacebookIdentityRead,
    FinishDiscoveryFacebookIdentityInput,
};
mod discovery_language;
pub use discovery_content::{
    DiscoveryContentClaim, DiscoveryContentRead, DiscoveryContentReadInput,
    FinishDiscoveryContentInput,
};
pub use discovery_language::{
    DiscoveryLanguageClaim, DiscoveryLanguageComment, DiscoveryLanguageSample,
};
mod discovery_profiles;
pub use discovery_profiles::{
    DiscoveryProfileClaim, DiscoveryProfileRead, DiscoveryProfileReadInput,
    FinishDiscoveryProfileInput,
};
#[cfg(test)]
mod discovery_author_tests;
mod discovery_public_pages;
mod discovery_qualification;
pub use discovery_public_pages::{
    DiscoveryPublicPageClaim, DiscoveryPublicPageInput, DiscoveryPublicPageRead,
    DiscoveryPublicPageReadInput, FinishDiscoveryPublicPageInput,
};
mod discovery_transcripts;
pub use discovery::{
    ApproveDiscoveryCampaignInput, CreateDiscoveryCampaignInput, DiscoveryCampaign,
    DiscoveryCampaignIdInput, DiscoveryCampaignSummary, DiscoveryCandidate, DiscoveryRefinement, DiscoveryExcludedAccount, DiscoverySearchRecovery,
    DiscoveryCandidateCounts, DiscoveryPlatformRun, DiscoveryPlatformScope, DiscoverySearchClaim,
    DiscoverySearchHit, DiscoverySearchHitInput, DiscoveryStep, DiscoveryVideoClaim,
    FinishDiscoverySearchInput, FinishDiscoveryVideoInput, IncreaseDiscoveryBudgetInput,
    IncreaseDiscoveryTargetInput, ListDiscoveryCandidatesInput, ListDiscoverySearchHitsInput,
    UpsertDiscoveryCandidateInput,
};
pub use discovery_transcripts::{
    DiscoveryTranscriptClaim, DiscoveryTranscriptRead, DiscoveryTranscriptReadInput,
    DiscoveryTranscriptSegment, FinishDiscoveryTranscriptInput,
};
mod creators;
pub use creators::save::{
    CanonicalCreatorSaveInput, CreatorDiscoverySelectionContext,
    CreatorSaveAdministratorEligibility, CreatorSaveAdministratorEligibilityReason,
    CreatorSaveBatchResult, CreatorSaveBatchSummary, CreatorSaveIntent,
    CreatorSavePreparationError, CreatorSavePreparationErrorKind, CreatorSavePreparationRowFailure,
    CreatorSavePreparedReason, CreatorSavePreparedRow, CreatorSavePreparedStatus,
    CreatorSaveProfile, CreatorSaveProvenance, CreatorSaveRouteSelection, CreatorSaveRowOutcome,
    CreatorSaveSelectionError, CreatorSaveSkipReason, MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES,
    PreparedCreatorSaveBatch, SelectedPreparedCreatorSaveBatch,
    assess_telegram_administrator_eligibility, require_telegram_administrator_eligibility,
};
pub use creators::{
    CreatorTagsError, MAX_CREATOR_TAG_CHARS, MAX_CREATOR_TAGS, creator_tags_description,
    creator_tags_schema, validate_creator_tags,
};
mod email;
mod gmail_bulk;
pub use gmail_bulk::*;
mod gateway;
mod identity;
mod mail;
mod models;
mod outreach;
pub use outreach::intents::{
    CreatorContentOutreachContext, CreatorOutreachContext, CreatorOutreachConversation,
    CreatorOutreachError, CreatorOutreachIdentity, CreatorOutreachOperation, CreatorOutreachTarget,
    CreatorRouteOutreachContext, PreparedCreatorOutreachIntent,
};
pub use outreach::ledger::{
    CreatorOutreachActionState, CreatorOutreachClaim, CreatorOutreachLease,
    CreatorOutreachMessageRecord, CreatorOutreachPhase, CreatorOutreachReceipt,
    CreatorOutreachRecord, CreatorOutreachRecordIds, CreatorOutreachStatus,
    CreatorOutreachVerification, CreatorRetiredOutreachPhase,
};
mod reply_triage;
pub mod repository;
mod repository_dispatch;
mod target_resolution;
pub use target_resolution::{
    CreatorContactTargetPreparation, CreatorContactTargetReadContext, CreatorTargetResolutionError,
};
#[cfg(test)]
mod test_support;

#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicU64, Ordering};
use std::{path::Path, sync::Arc};

use changes::CreatorChangeBus;
#[cfg(test)]
use repository_dispatch::RepositorySegmentTestHooks;

pub use email::{
    EmailBatchActionResult, EmailBatchChunkResult, EmailBatchCommitInput, EmailBatchContentInput,
    EmailBatchItemInput, EmailBatchItemResult, EmailBatchOutcome, EmailBatchPreparation,
    EmailBatchRecipient, EmailBatchReconciliationItem, EmailBatchReconciliationOutcome,
    EmailBatchValidation, EmailSendEffect, EmailSendFailure, ReconcileEmailBatchInput,
    SendEmailManyInput, SendManyEmailInput,
};
pub(crate) use email::{
    EmailBatchCreatorSnapshot, EmailBatchPreflightSnapshot, PREFLIGHT_CREATOR_CHUNK_SIZE,
};
pub use gateway::{CreatorGateways, DirectMessageAccess, EmailGateway};
pub use models::*;
/// The Creator schema version this build creates and requires — the single
/// source every version assertion (in-crate and downstream) must reference
/// instead of a literal (DATA-06 release contract stays auditable).
pub use repository::CURRENT_SCHEMA_VERSION as CREATOR_SCHEMA_VERSION;
use repository::{CreatorRuntimeRepository, SqliteRepository};
pub use repository_dispatch::{CreatorRepositoryCallError, CreatorRepositoryDispatchFailure};

#[derive(Clone)]
pub struct CreatorRuntime {
    repository: Arc<dyn CreatorRuntimeRepository>,
    /// Explicit test-support evidence only. The value counts successful
    /// acquisitions of the exclusive SQLite writer connection owner; it does
    /// not describe SQLite writes, commits, or changed rows.
    #[cfg(feature = "test-support")]
    writer_lease_acquisition_counter: Arc<AtomicU64>,
    repository_dispatch: repository_dispatch::RepositoryDispatch,
    gateways: CreatorGateways,
    changes: CreatorChangeBus,
    /// Idempotency keys with a non-idempotent provider write currently in
    /// flight in this process; blocks a concurrent same-key call from
    /// repeating the write while the first attempt is still awaiting the
    /// provider.
    in_flight_actions: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    /// In-flight mailbox sync runs, keyed by mailbox. Process-local:
    /// concurrent Desktop/MCP callers coalesce under one operation, and the
    /// durable watermarks live in the repository, never here.
    email_sync_registry: Arc<email::EmailSyncRegistry>,
    #[cfg(test)]
    repository_segment_test_hooks: Arc<RepositorySegmentTestHooks>,
}

impl CreatorRuntime {
    pub fn inspect_database(path: impl AsRef<Path>) -> CreatorDatabaseStatus {
        repository::inspect_database(path.as_ref())
    }

    pub fn upgrade_database(
        path: impl AsRef<Path>,
    ) -> Result<CreatorDatabaseUpgradeResult, CreatorDatabaseUpgradeError> {
        repository::upgrade_database(path.as_ref())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v27_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v27_for_test(path.as_ref())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v28_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v28_for_test(path.as_ref())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v29_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v29_for_test(path.as_ref())
    }

    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v31_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v31_for_test(path.as_ref())
    }

    /// Rebuild a current database as the released v35 predecessor so consumer
    /// crates can prove the Campaign retirement upgrade end to end.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v35_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v35_for_test(path.as_ref())
    }

    /// Structure-only v36 reconstruction for consumer migration fixtures.
    /// This never runs in normal open or in a production recovery path.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn downgrade_database_to_v36_for_test(path: impl AsRef<Path>) -> Result<(), String> {
        repository::downgrade_database_to_v36_for_test(path.as_ref())
    }

    /// Opens the Creator repository in `data_dir` and binds its outbound gateways.
    ///
    /// # Errors
    ///
    /// Returns an error when the data directory or SQLite database cannot be
    /// opened, or when the current Creator schema cannot be initialized or
    /// validated.
    pub fn open(data_dir: impl AsRef<Path>, gateways: CreatorGateways) -> Result<Self, String> {
        let repository = Arc::new(SqliteRepository::open(data_dir.as_ref().join("crm.db"))?);
        #[cfg(feature = "test-support")]
        let writer_lease_acquisition_counter = repository.writer_lease_acquisition_counter();
        Ok(Self {
            repository,
            #[cfg(feature = "test-support")]
            writer_lease_acquisition_counter,
            repository_dispatch: repository_dispatch::RepositoryDispatch::new(),
            gateways,
            changes: CreatorChangeBus::new(),
            in_flight_actions: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            email_sync_registry: Arc::new(email::EmailSyncRegistry::default()),
            #[cfg(test)]
            repository_segment_test_hooks: Arc::new(RepositorySegmentTestHooks::default()),
        })
    }

    /// Snapshot the number of successful exclusive writer-connection owner
    /// lease acquisitions made by this repository instance.
    ///
    /// Test support only: this is deliberately absent from the default build
    /// and is not production telemetry. The count must not be interpreted as
    /// SQLite transactions, commits, or changed rows.
    #[cfg(feature = "test-support")]
    #[doc(hidden)]
    pub fn writer_lease_acquisition_count_snapshot_for_test(&self) -> u64 {
        self.writer_lease_acquisition_counter
            .load(Ordering::Relaxed)
    }

    pub(crate) fn in_flight_actions(
        &self,
    ) -> Arc<std::sync::Mutex<std::collections::HashSet<String>>> {
        self.in_flight_actions.clone()
    }
}

pub use contact_routes::extraction::{
    CreatorContactProfileField, CreatorContactProfileLink, discover_creator_contacts,
};
