//! Follow one actual saved engine result through the existing native facade.
use super::{
    OpenCli, OpenCliDiscoverySearchError, OpenCliError, VERIFIED_OPENCLI_VERSION,
    provider_stop_code,
};
use crate::LocalPlatform;
use onecrew_creator_runtime::{DiscoveryWebSourceRead, FinishDiscoveryWebSourceInput};

/// Independent facade times. None means unattempted, including after an earlier
/// failure. These are components of elapsed_ms, not additional elapsed time.
#[derive(Default, serde::Serialize)]
struct NativeTimings {
    identity_ms: Option<u64>,
    profile_ms: Option<u64>,
    content_ms: Option<u64>,
}

async fn timed<T>(slot: &mut Option<u64>, request: impl std::future::Future<Output = T>) -> T {
    let started = std::time::Instant::now();
    let result = request.await;
    *slot = Some(started.elapsed().as_millis().min(u64::MAX as u128) as u64);
    result
}

impl LocalPlatform {
    pub async fn read_discovery_web_source_once(
        &self,
        campaign: &str,
        revision: u32,
        hit: &str,
    ) -> Result<DiscoveryWebSourceRead, OpenCliDiscoverySearchError> {
        let creators = self
            .creators()
            .map_err(OpenCliDiscoverySearchError::Campaign)?;
        let approved = creators
            .get_discovery_campaign_async(campaign.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let claim = creators
            .claim_discovery_web_source_async(campaign.into(), revision, hit.into())
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        let started = std::time::Instant::now();
        let mut issued_native_commands = 0_u32;
        let mut native_timings = NativeTimings::default();
        let result = async {
            let cli = OpenCli::inspect_system().await?;
            if cli.inventory().version != VERIFIED_OPENCLI_VERSION
                || approved.opencli_version != cli.inventory().version
            {
                return Err(OpenCliError::VersionChanged);
            }
            if claim.target_platform == "douyin" {
                issued_native_commands += 1;
                let content = timed(
                    &mut native_timings.content_ms,
                    cli.douyin_original_content(&claim.profile_ref, &claim.video_id, None),
                )
                .await?;
                issued_native_commands += 1;
                let profile = timed(
                    &mut native_timings.profile_ms,
                    cli.douyin_public_profile(&claim.profile_ref, &content.author_id),
                )
                .await?;
                Ok(serde_json::json!({"profile":profile,"content":content}))
            } else if claim.target_platform == "twitter" {
                let (handle, post) =
                    onecrew_creator_runtime::web_source_twitter_target(&claim.source_url)
                        .ok_or(OpenCliError::InvalidOutput)?;
                issued_native_commands += 1;
                let identity = timed(
                    &mut native_timings.identity_ms,
                    cli.twitter_post_identity(&claim.profile_ref, &handle, &post),
                )
                .await?;
                issued_native_commands += 1;
                let profile = timed(
                    &mut native_timings.profile_ms,
                    cli.twitter_identity_profile(&claim.profile_ref, &handle),
                )
                .await?;
                if identity.account_id != profile.account_id {
                    return Err(OpenCliError::InvalidOutput);
                }
                issued_native_commands += 1;
                let content = timed(
                    &mut native_timings.content_ms,
                    cli.twitter_post_content(
                        &claim.profile_ref,
                        &handle,
                        &post,
                        &identity.account_id,
                    ),
                )
                .await?;
                Ok(serde_json::json!({"identity":identity,"profile":profile,"content":content}))
            } else {
                issued_native_commands += 1;
                let native = timed(
                    &mut native_timings.content_ms,
                    cli.youtube_video_evidence(&claim.profile_ref, &claim.video_id),
                )
                .await?;
                serde_json::to_value(native).map_err(|_| OpenCliError::InvalidOutput)
            }
        }
        .await;
        let (native_material, stop_code) = match result {
            Ok(data) => (Some(data), None),
            Err(error) => (None, Some(provider_stop_code(&error).into())),
        };
        let read = creators
            .finish_discovery_web_source_async(FinishDiscoveryWebSourceInput {
                claim,
                native_material,
                stop_code,
            })
            .await
            .map_err(|e| OpenCliDiscoverySearchError::Campaign(e.to_string()))?;
        super::contact_audit::record(
            &self.data_dir,
            campaign,
            serde_json::json!({"kind":"web_source","call_number":read.claim.call_number,
            "search_hit_id":read.claim.search_hit_id,"origin_platform":read.claim.origin_platform,"target_platform":read.claim.target_platform,
            "reserved_native_calls":match read.claim.target_platform.as_str() {"twitter"=>3,"douyin"=>2,_=>1},
            "issued_native_commands":issued_native_commands,
            "native_timings":native_timings,
            "elapsed_ms":started.elapsed().as_millis().min(u64::MAX as u128) as u64,"provider_status":read.status,"stop_code":read.stop_code,
            "native_identity_observed":read.native_author_id.is_some(),"contact_or_recommendation_promoted":false}),
        );
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_stage_timing_keeps_failures_and_unattempted_stages_distinct() {
        let mut timings = NativeTimings::default();
        let result: Result<(), &str> = async {
            timed(&mut timings.identity_ms, async { Ok::<(), &str>(()) }).await?;
            timed(&mut timings.profile_ms, async {
                Err("profile unavailable")
            })
            .await?;
            timed(&mut timings.content_ms, async { Ok(()) }).await
        }
        .await;
        assert_eq!(result, Err("profile unavailable"));
        assert!(timings.identity_ms.is_some());
        assert!(timings.profile_ms.is_some());
        assert_eq!(timings.content_ms, None);
        let stored = serde_json::to_value(timings).unwrap();
        assert!(stored["identity_ms"].is_u64());
        assert!(stored["profile_ms"].is_u64());
        assert!(stored["content_ms"].is_null());
    }
}
