//! Fresh profile preparation and trusted Creator confirmation, without a staging cache.
mod candidates;
mod execute;
mod models;
mod prepare;
mod projection;
mod reads;
#[cfg(feature = "test-support")]
pub mod test_support;

pub use candidates::{
    AdministratorSelectionSource, CreatorSaveCandidateInput, CreatorSaveCommunityKind,
    CreatorSaveConnection, CreatorSaveInputError, CreatorSaveProvenanceInput,
    CreatorSaveSearchOrigin, CreatorSaveTarget, TelegramCommunityLocator,
};
pub use models::{
    CreatorSaveEffect, CreatorSaveError, CreatorSavePreparationContext,
    CreatorSaveTelegramAdministratorRow, CreatorSaveTelegramAdministrators,
    PrepareCreatorSaveInput, TelegramCommunityAdministratorsInput,
};
pub(crate) use models::{PreparedSaveAction, SelectedSaveAction};
pub(crate) fn valid_save_batch_size(size: usize) -> bool {
    (1..=onecrew_creator_runtime::MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS).contains(&size)
}

/// Seal a review through the production factory for in-crate tests, so result
/// store tests bound the projection the save owner really emits instead of a
/// hand-built one. It adds no capability: every argument is already required by
/// `seal_prepared_review`, and the review it returns is the production value.
#[cfg(test)]
pub(crate) fn seal_prepared_review_for_test(
    prepared: onecrew_creator_runtime::PreparedCreatorSaveBatch,
    context: crate::OwnerReviewContext,
    lifetime: std::time::Duration,
) -> Result<crate::PreparedOwnerReview, CreatorSaveError> {
    prepare::seal_prepared_review(prepared, context, &[], &[], false, lifetime)
}
