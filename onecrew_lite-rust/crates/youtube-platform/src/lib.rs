//! Application-facing YouTube facade.
//!
//! The facade binds caller-supplied provider authentication to typed YouTube
//! client operations, publishes the inputs YouTube reads accept, and extracts
//! the protocol-native evidence of a failure
//! ([`YoutubePlatformError::evidence`]). Credential storage and Creator
//! workflows remain outside this crate.

mod error;
mod locator;
mod platform;

pub use error::{
    YoutubeApiFailureKind, YoutubeClientError, YoutubePlatformError, YoutubeRequestFailureKind,
    YoutubeTransportError,
};
pub use locator::{YoutubeChannelLocator, parse_channel_locator};
pub use onecrew_youtube_client::{
    PUBLISHED_AFTER_FORMAT, SEARCH_DEFAULT_COUNT, SEARCH_MAX_COUNT, SEARCH_MIN_COUNT,
    YoutubeChannelLink, YoutubeChannelSummary, YoutubeComment, YoutubeCreator,
    YoutubeProfileSection, YoutubeReadDepth, YoutubeSearchChannel, YoutubeSearchHit,
    YoutubeSearchInput, YoutubeSearchKind, YoutubeSearchMatch, YoutubeSearchOrder,
    YoutubeSearchPage, YoutubeVideo, accepts_channel_id, accepts_published_after,
    accepts_search_count, accepts_search_query,
};
pub use onecrew_youtube_transport::{MAX_IN_FLIGHT_REQUESTS, YoutubeRequestLimiter};
pub use platform::YoutubePlatform;
