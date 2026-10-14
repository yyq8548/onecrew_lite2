use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;
use serde_json::{Value, json};

/// Community-maintained twitter-openapi placeholder file (actively updated) —
/// the first live source we consult to refresh a stale fallback queryId, exactly
/// like twitter-cli's `TWITTER_OPENAPI_URL`.
pub(crate) const OPENAPI_URL: &str = "https://raw.githubusercontent.com/fa0311/twitter-openapi/refs/heads/main/src/config/placeholder.json";

/// Cap on JS bundles fetched during a live scan (a rare recovery path). The
/// queryId-bearing bundles are near the top of the list; the openapi source is
/// tried first anyway.
pub(crate) const MAX_SCAN_BUNDLES: usize = 40;

/// `queryId` for one operation from the community twitter-openapi placeholder JSON.
pub(crate) fn openapi_query_id(json_text: &str, operation: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(json_text).ok()?;
    parsed
        .get(operation)?
        .get("queryId")?
        .as_str()
        .map(str::to_string)
}

/// Distinct `abs.twimg.com` client-web JS bundle URLs referenced by x.com HTML.
pub(crate) fn bundle_urls(html: &str) -> Vec<String> {
    let re = Regex::new(r#"(https://abs\.twimg\.com/responsive-web/client-web[^"']+\.js)"#)
        .expect("static bundle-url regex");
    let mut seen = BTreeSet::new();
    re.captures_iter(html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .filter(|url| seen.insert(url.clone()))
        .take(MAX_SCAN_BUNDLES)
        .collect()
}

/// `(operationName, queryId)` pairs mined from a JS bundle (twitter-cli's regex).
pub(crate) fn scan_js_query_ids(js: &str) -> Vec<(String, String)> {
    let re = Regex::new(r#"queryId:\s*"([A-Za-z0-9_-]+)"[^}]{0,200}operationName:\s*"([^"]+)""#)
        .expect("static queryId regex");
    re.captures_iter(js)
        .filter_map(|c| {
            Some((
                c.get(2)?.as_str().to_string(),
                c.get(1)?.as_str().to_string(),
            ))
        })
        .collect()
}

/// Drop `false`-valued feature flags — Twitter defaults missing ones to false, so
/// this keeps the GraphQL GET URL short enough to avoid 414 (twitter-cli does the
/// same in `_build_graphql_url`).
pub(crate) fn compact_features(features: &Value) -> Value {
    match features.as_object() {
        Some(map) => Value::Object(
            map.iter()
                .filter(|(_, value)| value.as_bool() != Some(false))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ),
        None => features.clone(),
    }
}

pub(crate) fn query_ids() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("HomeTimeline", "c-CzHF1LboFilMpsx4ZCrQ"),
        ("HomeLatestTimeline", "BKB7oi212Fi7kQtCBGE4zA"),
        ("UserByScreenName", "1VOOyvKkiI3FMmkeDNxM9A"),
        ("UserTweets", "q6xj5bs0hapm9309hexA_g"),
        ("TweetDetail", "xd_EMdYvB9hfZsZ6Idri0w"),
        ("Likes", "lIDpu_NWL7_VhimGGt0o6A"),
        ("SearchTimeline", "VhUd6vHVmLBcw0uX-6jMLA"),
        ("Bookmarks", "2neUNDqrrFzbLui8yallcQ"),
        ("Followers", "IOh4aS6UdGWGJUYTqliQ7Q"),
        ("Following", "zx6e-TLzRkeDO_a7p4b3JQ"),
        ("ListLatestTweetsTimeline", "RlZzktZY_9wJynoepm8ZsA"),
        ("TweetResultByRestId", "7xflPyRiUxGVbJd4uWmbfg"),
        ("BookmarkFoldersSlice", "i78YDd0Tza-dV4SYs58kRg"),
        ("BookmarkFolderTimeline", "hNY7X2xE2N7HVF6Qb_mu6w"),
        ("CreateTweet", "wUgPBh9hEKhMMGlg8uDuFw"),
        ("DeleteTweet", "VaenaVgh5q5ih7kvyVjgtg"),
        ("FavoriteTweet", "lI07N6Otwv1PhnEgXILM7A"),
        ("UnfavoriteTweet", "ZYKSe-w7KEslx3JhSIk5LA"),
        ("CreateRetweet", "ojPdsZsimiJrUGLR1sjUtA"),
        ("DeleteRetweet", "iQtK4dl5hBmXewYZuEOKVw"),
        ("CreateBookmark", "aoDbu3RHznuiSkQ9aNM67Q"),
        ("DeleteBookmark", "Wlmlj2-xzyS1GN3a6cj-mQ"),
    ])
}

/// Shared read feature set, aligned verbatim with twitter-cli's `_DEFAULT_FEATURES`.
/// Twitter defaults any missing feature to `false`, and `compact_features` drops
/// the `false` ones from the URL, so this is a superset the read GraphQL ops share.
pub fn default_features() -> Value {
    json!({
        "responsive_web_graphql_exclude_directive_enabled": true,
        "verified_phone_label_enabled": false,
        "creator_subscriptions_tweet_preview_api_enabled": true,
        "responsive_web_graphql_timeline_navigation_enabled": true,
        "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
        "c9s_tweet_anatomy_moderator_badge_enabled": true,
        "tweetypie_unmention_optimization_enabled": true,
        "responsive_web_edit_tweet_api_enabled": true,
        "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
        "view_counts_everywhere_api_enabled": true,
        "longform_notetweets_consumption_enabled": true,
        "responsive_web_twitter_article_tweet_consumption_enabled": true,
        "tweet_awards_web_tipping_enabled": false,
        "longform_notetweets_rich_text_read_enabled": true,
        "longform_notetweets_inline_media_enabled": true,
        "rweb_video_timestamps_enabled": true,
        "responsive_web_media_download_video_enabled": true,
        "freedom_of_speech_not_reach_fetch_enabled": true,
        "standardized_nudges_misinfo": true,
        "responsive_web_enhance_cards_enabled": false,
    })
}

/// Feature flags the current web client sends with `CreateTweet`. This query
/// version validates its own feature set, so post/reply must send these rather
/// than the read-oriented `default_features()`.
pub(crate) fn create_tweet_features() -> Value {
    json!({
        "articles_preview_enabled": true,
        "c9s_tweet_anatomy_moderator_badge_enabled": true,
        "communities_web_enable_tweet_community_results_fetch": true,
        "content_disclosure_ai_generated_indicator_enabled": true,
        "content_disclosure_indicator_enabled": true,
        "freedom_of_speech_not_reach_fetch_enabled": true,
        "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
        "longform_notetweets_consumption_enabled": true,
        "longform_notetweets_inline_media_enabled": false,
        "longform_notetweets_rich_text_read_enabled": true,
        "post_ctas_fetch_enabled": false,
        "premium_content_api_read_enabled": false,
        "profile_label_improvements_pcf_label_in_post_enabled": true,
        "responsive_web_edit_tweet_api_enabled": true,
        "responsive_web_graphql_timeline_navigation_enabled": true,
        "responsive_web_grok_analysis_button_from_backend": true,
        "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
        "responsive_web_grok_analyze_post_followups_enabled": true,
        "responsive_web_grok_annotations_enabled": true,
        "responsive_web_grok_community_note_auto_translation_is_enabled": true,
        "responsive_web_grok_image_annotation_enabled": true,
        "responsive_web_grok_imagine_annotation_enabled": true,
        "responsive_web_grok_share_attachment_enabled": true,
        "responsive_web_grok_show_grok_translated_post": true,
        "responsive_web_jetfuel_frame": true,
        "responsive_web_profile_redirect_enabled": true,
        "responsive_web_twitter_article_tweet_consumption_enabled": true,
        "rweb_cashtags_composer_attachment_enabled": true,
        "rweb_cashtags_enabled": true,
        "rweb_conversational_replies_downvote_enabled": false,
        "rweb_tipjar_consumption_enabled": false,
        "standardized_nudges_misinfo": true,
        "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
        "verified_phone_label_enabled": false,
        "view_counts_everywhere_api_enabled": true,
    })
}
