use super::{
    LocalPlatform, OpenCli, OpenCliDiscoverySearchError, OpenCliError, VERIFIED_OPENCLI_VERSION,
    provider_stop_code,
};
use onecrew_creator_runtime::{
    DiscoveryFacebookIdentityRead, FinishDiscoveryFacebookIdentityInput,
};

impl LocalPlatform {
    /// Record authority and budget before the one native read; never replay it.
    pub async fn read_discovery_facebook_identity_once(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryFacebookIdentityRead, OpenCliDiscoverySearchError> {
        let owner = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = owner
            .get_discovery_campaign_async(campaign.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let claim = owner
            .claim_discovery_facebook_identity_async(campaign.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            let native = cli
                .facebook_source_identity(&claim.profile_ref, &claim.source_url)
                .await?;
            serde_json::to_value(native).map_err(|_| OpenCliError::InvalidOutput)
        }
        .await;
        let (native_identity, stop_code) = match result {
            Ok(native) => (Some(native), None),
            Err(error) => (None, Some(provider_stop_code(&error).into())),
        };
        owner
            .finish_discovery_facebook_identity_async(FinishDiscoveryFacebookIdentityInput {
                claim,
                native_identity,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn saved_facebook_identity_requires_authority_before_provider_dispatch() {
        let dir = tempfile::tempdir().unwrap();
        let platform = LocalPlatform::open(dir.path()).unwrap();
        assert!(matches!(
            platform
                .read_discovery_facebook_identity_once("missing", 1, "missing")
                .await,
            Err(OpenCliDiscoverySearchError::Campaign(_))
        ));
    }
}
