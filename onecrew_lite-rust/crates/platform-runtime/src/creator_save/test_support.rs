//! Offline fixtures for cross-crate adapter tests; excluded by default.

use crate::{
    CreatorSaveError, CreatorSavePreparationContext, LocalPlatform, OwnerReviewContext,
    PreparedOwnerReview, ResultRef,
};
use onecrew_creator_runtime::CanonicalCreatorSaveInput;
use std::time::Duration;

/// Prepare real canonical rows through the production sealed-review factory.
/// A peer still performs its one result mint and binds its exact continuation.
pub async fn prepare_creator_save_review(
    platform: &LocalPlatform,
    entries: Vec<CanonicalCreatorSaveInput>,
    context: OwnerReviewContext,
    intent: onecrew_creator_runtime::CreatorSaveIntent,
) -> Result<PreparedOwnerReview, CreatorSaveError> {
    if !super::valid_save_batch_size(entries.len()) {
        return Err(CreatorSaveError::InvalidBatchSize);
    }
    let prepared = platform
        .creators
        .prepare_creator_save_batch_async(entries, intent)
        .await
        .map_err(CreatorSaveError::from)?;
    super::prepare::seal_prepared_review(
        prepared,
        context,
        &[],
        &[],
        false,
        super::prepare::PREPARED_TTL,
    )
}

/// Prepare real canonical rows and mint their normal review through the
/// production owner without invoking a provider.
pub async fn mint_creator_save_review(
    platform: &LocalPlatform,
    entries: Vec<CanonicalCreatorSaveInput>,
    context: CreatorSavePreparationContext,
    intent: onecrew_creator_runtime::CreatorSaveIntent,
) -> Result<ResultRef, CreatorSaveError> {
    mint_review(
        platform,
        entries,
        context,
        intent,
        super::prepare::PREPARED_TTL,
    )
    .await
}

/// Exercise production expiration without sleeping for the normal lifetime.
pub async fn mint_expired_creator_save_review(
    platform: &LocalPlatform,
    entries: Vec<CanonicalCreatorSaveInput>,
    context: CreatorSavePreparationContext,
    intent: onecrew_creator_runtime::CreatorSaveIntent,
) -> Result<ResultRef, CreatorSaveError> {
    let reference = mint_review(
        platform,
        entries,
        context,
        intent,
        super::prepare::PREPARED_TTL,
    )
    .await?;
    platform
        .result_store
        .expire_creator_save_fixture(&reference)?;
    Ok(reference)
}

/// Advance the clock of an existing real prepared save in offline adapter
/// tests. This can only revoke authority; it changes no body, target or receipt.
pub fn expire_creator_save_review(
    platform: &LocalPlatform,
    reference: &ResultRef,
) -> Result<(), CreatorSaveError> {
    platform
        .result_store
        .expire_creator_save_fixture(reference)?;
    Ok(())
}

async fn mint_review(
    platform: &LocalPlatform,
    entries: Vec<CanonicalCreatorSaveInput>,
    context: CreatorSavePreparationContext,
    intent: onecrew_creator_runtime::CreatorSaveIntent,
    lifetime: Duration,
) -> Result<ResultRef, CreatorSaveError> {
    if !super::valid_save_batch_size(entries.len()) {
        return Err(CreatorSaveError::InvalidBatchSize);
    }
    let prepared = platform
        .creators
        .prepare_creator_save_batch_async(entries, intent)
        .await
        .map_err(CreatorSaveError::from)?;
    super::prepare::mint_prepared_review(platform, prepared, context, &[], lifetime)
}
