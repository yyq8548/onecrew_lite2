use std::sync::Arc;

use onecrew_creator_runtime::ManualCreatorBatchCreateResult;

use super::models::PreparedCreatorBatchAction;
use crate::{LocalPlatform, SubmittedOwnerGrant};

impl LocalPlatform {
    /// Consume the single-use grant and write the exact batch the owner
    /// reviewed.
    ///
    /// The Creator runtime still owns the idempotency claim, the fingerprint,
    /// per-item validation, and the writes themselves. This hands it only the
    /// intent that was sealed before the review, so a resumed call cannot
    /// substitute different items or a different key.
    ///
    /// The batch is a blocking repository write with no async wrapper of its
    /// own, so the blocking pool owns it here rather than the async worker.
    ///
    /// # Errors
    /// Returns the Creator owner's own rejection unchanged, or a lost blocking
    /// task. A refused grant means the store never yielded this authority, so
    /// no batch was started.
    pub async fn execute_manual_creator_batch_submitted(
        self: &Arc<Self>,
        grant: SubmittedOwnerGrant,
    ) -> Result<ManualCreatorBatchCreateResult, String> {
        let action = grant
            .into_creator_batch()
            .map_err(|error| error.to_string())?;
        let PreparedCreatorBatchAction { intent, .. } = action;
        let creators = Arc::clone(&self.creators);
        tokio::task::spawn_blocking(move || creators.create_manual_creator_batch(intent))
            .await
            .map_err(|_| {
                "the Creator batch outcome is unavailable; read the Creators before retrying"
                    .to_owned()
            })?
    }
}
