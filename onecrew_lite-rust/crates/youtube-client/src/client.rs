use std::collections::{BTreeMap, BTreeSet};

use onecrew_youtube_transport::{YoutubeTransport, YoutubeTransportError};
use serde::{Deserialize, de::DeserializeOwned};
use url::Url;

use crate::{
    YoutubeChannelLink, YoutubeChannelSummary, YoutubeClientError, YoutubeComment, YoutubeCreator,
    YoutubeProfileSection, YoutubeSearchChannel, YoutubeSearchHit, YoutubeSearchInput,
    YoutubeSearchMatch, YoutubeSearchPage, YoutubeVideo, accepts_channel_id,
    accepts_published_after, accepts_search_count, accepts_search_query, parser::parse_about_html,
};

const PUBLIC_PAGE_LIMIT: usize = 4 << 20;
#[derive(Clone)]
pub struct YoutubeClient {
    transport: YoutubeTransport,
    #[cfg(test)]
    fixture: Option<std::sync::Arc<tests::ReadFixture>>,
}

impl YoutubeClient {
    pub fn new(transport: YoutubeTransport) -> Self {
        Self {
            transport,
            #[cfg(test)]
            fixture: None,
        }
    }

    async fn api_get<T: DeserializeOwned>(
        &self,
        resource: &str,
        query: &[(&str, String)],
    ) -> Result<T, YoutubeTransportError> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.api_get(resource, query);
        }
        self.transport.api_get(resource, query).await
    }

    async fn public_text(
        &self,
        url: &Url,
        allowed_hosts: &BTreeSet<String>,
        limit: usize,
    ) -> Result<String, YoutubeTransportError> {
        #[cfg(test)]
        if let Some(fixture) = &self.fixture {
            return fixture.public_text(url);
        }
        self.transport.public_text(url, allowed_hosts, limit).await
    }

    pub async fn channel_by_handle(
        &self,
        handle: &str,
    ) -> Result<YoutubeChannelSummary, YoutubeClientError> {
        let response: ChannelsResponse = self
            .api_get(
                "channels",
                &[
                    ("part", "snippet".into()),
                    ("forHandle", handle.trim().into()),
                ],
            )
            .await?;
        let channel = response
            .items
            .into_iter()
            .next()
            .ok_or(YoutubeClientError::ChannelNotFound)?;
        let thumbnail_url = channel_thumbnail(&channel.snippet);
        Ok(YoutubeChannelSummary {
            channel_id: channel.id,
            title: channel.snippet.title,
            thumbnail_url,
            description: channel.snippet.description,
            custom_url: non_empty(channel.snippet.custom_url),
            country: non_empty(channel.snippet.country),
        })
    }

    pub async fn creator_by_handle(
        &self,
        handle: &str,
        depth: crate::YoutubeReadDepth,
    ) -> Result<YoutubeCreator, YoutubeClientError> {
        let response: ChannelsResponse = self
            .api_get(
                "channels",
                &[
                    ("part", "snippet,statistics,contentDetails".into()),
                    ("forHandle", handle.trim().into()),
                ],
            )
            .await?;
        let channel = response
            .items
            .into_iter()
            .next()
            .ok_or(YoutubeClientError::ChannelNotFound)?;
        Ok(self.creator_from_channel(channel, None, depth).await)
    }

    pub async fn creator_by_channel_id(
        &self,
        channel_id: &str,
        depth: crate::YoutubeReadDepth,
    ) -> Result<YoutubeCreator, YoutubeClientError> {
        let channel_id = channel_id.trim();
        if !accepts_channel_id(channel_id) {
            return Err(YoutubeClientError::InvalidChannelId);
        }
        let response: ChannelsResponse = self
            .api_get(
                "channels",
                &[
                    ("part", "snippet,statistics,contentDetails".into()),
                    ("id", channel_id.into()),
                ],
            )
            .await?;
        let channel = response
            .items
            .into_iter()
            .next()
            .ok_or(YoutubeClientError::ChannelNotFound)?;
        Ok(self.creator_from_channel(channel, None, depth).await)
    }

    /// Reads one search page and one batch of channel metadata. Every search
    /// hit retains its position; this does not run channel-get enrichment.
    ///
    /// The page's channel metadata is returned once per channel in
    /// [`YoutubeSearchPage::channels`], rather than copied into every hit
    /// that matched that channel.
    ///
    /// # Errors
    ///
    /// Returns an input error for an empty query, a count outside 1–50, or a
    /// publication bound outside [`crate::PUBLISHED_AFTER_FORMAT`], and
    /// propagates transport failures from either provider request.
    pub async fn search(
        &self,
        input: YoutubeSearchInput,
    ) -> Result<YoutubeSearchPage, YoutubeClientError> {
        if !accepts_search_query(&input.query) {
            return Err(YoutubeClientError::EmptyQuery);
        }
        if !accepts_search_count(input.max_results) {
            return Err(YoutubeClientError::InvalidSearchCount);
        }
        let published_after = non_empty_option(input.published_after);
        if let Some(published_after) = published_after.as_deref()
            && !accepts_published_after(published_after)
        {
            return Err(YoutubeClientError::InvalidPublishedAfter);
        }
        let mut query = vec![
            ("part", "snippet".into()),
            ("q", input.query.clone()),
            ("type", input.kind.as_str().into()),
            ("maxResults", input.max_results.to_string()),
            // Always sent. `relevance` is YouTube's own default, so naming it
            // leaves the page YouTube returns unchanged while giving a
            // confirmed plan one canonical spelling of the same scope.
            ("order", input.order.as_str().into()),
        ];
        if let Some(published_after) = published_after {
            query.push(("publishedAfter", published_after));
        }
        if let Some(region) = non_empty_option(input.region_code) {
            query.push(("regionCode", region));
        }
        if let Some(language) = non_empty_option(input.language) {
            query.push(("relevanceLanguage", language));
        }
        if let Some(page_token) = non_empty_option(input.page_token) {
            query.push(("pageToken", page_token));
        }
        let search: SearchResponse = self.api_get("search", &query).await?;
        let next_page_token = search.next_page_token;
        let mut hits = Vec::with_capacity(search.items.len());
        let mut channel_ids = BTreeSet::new();
        for item in search.items {
            let channel_id =
                non_empty(item.id.channel_id).or_else(|| non_empty(item.snippet.channel_id));
            if let Some(channel_id) = channel_id {
                channel_ids.insert(channel_id.clone());
                hits.push(YoutubeSearchHit {
                    channel_url: format!("https://www.youtube.com/channel/{channel_id}"),
                    channel_id,
                    matched: YoutubeSearchMatch {
                        title: item.snippet.title,
                        description: item.snippet.description,
                        video_id: non_empty(item.id.video_id),
                        published_at: non_empty(item.snippet.published_at),
                    },
                });
            }
        }
        if hits.is_empty() {
            return Ok(YoutubeSearchPage {
                items: hits,
                channels: BTreeMap::new(),
                next_page_token,
            });
        }
        let response: ChannelsResponse = self
            .api_get(
                "channels",
                &[
                    ("part", "snippet,statistics".into()),
                    ("id", channel_ids.into_iter().collect::<Vec<_>>().join(",")),
                ],
            )
            .await?;
        let channels = response
            .items
            .into_iter()
            .map(|channel| {
                let thumbnail_url = channel_thumbnail(&channel.snippet);
                (
                    channel.id,
                    YoutubeSearchChannel {
                        title: channel.snippet.title,
                        thumbnail_url,
                        description: channel.snippet.description,
                        custom_url: non_empty(channel.snippet.custom_url),
                        country: non_empty(channel.snippet.country),
                        subscriber_count: parse_count(channel.statistics.subscriber_count),
                        view_count: parse_count(channel.statistics.view_count),
                        video_count: parse_count(channel.statistics.video_count),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        Ok(YoutubeSearchPage {
            items: hits,
            channels,
            next_page_token,
        })
    }

    async fn creator_from_channel(
        &self,
        channel: ChannelItem,
        matched: Option<YoutubeSearchMatch>,
        depth: crate::YoutubeReadDepth,
    ) -> YoutubeCreator {
        let channel_url = format!("https://www.youtube.com/channel/{}", channel.id);
        let (activity, about) = futures::join!(
            self.recent_activity(&channel.content_details.related_playlists.uploads, depth),
            self.about_page(&channel_url),
        );
        let (videos, comments, mut unavailable_sections, omitted_sections) = activity;
        if about.is_err() {
            unavailable_sections.push(YoutubeProfileSection::About);
        }
        let about = about.unwrap_or_default();
        let about_url = format!("{channel_url}/about");
        let thumbnail_url = channel_thumbnail(&channel.snippet);
        let (description, description_source_url) = if about.description.is_empty() {
            (channel.snippet.description, None)
        } else {
            (about.description, Some(about_url.clone()))
        };
        YoutubeCreator {
            channel_url,
            matched,
            channel_id: channel.id,
            title: channel.snippet.title,
            thumbnail_url,
            description,
            description_source_url,
            custom_url: non_empty(channel.snippet.custom_url),
            country: non_empty(channel.snippet.country),
            subscriber_count: parse_count(channel.statistics.subscriber_count),
            view_count: parse_count(channel.statistics.view_count),
            video_count: parse_count(channel.statistics.video_count),
            recent_videos: videos,
            public_comments: comments,
            about_links: about
                .links
                .into_iter()
                .map(|target_url| YoutubeChannelLink {
                    target_url,
                    source_url: about_url.clone(),
                })
                .collect(),
            hidden_business_email: about.hidden_business_email,
            unavailable_sections,
            omitted_sections,
        }
    }

    async fn recent_activity(
        &self,
        uploads_id: &str,
        depth: crate::YoutubeReadDepth,
    ) -> (
        Vec<YoutubeVideo>,
        Vec<YoutubeComment>,
        Vec<YoutubeProfileSection>,
        Vec<YoutubeProfileSection>,
    ) {
        if uploads_id.is_empty() {
            return (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Self::omitted_comments(depth),
            );
        }
        let playlist = self
            .api_get::<PlaylistItemsResponse>(
                "playlistItems",
                &[
                    ("part", "snippet".into()),
                    ("playlistId", uploads_id.into()),
                    ("maxResults", "5".into()),
                ],
            )
            .await;
        let Ok(playlist) = playlist else {
            return (
                Vec::new(),
                Vec::new(),
                vec![YoutubeProfileSection::RecentVideos],
                Self::omitted_comments(depth),
            );
        };
        let videos = playlist
            .items
            .into_iter()
            .filter_map(|item| {
                let video_id = non_empty(item.snippet.resource_id.video_id)?;
                Some(YoutubeVideo {
                    video_id,
                    title: item.snippet.title,
                    description: item.snippet.description,
                    published_at: non_empty(item.snippet.published_at),
                })
            })
            .collect::<Vec<_>>();
        let Some(video) = videos.first() else {
            return (
                videos,
                Vec::new(),
                Vec::new(),
                Self::omitted_comments(depth),
            );
        };
        if depth == crate::YoutubeReadDepth::Light {
            return (
                videos,
                Vec::new(),
                Vec::new(),
                vec![YoutubeProfileSection::PublicComments],
            );
        }
        let comments = self
            .api_get::<CommentThreadsResponse>(
                "commentThreads",
                &[
                    ("part", "snippet".into()),
                    ("videoId", video.video_id.clone()),
                    ("maxResults", "10".into()),
                    ("textFormat", "plainText".into()),
                    ("order", "relevance".into()),
                ],
            )
            .await
            .map(|response| {
                response
                    .items
                    .into_iter()
                    .map(|item| {
                        let comment = item.snippet.top_level_comment;
                        YoutubeComment {
                            comment_id: comment.id,
                            author_name: comment.snippet.author_display_name,
                            author_channel_id: non_empty(comment.snippet.author_channel_id.value),
                            text: comment.snippet.text_display,
                            published_at: non_empty(comment.snippet.published_at),
                        }
                    })
                    .collect()
            });
        match comments {
            Ok(comments) => (videos, comments, Vec::new(), Vec::new()),
            Err(_) => (
                videos,
                Vec::new(),
                vec![YoutubeProfileSection::PublicComments],
                Vec::new(),
            ),
        }
    }

    fn omitted_comments(depth: crate::YoutubeReadDepth) -> Vec<YoutubeProfileSection> {
        match depth {
            crate::YoutubeReadDepth::Light => vec![YoutubeProfileSection::PublicComments],
            crate::YoutubeReadDepth::Full => Vec::new(),
        }
    }

    async fn about_page(
        &self,
        channel_url: &str,
    ) -> Result<crate::parser::AboutPage, YoutubeClientError> {
        let about_url = Url::parse(&format!("{channel_url}/about"))
            .map_err(|_| YoutubeTransportError::InvalidUrl)?;
        let youtube_hosts = BTreeSet::from(["youtube.com".to_string()]);
        let about_body = self
            .public_text(&about_url, &youtube_hosts, PUBLIC_PAGE_LIMIT)
            .await?;
        parse_about_html(&about_body)
    }
}

fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn channel_thumbnail(snippet: &ChannelSnippet) -> Option<String> {
    [
        snippet.thumbnails.high.url.as_str(),
        snippet.thumbnails.medium.url.as_str(),
        snippet.thumbnails.default_image.url.as_str(),
    ]
    .into_iter()
    .find(|url| !url.is_empty())
    .map(str::to_owned)
}

fn non_empty_option(value: Option<String>) -> Option<String> {
    value.and_then(non_empty)
}

fn parse_count(value: String) -> Option<u64> {
    value.parse().ok()
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<SearchItem>,
    #[serde(default, rename = "nextPageToken")]
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    #[serde(default)]
    id: SearchId,
    #[serde(default)]
    snippet: SearchSnippet,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchId {
    #[serde(default)]
    channel_id: String,
    #[serde(default)]
    video_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchSnippet {
    #[serde(default)]
    channel_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    published_at: String,
}

#[derive(Debug, Deserialize)]
struct ChannelsResponse {
    #[serde(default)]
    items: Vec<ChannelItem>,
}

#[derive(Debug, Deserialize)]
struct ChannelItem {
    id: String,
    #[serde(default)]
    snippet: ChannelSnippet,
    #[serde(default)]
    statistics: ChannelStatistics,
    #[serde(default, rename = "contentDetails")]
    content_details: ChannelContentDetails,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelSnippet {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    custom_url: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    thumbnails: ChannelThumbnails,
}

#[derive(Debug, Default, Deserialize)]
struct ChannelThumbnails {
    #[serde(default)]
    high: ChannelThumbnail,
    #[serde(default)]
    medium: ChannelThumbnail,
    #[serde(default, rename = "default")]
    default_image: ChannelThumbnail,
}

#[derive(Debug, Default, Deserialize)]
struct ChannelThumbnail {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelStatistics {
    #[serde(default)]
    subscriber_count: String,
    #[serde(default)]
    view_count: String,
    #[serde(default)]
    video_count: String,
}

#[derive(Debug, Default, Deserialize)]
struct ChannelContentDetails {
    #[serde(default, rename = "relatedPlaylists")]
    related_playlists: RelatedPlaylists,
}

#[derive(Debug, Default, Deserialize)]
struct RelatedPlaylists {
    #[serde(default)]
    uploads: String,
}

#[derive(Debug, Deserialize)]
struct PlaylistItemsResponse {
    #[serde(default)]
    items: Vec<PlaylistItem>,
}

#[derive(Debug, Deserialize)]
struct PlaylistItem {
    snippet: PlaylistSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaylistSnippet {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    resource_id: ResourceId,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceId {
    #[serde(default)]
    video_id: String,
}

#[derive(Debug, Deserialize)]
struct CommentThreadsResponse {
    #[serde(default)]
    items: Vec<CommentThread>,
}

#[derive(Debug, Deserialize)]
struct CommentThread {
    snippet: CommentThreadSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentThreadSnippet {
    top_level_comment: Comment,
}

#[derive(Debug, Deserialize)]
struct Comment {
    id: String,
    snippet: CommentSnippet,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommentSnippet {
    #[serde(default)]
    author_display_name: String,
    #[serde(default)]
    author_channel_id: AuthorChannelId,
    #[serde(default)]
    text_display: String,
    #[serde(default)]
    published_at: String,
}

#[derive(Debug, Default, Deserialize)]
struct AuthorChannelId {
    #[serde(default)]
    value: String,
}

#[cfg(test)]
#[test]
fn export_public_platform_read_node_oracle() {
    let directory = std::path::PathBuf::from(std::env::var("ONECREW_PUBLIC_READ_ORACLE_DIRECTORY").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let cases = [
        serde_json::json!({"id":"UC_x5XG1OV2P6uZZ5FSM9Ttw"}),
        serde_json::json!({"id":"UC_x5XG1OV2P6uZZ5FSM9Ttw","snippet":{"title":"Channel","description":"API description","customUrl":"@channel","country":"US","thumbnails":{"high":{"url":"https://example.com/high"},"medium":{"url":"https://example.com/medium"}}},"statistics":{"subscriberCount":"18446744073709551615","viewCount":"9007199254740993","videoCount":"+7"}}),
        serde_json::json!({"id":"UC_x5XG1OV2P6uZZ5FSM9Ttw","statistics":{"subscriberCount":"18446744073709551616","viewCount":" 7","videoCount":"1,000"}}),
        serde_json::json!({"id":"UC_x5XG1OV2P6uZZ5FSM9Ttw","snippet":null}),
    ].into_iter().map(|input| {
        let parsed = serde_json::from_value::<ChannelItem>(input.clone());
        let expected = parsed.ok().map(|channel| serde_json::json!({
            "channel_id":channel.id,"title":channel.snippet.title,
            "thumbnail_url":channel_thumbnail(&channel.snippet),"description":channel.snippet.description,
            "custom_url":non_empty(channel.snippet.custom_url.clone()),"country":non_empty(channel.snippet.country.clone()),
            "subscriber_count":parse_count(channel.statistics.subscriber_count),"view_count":parse_count(channel.statistics.view_count),"video_count":parse_count(channel.statistics.video_count),
        }));
        serde_json::json!({"input":input,"expected":expected})
    }).collect::<Vec<_>>();
    let counts = ["", "0", "+1", "-1", "1.0", "1e3", "1,000", " 1", "1 ", "18446744073709551615", "18446744073709551616"].into_iter().map(|input| serde_json::json!({"input":input,"expected":parse_count(input.to_owned())})).collect::<Vec<_>>();
    std::fs::write(directory.join("youtube.json"), serde_json::to_vec_pretty(&serde_json::json!({"channels":cases,"counts":counts,"light_omitted":YoutubeClient::omitted_comments(crate::YoutubeReadDepth::Light),"full_omitted":YoutubeClient::omitted_comments(crate::YoutubeReadDepth::Full)})).unwrap()).unwrap();
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use futures::executor::block_on;
    use serde::de::DeserializeOwned;
    use serde_json::{Value, json};
    use url::Url;

    use crate::{
        YoutubeProfileSection, YoutubeReadDepth, YoutubeSearchInput, YoutubeSearchKind,
        YoutubeSearchOrder,
    };

    use super::{
        SearchResponse, YoutubeClient, YoutubeClientError, YoutubeTransport, YoutubeTransportError,
    };

    const CHANNEL_ID: &str = "UC_x5XG1OV2P6uZZ5FSM9Ttw";

    #[derive(Debug)]
    struct ApiRequest {
        resource: String,
        query: BTreeMap<String, String>,
    }

    /// Offline responses sit at the client/transport seam so the production
    /// orchestration runs while every attempted provider request is observed.
    pub(super) struct ReadFixture {
        responses: BTreeMap<String, Value>,
        requests: Mutex<Vec<ApiRequest>>,
        public_pages: Mutex<Vec<String>>,
        public_page_body: Mutex<Option<String>>,
    }

    impl ReadFixture {
        pub(super) fn api_get<T: DeserializeOwned>(
            &self,
            resource: &str,
            query: &[(&str, String)],
        ) -> Result<T, YoutubeTransportError> {
            self.requests.lock().unwrap().push(ApiRequest {
                resource: resource.to_owned(),
                query: query
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), value.clone()))
                    .collect(),
            });
            let response = self
                .responses
                .get(resource)
                .ok_or(YoutubeTransportError::Decode)?;
            serde_json::from_value(response.clone()).map_err(|_| YoutubeTransportError::Decode)
        }

        pub(super) fn public_text(&self, url: &Url) -> Result<String, YoutubeTransportError> {
            self.public_pages.lock().unwrap().push(url.to_string());
            if let Some(body) = self.public_page_body.lock().unwrap().as_ref() {
                return Ok(body.clone());
            }
            Ok(
                r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{
                "description":"About our channel", "signInForBusinessEmail":{"text":"Sign in"},
                "links":[
                  {"channelExternalLinkViewModel":{"link":{"content":"x.com/Creator"}}},
                  {"channelExternalLinkViewModel":{"link":{
                    "content":"tiktok.com/@creator",
                    "commandRuns":[{"onTap":{"innertubeCommand":{"urlEndpoint":{
                      "url":"https://www.youtube.com/redirect?q=https%3A%2F%2Fwww.tiktok.com%2F%40creator"
                    }}}}]
                  }}},
                  {"channelExternalLinkViewModel":{"link":{
                    "content":"tiktok.com/@creator.name",
                    "commandRuns":[{"onTap":{"innertubeCommand":{"urlEndpoint":{
                      "url":"https://www.youtube.com/redirect?q=https%3A%2F%2Fwww.tiktok.com%2F%40creator.name"
                    }}}}]
                  }}}
                ]
            }}};</script>"#
                    .to_owned(),
            )
        }
    }

    fn fixture_client(responses: &[(&str, Value)]) -> (YoutubeClient, Arc<ReadFixture>) {
        let fixture = Arc::new(ReadFixture {
            responses: responses
                .iter()
                .map(|(resource, response)| ((*resource).to_owned(), response.clone()))
                .collect(),
            requests: Mutex::new(Vec::new()),
            public_pages: Mutex::new(Vec::new()),
            public_page_body: Mutex::new(None),
        });
        let mut client = YoutubeClient::new(YoutubeTransport::new("test-key").unwrap());
        client.fixture = Some(Arc::clone(&fixture));
        (client, fixture)
    }

    fn channel_response() -> Value {
        json!({"items": [{
            "id": CHANNEL_ID,
            "snippet": {
                "title": "Channel title", "description": "API description",
                "country": "TR", "customUrl": "@channel",
                "thumbnails": {"high": {"url": "https://i.ytimg.com/channel.jpg"}}
            },
            "statistics": {"subscriberCount": "125", "viewCount": "1234", "videoCount": "12"},
            "contentDetails": {"relatedPlaylists": {"uploads": "UU-uploads"}}
        }]})
    }

    fn search_input(kind: YoutubeSearchKind) -> YoutubeSearchInput {
        YoutubeSearchInput {
            query: "  technology  ".into(),
            kind,
            max_results: 50,
            order: YoutubeSearchOrder::Relevance,
            published_after: None,
            region_code: Some("TR".into()),
            language: Some("tr".into()),
            page_token: Some("opaque+/= cursor".into()),
        }
    }

    #[test]
    fn video_search_preserves_hits_and_cursor_with_one_metadata_batch_and_no_enrichment() {
        let (client, fixture) = fixture_client(&[
            (
                "search",
                json!({"items": [
                {"id": {"videoId": "video-2"}, "snippet": {"channelId": CHANNEL_ID, "title": "Second upload", "publishedAt": "2026-09-12T00:00:00Z"}},
                {"id": {"videoId": "video-1"}, "snippet": {"channelId": CHANNEL_ID, "title": "First upload"}},
                {"id": {"videoId": "video-removed"}, "snippet": {"channelId": "UC-removed", "title": "Unavailable channel"}}
            ], "nextPageToken": "next+/= provider cursor"}),
            ),
            ("channels", channel_response()),
        ]);

        let page = block_on(client.search(search_input(YoutubeSearchKind::Video))).unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.items[0].matched.video_id.as_deref(), Some("video-2"));
        assert_eq!(page.items[1].matched.video_id.as_deref(), Some("video-1"));
        assert_eq!(page.items[0].channel_id, CHANNEL_ID);
        assert_eq!(page.items[1].channel_id, CHANNEL_ID);
        // The two hits that share a channel read it from one page-level entry.
        assert_eq!(page.channels.len(), 1);
        assert_eq!(page.channels[CHANNEL_ID].subscriber_count, Some(125));
        assert_eq!(page.channels[CHANNEL_ID].view_count, Some(1234));
        // The batch described no such channel; its hit is still returned.
        assert_eq!(page.items[2].channel_id, "UC-removed");
        assert!(!page.channels.contains_key("UC-removed"));
        assert_eq!(
            page.next_page_token.as_deref(),
            Some("next+/= provider cursor")
        );

        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "{requests:?}");
        assert_eq!(requests[0].resource, "search");
        assert_eq!(requests[0].query["type"], "video");
        // Only the blank check trims; the query is sent as given.
        assert_eq!(requests[0].query["q"], "  technology  ");
        assert_eq!(requests[0].query["maxResults"], "50");
        assert_eq!(requests[0].query["order"], "relevance");
        assert!(!requests[0].query.contains_key("publishedAfter"));
        assert_eq!(requests[0].query["regionCode"], "TR");
        assert_eq!(requests[0].query["relevanceLanguage"], "tr");
        assert_eq!(requests[0].query["pageToken"], "opaque+/= cursor");
        assert_eq!(requests[1].resource, "channels");
        assert_eq!(requests[1].query["part"], "snippet,statistics");
        let requested_ids = requests[1].query["id"].split(',').collect::<Vec<_>>();
        assert_eq!(requested_ids.len(), 2);
        assert!(requested_ids.contains(&CHANNEL_ID));
        assert!(requested_ids.contains(&"UC-removed"));
        assert!(fixture.public_pages.lock().unwrap().is_empty());
        let encoded = serde_json::to_value(&page).unwrap();
        assert!(encoded.get("creators").is_none());
        // A hit names its channel and never carries a copy of it.
        assert!(encoded["items"][0].get("channel").is_none());
        for field in [
            "contacts",
            "recent_videos",
            "public_comments",
            "hidden_business_email",
        ] {
            assert!(encoded["items"][0].get(field).is_none(), "{field}");
            assert!(
                encoded["channels"][CHANNEL_ID].get(field).is_none(),
                "{field}"
            );
        }
    }

    #[test]
    fn channel_search_retains_channel_hit_fields_without_about_or_activity_requests() {
        let (client, fixture) = fixture_client(&[
            (
                "search",
                json!({"items": [{
                    "id": {"channelId": CHANNEL_ID},
                    "snippet": {"title": "Search title", "description": "Search description"}
                }]}),
            ),
            ("channels", channel_response()),
        ]);
        let page = block_on(client.search(search_input(YoutubeSearchKind::Channel))).unwrap();
        assert_eq!(page.items.len(), 1);
        let hit = &page.items[0];
        assert_eq!(
            hit.channel_url,
            format!("https://www.youtube.com/channel/{CHANNEL_ID}")
        );
        assert_eq!(hit.matched.title, "Search title");
        assert!(hit.matched.video_id.is_none());
        assert_eq!(page.channels[CHANNEL_ID].description, "API description");
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests.len(), 2, "{requests:?}");
        assert_eq!(requests[0].query["type"], "channel");
        assert!(fixture.public_pages.lock().unwrap().is_empty());
    }

    #[test]
    fn an_empty_search_page_keeps_its_cursor_without_a_channel_batch() {
        let (client, fixture) = fixture_client(&[(
            "search",
            json!({
                "items": [], "nextPageToken": "continue"
            }),
        )]);
        let page = block_on(client.search(search_input(YoutubeSearchKind::Channel))).unwrap();
        assert!(page.items.is_empty());
        assert!(page.channels.is_empty());
        assert_eq!(page.next_page_token.as_deref(), Some("continue"));
        assert_eq!(fixture.requests.lock().unwrap().len(), 1);
        assert!(fixture.public_pages.lock().unwrap().is_empty());
    }

    #[test]
    fn invalid_search_count_and_empty_query_are_rejected_before_any_provider_request() {
        let (client, fixture) = fixture_client(&[]);
        for count in [0, 51, u8::MAX] {
            let mut input = search_input(YoutubeSearchKind::Channel);
            input.max_results = count;
            assert!(matches!(
                block_on(client.search(input)),
                Err(YoutubeClientError::InvalidSearchCount)
            ));
        }
        let mut input = search_input(YoutubeSearchKind::Video);
        input.query = " \t\n".into();
        assert!(matches!(
            block_on(client.search(input)),
            Err(YoutubeClientError::EmptyQuery)
        ));
        assert!(fixture.requests.lock().unwrap().is_empty());
        assert!(fixture.public_pages.lock().unwrap().is_empty());
    }

    #[test]
    fn search_validates_trimmed_query_but_sends_the_original_query() {
        let (client, fixture) =
            fixture_client(&[("search", json!({ "items": [], "nextPageToken": null }))]);
        let mut input = search_input(YoutubeSearchKind::Channel);
        input.query = "  machine learning  ".into();
        block_on(client.search(input)).expect("search with surrounding spaces");
        assert_eq!(
            fixture.requests.lock().unwrap()[0].query["q"],
            "  machine learning  "
        );
    }

    /// The page describes each channel once, however many hits matched it.
    ///
    /// Copying the channel into every hit is what made one 50-hit page repeat
    /// the same channel description dozens of times, and channel descriptions
    /// were the single largest share of a search read.
    #[test]
    fn a_channel_many_hits_matched_is_described_once_for_the_whole_page() {
        const DESCRIPTION: &str = "The channel description every hit would have copied";
        let hits = (0..8)
            .map(|index| {
                json!({
                    "id": {"videoId": format!("video-{index}")},
                    "snippet": {"channelId": CHANNEL_ID, "title": format!("Upload {index}")}
                })
            })
            .collect::<Vec<_>>();
        let (client, _) = fixture_client(&[
            ("search", json!({ "items": hits })),
            (
                "channels",
                json!({"items": [{
                    "id": CHANNEL_ID,
                    "snippet": {"title": "Channel title", "description": DESCRIPTION},
                    "statistics": {"subscriberCount": "125"}
                }]}),
            ),
        ]);

        let page = block_on(client.search(search_input(YoutubeSearchKind::Video))).unwrap();
        assert_eq!(page.items.len(), 8);
        assert_eq!(page.channels.len(), 1);
        let encoded = serde_json::to_string(&page).unwrap();
        assert_eq!(
            encoded.matches(DESCRIPTION).count(),
            1,
            "the page carries its channel description once: {encoded}"
        );
    }

    #[test]
    fn a_dated_search_sends_youtube_its_order_and_publication_bound() {
        let (client, fixture) =
            fixture_client(&[("search", json!({"items": [], "nextPageToken": null}))]);
        let mut input = search_input(YoutubeSearchKind::Video);
        input.order = YoutubeSearchOrder::Date;
        input.published_after = Some("2026-06-18T00:00:00Z".into());

        block_on(client.search(input)).expect("a dated search");

        let requests = fixture.requests.lock().unwrap();
        assert_eq!(requests[0].query["order"], "date");
        assert_eq!(requests[0].query["publishedAfter"], "2026-06-18T00:00:00Z");
    }

    #[test]
    fn a_publication_bound_outside_the_accepted_form_is_refused_before_any_request() {
        let (client, fixture) = fixture_client(&[]);
        for refused in [
            "2026-06-18",
            "2026-06-18T00:00:00+02:00",
            "2026-06-18T00:00:00.000Z",
            "yesterday",
        ] {
            let mut input = search_input(YoutubeSearchKind::Video);
            input.published_after = Some(refused.into());
            assert!(
                matches!(
                    block_on(client.search(input)),
                    Err(YoutubeClientError::InvalidPublishedAfter)
                ),
                "{refused}"
            );
        }
        assert!(fixture.requests.lock().unwrap().is_empty());

        // A blank bound is an absent bound, like every other blank criterion.
        let (client, fixture) =
            fixture_client(&[("search", json!({"items": [], "nextPageToken": null}))]);
        let mut input = search_input(YoutubeSearchKind::Video);
        input.published_after = Some("  ".into());
        block_on(client.search(input)).expect("a blank bound is no bound");
        assert!(
            !fixture.requests.lock().unwrap()[0]
                .query
                .contains_key("publishedAfter")
        );
    }

    #[test]
    fn channel_get_preserves_complete_about_tiktok_urls_for_creator_save() {
        let (client, _) = fixture_client(&[
            ("channels", channel_response()),
            ("playlistItems", json!({"items": []})),
        ]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        let public_profile = serde_json::to_string(&channel).unwrap();

        for url in [
            "https://www.tiktok.com/@creator",
            "https://www.tiktok.com/@creator.name",
        ] {
            assert!(
                public_profile.contains(&format!("\"{url}\"")),
                "the public profile must retain the complete About destination {url}: {public_profile}"
            );
        }
    }

    #[test]
    fn channel_get_raw_profile_fixture_preserves_the_evidence_used_for_saving() {
        let (client, fixture) = fixture_client(&[
            ("channels", channel_response()),
            (
                "playlistItems",
                serde_json::from_str(include_str!("../tests/fixtures/creator-playlist.json"))
                    .unwrap(),
            ),
            ("commentThreads", json!({"items": []})),
        ]);
        *fixture.public_page_body.lock().unwrap() =
            Some(include_str!("../tests/fixtures/creator-about.html").into());
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        // This public provider snapshot also feeds the runtime save/readback
        // regression, so its fixture cannot bypass the actual About parser.
        let expected: Value =
            serde_json::from_str(include_str!("../tests/fixtures/creator-profile.json")).unwrap();
        assert_eq!(serde_json::to_value(channel).unwrap(), expected);
        assert_eq!(fixture.requests.lock().unwrap().len(), 3);
        assert_eq!(fixture.public_pages.lock().unwrap().len(), 1);
    }

    #[test]
    fn channel_get_marks_failed_sections_without_marking_unrequested_or_empty_sections() {
        let (client, fixture) = fixture_client(&[
            ("channels", channel_response()),
            ("playlistItems", json!({"items": []})),
        ]);
        *fixture.public_page_body.lock().unwrap() =
            Some("<html>About was not returned</html>".into());
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert_eq!(channel.unavailable_sections, [YoutubeProfileSection::About]);
        assert_eq!(channel.description, "API description");
        assert_eq!(channel.description_source_url, None);
        assert_eq!(
            fixture.requests.lock().unwrap().len(),
            2,
            "an empty uploads page has no comments target"
        );

        *fixture.public_page_body.lock().unwrap() = Some(r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{"links":[]}}};</script>"#.into());
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert!(
            channel.unavailable_sections.is_empty(),
            "an empty About block is a successful read"
        );
        assert_eq!(channel.description_source_url, None);

        let (client, fixture) = fixture_client(&[("channels", channel_response())]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert_eq!(
            channel.unavailable_sections,
            [YoutubeProfileSection::RecentVideos]
        );
        assert_eq!(
            channel.description_source_url,
            Some(format!("{}/about", channel.channel_url))
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);

        let (client, _) = fixture_client(&[
            ("channels", channel_response()),
            (
                "playlistItems",
                serde_json::from_str(include_str!("../tests/fixtures/creator-playlist.json"))
                    .unwrap(),
            ),
        ]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert_eq!(
            channel.unavailable_sections,
            [YoutubeProfileSection::PublicComments]
        );
        assert_eq!(
            channel.recent_videos.len(),
            2,
            "failed comments do not discard fetched descriptions"
        );
    }

    #[test]
    fn channel_get_retains_about_recent_video_comment_and_contact_enrichment() {
        let (client, fixture) = fixture_client(&[
            ("channels", channel_response()),
            (
                "playlistItems",
                json!({"items": [{"snippet": {
                    "resourceId": {"videoId": "recent-video"}, "title": "Recent upload",
                    "description": "Business: hello@example.com", "publishedAt": "2026-09-12T00:00:00Z"
                }}]}),
            ),
            (
                "commentThreads",
                json!({"items": [{"snippet": {"topLevelComment": {
                    "id": "comment-1", "snippet": {"authorDisplayName": "Viewer", "textDisplay": "Useful video"}
                }}}]}),
            ),
        ]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert_eq!(channel.channel_id, CHANNEL_ID);
        assert_eq!(channel.description, "About our channel");
        assert!(channel.hidden_business_email);
        assert_eq!(channel.recent_videos[0].video_id, "recent-video");
        assert_eq!(channel.public_comments[0].comment_id, "comment-1");
        assert_eq!(
            channel.recent_videos[0].description,
            "Business: hello@example.com"
        );
        assert!(channel.about_links.iter().any(|link| {
            link.target_url == "https://x.com/Creator"
                && link.source_url == format!("https://www.youtube.com/channel/{CHANNEL_ID}/about")
        }));
        assert!(channel.unavailable_sections.is_empty());
        let requests = fixture.requests.lock().unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|request| request.resource.as_str())
                .collect::<Vec<_>>(),
            ["channels", "playlistItems", "commentThreads"]
        );
        assert_eq!(
            requests[0].query["part"],
            "snippet,statistics,contentDetails"
        );
        assert_eq!(
            fixture.public_pages.lock().unwrap().as_slice(),
            [format!(
                "https://www.youtube.com/channel/{CHANNEL_ID}/about"
            )]
        );
    }

    #[test]
    fn light_channel_read_omits_comments_explicitly_but_keeps_recent_and_about_evidence() {
        let (client, fixture) = fixture_client(&[
            ("channels", channel_response()),
            (
                "playlistItems",
                json!({"items": [{"snippet": {
                    "resourceId": {"videoId": "recent-video"},
                    "title": "Recent upload",
                    "description": "Business: hello@example.com"
                }}]}),
            ),
        ]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Light)).unwrap();
        assert!(channel.public_comments.is_empty());
        assert_eq!(
            channel.omitted_sections,
            [YoutubeProfileSection::PublicComments]
        );
        assert_eq!(
            channel.recent_videos[0].description,
            "Business: hello@example.com"
        );
        assert_eq!(channel.description, "About our channel");
        assert!(
            channel
                .about_links
                .iter()
                .any(|link| link.target_url == "https://x.com/Creator")
        );
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    }

    #[test]
    fn full_channel_read_keeps_comments_requested_and_has_no_omitted_sections() {
        let (client, fixture) = fixture_client(&[
            ("channels", channel_response()),
            ("playlistItems", json!({"items": []})),
        ]);
        let channel =
            block_on(client.creator_by_channel_id(CHANNEL_ID, YoutubeReadDepth::Full)).unwrap();
        assert!(channel.omitted_sections.is_empty());
        assert_eq!(fixture.requests.lock().unwrap().len(), 2);
    }

    /// The platform-native data a peer receives for one channel is the serde
    /// serialization of this public profile snapshot: About links keep their
    /// complete destinations as published, video descriptions keep their
    /// text, and the facade classifies no contact.
    #[test]
    fn the_public_profile_fixture_keeps_about_links_verbatim_without_contact_classification() {
        let channel: crate::YoutubeCreator =
            serde_json::from_str(include_str!("../tests/fixtures/creator-profile.json")).unwrap();
        let native = serde_json::to_value(&channel).unwrap();
        assert!(native.is_object());
        assert!(native.get("contacts").is_none());
        assert_eq!(
            native["about_links"][1]["target_url"],
            "https://www.tiktok.com/@creator"
        );
        assert_eq!(
            native["about_links"][2]["target_url"],
            "https://www.tiktok.com/@creator.name"
        );
        assert!(
            native["recent_videos"][0]["description"]
                .as_str()
                .unwrap()
                .contains("https://sponsor.example.com/offer")
        );
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/creator-profile.json")).unwrap();
        assert_eq!(native, fixture);
    }

    #[test]
    fn search_response_preserves_next_page_token() {
        let response: SearchResponse = serde_json::from_value(serde_json::json!({
            "items": [],
            "nextPageToken": "youtube-next"
        }))
        .expect("search response");

        assert_eq!(response.next_page_token.as_deref(), Some("youtube-next"));
    }
}
