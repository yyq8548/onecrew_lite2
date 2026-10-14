//! Source-bound preparation for identity review. A pair is not a merge decision.
//! No name, shared sponsor URL or search result may create an identity edge.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CreatorRepositoryCallError, CreatorRuntime, DiscoveryReviewMaterial};

pub(crate) mod groups;
mod queries;
pub(crate) mod relationships;
pub use groups::{
    DiscoveryIdentityGroup, DiscoveryIdentityGroupCounts, DiscoveryIdentityGroupMember,
    DiscoveryIdentityGroupReview, DiscoveryIdentityGroupReviewState, DiscoveryIdentityGroupsBasis,
    DiscoveryIdentityGroupsSnapshot,
};
pub use queries::{GetDiscoveryIdentityGroupsInput, GetDiscoveryIdentityReviewInput};
mod revocation;
pub use revocation::{
    DiscoveryIdentityRevocationBasis, PreparedDiscoveryIdentityRevocation,
    RevokeDiscoveryIdentityReviewInput,
};
mod submission;
pub use submission::{
    DiscoveryIdentityDecision, DiscoveryIdentityFinding, DiscoveryIdentityQuote,
    DiscoveryIdentityRelationship, DiscoveryIdentityReviewKind, DiscoveryIdentityReviewReceipt,
    DiscoveryIdentitySubmissionBasis, PreparedDiscoveryIdentityReview,
    StoredDiscoveryIdentityReview, SubmitDiscoveryIdentityReviewInput,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryIdentityMergeMaterialInput {
    #[schemars(length(min = 1, max = 128))]
    pub campaign_id: String,
    #[schemars(range(min = 1))]
    pub expected_revision: u32,
    #[schemars(length(min = 1, max = 128))]
    pub left_search_hit_id: String,
    #[schemars(length(min = 1, max = 128))]
    pub right_search_hit_id: String,
}

/// Both accounts retain their native identity and full source-bound evidence.
/// This output cannot be submitted as client-provided facts or a verdict.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DiscoveryIdentityMergeMaterial {
    pub format_version: u32,
    pub campaign_id: String,
    pub criteria_revision: u32,
    pub accounts: [DiscoveryReviewMaterial; 2],
    pub material_hash: String,
}

fn validate(input: &DiscoveryIdentityMergeMaterialInput) -> Result<(), String> {
    if input.expected_revision == 0
        || [
            &input.campaign_id,
            &input.left_search_hit_id,
            &input.right_search_hit_id,
        ]
        .iter()
        .any(|id| id.trim().is_empty() || id.len() > 128 || id.chars().any(char::is_control))
        || input.left_search_hit_id == input.right_search_hit_id
    {
        return Err("identity_merge_input_invalid".into());
    }
    Ok(())
}

fn prepare_pair(
    input: &DiscoveryIdentityMergeMaterialInput,
    mut left: DiscoveryReviewMaterial,
    mut right: DiscoveryReviewMaterial,
) -> Result<DiscoveryIdentityMergeMaterial, String> {
    if left.campaign_id != input.campaign_id
        || right.campaign_id != input.campaign_id
        || left.criteria_revision != input.expected_revision
        || right.criteria_revision != input.expected_revision
        || left.hit.id != input.left_search_hit_id
        || right.hit.id != input.right_search_hit_id
        || left.criteria != right.criteria
        || left.approved_scopes != right.approved_scopes
        || left.approved_at != right.approved_at
        || left.native_author_id.is_empty()
        || right.native_author_id.is_empty()
    {
        return Err("identity_merge_source_changed".into());
    }
    let left_key = (left.author_platform(), &left.native_author_id);
    let right_key = (right.author_platform(), &right.native_author_id);
    if left_key == right_key || left.candidate_id == right.candidate_id {
        return Err("identity_merge_same_account".into());
    }
    if left_key > right_key {
        std::mem::swap(&mut left, &mut right);
    }
    let accounts = [left, right];
    let material_hash = crate::discovery_library::digest(&(
        1,
        &input.campaign_id,
        input.expected_revision,
        &accounts,
    ))?;
    Ok(DiscoveryIdentityMergeMaterial {
        format_version: 1,
        campaign_id: input.campaign_id.clone(),
        criteria_revision: input.expected_revision,
        accounts,
        material_hash,
    })
}

impl CreatorRuntime {
    /// Saved evidence only: no provider calls, budget claim, merge or verdict write.
    pub fn prepare_discovery_identity_merge_material(
        &self,
        input: DiscoveryIdentityMergeMaterialInput,
    ) -> Result<DiscoveryIdentityMergeMaterial, String> {
        validate(&input)?;
        let [left, right] = self.repository.read_discovery_identity_merge_basis(
            &input.campaign_id,
            input.expected_revision,
            &input.left_search_hit_id,
            &input.right_search_hit_id,
        )?;
        prepare_pair(
            &input,
            crate::discovery_review::build_material(left)?,
            crate::discovery_review::build_material(right)?,
        )
    }

    pub async fn prepare_discovery_identity_merge_material_async(
        &self,
        input: DiscoveryIdentityMergeMaterialInput,
    ) -> Result<DiscoveryIdentityMergeMaterial, CreatorRepositoryCallError<String>> {
        validate(&input).map_err(CreatorRepositoryCallError::Owner)?;
        let repository = self.repository.clone();
        let read_input = input.clone();
        let [left, right] = self
            .repository_read_segment(move || {
                repository.read_discovery_identity_merge_basis(
                    &read_input.campaign_id,
                    read_input.expected_revision,
                    &read_input.left_search_hit_id,
                    &read_input.right_search_hit_id,
                )
            })
            .await?;
        self.repository_cpu_segment(move || {
            prepare_pair(
                &input,
                crate::discovery_review::build_material(left)?,
                crate::discovery_review::build_material(right)?,
            )
        })
        .await
    }
}

#[cfg(test)]
pub(crate) mod tests;
