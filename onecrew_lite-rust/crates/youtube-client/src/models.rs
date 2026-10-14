use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum YoutubeSearchKind {
    Channel,
    Video,
}

impl YoutubeSearchKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Video => "video",
        }
    }
}

/// The order YouTube ranks a search page by, sent as `order` on every search.
///
/// `Relevance` is YouTube's own documented default, so naming it explicitly
/// changes no result while giving a confirmed search plan one canonical
/// spelling of the same scope — the way the page size is compiled explicitly
/// rather than left absent. `Date` ranks by upload time, which is what makes
/// a page's [`YoutubeSearchMatch::published_at`] evidence of recent activity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeSearchOrder {
    #[default]
    Relevance,
    Date,
}

impl YoutubeSearchOrder {
    /// The provider's own `order` value. These are YouTube's spellings, not
    /// OneCrew names: changing one silently changes which page YouTube ranks.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Date => "date",
        }
    }
}

/// The caller-selected evidence depth for one channel read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeReadDepth {
    /// Read channel metadata, recent video descriptions, and About links, but
    /// intentionally do not request public comments.
    Light,
    /// Preserve the complete channel-read behavior, including comments from
    /// the first recent video when available.
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSearchInput {
    pub query: String,
    pub kind: YoutubeSearchKind,
    pub max_results: u8,
    /// How YouTube ranks this page. Always sent.
    pub order: YoutubeSearchOrder,
    /// Keeps only resources published at or after this instant, in the form
    /// [`crate::PUBLISHED_AFTER_FORMAT`]. With
    /// [`YoutubeSearchOrder::Date`] this is what lets one search page carry
    /// its own recent-activity evidence.
    pub published_after: Option<String>,
    pub region_code: Option<String>,
    pub language: Option<String>,
    pub page_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSearchPage {
    pub items: Vec<YoutubeSearchHit>,
    /// The page's channel metadata once, keyed by the `channel_id` its hits
    /// carry. A channel that several hits match is described once here
    /// instead of being copied into each of them.
    ///
    /// This must stay a map and never become a list. The card projection
    /// recognizes a page of records structurally, as the one array of two or
    /// more objects at the shallowest depth
    /// (`mcp-adapter`'s `operation_registry::v2::projection::exact`). A
    /// second array beside `items` would make that depth ambiguous, and the
    /// page would lose its rows and fall back to leaf-by-leaf fields. A map
    /// is not an array, so `items` stays the only candidate.
    pub channels: BTreeMap<String, YoutubeSearchChannel>,
    pub next_page_token: Option<String>,
}

/// One search result, retaining provider order and each distinct video hit.
///
/// A hit names its channel; the channel's metadata is read from the page's
/// [`YoutubeSearchPage::channels`] under that `channel_id`. A channel that
/// disappears between the search and the metadata batch is simply absent
/// from that map, without its hit being lost.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSearchHit {
    pub channel_id: String,
    pub channel_url: String,
    pub matched: YoutubeSearchMatch,
}

/// Channel metadata returned by channels.list during a search, held once per
/// channel in [`YoutubeSearchPage::channels`]. Search does not fetch About
/// pages, uploads or comments, or extract contact routes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSearchChannel {
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub description: String,
    pub custom_url: Option<String>,
    pub country: Option<String>,
    pub subscriber_count: Option<u64>,
    pub view_count: Option<u64>,
    pub video_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeSearchMatch {
    pub title: String,
    pub description: String,
    pub video_id: Option<String>,
    /// When the matched resource itself was published, as YouTube returns it:
    /// for a video hit, that video's publication time. It is evidence about
    /// this hit, not about the channel's latest upload; ordering a search by
    /// [`YoutubeSearchOrder::Date`] is what makes the newest hit per channel
    /// the one a page carries.
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeVideo {
    pub video_id: String,
    pub title: String,
    pub description: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeComment {
    pub comment_id: String,
    pub author_name: String,
    pub author_channel_id: Option<String>,
    pub text: String,
    pub published_at: Option<String>,
}

/// A bounded profile section that could not be read. Empty data in that
/// section does not establish that the channel publishes no such facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeProfileSection {
    About,
    RecentVideos,
    PublicComments,
}

/// An outbound destination published in the channel's About links block.
/// Provider parsing unwraps YouTube redirects but does not classify contacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeChannelLink {
    pub target_url: String,
    pub source_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeCreator {
    /// Stable Creator ID. Handles are presentation aliases only.
    pub channel_id: String,
    pub channel_url: String,
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub description: String,
    /// Public page that supplied this description: About when present,
    /// otherwise the channel page for API-backed channel metadata.
    pub description_source_url: Option<String>,
    pub custom_url: Option<String>,
    pub country: Option<String>,
    pub subscriber_count: Option<u64>,
    pub view_count: Option<u64>,
    pub video_count: Option<u64>,
    pub matched: Option<YoutubeSearchMatch>,
    /// The existing uploads read requests at most five videos. Description
    /// text is retained in full; consumers may create display summaries.
    pub recent_videos: Vec<YoutubeVideo>,
    /// At most ten comments from the first returned recent video.
    pub public_comments: Vec<YoutubeComment>,
    pub about_links: Vec<YoutubeChannelLink>,
    pub hidden_business_email: bool,
    pub unavailable_sections: Vec<YoutubeProfileSection>,
    /// Sections deliberately omitted by the selected [`YoutubeReadDepth`].
    /// This is distinct from [`Self::unavailable_sections`], where a request
    /// was attempted but failed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub omitted_sections: Vec<YoutubeProfileSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YoutubeChannelSummary {
    pub channel_id: String,
    pub title: String,
    pub thumbnail_url: Option<String>,
    pub description: String,
    pub custom_url: Option<String>,
    pub country: Option<String>,
}
