use std::{fmt, time::Instant};

use onecrew_creator_runtime::ManualCreatorBatchCreateInput;

/// Only runtime preparation constructs this authority. Its fields stay private
/// to the Creator batch owner, and no wire decoder or `Clone` can mint a second
/// batch.
///
/// The sealed `intent` carries the batch's own `idempotency_key`, so the key the
/// owner reviewed is the key execution claims under; a resumed call cannot
/// substitute a different one and write the same Creators twice.
pub(crate) struct PreparedCreatorBatchAction {
    pub(super) intent: ManualCreatorBatchCreateInput,
    pub(super) expires: Instant,
}

impl PreparedCreatorBatchAction {
    /// Sealed bytes for the store's payload budget and digest. The tag keeps
    /// this authority distinct from every other owner's encoding.
    pub(crate) fn encoded_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&("creator_batch_authority_v1", &self.intent))
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.expires
    }
}

impl fmt::Debug for PreparedCreatorBatchAction {
    /// Creator names, notes, and contact routes never reach a log through this
    /// type; only how many rows it holds and when it stops being usable.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedCreatorBatchAction")
            .field("items", &self.intent.items.len())
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}
