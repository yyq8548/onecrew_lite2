use super::{
    OpenCli, OpenCliDiscoverySearchError, OpenCliError, VERIFIED_OPENCLI_VERSION,
    provider_stop_code,
};
use crate::LocalPlatform;
use onecrew_creator_runtime::{DiscoverySourceAuthorRead, FinishDiscoverySourceAuthorInput};
use schemars::JsonSchema;
use serde::Serialize;

/// Independent facade durations, excluding owner claim/save work. Missing
/// means the request was never attempted, rather than a zero-cost request.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct OpenCliSourceAuthorTimings {
    pub content_ms: Option<u64>,
    pub profile_ms: Option<u64>,
}
fn elapsed_ms(started: std::time::Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

impl LocalPlatform {
    /// One governed logical observation contains two native page reads. Each
    /// facade call rechecks browser/version; the joint owner commits once.
    pub async fn read_discovery_source_author_once(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<(DiscoverySourceAuthorRead, OpenCliSourceAuthorTimings), OpenCliDiscoverySearchError>
    {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let claim = creators
            .claim_discovery_source_author_async(campaign.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let mut timings = OpenCliSourceAuthorTimings::default();
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            let started = std::time::Instant::now();
            let content = cli
                .douyin_original_content(&claim.profile_ref, &claim.post_id, None)
                .await;
            timings.content_ms = Some(elapsed_ms(started));
            let content = content?;
            let started = std::time::Instant::now();
            let profile = cli
                .douyin_public_profile(&claim.profile_ref, &content.author_id)
                .await;
            timings.profile_ms = Some(elapsed_ms(started));
            let profile = profile?;
            Ok((
                serde_json::to_value(profile).map_err(|_| OpenCliError::InvalidOutput)?,
                serde_json::to_value(content).map_err(|_| OpenCliError::InvalidOutput)?,
            ))
        }
        .await;
        let (native_profile, native_content, stop_code) = match result {
            Ok((profile, content)) => (Some(profile), Some(content), None),
            Err(error) => (None, None, Some(provider_stop_code(&error).into())),
        };
        creators
            .finish_discovery_source_author_async(FinishDiscoverySourceAuthorInput {
                claim,
                native_profile,
                native_content,
                stop_code,
            })
            .await
            .map(|read| (read, timings))
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))
    }
}
