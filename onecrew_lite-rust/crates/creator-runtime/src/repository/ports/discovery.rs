use crate::{
    CreateDiscoveryCampaignInput, DiscoveryCampaign, DiscoveryCandidate, DiscoveryCandidateCounts,
    DiscoveryPlatformScope, DiscoverySearchClaim, DiscoverySearchHit, DiscoveryTranscriptClaim,
    DiscoveryTranscriptRead, DiscoveryVideoClaim, FinishDiscoverySearchInput,
    FinishDiscoveryTranscriptInput, FinishDiscoveryVideoInput, IncreaseDiscoveryBudgetInput,
    IncreaseDiscoveryTargetInput, ListDiscoveryCandidatesInput, ListDiscoverySearchHitsInput,
    UpsertDiscoveryCandidateInput,
};

pub trait DiscoveryCampaignRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(write))]
    fn append_discovery_queries(&self, _account: &str, _input: &crate::AppendDiscoveryQueries) -> Result<(), String> {
        Err("query_round_unavailable".into())
    }
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn discovery_read_stops(&self, campaign: &str)
    -> Result<Vec<crate::DiscoveryReadStop>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn discovery_recommendation_revision(&self, campaign: &str) -> Result<String, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn discovery_candidate_is_qualified(
        &self,
        campaign: &str,
        candidate: &str,
    ) -> Result<bool, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn discovery_library_table_name(
        &self,
        campaign_id: &str,
    ) -> Result<crate::DiscoveryLibraryTableName, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn recent_discovery_campaigns(&self) -> Result<Vec<crate::DiscoveryCampaignSummary>, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_identity_groups_basis(
        &self,
        campaign: &str,
        revision: u32,
    ) -> Result<crate::DiscoveryIdentityGroupsBasis, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_identity_revocation(
        &self,
        input: &crate::RevokeDiscoveryIdentityReviewInput,
    ) -> Result<crate::DiscoveryIdentityRevocationBasis, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn commit_discovery_identity_revocation(
        &self,
        prepared: &crate::PreparedDiscoveryIdentityRevocation,
    ) -> Result<crate::StoredDiscoveryIdentityReview, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_identity_submission(
        &self,
        pair: &crate::DiscoveryIdentityMergeMaterialInput,
        decision_id: &str,
    ) -> Result<crate::DiscoveryIdentitySubmissionBasis, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_discovery_identity_review(
        &self,
        campaign_id: &str,
        decision_id: &str,
    ) -> Result<Option<crate::StoredDiscoveryIdentityReview>, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn commit_discovery_identity_review(
        &self,
        prepared: &crate::PreparedDiscoveryIdentityReview,
    ) -> Result<crate::StoredDiscoveryIdentityReview, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_review_submission(
        &self,
        account: &str,
        campaign: &str,
        request: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryReviewSubmissionBasis, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_discovery_review(
        &self,
        account: &str,
        campaign: &str,
        request: &str,
    ) -> Result<Option<crate::StoredDiscoveryReview>, String>;

    // Preparation owns its separate read/CPU segments; this port is the single writer phase.
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_discovery_review(
        &self,
        prepared: &crate::PreparedDiscoveryReview,
    ) -> Result<crate::StoredDiscoveryReview, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_review_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        search_hit_id: &str,
    ) -> Result<crate::DiscoveryReviewBasis, String>;

    /// Select only author-isolated saved native work while another author is scored.
    /// Advisory selection composes separate read owners, not one atomic snapshot.
    #[cfg_attr(any(), repository_execution(segmented_read))]
    fn next_discovery_independent_read(
        &self,
        _campaign: &str,
        _revision: u32,
        _scoring_hit: &str,
        _scoring_hits: &[String],
    ) -> Result<Option<crate::DiscoveryStep>, String> {
        Ok(None)
    }

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_identity_merge_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        left_search_hit_id: &str,
        right_search_hit_id: &str,
    ) -> Result<[crate::DiscoveryReviewBasis; 2], String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn read_discovery_library_basis(
        &self,
        campaign_id: &str,
        expected_revision: u32,
    ) -> Result<crate::DiscoveryLibraryBasis, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn reserve_discovery_library_version(
        &self,
        basis: &crate::DiscoveryLibraryBasis,
        snapshot_hash: &str,
    ) -> Result<u64, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn reserve_discovery_library_publication(
        &self,
        basis: &crate::DiscoveryIdentityGroupsBasis,
        source_hash: &str,
        identity_hash: &str,
    ) -> Result<crate::DiscoveryLibraryVersions, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_contact_lookup(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<crate::DiscoveryContactLookup>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_contact_lookup(
        &self,
        input: crate::DiscoveryContactLookup,
    ) -> Result<crate::DiscoveryContactLookup, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_contact_lookup(
        &self,
        input: crate::DiscoveryContactLookup,
    ) -> Result<crate::DiscoveryContactLookup, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_public_page(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<crate::DiscoveryPublicPageInput>, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn discovery_profile_needs_identity(&self, campaign: &str, hit: &str) -> Result<bool, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryIdentityClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_identity(
        &self,
        input: crate::FinishDiscoveryIdentityInput,
    ) -> Result<crate::DiscoveryIdentityRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryIdentityRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_content_hit(
        &self,
        campaign: &str,
        platform: &str,
    ) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_facebook_target_hit(&self, campaign: &str) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_facebook_targets(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_facebook_targets(
        &self,
        claim: crate::DiscoveryProfileClaim,
        urls: Option<Vec<String>>,
        stop: Option<String>,
    ) -> Result<crate::DiscoveryFacebookTargetsRead, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_language(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<Option<crate::DiscoveryLanguageClaim>, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_language(
        &self,
        claim: crate::DiscoveryLanguageClaim,
        sample: Option<crate::DiscoveryLanguageSample>,
        stop: Option<String>,
    ) -> Result<(), String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_content(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryContentClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_content(
        &self,
        input: crate::FinishDiscoveryContentInput,
    ) -> Result<crate::DiscoveryContentRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_content(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryContentRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_source_author_hit(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_facebook_identity_hit(
        &self,
        campaign: &str,
        excluded: &[String],
    ) -> Result<Option<String>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_facebook_identity(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryFacebookIdentityClaim, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_facebook_identity(
        &self,
        input: crate::FinishDiscoveryFacebookIdentityInput,
    ) -> Result<crate::DiscoveryFacebookIdentityRead, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_facebook_identity(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryFacebookIdentityRead, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_source_author(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoverySourceAuthorClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_source_author(
        &self,
        input: crate::FinishDiscoverySourceAuthorInput,
    ) -> Result<crate::DiscoverySourceAuthorRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_source_author(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoverySourceAuthorRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_web_source_hit(
        &self,
        campaign: &str,
        excluded: &[String],
    ) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_web_source(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryWebSourceClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_web_source(
        &self,
        input: crate::FinishDiscoveryWebSourceInput,
    ) -> Result<crate::DiscoveryWebSourceRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_web_source(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryWebSourceRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_profile_hit(
        &self,
        campaign: &str,
        excluded_platforms: &[String],
    ) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_profile(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_profile(
        &self,
        input: crate::FinishDiscoveryProfileInput,
    ) -> Result<crate::DiscoveryProfileRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_profile(
        &self,
        campaign: &str,
        hit: &str,
    ) -> Result<crate::DiscoveryProfileRead, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_public_page(
        &self,
        input: crate::DiscoveryPublicPageInput,
    ) -> Result<crate::DiscoveryPublicPageClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_public_page(
        &self,
        input: crate::FinishDiscoveryPublicPageInput,
    ) -> Result<crate::DiscoveryPublicPageRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_public_page(
        &self,
        campaign: &str,
        candidate: &str,
        url: &str,
    ) -> Result<crate::DiscoveryPublicPageRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_transcript_hit(
        &self,
        campaign_id: &str,
    ) -> Result<Option<DiscoverySearchHit>, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_transcript(
        &self,
        campaign_id: &str,
        revision: u32,
        profile: &str,
        hit: &str,
    ) -> Result<DiscoveryTranscriptClaim, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn finish_discovery_transcript(
        &self,
        input: FinishDiscoveryTranscriptInput,
    ) -> Result<DiscoveryTranscriptRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_discovery_transcript(
        &self,
        campaign_id: &str,
        hit: &str,
    ) -> Result<DiscoveryTranscriptRead, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_search_keyword(
        &self,
        campaign_id: &str,
        scope: &DiscoveryPlatformScope,
    ) -> Result<Option<String>, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn next_discovery_video_hit(
        &self,
        campaign_id: &str,
    ) -> Result<Option<DiscoverySearchHit>, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_video(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        profile_ref: &str,
        search_hit_id: &str,
    ) -> Result<DiscoveryVideoClaim, String>;

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn finish_discovery_video(
        &self,
        input: FinishDiscoveryVideoInput,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_search(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        platform: &str,
        profile_ref: &str,
    ) -> Result<DiscoverySearchClaim, String>;

    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_discovery_search_with_recovery(
        &self,
        campaign_id: &str,
        expected_revision: u32,
        platform: &str,
        profile_ref: &str,
        recovery: Option<&crate::discovery::DiscoverySearchRecovery>,
    ) -> Result<DiscoverySearchClaim, String> {
        if recovery.is_some() {
            return Err("Discovery repair recovery is not allowed".into());
        }
        self.claim_discovery_search(campaign_id, expected_revision, platform, profile_ref)
    }

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn finish_discovery_search(
        &self,
        input: FinishDiscoverySearchInput,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn list_discovery_search_hits(
        &self,
        input: ListDiscoverySearchHitsInput,
    ) -> Result<Vec<DiscoverySearchHit>, String>;

    #[cfg_attr(any(), repository_execution(migration_maintenance))]
    fn interrupt_discovery_searches(&self) -> Result<u32, String>;

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn create_discovery_campaign(
        &self,
        input: CreateDiscoveryCampaignInput,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn approve_discovery_campaign(
        &self,
        id: &str,
        expected_revision: u32,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn increase_discovery_budget(
        &self,
        account: &str,
        input: IncreaseDiscoveryBudgetInput,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn increase_discovery_target(
        &self,
        account: &str,
        input: IncreaseDiscoveryTargetInput,
    ) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn get_discovery_campaign(&self, id: &str) -> Result<DiscoveryCampaign, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_discovery_candidate(
        &self,
        input: UpsertDiscoveryCandidateInput,
    ) -> Result<DiscoveryCandidate, String>;

    #[cfg_attr(any(), repository_execution(write))]
    fn approve_discovery_candidate(
        &self,
        input: crate::ApproveDiscoveryCandidateInput,
    ) -> Result<crate::DiscoveryManualApproval, String>;

    #[cfg_attr(any(), repository_execution(pure_read))]
    fn list_discovery_candidates(
        &self,
        input: ListDiscoveryCandidatesInput,
    ) -> Result<Vec<DiscoveryCandidate>, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn count_discovery_candidates(
        &self,
        campaign_id: &str,
    ) -> Result<DiscoveryCandidateCounts, String>;

    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn count_discovery_candidates_excluding(
        &self,
        campaign_id: &str,
        excluded_platforms: &[String],
    ) -> Result<DiscoveryCandidateCounts, String>;
}
