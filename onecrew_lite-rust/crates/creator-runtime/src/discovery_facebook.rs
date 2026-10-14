//! Saved, budgeted Facebook profile-list locators. Captions still prove their own author.
use crate::{CreatorRepositoryCallError, CreatorRuntime, DiscoveryProfileClaim};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DiscoveryFacebookTargetsRead {
    pub claim: DiscoveryProfileClaim,
    pub status: String,
    pub source_urls: Option<Vec<String>>,
    pub stop_code: Option<String>,
    pub captured_at: i64,
}

pub(crate) fn valid_reel(source: &str) -> bool {
    source
        .strip_prefix("https://www.facebook.com/reel/")
        .is_some_and(|id| {
            (1..=25).contains(&id.len())
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
        })
}

impl CreatorRuntime {
    pub async fn claim_discovery_facebook_targets_async(
        &self,
        campaign: String,
        revision: u32,
        hit: String,
    ) -> Result<DiscoveryProfileClaim, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner
                .repository
                .claim_discovery_facebook_targets(&campaign, revision, &hit)
        })
        .await
    }

    pub async fn finish_discovery_facebook_targets_async(
        &self,
        claim: DiscoveryProfileClaim,
        urls: Option<Vec<String>>,
        stop: Option<String>,
    ) -> Result<DiscoveryFacebookTargetsRead, CreatorRepositoryCallError<String>> {
        let owner = self.clone();
        self.repository_write_segment(move || {
            owner
                .repository
                .finish_discovery_facebook_targets(claim, urls, stop)
        })
        .await
    }
}
