//! Authenticated composition for the existing Creator review owner.
use onecrew_creator_runtime::{
    DiscoveryCampaign, DiscoveryReviewReceipt, GetDiscoveryReviewInput,
    IncreaseDiscoveryBudgetInput, IncreaseDiscoveryTargetInput, SubmitDiscoveryReviewInput,
};

use crate::LocalPlatform;

impl LocalPlatform {
    pub async fn increase_discovery_target(
        &self,
        input: IncreaseDiscoveryTargetInput,
    ) -> Result<DiscoveryCampaign, String> {
        let account = self.library_access_user().await?;
        let campaign = self
            .creators()?
            .increase_discovery_target_async(account.clone(), input)
            .await
            .map_err(|error| match error {
                onecrew_creator_runtime::CreatorRepositoryCallError::Owner(error) => error,
                onecrew_creator_runtime::CreatorRepositoryCallError::Dispatch(_) => {
                    "target_increase_outcome_unknown".to_owned()
                }
            })?;
        if self
            .library_access_user()
            .await
            .map_err(|_| "target_increase_account_changed".to_owned())?
            != account
        {
            return Err("target_increase_account_changed".into());
        }
        Ok(campaign)
    }

    pub async fn increase_discovery_budget(
        &self,
        input: IncreaseDiscoveryBudgetInput,
    ) -> Result<DiscoveryCampaign, String> {
        let account = self.library_access_user().await?;
        let campaign = self
            .creators()?
            .increase_discovery_budget_async(account.clone(), input)
            .await
            .map_err(|_| "budget_increase_outcome_unknown".to_owned())?;
        if self.library_access_user().await? != account {
            return Err("budget_increase_account_changed".into());
        }
        Ok(campaign)
    }

    pub async fn submit_discovery_review(
        &self,
        input: SubmitDiscoveryReviewInput,
    ) -> Result<DiscoveryReviewReceipt, String> {
        let account = self.library_access_user().await?;
        let receipt = self
            .creators()?
            .submit_discovery_review_async(account.clone(), input)
            .await
            .map_err(review_submission_error)?;
        if self.library_access_user().await? != account {
            return Err("review_account_changed_after_submit".into());
        }
        Ok(receipt)
    }

    pub async fn get_discovery_review(
        &self,
        input: GetDiscoveryReviewInput,
    ) -> Result<Option<DiscoveryReviewReceipt>, String> {
        let account = self.library_access_user().await?;
        let receipt = self
            .creators()?
            .get_discovery_review_async(account.clone(), input.campaign_id, input.request_id)
            .await
            .map_err(|_| "review_read_unavailable".to_owned())?;
        if self.library_access_user().await? != account {
            return Err("review_account_changed".into());
        }
        Ok(receipt)
    }
}

// These owner errors are emitted before the transaction commits. Dispatch and
// storage failures still require receipt readback; they must never be retried.
fn review_submission_error(
    error: onecrew_creator_runtime::CreatorRepositoryCallError<String>,
) -> String {
    match error {
        onecrew_creator_runtime::CreatorRepositoryCallError::Owner(code)
            if matches!(
                code.as_str(),
                "initial_match_single_finding_required"
                    | "initial_match_finding_invalid"
                    | "initial_match_required_failure_must_be_low"
                    | "initial_match_required_count_unknown"
                    | "initial_match_author_evidence_required"
                    | "review_material_changed"
                    | "review_input_too_large"
                    | "review_input_invalid"
                    | "review_conditions_mismatch"
                    | "review_citation_invalid"
                    | "review_first_hand_evidence_required"
                    | "review_transcript_required"
                    | "review_match_level_invalid"
            ) =>
        {
            code
        }
        _ => "review_submit_outcome_unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejected_review_is_not_reported_as_an_uncertain_commit() {
        use crate::discovery_identity_test_support::{authenticate, saved_pair};
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        authenticate(&platform, "fixture-account");
        let pair = saved_pair(&platform);
        let request = SubmitDiscoveryReviewInput {
            campaign_id: pair.campaign_id.clone(),
            expected_revision: 1,
            search_hit_id: pair.left_search_hit_id,
            material_hash: "0".repeat(64),
            request_id: "known-rejection".into(),
            reviewer: onecrew_creator_runtime::DiscoveryReviewer {
                kind: onecrew_creator_runtime::DiscoveryReviewerKind::Agent,
                label: onecrew_creator_runtime::INITIAL_MATCH_REVIEWER.into(),
            },
            findings: vec![],
        };
        assert_eq!(
            platform.submit_discovery_review(request).await.unwrap_err(),
            "review_material_changed"
        );
        assert!(
            platform
                .get_discovery_review(GetDiscoveryReviewInput {
                    campaign_id: pair.campaign_id,
                    request_id: "known-rejection".into()
                })
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn review_entrypoints_require_an_active_account() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        let target = platform
            .increase_discovery_target(IncreaseDiscoveryTargetInput {
                campaign_id: "campaign".into(),
                expected_revision: 1,
                expected_target_count: 30,
                new_target_count: 200,
                decision_id: "target-200".into(),
            })
            .await;
        assert_eq!(target.unwrap_err(), "library_identity_unavailable");
        let get = platform
            .get_discovery_review(GetDiscoveryReviewInput {
                campaign_id: "campaign".into(),
                request_id: "request".into(),
            })
            .await;
        assert_eq!(get.unwrap_err(), "library_identity_unavailable");
        let submit = platform
            .submit_discovery_review(SubmitDiscoveryReviewInput {
                campaign_id: "campaign".into(),
                expected_revision: 1,
                search_hit_id: "hit".into(),
                material_hash: "0".repeat(64),
                request_id: "request".into(),
                reviewer: onecrew_creator_runtime::DiscoveryReviewer {
                    kind: onecrew_creator_runtime::DiscoveryReviewerKind::Human,
                    label: "reviewer".into(),
                },
                findings: vec![],
            })
            .await;
        assert_eq!(submit.unwrap_err(), "library_identity_unavailable");
    }
}
