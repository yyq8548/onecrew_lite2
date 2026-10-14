use super::{
    CancelResultInput, ClaimResultContinuationInput, ClaimedResultContinuation,
    CompleteResultContinuationInput, CompletedResultContinuation, ExecutedResultInput,
    RequestResultEditInput, ResultBinding, ResultChoiceAnswer, ResultCleanupMetrics, ResultOrigin,
    ResultRef, ResultSnapshot, ResultStoreError, ResultTerminalUpdate, SubmittedOwnerAction,
};
use crate::LocalPlatform;

impl LocalPlatform {
    /// Store exactly the owner's immutable review with a peer execution frame.
    /// No caller-provided presentation can replace its reviewed business intent.
    pub fn mint_owner_review(
        &self,
        review: super::PreparedOwnerReview,
        frame: super::ExecutedBatch,
        peer: Option<super::PeerResumePayload>,
        completed_sections: Vec<super::PresentationSection>,
    ) -> Result<ResultRef, ResultStoreError> {
        self.result_store
            .mint_owner_review(review, frame, peer, completed_sections)
    }

    /// Atomically consume only the choices permitted by the current review.
    pub fn submit_owner_review(
        &self,
        binding: &ResultBinding,
        selection: super::OwnerReviewSelection,
    ) -> Result<super::SubmittedOwnerGrant, ResultStoreError> {
        self.result_store.submit_owner_review(binding, selection)
    }

    /// Atomically identify and consume the actual pending authority once.
    pub fn submit_result_review(
        &self,
        binding: &ResultBinding,
        selection: super::OwnerReviewSelection,
    ) -> Result<super::ResultConfirmationSubmission, ResultStoreError> {
        self.result_store.submit_result_review(binding, selection)
    }

    /// Continue one presented result with its next independently reviewed call.
    /// A failed advance leaves the submitted result intact for terminal recovery.
    pub fn advance_owner_review(
        &self,
        submitted_binding: &ResultBinding,
        review: super::PreparedOwnerReview,
        frame: super::ExecutedBatch,
        peer: Option<super::PeerResumePayload>,
        completed_sections: Vec<super::PresentationSection>,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.advance_owner_review(
            submitted_binding,
            review,
            frame,
            peer,
            completed_sections,
        )
    }

    /// Continue a retained generic attended operation (for example Email) in
    /// the same result. This path cannot construct or consume business authority.
    pub fn advance_result_confirmation(
        &self,
        submitted_binding: &ResultBinding,
        input: ExecutedResultInput,
        completed_sections: Vec<super::PresentationSection>,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store
            .advance_result_confirmation(submitted_binding, input, completed_sections)
    }

    /// Read the retained completed prefix without copying the current review.
    pub fn completed_result_sections(
        &self,
        binding: &ResultBinding,
    ) -> Result<Vec<super::PresentationSection>, ResultStoreError> {
        self.result_store.completed_result_sections(binding)
    }

    /// Mint one process-local result after governed execution has produced a
    /// bounded, secret-free projection. This is the only result minting path.
    pub fn mint_execution_result(
        &self,
        input: ExecutedResultInput,
    ) -> Result<ResultRef, ResultStoreError> {
        self.result_store.mint_from_execute(input)
    }

    /// Make an execute-minted result visible once and return the first full
    /// binding for App-only state calls.
    pub fn present_result(
        &self,
        result_ref: &ResultRef,
        origin: &ResultOrigin,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.present_result_with_locale(result_ref, origin, self.presentation_preferences.locale())
    }

    /// Freeze a card's explicit locale on its result record at first presentation.
    /// Reads and transitions retain it independently of the Web preference.
    pub fn present_result_with_locale(
        &self,
        result_ref: &ResultRef,
        origin: &ResultOrigin,
        locale: crate::PresentationLocale,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store
            .present_with_locale(result_ref, origin, locale)
    }

    pub fn read_result(&self, binding: &ResultBinding) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.read(binding)
    }

    pub fn read_current_result(
        &self,
        binding: &ResultBinding,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.current(binding)
    }

    pub fn submit_result_confirmation(
        &self,
        binding: &ResultBinding,
    ) -> Result<SubmittedOwnerAction, ResultStoreError> {
        self.result_store.submit(binding)
    }

    pub fn select_result_options(
        &self,
        binding: &ResultBinding,
        answers: Vec<ResultChoiceAnswer>,
    ) -> Result<SubmittedOwnerAction, ResultStoreError> {
        self.result_store.select(binding, answers)
    }

    pub fn cancel_result(
        &self,
        input: CancelResultInput,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.cancel(input)
    }

    pub fn revise_search_plan_result(
        &self,
        binding: &ResultBinding,
        patch: crate::SearchPlanPatch,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.revise_search_plan(binding, patch)
    }

    pub fn request_result_edit(
        &self,
        input: RequestResultEditInput,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.request_edit(input)
    }

    pub fn transition_result_terminal(
        &self,
        binding: &ResultBinding,
        update: ResultTerminalUpdate,
    ) -> Result<ResultSnapshot, ResultStoreError> {
        self.result_store.terminal(binding, update)
    }

    pub fn claim_result_continuation(
        &self,
        input: ClaimResultContinuationInput,
    ) -> Result<ClaimedResultContinuation, ResultStoreError> {
        self.result_store.claim_continuation(input)
    }

    pub fn complete_result_continuation(
        &self,
        input: CompleteResultContinuationInput,
    ) -> Result<CompletedResultContinuation, ResultStoreError> {
        self.result_store.complete_continuation(input)
    }

    pub fn cleanup_results(&self) -> Result<ResultCleanupMetrics, ResultStoreError> {
        self.result_store.cleanup()
    }
}
