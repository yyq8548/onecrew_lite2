//! The Web entry point for one approved, read-only OpenCLI discovery step.
use crate::ServiceState;
use onecrew_creator_runtime::{
    CreateDiscoveryCampaignInput, DiscoveryCampaign, DiscoveryCandidate, DiscoveryCandidateCounts,
    DiscoverySearchHit, ListDiscoveryCandidatesInput, ListDiscoverySearchHitsInput,
};
use onecrew_local_runtime::OpenCliDiscoverySearchResult;

pub(crate) async fn identity_material_get(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryIdentityMergeMaterialInput,
) -> Result<onecrew_creator_runtime::DiscoveryIdentityMergeMaterial, String> {
    state
        .platform()
        .await?
        .prepare_discovery_identity_merge_material(input)
        .await
}

pub(crate) async fn identity_groups_get(
    state: &ServiceState,
    input: onecrew_creator_runtime::GetDiscoveryIdentityGroupsInput,
) -> Result<onecrew_creator_runtime::DiscoveryIdentityGroupsSnapshot, String> {
    state
        .platform()
        .await?
        .get_discovery_identity_groups(input)
        .await
}

pub(crate) async fn identity_review_get(
    state: &ServiceState,
    input: onecrew_creator_runtime::GetDiscoveryIdentityReviewInput,
) -> Result<Option<onecrew_creator_runtime::DiscoveryIdentityReviewReceipt>, String> {
    state
        .platform()
        .await?
        .get_discovery_identity_review(input)
        .await
}

pub(crate) async fn identity_review_submit(
    state: &ServiceState,
    input: onecrew_creator_runtime::SubmitDiscoveryIdentityReviewInput,
) -> Result<onecrew_creator_runtime::DiscoveryIdentityReviewReceipt, String> {
    state
        .platform()
        .await?
        .submit_discovery_identity_review(input)
        .await
}

pub(crate) async fn identity_review_revoke(
    state: &ServiceState,
    input: onecrew_creator_runtime::RevokeDiscoveryIdentityReviewInput,
) -> Result<onecrew_creator_runtime::DiscoveryIdentityReviewReceipt, String> {
    state
        .platform()
        .await?
        .revoke_discovery_identity_review(input)
        .await
}

pub(crate) async fn validate_intent(
    state: &ServiceState,
    input: onecrew_creator_runtime::ValidateDiscoveryIntentInput,
) -> Result<onecrew_creator_runtime::DiscoveryIntentValidationResult, String> {
    state
        .platform()
        .await?
        .validate_discovery_intent(input)
        .await
}

pub(crate) async fn submit_review(
    state: &ServiceState,
    input: onecrew_creator_runtime::SubmitDiscoveryReviewInput,
) -> Result<onecrew_creator_runtime::DiscoveryReviewReceipt, String> {
    state.platform().await?.submit_discovery_review(input).await
}

pub(crate) async fn get_review(
    state: &ServiceState,
    input: onecrew_creator_runtime::GetDiscoveryReviewInput,
) -> Result<Option<onecrew_creator_runtime::DiscoveryReviewReceipt>, String> {
    state.platform().await?.get_discovery_review(input).await
}

pub(crate) async fn evaluate_match(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryReviewMaterialInput,
) -> Result<Option<onecrew_creator_runtime::DiscoveryReviewReceipt>, String> {
    let credential = crate::discovery_execution::account(state).await?;
    let result = state
        .platform()
        .await?
        .evaluate_discovery_match(input)
        .await;
    if let Ok(Some(receipt)) = &result {
        state.discovery_execution.resolve_score(
            &credential,
            &receipt.campaign_id,
            receipt.criteria_revision,
            &receipt.search_hit_id,
        );
    }
    result
}

pub(crate) async fn reevaluate_match(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryReviewMaterialInput,
) -> Result<Option<onecrew_creator_runtime::DiscoveryReviewReceipt>, String> {
    state
        .platform()
        .await?
        .reevaluate_discovery_match(input)
        .await
}

pub(crate) async fn sync_library(
    state: &ServiceState,
    campaign_id: String,
) -> Result<onecrew_local_runtime::DiscoveryLibrarySyncResult, String> {
    let credential = crate::discovery_execution::account(state).await?;
    let snapshot = state.discovery_execution.snapshot(&credential);
    let result = state
        .platform()
        .await?
        .sync_discovery_library(campaign_id.clone())
        .await;
    // Register only after the local receipt is published. The independent worker
    // checks the accepted policy before uploading.
    if result.is_ok()
        && crate::table_sync::register_result(state, &campaign_id, &credential)
            .await
            .is_err()
    {
        log::warn!("recommendation table background registration failed");
    }
    if let Some(run) = snapshot
        && run.campaign_id == campaign_id
    {
        let advanced = state
            .discovery_execution
            .snapshot(&credential)
            .is_some_and(|current| {
                current.used != run.used || current.updated_at != run.updated_at
            });
        state
            .discovery_execution
            .record_sync(&credential, &campaign_id, run.revision, &result);
        if advanced {
            state
                .discovery_execution
                .mark_sync_pending(&credential, &campaign_id, run.revision);
        }
    }
    result
}

pub(crate) async fn get_profile(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryProfileReadInput,
) -> Result<onecrew_creator_runtime::DiscoveryProfileRead, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_profile_async(input.campaign_id, input.search_hit_id)
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn get_identity(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryIdentityReadInput,
) -> Result<onecrew_creator_runtime::DiscoveryIdentityRead, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_identity_async(input.campaign_id, input.search_hit_id)
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn get_content(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryContentReadInput,
) -> Result<onecrew_creator_runtime::DiscoveryContentRead, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_content_async(input.campaign_id, input.search_hit_id)
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn get_transcript(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryTranscriptReadInput,
) -> Result<onecrew_creator_runtime::DiscoveryTranscriptRead, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_transcript_async(input.campaign_id, input.search_hit_id)
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn get_public_page(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryPublicPageReadInput,
) -> Result<onecrew_creator_runtime::DiscoveryPublicPageRead, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_public_page_async(input.campaign_id, input.candidate_id, input.requested_url)
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn get_review_material(
    state: &ServiceState,
    input: onecrew_creator_runtime::DiscoveryReviewMaterialInput,
) -> Result<onecrew_creator_runtime::DiscoveryReviewMaterial, String> {
    state
        .platform()
        .await?
        .creators()?
        .prepare_discovery_review_material_async(
            input.campaign_id,
            input.expected_revision,
            input.search_hit_id,
        )
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn advance(
    state: &ServiceState,
    input: onecrew_local_runtime::OpenCliDiscoveryAdvanceInput,
) -> Result<onecrew_local_runtime::OpenCliDiscoveryAdvanceResult, String> {
    advance_deferred(state, input, vec![]).await
}

pub(crate) async fn advance_deferred(
    state: &ServiceState,
    input: onecrew_local_runtime::OpenCliDiscoveryAdvanceInput,
    deferred_platforms: Vec<String>,
) -> Result<onecrew_local_runtime::OpenCliDiscoveryAdvanceResult, String> {
    advance_scoring(state, input, deferred_platforms, vec![]).await
}

pub(crate) async fn advance_scoring(
    state: &ServiceState,
    input: onecrew_local_runtime::OpenCliDiscoveryAdvanceInput,
    deferred_platforms: Vec<String>,
    scoring_hits: Vec<String>,
) -> Result<onecrew_local_runtime::OpenCliDiscoveryAdvanceResult, String> {
    let campaign = input.campaign_id.clone();
    state
        .platform()
        .await?
        .advance_discovery_campaign_scoring(input, deferred_platforms, scoring_hits)
        .await
        .map_err(|error| {
            let code = advance_failure_code(&error);
            let step = match &error {
                onecrew_local_runtime::OpenCliDiscoverySearchError::Advance { step, .. } => {
                    Some(step.as_ref())
                }
                _ => None,
            };
            crate::discovery_execution::record_read_failure(state, &campaign, step, &code, None);
            code
        })
}

fn advance_failure_code(error: &onecrew_local_runtime::OpenCliDiscoverySearchError) -> String {
    use onecrew_local_runtime::OpenCliDiscoverySearchError;
    match error {
        OpenCliDiscoverySearchError::Advance { failure, .. } => advance_failure_code(failure),
        OpenCliDiscoverySearchError::Dependency(error)
        | OpenCliDiscoverySearchError::Provider(error) => {
            onecrew_local_runtime::discovery_provider_stop_code(error).into()
        }
        OpenCliDiscoverySearchError::Campaign(_) => "discovery_read_outcome_unknown".into(),
    }
}

pub(crate) async fn create(
    state: &ServiceState,
    input: CreateDiscoveryCampaignInput,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .creators()?
        .create_discovery_campaign_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn approve(
    state: &ServiceState,
    id: String,
    expected_revision: u32,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .creators()?
        .approve_discovery_campaign_async(id, expected_revision)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn increase_target(
    state: &ServiceState,
    input: onecrew_creator_runtime::IncreaseDiscoveryTargetInput,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .increase_discovery_target(input)
        .await
}

pub(crate) async fn increase_budget(
    state: &ServiceState,
    input: onecrew_creator_runtime::IncreaseDiscoveryBudgetInput,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .increase_discovery_budget(input)
        .await
}

pub(crate) async fn get(state: &ServiceState, id: String) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .creators()?
        .get_discovery_campaign_async(id)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn recent(
    state: &ServiceState,
) -> Result<Vec<onecrew_creator_runtime::DiscoveryCampaignSummary>, String> {
    state
        .platform()
        .await?
        .creators()?
        .recent_discovery_campaigns_async()
        .await
        .map_err(|_| "discovery_read_unavailable".into())
}

pub(crate) async fn search_youtube_once(
    state: &ServiceState,
    id: String,
    expected_revision: u32,
    profile_ref: String,
    limit: u8,
) -> Result<OpenCliDiscoverySearchResult, String> {
    state
        .platform()
        .await?
        .search_discovery_youtube_once(&id, expected_revision, &profile_ref, limit)
        .await
        .map_err(|error| format!("{error:?}"))
}

pub(crate) async fn search_once(
    state: &ServiceState,
    id: String,
    expected_revision: u32,
    platform: String,
    profile_ref: String,
    limit: u8,
) -> Result<OpenCliDiscoverySearchResult, String> {
    state
        .platform()
        .await?
        .search_discovery_once(&id, expected_revision, &platform, &profile_ref, limit)
        .await
        .map_err(|error| format!("{error:?}"))
}

pub(crate) async fn verify_youtube_hit_once(
    state: &ServiceState,
    id: String,
    expected_revision: u32,
    profile_ref: String,
    search_hit_id: String,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .verify_discovery_youtube_hit_once(&id, expected_revision, &profile_ref, &search_hit_id)
        .await
        .map_err(|error| format!("{error:?}"))
}

pub(crate) async fn list_hits(
    state: &ServiceState,
    input: ListDiscoverySearchHitsInput,
) -> Result<Vec<DiscoverySearchHit>, String> {
    state
        .platform()
        .await?
        .creators()?
        .list_discovery_search_hits_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn list_candidates(
    state: &ServiceState,
    input: ListDiscoveryCandidatesInput,
) -> Result<Vec<DiscoveryCandidate>, String> {
    state
        .platform()
        .await?
        .creators()?
        .list_discovery_candidates_async(input)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn count_candidates(
    state: &ServiceState,
    campaign_id: String,
) -> Result<DiscoveryCandidateCounts, String> {
    state
        .platform()
        .await?
        .creators()?
        .count_discovery_candidates_async(campaign_id)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn prepare_draft(
    state: &ServiceState,
    input: onecrew_local_runtime::PrepareDiscoveryDraftInput,
) -> Result<DiscoveryCampaign, String> {
    state
        .platform()
        .await?
        .prepare_discovery_draft(input)
        .await
        .map_err(|error| error.code().to_owned())
}

pub(crate) async fn draft_progress(
    state: &ServiceState,
    id: String,
) -> Result<Option<onecrew_local_runtime::DraftProgress>, String> {
    state.platform().await?.discovery_draft_progress(id).await
}

#[cfg(test)]
mod contract_manifest_tests {
    use onecrew_creator_runtime::{
        DiscoveryIdentityGroupsSnapshot, DiscoveryReviewMaterial, DiscoveryReviewMaterialInput,
        DiscoveryReviewReceipt, GetDiscoveryIdentityGroupsInput, GetDiscoveryReviewInput,
        SubmitDiscoveryReviewInput,
    };
    use onecrew_creator_runtime::{
        DiscoveryIdentityMergeMaterial, DiscoveryIdentityMergeMaterialInput,
        DiscoveryIdentityReviewReceipt, GetDiscoveryIdentityReviewInput,
        RevokeDiscoveryIdentityReviewInput, SubmitDiscoveryIdentityReviewInput,
    };
    use onecrew_schema_manifest::{SchemaDirection, SchemaManifest, contract_schema_root};

    #[test]
    fn web_discovery_contract_manifest_is_exact_and_exportable() {
        let manifest = SchemaManifest::new(
            "web-discovery",
            vec![
                contract_schema_root::<crate::discovery_execution::ExecutionInput>(
                    "ExecutionInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<crate::discovery_execution::ExecutionStatus>(
                    "ExecutionStatus",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<onecrew_local_runtime::DraftProgress>(
                    "DraftProgress",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<crate::components::ComponentsReport>(
                    "ComponentsReport",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<crate::discovery_evidence::CollectEvidenceInput>(
                    "CollectEvidenceInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<crate::discovery_evidence::EvidenceCollection>(
                    "EvidenceCollection",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryIdentityMergeMaterialInput>(
                    "DiscoveryIdentityMergeMaterialInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryIdentityMergeMaterial>(
                    "DiscoveryIdentityMergeMaterial",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<GetDiscoveryIdentityReviewInput>(
                    "GetDiscoveryIdentityReviewInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<SubmitDiscoveryIdentityReviewInput>(
                    "SubmitDiscoveryIdentityReviewInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<RevokeDiscoveryIdentityReviewInput>(
                    "RevokeDiscoveryIdentityReviewInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryIdentityReviewReceipt>(
                    "DiscoveryIdentityReviewReceipt",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<GetDiscoveryIdentityGroupsInput>(
                    "GetDiscoveryIdentityGroupsInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryIdentityGroupsSnapshot>(
                    "DiscoveryIdentityGroupsSnapshot",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryReviewMaterialInput>(
                    "DiscoveryReviewMaterialInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryReviewMaterial>(
                    "DiscoveryReviewMaterial",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
                contract_schema_root::<SubmitDiscoveryReviewInput>(
                    "SubmitDiscoveryReviewInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<GetDiscoveryReviewInput>(
                    "GetDiscoveryReviewInput",
                    SchemaDirection::Deserialize,
                )
                .unwrap(),
                contract_schema_root::<DiscoveryReviewReceipt>(
                    "DiscoveryReviewReceipt",
                    SchemaDirection::Serialize,
                )
                .unwrap(),
            ],
        )
        .unwrap();
        assert_eq!(manifest.roots.len(), 19);
        let encoded = serde_json::to_vec_pretty(&manifest).unwrap();
        if let Some(path) = std::env::var_os("ONECREW_WEB_DISCOVERY_MANIFEST_OUT") {
            let path = std::path::PathBuf::from(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, encoded).unwrap();
        }
    }
}
