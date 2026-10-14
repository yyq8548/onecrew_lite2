//! Application runtime for local accounts, credentials, and governed provider
//! dispatch.
//!
//! This crate composes the vault, Creator runtime, provider adapters, and in-flight
//! operator login state behind one process-local service. The Desktop shell
//! injects native host facts and owns process and MCP lifecycle; protocol and
//! persistence details remain with their lower-level owners.

mod account;
mod mail_access;
pub use onecrew_access_client::mail::MailConnection as AccessMailConnection;
mod browser;
mod capabilities;
mod catalog;
mod changes;
pub use catalog::{
    CapabilityCatalog, CatalogBuildError, CatalogSurface, OperationId, OperationSummary, ToolkitId,
    ToolkitSummary,
};
mod connection_check;
mod connection_inventory;
mod construction;
mod creator_batch;
mod creator_export;
mod creator_save;
#[cfg(feature = "test-support")]
pub use creator_save::test_support;
mod credentials;
mod data_upgrade;
mod direct_message_access;
mod discovery_draft;
pub use discovery_draft::{DiscoveryDraftError, DraftProgress, PrepareDiscoveryDraftInput};
mod discovery_identity_review;
#[cfg(any(test, feature = "test-support"))]
#[path = "discovery_identity_review/fixture.rs"]
pub mod discovery_identity_test_support;
mod discovery_language;
mod discovery_match;
pub use discovery_match::{DISCOVERY_SCORE_CONCURRENCY, DiscoveryMatchPreparation, PreparedDiscoveryMatch};
mod discovery_review;
mod email;
mod interaction;
mod library_sync;
mod opencli;
mod outreach;
mod owner_execution;
mod profiles;
mod proxy;
mod search_plan;
mod telegram;
pub mod toolkits;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use onecrew_creator_runtime::CreatorRuntime;
use onecrew_creator_sync::CreatorSyncRuntime;
use onecrew_local_vault::LocalVault;
use onecrew_sessions::PlatformSessions;

pub use account::{
    AccountConnectionOperationInput, BrowserAccountConnectionCompleteInput,
    BrowserAccountConnectionStartInput, BrowserAccountConnectionStatus, ClearXChatPasscodeInput,
    DirectAccountConnectionCancelInput, DirectAccountConnectionPhase, InstagramPlatformDetail,
    InstagramPlatformGetInput, InstagramPlatformGetOutcome, OperatorAccountConnectionAction,
    OperatorAccountConnectionBeginInput, OperatorAccountConnectionCancelInput,
    OperatorAccountConnectionFinalizeInput, OperatorAccountConnectionHandoff,
    OperatorAccountConnectionHandoffKind, OperatorAccountConnectionInAppHandoff,
    OperatorAccountConnectionIntent, OperatorAccountConnectionMethod,
    OperatorAccountConnectionPhase, OperatorAccountConnectionPrepareInput,
    OperatorAccountConnectionStatus, OperatorAccountConnectionStatusInput,
    OperatorAccountCredentialAvailability, OperatorAccountDisconnectError,
    OperatorAccountDisconnectInput, OperatorAccountDisconnectOutcome, OperatorAccountPlatform,
    OperatorAccountProjection, SetXChatPasscodeInput, TelegramAccountConnectionMethod,
    TelegramAccountConnectionOtpInput, TelegramAccountConnectionStartInput,
    TelegramAccountConnectionStatus, TelegramAccountConnectionTwoFactorInput,
    TelegramInAppQrChallenge, TelegramLoginCodeDelivery, TelegramLoginCodeMethod,
    TelegramPlatformDetail, TelegramPlatformGetInput, TelegramPlatformGetOutcome, TelegramQrCode,
    XChatConnectionPreference, XChatPasscodeStatus, XPlatformDetail, XPlatformGetInput,
    XPlatformGetOutcome,
};
pub use browser::{
    BrowserCapabilityCode, BrowserCapabilityState, BrowserCompletionNotice, BrowserCookieProvider,
    BrowserStatus, ChromeChannel, ChromeInstallScope, DesktopOs,
};
pub use changes::{PlatformChange, PlatformChangeScope};
pub use connection_check::{
    ConnectionCheck, ConnectionCheckEgress, ConnectionCheckError, ConnectionCheckStatus,
};
pub use connection_inventory::{
    InstagramConnectionSummary, LocalConnectionAvailability, MailboxAccountSummary,
    MailboxProviderKind, PlatformConnectionInventory, PlatformConnectionResolutionError,
    PlatformConnectionSelector, TelegramConnectionSummary, XConnectionSummary,
    YoutubeCredentialSummary,
};
pub use construction::LocalPlatformHostPaths;
pub use creator_export::{CreatorExportDestination, CreatorExportInput, CreatorExportResult};
pub use creator_save::{
    AdministratorSelectionSource, CreatorSaveCandidateInput, CreatorSaveCommunityKind,
    CreatorSaveConnection, CreatorSaveEffect, CreatorSaveError, CreatorSaveInputError,
    CreatorSavePreparationContext, CreatorSaveProvenanceInput, CreatorSaveSearchOrigin,
    CreatorSaveTarget, CreatorSaveTelegramAdministratorRow, CreatorSaveTelegramAdministrators,
    PrepareCreatorSaveInput, TelegramCommunityAdministratorsInput, TelegramCommunityLocator,
};
pub use credentials::services::ResendCredentialTest;
pub use credentials::{
    ConnectedServiceCredentialProjection, MailboxCredentialProjectionDetails,
    RenameYoutubeCredentialInput, ResendCredentialConnectInput, ResendCredentialDeleteInput,
    ResendCredentialDeleteOutcome, ResendCredentialDeletionError, RotateResendCredentialInput,
    RotateYoutubeCredentialInput, ServiceCredentialConnectResult, ServiceCredentialMutationError,
    ServiceCredentialProjection, ServiceCredentialValidationProjection, YoutubeApiKeyConnectInput,
    YoutubeCredentialAvailability, YoutubeCredentialDeleteError, YoutubeCredentialDeleteInput,
    YoutubeCredentialDeleteOutcome, YoutubeCredentialIssue, YoutubeCredentialProjectionDetails,
    YoutubeCredentialRejectionIssue, YoutubePlatformDetail, YoutubePlatformGetInput,
    YoutubePlatformGetOutcome, YoutubePlatformTestError, YoutubePlatformTestInput,
    YoutubePlatformTestResult,
};
pub use data_upgrade::{
    LocalDataFailureKind, LocalDataPreflight, LocalDataState, LocalDataStore,
    LocalDataStoreUpgradeResult, LocalDataStoreUpgradeStatus, LocalDataUpgradeError,
    LocalDataUpgradeResult, LocalDataUpgradeStatus, LocalDataUpgradeTarget,
    UPGRADE_APPLIED_INITIALIZATION_FAILED_MESSAGE, UPGRADE_FAILURE_MESSAGE, UPGRADE_MESSAGE,
    UPGRADE_PRIMARY_ACTION, UPGRADE_SECONDARY_ACTION, UPGRADE_SUCCESS_MESSAGE, UPGRADE_TITLE,
};
pub use email::EmailCallOutput;
pub use interaction::search_plan_projection;
pub use interaction::{
    CallDisposition, CancelResultInput, ClaimResultContinuationInput, ClaimedResultContinuation,
    CompleteResultContinuationInput, CompletedResultContinuation, CreatorSaveBatchOperation,
    CreatorSaveBatchOutcome, CreatorSaveBatchReason, CreatorSaveBatchReview, CreatorSaveBatchRow,
    CreatorSaveBatchRowStatus, CreatorSaveBatchSelection, CreatorSaveCollision,
    CreatorSaveCollisionStatus, CreatorSavePreparedFailure, CreatorSavePreview,
    CreatorSaveResultSummary, CreatorSaveReviewFailureSummary, CreatorSaveReviewRowSummary,
    CreatorSaveReviewSummary, CreatorSaveRoute, ExecutedBatch, ExecutedCallOutcome,
    ExecutedResultInput, InitialResultState, NoticeTone, OwnerActionPayload, PendingActionView,
    PendingUserAction, PresentationField, PresentationItem, PresentationLocale,
    PresentationPreferences, PresentationProjection, PresentationSection,
    RESULT_CONTINUATION_DISPATCH_LEASE_MS, RequestResultEditInput, ResultBinding,
    ResultChoiceAnswer, ResultChoiceOption, ResultChoiceQuestion, ResultChoiceSelectionMode,
    ResultCleanupMetrics, ResultContinuationClaimMode, ResultContinuationCompletionOutcome,
    ResultContinuationContext, ResultContinuationCreatorSaveOutcome, ResultContinuationLeaseId,
    ResultContinuationOutcomeSummary, ResultContinuationRequest, ResultContinuationSnapshot,
    ResultContinuationState, ResultFailure, ResultFingerprint, ResultOrigin,
    ResultPresentationKind, ResultRef, ResultResolution, ResultResourceKind, ResultResourceRef,
    ResultSnapshot, ResultState, ResultStoreError, ResultTerminalUpdate, ResultUnknown,
    ResultViewBinding, ResultViewIdentity, ResultViewKind, SubmittedActionView,
    SubmittedOwnerAction, TerminalResult,
};
pub use interaction::{
    CreatorSaveCallResult, OwnerCallCompletion, OwnerCallDigest, OwnerCallOutput,
    OwnerReviewContext, OwnerReviewKind, OwnerReviewSelection, PeerResumePayload,
    PreparedOwnerNoop, PreparedOwnerReview, ResultConfirmationSubmission, SubmittedOwnerGrant,
};
pub use library_sync::{DiscoveryLibrarySyncResult, REQUIRED_LIBRARY_CAPABILITIES};
pub use onecrew_creator_runtime::{
    CreatorSaveBatchSummary, CreatorSaveIntent, CreatorSavePreparedReason,
    CreatorSavePreparedStatus,
};
pub use onecrew_dm_contracts::{DmDiagnostic, DmDiagnosticOrigin, DmFailure};
pub use opencli::{
    BilibiliPublicProfile, ChromeSetupPage, OpenCliAddonStatus, OpenCliAuthObservation,
    OpenCliAvailability, OpenCliDiscoveryAdvanceInput, OpenCliDiscoveryAdvanceResult,
    OpenCliDiscoveryPlatformSearchInput, OpenCliDiscoverySearchError, OpenCliDiscoverySearchInput,
    OpenCliDiscoverySearchResult, OpenCliDiscoverySources, OpenCliDiscoveryVideoInput,
    OpenCliSetupReport, OpenCliSourceAuthorTimings, TiktokPublicProfile,
    provider_stop_code as discovery_provider_stop_code,
};
pub use outreach::{OutreachCompletion, OutreachError, OutreachPhase, OutreachReceiptReference};
pub use owner_execution::{OwnerExecutionEffect, OwnerExecutionFailure};
pub use profiles::ProfileRow;
pub use proxy::{ProxyConfig, ProxyKind, ProxyProbe};
pub use search_plan::{
    BoundSearchAuthorization, ConfirmedSearchPlanEditor, OperationSearchAuthorization,
    SEARCH_PLAN_MAX_ACTIVE, SEARCH_PLAN_MAX_BYTES, SEARCH_PLAN_MAX_SCOPES, SEARCH_PLAN_TTL_SECONDS,
    SearchPlanApprovalError, SearchPlanConnection, SearchPlanEditorDraft,
    SearchPlanEditorValidationError, SearchPlanEditorValidationReason, SearchPlanPatch,
    SearchPlanPolicy, SearchPlanScope,
};

use account::PendingOperatorAccountConnection;
use browser::{PendingBrowserLogin, TgCredsBrowser};
use telegram::{PendingTgLogin, PendingTgQrLogin};

pub struct LocalPlatform {
    vault: LocalVault,
    /// The process's one `PlatformSessions`: every facade a toolkit function
    /// opens, and every facade the provider adapter opens, comes from this
    /// instance, so a live X session is built once and reused by both.
    sessions: Arc<PlatformSessions>,
    direct_messages: Arc<direct_message_access::SharedDirectMessageAccess>,
    creator_sync: Arc<CreatorSyncRuntime>,
    /// App data dir; per-profile Chrome `user-data-dir`s live under
    /// `<data_dir>/profiles/<id>`.
    data_dir: PathBuf,
    result_store: interaction::ResultMemoryStore,
    search_authority: search_plan::SearchAuthority,
    discovery_draft_gate: tokio::sync::Semaphore,
    discovery_score_gate: discovery_match::ScoreGate,
    discovery_draft_progress: std::sync::Mutex<Option<discovery_draft::DraftProgressState>>,
    opencli_auth_observations: std::sync::Mutex<Vec<OpenCliAuthObservation>>,
    presentation_preferences: Arc<PresentationPreferences>,
    /// Native host path injected by the Desktop shell. `None` is an explicit
    /// capability-unavailable state; it never blocks unrelated runtime work.
    system_desktop_dir: Option<PathBuf>,
    pending_tg: Mutex<HashMap<String, PendingTgLogin>>,
    pending_tg_qr: Mutex<HashMap<String, PendingTgQrLogin>>,
    pending_browser: Mutex<HashMap<String, PendingBrowserLogin>>,
    /// High-level connection flows. Public methods expose only opaque flow IDs
    /// and no-secret projections; browser/Telegram credentials remain in the
    /// private in-process state populated by desktop-only commands.
    pending_account_connections: Mutex<HashMap<String, PendingOperatorAccountConnection>>,
    /// The (single) throwaway Chrome opened at my.telegram.org so the user can
    /// copy their api_id/api_hash — held only to keep its 家宽 forwarder alive,
    /// then closed + discarded when the login proceeds or the dialog closes.
    tg_creds: Mutex<Option<TgCredsBrowser>>,
    creators: Arc<CreatorRuntime>,
    changes: changes::PlatformChangeBus,
    credential_services: Arc<credentials::services::CredentialServices>,
    /// Where the browser capability projection gets its host facts. Real hosts
    /// ask the machine; tests pick the answers, so every arm is reachable
    /// regardless of what the machine running them happens to be.
    browser_host: Arc<dyn browser::BrowserHostProbe>,
}

impl LocalPlatform {
    /// Returns the already initialized Creator application handle.
    ///
    /// Vault-to-Creator projection writes belong to explicit startup and account
    /// or credential lifecycle owners. Keeping this accessor pure prevents an
    /// ordinary Creator read from decrypting every Vault profile and opening a
    /// write transaction as an unrelated side effect.
    pub fn creators(&self) -> Result<CreatorRuntime, String> {
        Ok(self.creators.as_ref().clone())
    }

    /// The Creator Sync coordinator this graph was opened with. Pairing,
    /// admission, and snapshot upload are that runtime's own methods; the
    /// platform only guarantees every adapter entry shares the one instance.
    pub fn creator_sync(&self) -> &Arc<CreatorSyncRuntime> {
        &self.creator_sync
    }
}
