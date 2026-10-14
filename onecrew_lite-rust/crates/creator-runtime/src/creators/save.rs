//! Canonical, provider-free save inputs and owned confirmation payloads.

use crate::*;

/// Materialized public profile/evidence budget, independent of the compact
/// selection request's historical 1 MiB budget and the 50-row business cap.
/// 16 MiB permits 50 profiles averaging 327 KiB — far above the ~1.6 KiB a
/// real materialized row measures — while keeping the held owner action
/// (this budget plus two 4 MiB projections) inside the result store's
/// `DEFAULT_MAX_TOTAL_PAYLOAD_BYTES`, which a 64 MiB budget could not.
pub const MAX_CREATOR_SAVE_MATERIALIZED_BATCH_BYTES: usize = 16 * 1024 * 1024;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The caller's explicit save policy.  The policy is held with the prepared
/// rows so confirmation cannot silently turn an "add only" request into an
/// update after the review has been shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveIntent {
    AddNew,
    SaveOrUpdate,
}

/// Owner-derived actionability for one prepared row.  Adapters may display
/// this value but must not infer it from identity or route fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSavePreparedStatus {
    EligibleNew,
    EligibleUpdate,
    ExistingSkipped,
    DuplicateSkipped,
    IdentityPending,
    ContactConflict,
    Failed,
}

/// Safe explanation for a prepared row that is not directly selectable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// A safe explanation paired with a non-selectable prepared row.
pub enum CreatorSavePreparedReason {
    ExistingCreator,
    DuplicateSelection,
    IdentityAmbiguous,
    ConflictingUpdate,
    RequiredContactRouteConflict,
    PreparationFailed,
}

/// Mutually exclusive, owner-derived counts for one immutable preparation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveBatchSummary {
    pub total: usize,
    pub eligible_new: usize,
    pub eligible_update: usize,
    pub existing_skipped: usize,
    pub duplicate_skipped: usize,
    pub blocked: usize,
    pub failed: usize,
}

/// Read-time status for a Telegram community administrator candidate. This
/// only says whether the candidate can enter Creator save preparation; it does
/// not reserve an identity, inspect collisions, or grant save authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreatorSaveAdministratorEligibility {
    Ready,
    Blocked {
        reason: CreatorSaveAdministratorEligibilityReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveAdministratorEligibilityReason {
    NotTelegramUser,
    Bot,
    MissingPublicHandle,
    MissingProfileUrl,
}

/// Apply the same pure identity and initial-route rule used by canonical
/// preparation to an administrator read. Provider DTOs are intentionally
/// mapped by the caller into this canonical candidate value object.
pub fn assess_telegram_administrator_eligibility(
    account: &PlatformAccountCandidate,
) -> CreatorSaveAdministratorEligibility {
    if account.platform != Platform::Telegram || account.target_kind != Some(TargetKind::User) {
        return CreatorSaveAdministratorEligibility::Blocked {
            reason: if account.target_kind == Some(TargetKind::Bot) {
                CreatorSaveAdministratorEligibilityReason::Bot
            } else {
                CreatorSaveAdministratorEligibilityReason::NotTelegramUser
            },
        };
    }
    if account
        .handle
        .as_deref()
        .is_none_or(|handle| handle.trim().is_empty())
    {
        return CreatorSaveAdministratorEligibility::Blocked {
            reason: CreatorSaveAdministratorEligibilityReason::MissingPublicHandle,
        };
    }
    if account
        .profile_url
        .as_deref()
        .is_none_or(|url| url.trim().is_empty())
    {
        return CreatorSaveAdministratorEligibility::Blocked {
            reason: CreatorSaveAdministratorEligibilityReason::MissingProfileUrl,
        };
    }
    CreatorSaveAdministratorEligibility::Ready
}

pub fn require_telegram_administrator_eligibility(
    account: &PlatformAccountCandidate,
) -> Result<(), CreatorSaveAdministratorEligibilityReason> {
    match assess_telegram_administrator_eligibility(account) {
        CreatorSaveAdministratorEligibility::Ready => Ok(()),
        CreatorSaveAdministratorEligibility::Blocked { reason } => Err(reason),
    }
}

/// Read-only save preparation failed for a batch or an identified input row.
/// The diagnostic retains only a caller-provided row identifier and a safe category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorSavePreparationError {
    pub row_id: Option<String>,
    pub kind: CreatorSavePreparationErrorKind,
}

/// One input row preparation rejected while the rest of the batch prepared
/// normally. Execution already reports per-row results through
/// [`CreatorSaveRowOutcome::Failed`]; preparation now reports its own the same
/// way instead of discarding every other row's work. The diagnostic keeps only
/// the caller's row identifier and a safe category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorSavePreparationRowFailure {
    pub row_id: String,
    pub kind: CreatorSavePreparationErrorKind,
    pub platform: Platform,
    pub display_name: String,
    pub handle: Option<String>,
    pub profile_url: Option<String>,
}

/// Stable failure categories owned by the canonical Creator save workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorSavePreparationErrorKind {
    Tags(CreatorTagsError),
    InvalidFields,
    InvalidSource,
    Identity,
    TelegramEligibility(CreatorSaveAdministratorEligibilityReason),
    ContactNormalization,
    Storage,
}

impl std::fmt::Display for CreatorSavePreparationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.kind {
            CreatorSavePreparationErrorKind::Tags(error) => error.fmt(formatter),
            CreatorSavePreparationErrorKind::InvalidFields => {
                formatter.write_str("Creator save input fields are invalid")
            }
            CreatorSavePreparationErrorKind::InvalidSource => {
                formatter.write_str("Creator save source is invalid or unavailable")
            }
            CreatorSavePreparationErrorKind::Identity => {
                formatter.write_str("Creator save identity is invalid, ambiguous or unavailable")
            }
            CreatorSavePreparationErrorKind::TelegramEligibility(reason) => {
                write!(
                    formatter,
                    "Telegram administrator is not eligible for Creator save: {reason:?}"
                )
            }
            CreatorSavePreparationErrorKind::ContactNormalization => {
                formatter.write_str("Creator save contact evidence cannot be normalized")
            }
            CreatorSavePreparationErrorKind::Storage => {
                formatter.write_str("Creator save repository preparation failed")
            }
        }
    }
}

impl std::error::Error for CreatorSavePreparationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            CreatorSavePreparationErrorKind::Tags(error) => Some(error),
            _ => None,
        }
    }
}

/// Profile display facts. Contact extraction consumes only the explicitly
/// sourced `CanonicalCreatorSaveInput::contact_evidence`, never these fields.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveProfile {
    pub avatar_url: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub website_urls: Vec<String>,
    pub follower_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorSaveProvenance {
    Discovery {
        candidate_id: String,
        origin: CreatorCandidateOrigin,
        /// Additional facts supplied by the current selection. Historical
        /// discovery facts may genuinely have no selection context.
        selection_context: Option<CreatorDiscoverySelectionContext>,
    },
    DirectSelection {
        source_ref: String,
        source_url: Option<String>,
        description: Option<String>,
        operator_account_ref_id: Option<AccountRefId>,
    },
    ProviderRead {
        source_ref: String,
        operation_id: String,
        source_url: Option<String>,
        description: Option<String>,
        operator_account_ref_id: Option<AccountRefId>,
    },
    AdministratorSelection {
        source_ref: String,
        role: CommunityAdministratorRole,
        community: CommunityReference,
        source: CommunityAdministratorSource,
    },
}

/// Facts about the selection of a real discovery result. This does not replace
/// or rewrite the original discovery identity and source encoding.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorDiscoverySelectionContext {
    pub operation_id: String,
    pub source_ref: String,
    pub source_url: Option<String>,
    pub description: Option<String>,
}

impl CreatorSaveProvenance {
    pub(crate) fn account_ref(&self) -> Option<&AccountRefId> {
        match self {
            Self::Discovery { origin, .. } => Some(origin.account_ref_id()),
            Self::DirectSelection {
                operator_account_ref_id,
                ..
            }
            | Self::ProviderRead {
                operator_account_ref_id,
                ..
            } => operator_account_ref_id.as_ref(),
            Self::AdministratorSelection { source, .. } => Some(match source {
                CommunityAdministratorSource::OperatorAccount {
                    operator_account_ref_id,
                }
                | CommunityAdministratorSource::CreatorContactRoute {
                    operator_account_ref_id,
                    ..
                } => operator_account_ref_id,
            }),
        }
    }

    pub(crate) fn administrator(
        &self,
    ) -> Option<(&CommunityReference, &CommunityAdministratorSource)> {
        match self {
            Self::Discovery {
                origin:
                    CreatorCandidateOrigin::CommunityAdministrator {
                        community, source, ..
                    },
                ..
            }
            | Self::AdministratorSelection {
                community, source, ..
            } => Some((community, source)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonicalCreatorSaveInput {
    pub row_id: String,
    pub account: PlatformAccountCandidate,
    pub profile: CreatorSaveProfile,
    /// Unix seconds captured by the platform workflow when the profile read completed.
    pub profile_observed_at: u64,
    pub provenance: CreatorSaveProvenance,
    pub contact_evidence: Vec<CreatorSaveContactEvidence>,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub quality: CreatorQualityInput,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorSavePreparedRow {
    pub row_id: String,
    pub intent: CreatorSaveIntent,
    pub status: CreatorSavePreparedStatus,
    pub reason: Option<CreatorSavePreparedReason>,
    pub account: PlatformAccountCandidate,
    pub profile: CreatorSaveProfile,
    pub profile_observed_at: u64,
    pub provenance: CreatorSaveProvenance,
    pub notes: Option<String>,
    pub tags: Option<Vec<String>>,
    pub quality: CreatorQualityInput,
    pub identity_resolution: IdentityResolution,
    pub route_options: Vec<CreatorCandidateSaveRouteOption>,
}

/// Only Creator preparation constructs this value. Wire deserialization cannot
/// fabricate trusted confirmation contents.
#[derive(Debug, Clone)]
pub struct PreparedCreatorSaveBatch {
    pub(crate) rows: Vec<PreparedSaveRow>,
    pub(crate) failures: Vec<CreatorSavePreparationRowFailure>,
    pub(crate) intent: CreatorSaveIntent,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PreparedSaveRow {
    pub(crate) input: CanonicalCreatorSaveInput,
    pub(crate) preview: CreatorSavePreparedRow,
    pub(crate) routes: Vec<(String, ContactRoutePreview)>,
    pub(crate) required_parent_routes: std::collections::BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSaveRouteSelection {
    pub row_id: String,
    pub route_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectedPreparedCreatorSaveBatch {
    pub(crate) rows: Vec<SelectedSaveRow>,
    pub(crate) intent: CreatorSaveIntent,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SelectedSaveRow {
    pub(crate) input: CanonicalCreatorSaveInput,
    pub(crate) confirmations: Vec<ContactRouteConfirmation>,
}

impl PreparedCreatorSaveBatch {
    /// Encode the complete held payload for the trusted result owner's size
    /// budget and fingerprint. This is not a wire construction API.
    pub fn encoded_payload(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&(self.intent, &self.rows)).map_err(|error| error.to_string())
    }

    pub fn rows(&self) -> Vec<CreatorSavePreparedRow> {
        self.rows.iter().map(|row| row.preview.clone()).collect()
    }

    /// Rows this batch could not prepare. They carry no confirmation authority
    /// and cannot be selected; a caller reports them beside the prepared rows.
    pub fn failures(&self) -> &[CreatorSavePreparationRowFailure] {
        &self.failures
    }

    pub fn intent(&self) -> CreatorSaveIntent {
        self.intent
    }

    pub fn summary(&self) -> CreatorSaveBatchSummary {
        let mut summary = CreatorSaveBatchSummary {
            total: self.rows.len() + self.failures.len(),
            failed: self.failures.len(),
            ..CreatorSaveBatchSummary::default()
        };
        for row in &self.rows {
            match row.preview.status {
                CreatorSavePreparedStatus::EligibleNew => summary.eligible_new += 1,
                CreatorSavePreparedStatus::EligibleUpdate => summary.eligible_update += 1,
                CreatorSavePreparedStatus::ExistingSkipped => summary.existing_skipped += 1,
                CreatorSavePreparedStatus::DuplicateSkipped => summary.duplicate_skipped += 1,
                CreatorSavePreparedStatus::IdentityPending
                | CreatorSavePreparedStatus::ContactConflict => summary.blocked += 1,
                CreatorSavePreparedStatus::Failed => summary.failed += 1,
            }
        }
        summary
    }

    /// Bind an exact route selection to these prepared observations.
    ///
    /// # Errors
    /// Rejects invalid row subsets, unknown/duplicate rows or routes, and
    /// omitted required initial-account or prepared LinkHub parent routes.
    pub fn select_routes(
        self,
        selections: Vec<CreatorSaveRouteSelection>,
    ) -> Result<SelectedPreparedCreatorSaveBatch, CreatorSaveSelectionError> {
        if selections.is_empty() || selections.len() > self.rows.len() {
            return Err(CreatorSaveSelectionError::InvalidRowSubset);
        }
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for selection in selections {
            if !seen.insert(selection.row_id.clone()) {
                return Err(CreatorSaveSelectionError::DuplicateRow);
            }
            let row = self
                .rows
                .iter()
                .find(|row| row.input.row_id == selection.row_id)
                .ok_or(CreatorSaveSelectionError::UnknownRow)?;
            if !matches!(
                row.preview.status,
                CreatorSavePreparedStatus::EligibleNew | CreatorSavePreparedStatus::EligibleUpdate
            ) {
                return Err(CreatorSaveSelectionError::RowNotEligible);
            }
            let selected: BTreeSet<_> = selection.route_ids.iter().collect();
            if selected.len() != selection.route_ids.len() {
                return Err(CreatorSaveSelectionError::DuplicateRoute);
            }
            if row
                .preview
                .route_options
                .iter()
                .any(|route| route.required && !selected.contains(&route.route_id))
            {
                return Err(CreatorSaveSelectionError::MissingRequiredRoute);
            }
            for route_id in &selection.route_ids {
                if row
                    .required_parent_routes
                    .get(route_id)
                    .is_some_and(|parents| parents.iter().any(|parent| !selected.contains(parent)))
                {
                    return Err(CreatorSaveSelectionError::MissingParentRoute);
                }
            }
            let confirmations = selection
                .route_ids
                .iter()
                .map(|id| {
                    let (_, preview) = row
                        .routes
                        .iter()
                        .find(|(route_id, _)| route_id == id)
                        .ok_or(CreatorSaveSelectionError::UnknownRoute)?;
                    let required = row
                        .preview
                        .route_options
                        .iter()
                        .any(|route| &route.route_id == id && route.required);
                    Ok(ContactRouteConfirmation {
                        preview: preview.clone(),
                        confidence: if required {
                            Confidence::Exact
                        } else {
                            Confidence::Asserted
                        },
                    })
                })
                .collect::<Result<Vec<_>, CreatorSaveSelectionError>>()?;
            rows.push(SelectedSaveRow {
                input: row.input.clone(),
                confirmations,
            });
        }
        // Preserve preparation order independently of client selection order.
        rows.sort_by_key(|row| {
            self.rows
                .iter()
                .position(|prepared| prepared.input.row_id == row.input.row_id)
        });
        Ok(SelectedPreparedCreatorSaveBatch {
            rows,
            intent: self.intent,
        })
    }
}

/// Why a submitted selection cannot consume the prepared Creator save content.
/// These categories contain no submitted identifiers or contact values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatorSaveSelectionError {
    InvalidRowSubset,
    DuplicateRow,
    UnknownRow,
    DuplicateRoute,
    MissingRequiredRoute,
    MissingParentRoute,
    UnknownRoute,
    RowNotEligible,
}

impl std::fmt::Display for CreatorSaveSelectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidRowSubset => {
                "Creator save route selections require a nonempty prepared row subset"
            }
            Self::DuplicateRow => "duplicate Creator save row selection",
            Self::UnknownRow => "unknown Creator save row",
            Self::DuplicateRoute => "duplicate Creator save route selection",
            Self::MissingRequiredRoute => "initial platform account route must be selected",
            Self::MissingParentRoute => {
                "LinkHub-derived contact route requires its prepared parent route to be selected"
            }
            Self::UnknownRoute => "unknown Creator save route",
            Self::RowNotEligible => "Creator save row is not eligible for confirmation",
        })
    }
}

impl std::error::Error for CreatorSaveSelectionError {}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CreatorSaveRowOutcome {
    Saved {
        row_id: String,
        creator: Box<CreatorRecord>,
        created: bool,
        outreach_context: Option<CommunityAdministratorOutreachContext>,
    },
    Failed {
        row_id: String,
        error: String,
    },
    Skipped {
        row_id: String,
        reason: CreatorSaveSkipReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
/// Why an add-new row was skipped after confirmation-time identity checking.
pub enum CreatorSaveSkipReason {
    ExistingCreator,
    DuplicateSelection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorSaveBatchResult {
    pub outcomes: Vec<CreatorSaveRowOutcome>,
}

impl CreatorRuntime {
    pub async fn prepare_creator_save_batch_async(
        &self,
        inputs: Vec<CanonicalCreatorSaveInput>,
        intent: CreatorSaveIntent,
    ) -> Result<PreparedCreatorSaveBatch, CreatorRepositoryCallError<CreatorSavePreparationError>>
    {
        let runtime = self.clone();
        self.repository_read_segment(move || runtime.prepare_creator_save_batch(inputs, intent))
            .await
    }

    pub async fn execute_creator_save_batch_async(
        &self,
        prepared: SelectedPreparedCreatorSaveBatch,
        execution_key: String,
    ) -> Result<CreatorSaveBatchResult, CreatorRepositoryCallError<String>> {
        let runtime = self.clone();
        self.repository_write_segment(move || {
            runtime.execute_creator_save_batch(prepared, execution_key)
        })
        .await
    }

    pub fn prepare_creator_save_batch(
        &self,
        inputs: Vec<CanonicalCreatorSaveInput>,
        intent: CreatorSaveIntent,
    ) -> Result<PreparedCreatorSaveBatch, CreatorSavePreparationError> {
        self.repository.prepare_creator_save_batch(inputs, intent)
    }

    /// Synchronous implementation for an already admitted blocking segment.
    /// Async adapters use [`Self::execute_creator_save_batch_async`] to own admission and dispatch.
    pub fn execute_creator_save_batch(
        &self,
        prepared: SelectedPreparedCreatorSaveBatch,
        execution_key: String,
    ) -> Result<CreatorSaveBatchResult, String> {
        let (result, changed) = self
            .repository
            .execute_creator_save_batch(prepared, execution_key)?;
        if changed {
            self.publish_changes([CreatorChangeScope::Creators]);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod runtime_tests;
