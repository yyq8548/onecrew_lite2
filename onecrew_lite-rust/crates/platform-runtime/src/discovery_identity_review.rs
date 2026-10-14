//! Authenticated composition of the existing identity-review owner.
use onecrew_creator_runtime::{
    DiscoveryIdentityGroupsSnapshot, DiscoveryIdentityMergeMaterial,
    DiscoveryIdentityMergeMaterialInput, DiscoveryIdentityReviewReceipt,
    GetDiscoveryIdentityGroupsInput, GetDiscoveryIdentityReviewInput,
    RevokeDiscoveryIdentityReviewInput, SubmitDiscoveryIdentityReviewInput,
};

use crate::LocalPlatform;

impl LocalPlatform {
    async fn verify_identity_review_account(
        &self,
        account: &str,
        wrote: bool,
    ) -> Result<(), String> {
        let code = if wrote {
            "identity_review_account_changed_after_write"
        } else {
            "identity_review_account_changed"
        };
        if self.library_access_user().await.map_err(|_| code)? != account {
            return Err(code.into());
        }
        Ok(())
    }

    pub async fn prepare_discovery_identity_merge_material(
        &self,
        input: DiscoveryIdentityMergeMaterialInput,
    ) -> Result<DiscoveryIdentityMergeMaterial, String> {
        let account = self.library_access_user().await?;
        let material = self
            .creators()?
            .prepare_discovery_identity_merge_material_async(input)
            .await
            .map_err(|_| "identity_review_material_unavailable")?;
        self.verify_identity_review_account(&account, false).await?;
        Ok(material)
    }

    pub async fn get_discovery_identity_groups(
        &self,
        input: GetDiscoveryIdentityGroupsInput,
    ) -> Result<DiscoveryIdentityGroupsSnapshot, String> {
        let account = self.library_access_user().await?;
        let groups = self
            .creators()?
            .get_discovery_identity_groups_async(input.campaign_id, input.expected_revision)
            .await
            .map_err(|_| "identity_review_groups_unavailable")?;
        self.verify_identity_review_account(&account, false).await?;
        Ok(groups)
    }

    pub async fn get_discovery_identity_review(
        &self,
        input: GetDiscoveryIdentityReviewInput,
    ) -> Result<Option<DiscoveryIdentityReviewReceipt>, String> {
        let account = self.library_access_user().await?;
        let receipt = self
            .creators()?
            .get_discovery_identity_review_async(
                account.clone(),
                input.campaign_id,
                input.decision_id,
            )
            .await
            .map_err(|_| "identity_review_read_unavailable")?;
        self.verify_identity_review_account(&account, false).await?;
        Ok(receipt)
    }

    pub async fn submit_discovery_identity_review(
        &self,
        input: SubmitDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, String> {
        let account = self.library_access_user().await?;
        let receipt = self
            .creators()?
            .submit_discovery_identity_review_async(account.clone(), input)
            .await
            .map_err(|_| "identity_review_submit_outcome_unknown")?;
        self.verify_identity_review_account(&account, true).await?;
        Ok(receipt)
    }

    pub async fn revoke_discovery_identity_review(
        &self,
        input: RevokeDiscoveryIdentityReviewInput,
    ) -> Result<DiscoveryIdentityReviewReceipt, String> {
        let account = self.library_access_user().await?;
        let receipt = self
            .creators()?
            .revoke_discovery_identity_review_async(account.clone(), input)
            .await
            .map_err(|_| "identity_review_revoke_outcome_unknown")?;
        self.verify_identity_review_account(&account, true).await?;
        Ok(receipt)
    }
}

#[cfg(test)]
#[path = "discovery_identity_review/authenticated_tests.rs"]
mod authenticated_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_creator_runtime::{
        DiscoveryIdentityDecision, DiscoveryReviewer, DiscoveryReviewerKind,
    };

    #[tokio::test]
    async fn identity_review_entrypoints_reject_missing_authenticated_account() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        let pair = DiscoveryIdentityMergeMaterialInput {
            campaign_id: "campaign".into(),
            expected_revision: 1,
            left_search_hit_id: "left".into(),
            right_search_hit_id: "right".into(),
        };
        let reviewer = || DiscoveryReviewer {
            kind: DiscoveryReviewerKind::Human,
            label: "reviewer".into(),
        };
        assert_eq!(
            platform
                .prepare_discovery_identity_merge_material(pair.clone())
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
        assert_eq!(
            platform
                .get_discovery_identity_groups(GetDiscoveryIdentityGroupsInput {
                    campaign_id: "campaign".into(),
                    expected_revision: 1
                })
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
        assert_eq!(
            platform
                .get_discovery_identity_review(GetDiscoveryIdentityReviewInput {
                    campaign_id: "campaign".into(),
                    decision_id: "decision".into()
                })
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
        assert_eq!(
            platform
                .submit_discovery_identity_review(SubmitDiscoveryIdentityReviewInput {
                    pair,
                    material_hash: "0".repeat(64),
                    decision_id: "decision".into(),
                    reviewer: reviewer(),
                    decision: DiscoveryIdentityDecision::Insufficient,
                    rationale: "missing evidence".into(),
                    findings: vec![]
                })
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
        assert_eq!(
            platform
                .revoke_discovery_identity_review(RevokeDiscoveryIdentityReviewInput {
                    campaign_id: "campaign".into(),
                    expected_revision: 1,
                    decision_id: "revoke".into(),
                    revokes_decision_id: "decision".into(),
                    reviewer: reviewer(),
                    rationale: "correction".into()
                })
                .await
                .unwrap_err(),
            "library_identity_unavailable"
        );
    }
}
