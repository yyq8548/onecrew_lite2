use onecrew_creator_runtime::{
    DiscoveryCampaign, DiscoveryCampaignIdInput, DiscoveryCandidate, DiscoveryCandidateCounts,
    DiscoverySearchHit, ListDiscoveryCandidatesInput, ListDiscoverySearchHitsInput,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::{
    super::RegistryOwners,
    shared::{classify_string_failure, metadata, read_policy},
};
use crate::operation_registry::{
    descriptor::SurfaceId,
    registration::{RegistryBuildError, RegistryBuilder},
};

const SURFACES: [SurfaceId; 3] = [SurfaceId::Full, SurfaceId::Compact, SurfaceId::Creators];
const FAILURE_CODE: &str = "discovery_read_unavailable";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DiscoverySourcesInput {}

pub(super) fn register(
    builder: &mut RegistryBuilder,
    owners: RegistryOwners,
) -> Result<(), RegistryBuildError> {
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryIdentityMergeMaterialInput, onecrew_creator_runtime::DiscoveryIdentityMergeMaterial, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_MATERIAL_GET, "creators", SURFACES),
        read_policy(),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.prepare_discovery_identity_merge_material(input).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::GetDiscoveryIdentityGroupsInput, onecrew_creator_runtime::DiscoveryIdentityGroupsSnapshot, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_GROUPS_GET, "creators", SURFACES),
        read_policy(),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.get_discovery_identity_groups(input).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::GetDiscoveryIdentityReviewInput, Option<onecrew_creator_runtime::DiscoveryIdentityReviewReceipt>, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_REVIEW_GET, "creators", SURFACES),
        read_policy(),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.get_discovery_identity_review(input).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::ValidateDiscoveryIntentInput, onecrew_creator_runtime::DiscoveryIntentValidationResult, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_INTENT_VALIDATE, "creators", SURFACES),
        read_policy(),
        move |_context, input| {
            let platform = platform.clone();
            async move { platform.validate_discovery_intent(input).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<DiscoverySourcesInput, onecrew_local_runtime::OpenCliDiscoverySources, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_SOURCES_GET, "creators", SURFACES),
        read_policy(),
        move |_context, _input| {
            let platform = platform.clone();
            async move { Ok::<_, String>(platform.opencli_setup_report().await.into()) }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let platform = owners.platform.clone();
    builder.register_operation::<onecrew_creator_runtime::GetDiscoveryReviewInput, Option<onecrew_creator_runtime::DiscoveryReviewReceipt>, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let platform = platform.clone();
            async move { platform.get_discovery_review(input).await }
        },
        classify_string_failure(FAILURE_CODE),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryReviewMaterialInput, onecrew_creator_runtime::DiscoveryReviewMaterial, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_REVIEW_MATERIAL_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.prepare_discovery_review_material_async(input.campaign_id,input.expected_revision,input.search_hit_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryPublicPageReadInput, onecrew_creator_runtime::DiscoveryPublicPageRead, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_PUBLIC_PAGE_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.get_discovery_public_page_async(input.campaign_id,input.candidate_id,input.requested_url).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryIdentityReadInput, onecrew_creator_runtime::DiscoveryIdentityRead, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_IDENTITY_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.get_discovery_identity_async(input.campaign_id,input.search_hit_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryContentReadInput, onecrew_creator_runtime::DiscoveryContentRead, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CONTENT_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.get_discovery_content_async(input.campaign_id,input.search_hit_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryProfileReadInput, onecrew_creator_runtime::DiscoveryProfileRead, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_PROFILE_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.get_discovery_profile_async(input.campaign_id,input.search_hit_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<onecrew_creator_runtime::DiscoveryTranscriptReadInput, onecrew_creator_runtime::DiscoveryTranscriptRead, _, _, _, _>(
        metadata(onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_TRANSCRIPT_GET,"creators",SURFACES),
        read_policy(),
        move |_context,input| {
            let creators = creators.clone();
            async move { creators.get_discovery_transcript_async(input.campaign_id,input.search_hit_id).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(FAILURE_CODE)),
    )?;
    let creators = owners.creators.clone();
    builder.register_operation::<DiscoveryCampaignIdInput, DiscoveryCampaign, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CAMPAIGN_GET,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move {
                creators
                    .get_discovery_campaign_async(input.campaign_id)
                    .await
            }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators.clone();
    builder
        .register_operation::<ListDiscoveryCandidatesInput, Vec<DiscoveryCandidate>, _, _, _, _>(
            metadata(
                onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CANDIDATES_LIST,
                "creators",
                SURFACES,
            ),
            read_policy(),
            move |_context, input| {
                let creators = creators.clone();
                async move { creators.list_discovery_candidates_async(input).await }
            },
            crate::operation_registry::execution::classify_repository_call(
                classify_string_failure(FAILURE_CODE),
            ),
        )?;

    let creators = owners.creators.clone();
    builder.register_operation::<DiscoveryCampaignIdInput, DiscoveryCandidateCounts, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_CANDIDATES_COUNT,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move {
                creators
                    .count_discovery_candidates_async(input.campaign_id)
                    .await
            }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )?;

    let creators = owners.creators;
    builder.register_operation::<ListDiscoverySearchHitsInput, Vec<DiscoverySearchHit>, _, _, _, _>(
        metadata(
            onecrew_local_runtime::OperationId::ONECREW_DISCOVERY_HITS_LIST,
            "creators",
            SURFACES,
        ),
        read_policy(),
        move |_context, input| {
            let creators = creators.clone();
            async move { creators.list_discovery_search_hits_async(input).await }
        },
        crate::operation_registry::execution::classify_repository_call(classify_string_failure(
            FAILURE_CODE,
        )),
    )
}
