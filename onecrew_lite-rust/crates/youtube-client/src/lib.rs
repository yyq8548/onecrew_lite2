//! Typed YouTube operations and response normalization.
//!
//! The client combines API-backed reads with public-page parsing and exposes
//! provider models without owning credentials, Creator persistence, or UI policy.

mod client;
mod constraints;
mod error;
mod models;
mod parser;

pub use client::YoutubeClient;
pub use constraints::{
    PUBLISHED_AFTER_FORMAT, SEARCH_DEFAULT_COUNT, SEARCH_MAX_COUNT, SEARCH_MIN_COUNT,
    accepts_channel_id, accepts_published_after, accepts_search_count, accepts_search_query,
};
pub use error::YoutubeClientError;
pub use models::{
    YoutubeChannelLink, YoutubeChannelSummary, YoutubeComment, YoutubeCreator,
    YoutubeProfileSection, YoutubeReadDepth, YoutubeSearchChannel, YoutubeSearchHit,
    YoutubeSearchInput, YoutubeSearchKind, YoutubeSearchMatch, YoutubeSearchOrder,
    YoutubeSearchPage, YoutubeVideo,
};

#[cfg(test)]
mod tests {
    use super::parser::parse_about_html;

    #[test]
    fn about_page_extracts_official_links_and_hidden_email_flag() {
        let body = r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{
            "description":"Creator", "signInForBusinessEmail":{"text":"Sign in"},
            "links":[{"channelExternalLinkViewModel":{"link":{"content":"x.com/Creator"}}}]
        }}};</script>"#;
        let parsed = parse_about_html(body).expect("parse about page");
        assert_eq!(parsed.description, "Creator");
        assert!(parsed.hidden_business_email);
        assert_eq!(parsed.links, ["https://x.com/Creator"]);
    }

    // The shape of a real curated link, captured from a live channel: every
    // entry carries a favicon whose own URL has a `?q=tbn:...` parameter, and
    // `serde_json::Map` orders keys alphabetically, so `favicon` is reached
    // before `link`. A generic "first URL-shaped string" walk therefore read
    // the favicon, unwrapped its `q` into `tbn:ANd9Gc…`, and dropped the
    // result as unparseable — silently losing every link a channel curated,
    // which is why one real run produced 1,439 contacts from video
    // descriptions and none at all from the About page.
    #[test]
    fn curated_links_survive_the_favicon_that_sorts_before_them() {
        let body = r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{
            "description":"LCX",
            "signInForBusinessEmail":{"content":"Sign in to see email address"},
            "links":[
              {"channelExternalLinkViewModel":{
                "title":{"content":"LCX.com"},
                "link":{"content":"lcx.com","commandRuns":[{"onTap":{"innertubeCommand":{
                  "urlEndpoint":{"url":"https://www.youtube.com/redirect?event=channel_description&redir_token=QUM4&q=https%3A%2F%2Fwww.lcx.com"}}}}]},
                "favicon":{"sources":[{"url":"https://encrypted-tbn3.gstatic.com/favicon-tbn?q=tbn:ANd9GcQM1VFy","width":16,"height":16}]}}},
              {"channelExternalLinkViewModel":{
                "title":{"content":"Telegram"},
                "link":{"content":"t.me/JoinLCX","commandRuns":[{"onTap":{"innertubeCommand":{
                  "urlEndpoint":{"url":"https://www.youtube.com/redirect?q=http%3A%2F%2Ft.me%2FJoinLCX"}}}}]},
                "favicon":{"sources":[{"url":"https://encrypted-tbn1.gstatic.com/favicon-tbn?q=tbn:ANd9GcS7"}]}}},
              {"channelExternalLinkViewModel":{
                "title":{"content":"Shop"},
                "link":{"content":"shop.lcx.com/collections/all"},
                "favicon":{"sources":[{"url":"https://encrypted-tbn0.gstatic.com/favicon-tbn?q=tbn:ANd9GcTC"}]}}}
            ]
        }}};</script>"#;
        let parsed = parse_about_html(body).expect("parse about page");
        assert!(parsed.hidden_business_email);
        assert_eq!(
            parsed.links,
            [
                // Unwrapped from YouTube's own redirect, which carries the
                // full destination even when the visible text is shortened.
                "https://www.lcx.com",
                "http://t.me/JoinLCX",
                // No command: the visible text is the destination.
                "https://shop.lcx.com/collections/all",
            ]
        );
    }

    // A schemeless display label is not an address. Prefixing a scheme onto a
    // bare word produced the contact route `https://linustech/`, which is
    // reachable by nobody.
    #[test]
    fn a_bare_word_never_becomes_a_contact_route() {
        let body = r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{
            "links":[
              {"channelExternalLinkViewModel":{"link":{"content":"linustech"}}},
              {"channelExternalLinkViewModel":{"link":{"content":"linustechtips.com"}}}
            ]
        }}};</script>"#;
        let parsed = parse_about_html(body).expect("parse about page");
        assert_eq!(parsed.links, ["https://linustechtips.com"]);
    }

    #[test]
    fn about_page_rejects_email_shaped_links() {
        let body = r#"<script>var ytInitialData = {"header":{"aboutChannelViewModel":{
            "links":[{"channelExternalLinkViewModel":{"link":{"url":"creator@example.com"}}}]
        }}};</script>"#;
        let parsed = parse_about_html(body).expect("parse about page");
        assert!(parsed.links.is_empty());
    }
}
