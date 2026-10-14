use onecrew_x_transport::{JsonTransport, Method};
use serde_json::{Value, json};
use url::Url;

use crate::advanced_search::compile_advanced_search;
use crate::graphql::default_features;
use crate::models::{
    BookmarkFolderPage, TimelineKind, TimelinePage, Tweet, User, UserListKind, UserPage,
};
use crate::parser::{
    find_first, parse_bookmark_folders, parse_timeline, parse_tweet, parse_user, parse_users,
};
use crate::{
    AdvancedSearchCriteria, AdvancedSearchPage, AdvancedSearchResultMode,
    accepts_advanced_search_count,
};

use super::request::add_cursor;
use super::{XClient, XClientError};

/// Smallest page size a post-thread read accepts.
pub const POST_THREAD_MIN_COUNT: u16 = 1;
/// Largest page size a post-thread read accepts. The accepted range is wider
/// than what X is asked for: [`XClient::tweet_detail`] truncates the count in
/// the provider request to 40, which is provider request behavior, not input
/// validation.
pub const POST_THREAD_MAX_COUNT: u16 = 100;
/// Page size of a post-thread read that names none.
pub const POST_THREAD_DEFAULT_COUNT: u16 = 40;

/// Whether `count` is inside the accepted post-thread page-size range.
pub fn accepts_post_thread_count(count: u16) -> bool {
    (POST_THREAD_MIN_COUNT..=POST_THREAD_MAX_COUNT).contains(&count)
}

impl<T: JsonTransport> XClient<T> {
    pub async fn search_page(
        &self,
        query: &str,
        count: u16,
        product: &str,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        let mut variables = json!({
            "rawQuery": query,
            "count": count.clamp(1, 100),
            "querySource": "typed_query",
            "product": product,
        });
        add_cursor(&mut variables, cursor);
        Ok(parse_timeline(
            &self
                .post("SearchTimeline", variables, Some(default_features()))
                .await?,
        ))
    }

    pub async fn search_advanced_page(
        &self,
        criteria: &AdvancedSearchCriteria,
        result_mode: AdvancedSearchResultMode,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<AdvancedSearchPage, XClientError> {
        if !accepts_advanced_search_count(count) {
            return Err(XClientError::Input(
                "advanced search count must be between 1 and 100".into(),
            ));
        }
        let effective_query = compile_advanced_search(criteria)
            .map_err(|error| XClientError::Input(error.to_string()))?;
        let page = self
            .search_page(&effective_query, count, result_mode.product(), cursor)
            .await?;
        Ok(AdvancedSearchPage {
            effective_query,
            page,
        })
    }

    pub async fn user(&self, username: &str) -> Result<Value, XClientError> {
        self.get(
            "UserByScreenName",
            json!({
                "screen_name": username.trim_start_matches('@'),
                "withSafetyModeUserFields": true,
            }),
            default_features(),
        )
        .await
    }

    pub async fn user_profile(&self, username: &str) -> Result<User, XClientError> {
        let payload = self.user(username).await?;
        find_first(&payload, parse_user)
            .ok_or_else(|| XClientError::Response("user profile was missing".into()))
    }

    /// The operating account's own profile, resolved once per client (two
    /// requests: the account list for the handle, then that profile). The
    /// answer cannot change while the session's cookies do not, and a single
    /// DM sync asks for it from several layers.
    pub async fn current_user(&self) -> Result<User, XClientError> {
        self.identity
            .get_or_try_init(|| self.resolve_current_user())
            .await
            .cloned()
    }

    async fn resolve_current_user(&self) -> Result<User, XClientError> {
        let url = Url::parse("https://x.com/i/api/1.1/account/multi/list.json")
            .map_err(|error| XClientError::Url(error.to_string()))?;
        let payload = self.transport.request(Method::GET, url, None).await?;
        let username = payload
            .pointer("/users/0/screen_name")
            .or_else(|| payload.pointer("/0/user/screen_name"))
            .and_then(Value::as_str)
            .ok_or_else(|| XClientError::Response("current username was missing".into()))?;
        self.user_profile(username).await
    }

    pub async fn timeline(
        &self,
        kind: TimelineKind,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        let mut variables = json!({
            "count": count.clamp(1, 40),
            "includePromotedContent": false,
            "latestControlAvailable": true,
            "requestContext": "launch",
        });
        if let Some(cursor) = cursor {
            variables["cursor"] = Value::String(cursor.into());
        }
        let payload = self
            .get(kind.operation(), variables, default_features())
            .await?;
        Ok(parse_timeline(&payload))
    }

    pub async fn user_tweets(
        &self,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        let mut variables = json!({
            "userId": user_id,
            "count": count.clamp(1, 40),
            "includePromotedContent": true,
            "withQuickPromoteEligibilityTweetFields": true,
            "withVoice": true,
            "withV2Timeline": true,
        });
        if let Some(cursor) = cursor {
            variables["cursor"] = Value::String(cursor.into());
        }
        Ok(parse_timeline(
            &self
                .get("UserTweets", variables, default_features())
                .await?,
        ))
    }

    pub async fn user_likes(
        &self,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        self.named_timeline(
            "Likes",
            json!({
                "userId": user_id,
                "count": count.clamp(1, 40),
                "includePromotedContent": false,
                "withClientEventToken": false,
                "withBirdwatchNotes": false,
                "withVoice": true,
            }),
            cursor,
        )
        .await
    }

    pub async fn list_timeline(
        &self,
        list_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        self.named_timeline(
            "ListLatestTweetsTimeline",
            json!({"listId": list_id, "count": count.clamp(1, 40)}),
            cursor,
        )
        .await
    }

    pub async fn bookmark_folder_timeline(
        &self,
        folder_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        self.named_timeline(
            "BookmarkFolderTimeline",
            json!({
                "bookmark_collection_id": folder_id,
                "count": count.clamp(1, 40),
                "includePromotedContent": false,
            }),
            cursor,
        )
        .await
    }

    async fn named_timeline(
        &self,
        operation: &str,
        mut variables: Value,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        add_cursor(&mut variables, cursor);
        Ok(parse_timeline(
            &self.get(operation, variables, default_features()).await?,
        ))
    }

    pub async fn bookmark_folders(
        &self,
        cursor: Option<&str>,
    ) -> Result<BookmarkFolderPage, XClientError> {
        let mut variables = json!({});
        add_cursor(&mut variables, cursor);
        Ok(parse_bookmark_folders(
            &self
                .get("BookmarkFoldersSlice", variables, default_features())
                .await?,
        ))
    }

    pub async fn article(&self, tweet_id: &str) -> Result<Tweet, XClientError> {
        let payload = self
            .get(
                "TweetResultByRestId",
                json!({
                    "tweetId": tweet_id,
                    "withCommunity": false,
                    "includePromotedContent": false,
                    "withVoice": false,
                }),
                default_features(),
            )
            .await?;
        find_first(&payload, parse_tweet)
            .filter(|tweet| tweet.article.is_some())
            .ok_or_else(|| XClientError::Response("tweet has no article content".into()))
    }

    pub async fn tweet_detail(
        &self,
        tweet_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XClientError> {
        // Provider request behavior: X is asked for at most 40 posts even when
        // a caller passes an accepted page size up to `POST_THREAD_MAX_COUNT`.
        let mut variables = json!({
            "focalTweetId": tweet_id,
            "count": count.clamp(1, 40),
            "referrer": "tweet",
            "with_rux_injections": false,
            "includePromotedContent": true,
            "rankingMode": "Relevance",
            "withCommunity": true,
            "withQuickPromoteEligibilityTweetFields": true,
            "withBirdwatchNotes": true,
            "withVoice": true,
        });
        if let Some(cursor) = cursor {
            variables["cursor"] = Value::String(cursor.into());
        }
        Ok(parse_timeline(
            &self
                .get("TweetDetail", variables, default_features())
                .await?,
        ))
    }

    pub async fn users(
        &self,
        kind: UserListKind,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<UserPage, XClientError> {
        let mut variables = json!({
            "userId": user_id,
            "count": count.clamp(1, 40),
            "includePromotedContent": false,
        });
        if let Some(cursor) = cursor {
            variables["cursor"] = Value::String(cursor.into());
        }
        let payload = self
            .post(kind.operation(), variables, Some(default_features()))
            .await?;
        Ok(parse_users(&payload))
    }
}

#[cfg(test)]
mod identity_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use onecrew_x_transport::{JsonTransport, Method, TransportError};
    use serde_json::{Value, json};
    use url::Url;

    use crate::client::XClient;

    /// Counts every request so the memoized identity can be proven, and
    /// answers the two calls `current_user` makes.
    #[derive(Clone, Default)]
    struct CountingTransport {
        requests: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl JsonTransport for CountingTransport {
        async fn request(
            &self,
            _method: Method,
            url: Url,
            _body: Option<Value>,
        ) -> Result<Value, TransportError> {
            self.requests.fetch_add(1, Ordering::SeqCst);
            if url.path().contains("multi/list.json") {
                return Ok(json!({ "users": [{ "screen_name": "operator" }] }));
            }
            Ok(json!({
                "data": {
                    "user": {
                        "result": {
                            "__typename": "User",
                            "rest_id": "42",
                            "core": { "name": "Operator", "screen_name": "operator" },
                            "legacy": {},
                        }
                    }
                }
            }))
        }

        async fn request_form(
            &self,
            _method: Method,
            _url: Url,
            _form: BTreeMap<String, String>,
        ) -> Result<Value, TransportError> {
            unreachable!("identity resolution never posts a form")
        }

        async fn fetch_text(&self, _url: Url) -> Result<String, TransportError> {
            unreachable!("identity resolution never scans bundles")
        }
    }

    #[tokio::test]
    async fn the_operating_identity_is_resolved_once_per_client() {
        let transport = CountingTransport::default();
        let requests = Arc::clone(&transport.requests);
        let client = XClient::new(transport);

        let first = client.current_user().await.expect("identity");
        let after_first = requests.load(Ordering::SeqCst);
        assert_eq!(first.username, "operator");
        assert_eq!(after_first, 2, "the account list then that profile");

        let second = client.current_user().await.expect("identity");
        assert_eq!(second.id, first.id);
        assert_eq!(
            requests.load(Ordering::SeqCst),
            after_first,
            "the cached identity costs no further requests"
        );

        // Clones share the cache — that is what lets XAccount hand the same
        // client to the chat service.
        let clone = client.clone();
        clone.current_user().await.expect("identity");
        assert_eq!(requests.load(Ordering::SeqCst), after_first);
    }
}

#[cfg(test)]
mod search_request_tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use onecrew_x_transport::{JsonTransport, Method, TransportError};
    use serde_json::{Value, json};
    use url::Url;

    use crate::{AdvancedSearchCriteria, AdvancedSearchFilter, AdvancedSearchResultMode, XClient};

    #[derive(Clone, Default)]
    struct CapturingTransport {
        request: Arc<Mutex<Option<(Method, Url, Value)>>>,
        response: Option<Value>,
    }

    #[async_trait]
    impl JsonTransport for CapturingTransport {
        async fn request(
            &self,
            method: Method,
            url: Url,
            body: Option<Value>,
        ) -> Result<Value, TransportError> {
            *self.request.lock().unwrap() =
                Some((method, url, body.expect("SearchTimeline uses a JSON body")));
            Ok(self.response.clone().unwrap_or_else(|| json!({})))
        }

        async fn request_form(
            &self,
            _method: Method,
            _url: Url,
            _form: BTreeMap<String, String>,
        ) -> Result<Value, TransportError> {
            unreachable!("search never posts a form")
        }

        async fn fetch_text(&self, _url: Url) -> Result<String, TransportError> {
            unreachable!("the fallback SearchTimeline query ID is available")
        }
    }

    #[tokio::test]
    async fn raw_search_query_is_forwarded_verbatim() {
        let transport = CapturingTransport::default();
        let request = Arc::clone(&transport.request);
        XClient::new(transport)
            .search_page(
                "\"AI agents\" (rust OR tauri) from:openai lang:en 🚀",
                17,
                "Latest",
                Some("raw-cursor"),
            )
            .await
            .unwrap();

        let (method, url, body) = request.lock().unwrap().clone().expect("request capture");
        assert_eq!(method, Method::POST);
        assert!(url.path().ends_with("/SearchTimeline"));
        assert_eq!(
            body["variables"],
            json!({
                "rawQuery": "\"AI agents\" (rust OR tauri) from:openai lang:en 🚀",
                "count": 17,
                "querySource": "typed_query",
                "product": "Latest",
                "cursor": "raw-cursor"
            })
        );
    }

    #[tokio::test]
    async fn advanced_search_captures_compiled_query_product_count_and_cursor() {
        let transport = CapturingTransport::default();
        let request = Arc::clone(&transport.request);
        let result = XClient::new(transport)
            .search_advanced_page(
                &AdvancedSearchCriteria {
                    exact_phrase: Some("local first".into()),
                    from_accounts: vec!["@OpenAI".into()],
                    min_likes: Some(10),
                    links: AdvancedSearchFilter::Exclude,
                    ..Default::default()
                },
                AdvancedSearchResultMode::Top,
                25,
                Some("advanced-cursor"),
            )
            .await
            .unwrap();

        assert_eq!(
            result.effective_query,
            "\"local first\" from:OpenAI -filter:links min_faves:10"
        );
        let (method, url, body) = request.lock().unwrap().clone().expect("request capture");
        assert_eq!(method, Method::POST);
        assert!(url.path().ends_with("/SearchTimeline"));
        assert_eq!(body["variables"]["rawQuery"], result.effective_query);
        assert_eq!(body["variables"]["product"], "Top");
        assert_eq!(body["variables"]["count"], 25);
        assert_eq!(body["variables"]["cursor"], "advanced-cursor");
        assert_eq!(body["variables"]["querySource"], "typed_query");
    }

    #[tokio::test]
    async fn advanced_search_preserves_the_complete_native_page_and_cursor() {
        let mut fixture: Value =
            serde_json::from_str(include_str!("../../fixtures/search_timeline.json")).unwrap();
        let items =
            fixture["data"]["search_by_raw_query"]["search_timeline"]["timeline"]["instructions"]
                [0]["entries"][0]["content"]["items"]
                .as_array_mut()
                .unwrap();
        let mut extra = items[0].clone();
        extra["item"]["itemContent"]["tweet_results"]["result"]["tweet"]["rest_id"] = json!("501");
        items.push(extra);
        let expected = crate::parse_timeline(&fixture);
        let transport = CapturingTransport {
            response: Some(fixture),
            ..Default::default()
        };
        let native = XClient::new(transport)
            .search_advanced_page(
                &AdvancedSearchCriteria {
                    all_words: vec!["rust".into()],
                    ..Default::default()
                },
                AdvancedSearchResultMode::Latest,
                1,
                Some("incoming-provider-cursor"),
            )
            .await
            .unwrap();

        assert_eq!(native.page, expected);
        assert_eq!(
            native.page.items.len(),
            2,
            "X count is a page hint, not local truncation"
        );
        let public = serde_json::to_value(native).unwrap();
        assert_eq!(public["effective_query"], "rust");
        assert_eq!(public["page"]["next_cursor"], "search-cursor");
        assert_eq!(public["page"]["items"][0]["id"], "500");
        assert_eq!(public["page"]["items"][1]["id"], "501");
    }

    #[tokio::test]
    async fn advanced_search_rejects_invalid_count_without_a_request() {
        for count in [0, 101] {
            let transport = CapturingTransport::default();
            let request = Arc::clone(&transport.request);
            let error = XClient::new(transport)
                .search_advanced_page(
                    &AdvancedSearchCriteria {
                        all_words: vec!["safe".into()],
                        ..Default::default()
                    },
                    AdvancedSearchResultMode::Top,
                    count,
                    None,
                )
                .await
                .expect_err("count outside the public contract must fail");
            assert!(matches!(error, crate::XClientError::Input(_)));
            assert!(request.lock().unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn advanced_search_requests_the_published_count_bounds() {
        for count in [
            crate::ADVANCED_SEARCH_MIN_COUNT,
            crate::ADVANCED_SEARCH_DEFAULT_COUNT,
            crate::ADVANCED_SEARCH_MAX_COUNT,
        ] {
            let transport = CapturingTransport::default();
            let request = Arc::clone(&transport.request);
            XClient::new(transport)
                .search_advanced_page(
                    &AdvancedSearchCriteria {
                        all_words: vec!["safe".into()],
                        ..Default::default()
                    },
                    AdvancedSearchResultMode::Top,
                    count,
                    None,
                )
                .await
                .unwrap();
            let (_, _, body) = request.lock().unwrap().clone().expect("request capture");
            assert_eq!(body["variables"]["count"], count, "{count}");
        }
    }
}

#[cfg(test)]
mod post_thread_tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use onecrew_x_transport::{JsonTransport, Method, TransportError};
    use serde_json::{Value, json};
    use url::Url;

    use crate::{
        POST_THREAD_DEFAULT_COUNT, POST_THREAD_MAX_COUNT, POST_THREAD_MIN_COUNT, XClient,
        accepts_post_thread_count,
    };

    #[test]
    fn post_thread_accepts_page_sizes_from_1_through_100_and_defaults_to_40() {
        assert_eq!(
            (
                POST_THREAD_MIN_COUNT,
                POST_THREAD_MAX_COUNT,
                POST_THREAD_DEFAULT_COUNT
            ),
            (1, 100, 40)
        );
        for (count, accepted) in [(0, false), (1, true), (40, true), (100, true), (101, false)] {
            assert_eq!(accepts_post_thread_count(count), accepted, "{count}");
        }
        assert!(accepts_post_thread_count(POST_THREAD_DEFAULT_COUNT));
    }

    /// Records the `variables` query parameter of the one GET a thread read makes.
    #[derive(Clone, Default)]
    struct CapturingTransport {
        variables: Arc<Mutex<Option<Value>>>,
    }

    #[async_trait]
    impl JsonTransport for CapturingTransport {
        async fn request(
            &self,
            method: Method,
            url: Url,
            _body: Option<Value>,
        ) -> Result<Value, TransportError> {
            assert_eq!(method, Method::GET);
            assert!(url.path().ends_with("/TweetDetail"));
            let variables = url
                .query_pairs()
                .find(|(name, _)| name == "variables")
                .map(|(_, value)| serde_json::from_str(&value).expect("JSON variables"))
                .expect("TweetDetail carries its variables in the query");
            *self.variables.lock().unwrap() = Some(variables);
            Ok(json!({}))
        }

        async fn request_form(
            &self,
            _method: Method,
            _url: Url,
            _form: BTreeMap<String, String>,
        ) -> Result<Value, TransportError> {
            unreachable!("a thread read never posts a form")
        }

        async fn fetch_text(&self, _url: Url) -> Result<String, TransportError> {
            unreachable!("the fallback TweetDetail query ID is available")
        }
    }

    #[tokio::test]
    async fn the_provider_request_truncates_an_accepted_page_size_to_40() {
        for (count, requested) in [
            (POST_THREAD_MIN_COUNT, 1),
            (POST_THREAD_DEFAULT_COUNT, 40),
            (41, 40),
            (POST_THREAD_MAX_COUNT, 40),
        ] {
            let transport = CapturingTransport::default();
            let variables = Arc::clone(&transport.variables);
            XClient::new(transport)
                .tweet_detail("500", count, None)
                .await
                .unwrap();
            let variables = variables.lock().unwrap().clone().expect("one request");
            assert_eq!(variables["focalTweetId"], "500", "{count}");
            assert_eq!(variables["count"], requested, "{count}");
        }
    }
}
