use onecrew_youtube_client::{
    YoutubeChannelSummary, YoutubeClient, YoutubeCreator, YoutubeReadDepth, YoutubeSearchInput,
    YoutubeSearchPage,
};
use onecrew_youtube_transport::{YoutubeRequestLimiter, YoutubeTransport};

use crate::YoutubePlatformError;

/// Application-facing YouTube read facade.
///
/// API-key selection and cooldown belong to the local credential store/runtime;
/// this facade intentionally owns no vault and exposes no key material.
#[derive(Clone)]
pub struct YoutubePlatform {
    client: YoutubeClient,
}

impl YoutubePlatform {
    pub fn from_api_key(api_key: impl Into<String>) -> Result<Self, YoutubePlatformError> {
        let transport = YoutubeTransport::new(api_key)?;
        Ok(Self {
            client: YoutubeClient::new(transport),
        })
    }

    /// Builds a provider client over a caller-owned request budget. Credential
    /// owners use this to share one six-request domain across all connections
    /// and provider entry points for the credential.
    pub fn from_api_key_with_request_limiter(
        api_key: impl Into<String>,
        request_limiter: YoutubeRequestLimiter,
    ) -> Result<Self, YoutubePlatformError> {
        let transport = YoutubeTransport::with_request_limiter(api_key, request_limiter)?;
        Ok(Self {
            client: YoutubeClient::new(transport),
        })
    }

    pub async fn resolve_handle(
        &self,
        handle: &str,
    ) -> Result<YoutubeChannelSummary, YoutubePlatformError> {
        self.client
            .channel_by_handle(handle)
            .await
            .map_err(Into::into)
    }

    pub async fn creator_by_handle(
        &self,
        handle: &str,
        depth: YoutubeReadDepth,
    ) -> Result<YoutubeCreator, YoutubePlatformError> {
        self.client
            .creator_by_handle(handle, depth)
            .await
            .map_err(Into::into)
    }

    pub async fn creator_by_channel_id(
        &self,
        channel_id: &str,
        depth: YoutubeReadDepth,
    ) -> Result<YoutubeCreator, YoutubePlatformError> {
        self.client
            .creator_by_channel_id(channel_id, depth)
            .await
            .map_err(Into::into)
    }

    /// Reads one search page: its hits in provider order, the page's channel
    /// metadata once per channel, and YouTube's own continuation token.
    ///
    /// The caller names the resource kind, page size, order and optional
    /// publication bound in the input; this facade adds no default of its own.
    ///
    /// # Errors
    ///
    /// Returns the client's input refusal for a query, count or publication
    /// bound YouTube does not accept, and the transport failure of either
    /// provider request.
    pub async fn search(
        &self,
        input: YoutubeSearchInput,
    ) -> Result<YoutubeSearchPage, YoutubePlatformError> {
        self.client.search(input).await.map_err(Into::into)
    }
}
