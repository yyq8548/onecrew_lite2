use std::time::Instant;

use onecrew_creator_runtime::{
    CreatorSaveAdministratorEligibility, CreatorSavePreparedRow, CreatorSaveRouteSelection,
    PreparedCreatorSaveBatch, SelectedPreparedCreatorSaveBatch,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    CreatorSaveBatchReview, CreatorSaveBatchSelection, CreatorSaveInputError,
    CreatorSavePreparedFailure, ResultOrigin, ResultStoreError,
};

/// Creator-save application projection of one native Telegram administrator.
/// The provider facts remain intact while eligibility is computed by the
/// canonical Creator rule for this save use case.
#[derive(Clone, Debug, Serialize)]
pub struct CreatorSaveTelegramAdministratorRow {
    pub role: crate::toolkits::telegram::TgAdministratorRole,
    pub user: crate::toolkits::telegram::TgPublicUser,
    pub eligibility: CreatorSaveAdministratorEligibility,
}

#[derive(Clone, Debug, Serialize)]
pub struct CreatorSaveTelegramAdministrators {
    pub community: crate::toolkits::telegram::TgPublicCommunity,
    pub administrators: Vec<CreatorSaveTelegramAdministratorRow>,
    pub page_limit: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareCreatorSaveInput {
    #[schemars(length(min = 1, max = onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS))]
    pub candidates: Vec<crate::CreatorSaveCandidateInput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TelegramCommunityAdministratorsInput {
    pub source_creator_id: onecrew_creator_runtime::CreatorId,
    pub source_contact_route_id: onecrew_creator_runtime::CreatorContactRouteId,
    pub operator_account_ref_id: onecrew_creator_runtime::AccountRefId,
}

/// Trusted adapter context; intentionally absent from wire deserialization/schema.
#[derive(Clone, Debug)]
pub struct CreatorSavePreparationContext {
    pub origin: ResultOrigin,
    pub call_id: String,
    pub operation_id: String,
}

#[derive(Clone, Debug)]
pub enum CreatorSaveError {
    InvalidBatchSize,
    ConflictingIntents,
    Input {
        candidate_index: usize,
        error: CreatorSaveInputError,
    },
    IdentityMismatch,
    SourceMismatch,
    ProviderRead {
        code: String,
        message: String,
    },
    CreatorUnavailable,
    Preparation(onecrew_creator_runtime::CreatorSavePreparationError),
    PreparationDispatch(onecrew_creator_runtime::CreatorRepositoryDispatchFailure),
    Result(ResultStoreError),
    Expired,
    WorkerUnavailable,
}

impl std::fmt::Display for CreatorSaveError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidBatchSize => write!(
                out,
                "Creator save batch must contain between one and {} selections",
                onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS
            ),
            Self::ConflictingIntents => {
                out.write_str("Creator save batch must use one explicit save intent")
            }
            Self::Input {
                candidate_index,
                error,
            } => {
                write!(
                    out,
                    "Candidate {}: {error}",
                    candidate_index.saturating_add(1)
                )
            }
            Self::IdentityMismatch => out
                .write_str("selection_identity_mismatch: provider identity changed; select again"),
            Self::SourceMismatch => {
                out.write_str("selection source no longer matches its community or account")
            }
            Self::ProviderRead { message, .. } => out.write_str(message),
            Self::CreatorUnavailable => {
                out.write_str("Creator save preparation or execution could not be completed")
            }
            Self::Preparation(error) => error.fmt(out),
            Self::PreparationDispatch(error) => error.fmt(out),
            Self::Result(error) => error.fmt(out),
            Self::Expired => out.write_str("prepared selection expired; prepare and review again"),
            Self::WorkerUnavailable => out.write_str(
                "Creator save worker outcome is unavailable; inspect the result before retrying",
            ),
        }
    }
}

impl std::error::Error for CreatorSaveError {}

/// A fresh-read workflow can commit local credential health before a later failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSaveEffect {
    NoMutation,
    LocalEffectsPossible,
}

impl CreatorSaveError {
    pub fn effect(&self) -> CreatorSaveEffect {
        match self {
            Self::InvalidBatchSize | Self::ConflictingIntents | Self::Input { .. } => {
                CreatorSaveEffect::NoMutation
            }
            // Includes ID mismatch after a successful YouTube health update, or
            // any later failure in a mixed-platform batch. Never claim rollback.
            _ => CreatorSaveEffect::LocalEffectsPossible,
        }
    }
}
impl
    From<
        onecrew_creator_runtime::CreatorRepositoryCallError<
            onecrew_creator_runtime::CreatorSavePreparationError,
        >,
    > for CreatorSaveError
{
    fn from(
        value: onecrew_creator_runtime::CreatorRepositoryCallError<
            onecrew_creator_runtime::CreatorSavePreparationError,
        >,
    ) -> Self {
        match value {
            onecrew_creator_runtime::CreatorRepositoryCallError::Owner(error) => {
                Self::Preparation(error)
            }
            onecrew_creator_runtime::CreatorRepositoryCallError::Dispatch(error) => {
                Self::PreparationDispatch(error)
            }
        }
    }
}
impl From<ResultStoreError> for CreatorSaveError {
    fn from(value: ResultStoreError) -> Self {
        Self::Result(value)
    }
}

/// Only fresh preparation constructs this in-memory authority.
#[derive(Debug)]
pub(crate) struct PreparedSaveAction {
    pub prepared: PreparedCreatorSaveBatch,
    pub review: CreatorSaveBatchReview,
    pub failures: Vec<CreatorSavePreparedFailure>,
    pub deadline: Instant,
    pub context: CreatorSavePreparationContext,
}

impl PreparedSaveAction {
    pub(crate) fn select(
        &self,
        selections: &[CreatorSaveBatchSelection],
    ) -> Result<SelectedSaveAction, ResultStoreError> {
        if Instant::now() >= self.deadline {
            return Err(ResultStoreError::Expired);
        }
        let selected = self
            .prepared
            .clone()
            .select_routes(
                selections
                    .iter()
                    .map(|selection| CreatorSaveRouteSelection {
                        row_id: selection.row_id.clone(),
                        route_ids: selection.confirmed_route_ids.clone(),
                    })
                    .collect(),
            )
            .map_err(|error| {
                use onecrew_creator_runtime::CreatorSaveSelectionError;
                // Project the owner's classification; do not revalidate the
                // submitted rows or routes against a second copy of the rules.
                ResultStoreError::InvalidInput(match error {
                    CreatorSaveSelectionError::InvalidRowSubset => "creator_save_batch_selection",
                    CreatorSaveSelectionError::DuplicateRow => "creator_save_duplicate_row",
                    CreatorSaveSelectionError::UnknownRow => "creator_save_unknown_row",
                    CreatorSaveSelectionError::DuplicateRoute
                    | CreatorSaveSelectionError::MissingRequiredRoute
                    | CreatorSaveSelectionError::MissingParentRoute
                    | CreatorSaveSelectionError::UnknownRoute => "creator_save_routes",
                    CreatorSaveSelectionError::RowNotEligible => "creator_save_row_not_eligible",
                })
            })?;
        log::debug!(
            "creator preparation phase=user_review selected={} wait_ms={}",
            selections.len(),
            super::prepare::PREPARED_TTL
                .saturating_sub(self.deadline.saturating_duration_since(Instant::now()))
                .as_millis(),
        );
        Ok(SelectedSaveAction {
            selected,
            rows: self.prepared.rows(),
            failures: self.failures.clone(),
            selections: selections.to_vec(),
            context: self.context.clone(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct SelectedSaveAction {
    pub selected: SelectedPreparedCreatorSaveBatch,
    pub rows: Vec<CreatorSavePreparedRow>,
    pub failures: Vec<CreatorSavePreparedFailure>,
    pub selections: Vec<CreatorSaveBatchSelection>,
    pub context: CreatorSavePreparationContext,
}
