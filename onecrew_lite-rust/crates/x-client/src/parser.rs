use serde_json::{Map, Value};

use crate::models::{
    Article, Author, BookmarkFolder, BookmarkFolderPage, Media, Metrics, TimelinePage, Tweet, User,
    UserPage,
};

pub fn parse_timeline(payload: &Value) -> TimelinePage {
    let mut page = TimelinePage::default();
    visit_objects(payload, &mut |object| {
        if object.get("cursorType").and_then(Value::as_str) == Some("Bottom") {
            page.next_cursor = object
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        let Some(result) = object
            .get("tweet_results")
            .and_then(|value| value.get("result"))
        else {
            return;
        };
        if let Some(mut tweet) = parse_tweet(result) {
            tweet.promoted = object.contains_key("promotedMetadata");
            if !page.items.iter().any(|item| item.id == tweet.id) {
                page.items.push(tweet);
            }
        }
    });
    page
}

pub fn parse_users(payload: &Value) -> UserPage {
    let mut page = UserPage::default();
    visit_objects(payload, &mut |object| {
        if object.get("cursorType").and_then(Value::as_str) == Some("Bottom") {
            page.next_cursor = object
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
        let Some(result) = object
            .get("user_results")
            .and_then(|value| value.get("result"))
        else {
            return;
        };
        if let Some(user) = parse_user(result)
            && !page.items.iter().any(|item| item.id == user.id)
        {
            page.items.push(user);
        }
    });
    page
}

pub fn parse_bookmark_folders(payload: &Value) -> BookmarkFolderPage {
    let slice = payload
        .pointer("/data/viewer/user_results/result/bookmark_collections_slice")
        .unwrap_or(&Value::Null);
    let items = slice
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = string(item.get("id"));
            (!id.is_empty()).then(|| BookmarkFolder {
                id,
                name: string(item.get("name")),
            })
        })
        .collect();
    BookmarkFolderPage {
        items,
        next_cursor: slice
            .pointer("/slice_info/next_cursor")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

pub fn parse_user(value: &Value) -> Option<User> {
    let id = string(value.get("rest_id"));
    if id.is_empty() || value.get("__typename").and_then(Value::as_str) == Some("UserUnavailable") {
        return None;
    }
    let legacy = value.get("legacy").unwrap_or(&Value::Null);
    let core = value.get("core").unwrap_or(&Value::Null);
    let blue_verified = value
        .get("is_blue_verified")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(User {
        id,
        name: string(core.get("name")).or_else_string(|| string(legacy.get("name"))),
        username: string(core.get("screen_name"))
            .or_else_string(|| string(legacy.get("screen_name"))),
        bio: string(legacy.get("description")),
        location: string(value.pointer("/location/location"))
            .or_else_string(|| string(legacy.get("location"))),
        profile_links: user_profile_links(legacy),
        followers: number(legacy.get("followers_count")),
        following: number(legacy.get("friends_count")),
        posts: number(legacy.get("statuses_count")),
        likes: number(legacy.get("favourites_count")),
        verified: blue_verified
            || legacy
                .get("verified")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        blue_verified,
        profile_image_url: string(value.pointer("/avatar/image_url"))
            .or_else_string(|| string(legacy.get("profile_image_url_https"))),
        viewer_follows: legacy.get("following").and_then(Value::as_bool),
        viewer_followed_by: legacy.get("followed_by").and_then(Value::as_bool),
        can_dm: legacy.get("can_dm").and_then(Value::as_bool),
    })
}

fn user_profile_links(legacy: &Value) -> Vec<crate::models::ProfileLink> {
    let mut links = Vec::new();
    for path in ["/entities/url/urls", "/entities/description/urls"] {
        for item in legacy
            .pointer(path)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let target_url =
                string(item.get("expanded_url")).or_else_string(|| string(item.get("url")));
            let display_text =
                string(item.get("display_url")).or_else_string(|| target_url.clone());
            if !target_url.is_empty()
                && !links.iter().any(|existing: &crate::models::ProfileLink| {
                    existing.display_text == display_text && existing.target_url == target_url
                })
            {
                links.push(crate::models::ProfileLink {
                    display_text,
                    target_url,
                });
            }
        }
    }
    links
}

pub fn parse_tweet(value: &Value) -> Option<Tweet> {
    let value =
        if value.get("__typename").and_then(Value::as_str) == Some("TweetWithVisibilityResults") {
            value.get("tweet")?
        } else {
            value
        };
    if value.get("__typename").and_then(Value::as_str) == Some("TweetTombstone") {
        return None;
    }
    let legacy = value.get("legacy")?;
    let user = value.pointer("/core/user_results/result")?;
    let author = parse_user(user).map(|user| Author {
        id: user.id,
        name: user.name,
        username: user.username,
        profile_image_url: user.profile_image_url,
        verified: user.verified,
        blue_verified: user.blue_verified,
        viewer_follows: user.viewer_follows,
        viewer_followed_by: user.viewer_followed_by,
        can_dm: user.can_dm,
    })?;
    let mut media = Vec::new();
    for item in legacy
        .pointer("/extended_entities/media")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = string(item.get("type"));
        let mut url = string(item.get("media_url_https"));
        if matches!(kind.as_str(), "video" | "animated_gif")
            && let Some(best) = item
                .pointer("/video_info/variants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|variant| {
                    variant.get("content_type").and_then(Value::as_str) == Some("video/mp4")
                })
                .max_by_key(|variant| number(variant.get("bitrate")))
        {
            url = string(best.get("url"));
        }
        media.push(Media {
            kind,
            url,
            width: item.pointer("/original_info/width").and_then(Value::as_u64),
            height: item
                .pointer("/original_info/height")
                .and_then(Value::as_u64),
        });
    }
    let urls = legacy
        .pointer("/entities/urls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("expanded_url").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    Some(Tweet {
        id: string(value.get("rest_id")),
        text: string(value.pointer("/note_tweet/note_tweet_results/result/text"))
            .or_else_string(|| string(legacy.get("full_text"))),
        author,
        metrics: Metrics {
            likes: number(legacy.get("favorite_count")),
            retweets: number(legacy.get("retweet_count")),
            replies: number(legacy.get("reply_count")),
            quotes: number(legacy.get("quote_count")),
            views: number(value.pointer("/views/count")),
            bookmarks: number(legacy.get("bookmark_count")),
        },
        created_at: string(legacy.get("created_at")),
        language: string(legacy.get("lang")),
        media,
        urls,
        promoted: false,
        article: parse_article(value),
        in_reply_to_status_id: legacy
            .get("in_reply_to_status_id_str")
            .and_then(Value::as_str)
            .map(str::to_owned),
        conversation_id: legacy
            .get("conversation_id_str")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn parse_article(value: &Value) -> Option<Article> {
    let result = value.pointer("/article/article_results/result")?;
    let title = string(result.get("title"));
    let blocks = result.pointer("/content_state/blocks")?.as_array()?;
    let entity_map = result.pointer("/content_state/entityMap");
    let mut parts = Vec::new();
    let mut ordered_index = 0;
    for block in blocks {
        let kind = block
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unstyled");
        let mut text = string(block.get("text"));
        if kind == "atomic" {
            if let Some(key) = block.pointer("/entityRanges/0/key").and_then(|key| {
                key.as_str()
                    .map(str::to_owned)
                    .or_else(|| key.as_u64().map(|v| v.to_string()))
            }) && let Some(markdown) = find_entity(entity_map, &key)
                .and_then(|entity| entity.pointer("/value/data/markdown"))
                .and_then(Value::as_str)
            {
                parts.push(markdown.to_owned());
            }
            ordered_index = 0;
            continue;
        }
        if text.is_empty() {
            continue;
        }
        if kind != "ordered-list-item" {
            ordered_index = 0;
        }
        text = match kind {
            "header-one" => format!("# {text}"),
            "header-two" => format!("## {text}"),
            "header-three" => format!("### {text}"),
            "blockquote" => format!("> {text}"),
            "unordered-list-item" => format!("- {text}"),
            "ordered-list-item" => {
                ordered_index += 1;
                format!("{ordered_index}. {text}")
            }
            "code-block" => format!("```\n{text}\n```"),
            _ => text,
        };
        parts.push(text);
    }
    Some(Article {
        title,
        text: parts.join("\n\n"),
    })
}

fn find_entity<'a>(entity_map: Option<&'a Value>, key: &str) -> Option<&'a Value> {
    match entity_map? {
        Value::Object(map) => map.get(key),
        Value::Array(items) => items.iter().find(|item| {
            item.get("key")
                .and_then(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| value.as_u64().map(|number| number.to_string()))
                })
                .as_deref()
                == Some(key)
        }),
        _ => None,
    }
}

fn visit_objects(value: &Value, visitor: &mut impl FnMut(&Map<String, Value>)) {
    match value {
        Value::Object(object) => {
            visitor(object);
            for child in object.values() {
                visit_objects(child, visitor);
            }
        }
        Value::Array(items) => {
            for child in items {
                visit_objects(child, visitor);
            }
        }
        _ => {}
    }
}

pub(crate) fn find_first<T>(
    value: &Value,
    parser: impl Fn(&Value) -> Option<T> + Copy,
) -> Option<T> {
    if let Some(parsed) = parser(value) {
        return Some(parsed);
    }
    match value {
        Value::Object(object) => object.values().find_map(|child| find_first(child, parser)),
        Value::Array(items) => items.iter().find_map(|child| find_first(child, parser)),
        _ => None,
    }
}

fn string(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn number(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| {
            value.as_u64().or_else(|| {
                value
                    .as_str()
                    .and_then(|text| text.replace(',', "").parse().ok())
            })
        })
        .unwrap_or_default()
}

trait StringFallback {
    fn or_else_string(self, fallback: impl FnOnce() -> String) -> String;
}

impl StringFallback for String {
    fn or_else_string(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_fixture_and_cursor() {
        let payload: Value =
            serde_json::from_str(include_str!("../fixtures/search_timeline.json")).unwrap();
        let page = parse_timeline(&payload);
        assert_eq!(page.next_cursor.as_deref(), Some("search-cursor"));
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "500");
        assert_eq!(page.items[0].media[0].url, "https://video-high.mp4");
        assert_eq!(page.items[0].metrics.views, 4321);
    }

    #[test]
    fn parses_followers_fixture() {
        let payload: Value =
            serde_json::from_str(include_str!("../fixtures/followers_page.json")).unwrap();
        let page = parse_users(&payload);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].username, "follower1");
        assert!(page.items[0].verified);
        assert!(page.items[0].blue_verified);
    }

    #[test]
    fn parses_viewer_relationship_state() {
        let user = parse_user(&serde_json::json!({
            "__typename": "User",
            "rest_id": "7",
            "core": {"name": "Creator", "screen_name": "creator"},
            "legacy": {
                "following": true,
                "followed_by": false,
                "can_dm": true,
                "verified": true,
                "entities": {
                    "url": {"urls": [{"display_url": "creator.example", "expanded_url": "https://creator.example", "url": "https://t.co/site"}]},
                    "description": {"urls": [{"display_url": "t.me/creator", "expanded_url": "https://t.me/creator", "url": "https://t.co/tg"}]}
                }
            }
        }))
        .unwrap();
        assert!(user.verified);
        assert!(!user.blue_verified);
        assert_eq!(user.viewer_follows, Some(true));
        assert_eq!(user.viewer_followed_by, Some(false));
        assert_eq!(user.can_dm, Some(true));
        assert_eq!(
            user.profile_links,
            vec![
                crate::models::ProfileLink {
                    display_text: "creator.example".into(),
                    target_url: "https://creator.example".into()
                },
                crate::models::ProfileLink {
                    display_text: "t.me/creator".into(),
                    target_url: "https://t.me/creator".into()
                },
            ]
        );
    }

    #[test]
    fn parses_reply_relationship_fields() {
        let tweet = parse_tweet(&serde_json::json!({
            "__typename": "Tweet",
            "rest_id": "reply-1",
            "legacy": {
                "full_text": "reply",
                "entities": {"urls": []},
                "in_reply_to_status_id_str": "root-1",
                "conversation_id_str": "root-1"
            },
            "core": {"user_results": {"result": {
                "__typename": "User",
                "rest_id": "7",
                "core": {"name": "Creator", "screen_name": "creator"},
                "legacy": {}
            }}}
        }))
        .unwrap();
        assert_eq!(tweet.in_reply_to_status_id.as_deref(), Some("root-1"));
        assert_eq!(tweet.conversation_id.as_deref(), Some("root-1"));
    }

    #[test]
    fn parses_bookmark_folder_page() {
        let payload = serde_json::json!({
            "data": {"viewer": {"user_results": {"result": {
                "bookmark_collections_slice": {
                    "items": [{"id": "folder-1", "name": "Research"}],
                    "slice_info": {"next_cursor": "next-folder"}
                }
            }}}}
        });
        let page = parse_bookmark_folders(&payload);
        assert_eq!(page.items[0].name, "Research");
        assert_eq!(page.next_cursor.as_deref(), Some("next-folder"));
    }

    #[test]
    fn parses_article_blocks() {
        let payload = serde_json::json!({
            "__typename": "Tweet",
            "rest_id": "42",
            "legacy": {"full_text": "article", "entities": {"urls": []}},
            "core": {"user_results": {"result": {
                "__typename": "User",
                "rest_id": "7",
                "core": {"name": "Writer", "screen_name": "writer"},
                "legacy": {}
            }}},
            "article": {"article_results": {"result": {
                "title": "Long form",
                "content_state": {
                    "blocks": [
                        {"type": "header-two", "text": "Section"},
                        {"type": "unstyled", "text": "Body"}
                    ]
                }
            }}}
        });
        let tweet = parse_tweet(&payload).unwrap();
        let article = tweet.article.unwrap();
        assert_eq!(article.title, "Long form");
        assert_eq!(article.text, "## Section\n\nBody");
    }
}
