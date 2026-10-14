use super::{
    LocalPlatform, OpenCli, OpenCliDiscoverySearchError, OpenCliError, VERIFIED_OPENCLI_VERSION,
    provider_stop_code,
};
use onecrew_creator_runtime::{DiscoveryIdentityRead, FinishDiscoveryIdentityInput};

impl LocalPlatform {
    /// A claim records one attempted read before dependency/provider execution.
    /// Every failure is durable; no retry or historical candidate rewrite occurs.
    pub async fn read_discovery_identity_once(
        &self,
        campaign_id: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryIdentityRead, OpenCliDiscoverySearchError> {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign_id.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let claim = creators
            .claim_discovery_identity_async(campaign_id.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            cli.twitter_post_identity(&claim.profile_ref, &claim.author_handle, &claim.post_id)
                .await
        }
        .await;
        let (account_id, observed_source_url, stop_code) = match result {
            Ok(value) => (Some(value.account_id), Some(value.source_url), None),
            Err(error) => (None, None, Some(provider_stop_code(&error).to_owned())),
        };
        creators
            .finish_discovery_identity_async(FinishDiscoveryIdentityInput {
                claim,
                account_id,
                observed_source_url,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecrew_creator_runtime::{
        CreateDiscoveryCampaignInput, DiscoveryPlatformScope, ListDiscoverySearchHitsInput,
    };
    use serde_json::json;

    #[tokio::test]
    async fn identity_read_requires_campaign_before_dependency_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        assert!(matches!(
            platform
                .read_discovery_identity_once("missing", 1, "missing")
                .await,
            Err(OpenCliDiscoverySearchError::Campaign(_))
        ));
    }

    #[tokio::test]
    #[ignore = "explicit current Chrome profile; one X search and one native focal identity read into isolated Campaign"]
    async fn campaign_native_post_identity_has_budget_and_restart_readback_live() {
        let profile = std::env::var("OPENCLI_TEST_PROFILE").expect("explicit current profile");
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        let owner = platform.creators().unwrap();
        owner.create_discovery_campaign(CreateDiscoveryCampaignInput {
            id:"identity-live".into(),brief:"Native post-author evidence acceptance, not qualification".into(),
            opencli_version:VERIFIED_OPENCLI_VERSION.into(),criteria:json!({"min_followers":10000,"requires_ai_tool_demo":true,"requires_public_tool_url":true}),
            scopes:vec![DiscoveryPlatformScope {platform:"twitter".into(),profile_ref:profile.clone(),keywords:vec!["AI tools".into()]}],
            target_count:30,max_calls: Some(2),
        }).unwrap();
        owner
            .approve_discovery_campaign("identity-live", 1)
            .unwrap();
        platform
            .search_discovery_once("identity-live", 1, "twitter", &profile, 5)
            .await
            .unwrap();
        let hit = owner
            .list_discovery_search_hits(ListDiscoverySearchHitsInput {
                campaign_id: "identity-live".into(),
                after_id: None,
                limit: 10,
            })
            .unwrap()
            .into_iter()
            .find(|hit| hit.author_id.is_some())
            .expect("saved native author locator");
        let advanced = platform
            .advance_discovery_campaign(super::super::OpenCliDiscoveryAdvanceInput {
                excluded_platforms: vec![],
                campaign_id: "identity-live".into(),
                expected_revision: 1,
            })
            .await
            .unwrap();
        assert!(matches!(
            advanced.step,
            onecrew_creator_runtime::DiscoveryStep::ReadIdentity { .. }
        ));
        let receipt = advanced.identity.expect("identity step receipt");
        assert_eq!(advanced.campaign.used_calls, 2);
        assert!(
            advanced.profile.is_none()
                && advanced.content.is_none()
                && advanced.transcript.is_none()
        );
        assert_eq!(receipt.status, "completed", "stop={:?}", receipt.stop_code);
        assert_eq!(receipt.claim.call_number, 2);
        assert_eq!(receipt.claim.profile_ref, profile);
        assert_eq!(receipt.claim.source_url, hit.source_url);
        assert!(receipt.account_id.is_some());
        assert!(
            platform
                .read_discovery_identity_once("identity-live", 1, &hit.id)
                .await
                .is_err()
        );
        assert_eq!(
            owner
                .get_discovery_campaign("identity-live")
                .unwrap()
                .used_calls,
            2
        );
        assert_eq!(
            owner
                .count_discovery_candidates("identity-live")
                .unwrap()
                .qualified,
            0
        );
        drop(owner);
        drop(platform);
        let platform = LocalPlatform::open(dir.path()).unwrap();
        assert_eq!(
            platform
                .creators()
                .unwrap()
                .get_discovery_identity("identity-live", &hit.id)
                .unwrap(),
            receipt
        );
        println!(
            "Budgeted native post author receipt survived restart; no customer Campaign changed"
        );
    }
}
