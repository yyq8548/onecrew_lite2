//! Creator policy for observed Hacker News usernames and native receipts.
use serde_json::Value;

pub(crate) fn valid_username(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

pub(crate) fn valid_profile(value: &Value) -> bool {
    let keys = [
        "author_id",
        "username",
        "biography_html",
        "biography",
        "profile_url",
        "created",
        "karma",
        "follower_count",
    ];
    let author = value["author_id"].as_str().unwrap_or_default();
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    }) && valid_username(author)
        && value["username"] == author
        && value["profile_url"] == format!("https://news.ycombinator.com/user?id={author}")
        && value["follower_count"].is_null()
        && ["created", "karma"].iter().all(|key| {
            value[*key]
                .as_u64()
                .is_some_and(|count| count <= 9_007_199_254_740_991)
        })
        && value["biography_html"]
            .as_str()
            .is_some_and(|bio| bio.len() <= 16000 && !bio.contains('\0'))
        && value["biography"]
            .as_str()
            .is_some_and(|bio| bio.len() <= 8000 && !bio.contains('\0'))
}

pub(crate) fn item_id(source: &str) -> Option<u64> {
    let id = source.strip_prefix("https://news.ycombinator.com/item?id=")?;
    if id.starts_with('0') || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    id.parse::<u64>()
        .ok()
        .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
}

pub(crate) fn valid_content(value: &Value, author: &str, source: &str) -> bool {
    let keys = [
        "item_id",
        "author_id",
        "source_url",
        "title",
        "text_html",
        "text",
        "external_url",
        "comment_ids",
        "comments",
    ];
    let Some(id) = item_id(source) else {
        return false;
    };
    value.as_object().is_some_and(|object| {
        object.len() == keys.len() && keys.iter().all(|key| object.contains_key(*key))
    }) && valid_username(author)
        && value["author_id"] == author
        && value["item_id"] == id
        && value["source_url"] == source
        && value["title"].as_str().is_some_and(|text| {
            !text.trim().is_empty() && text.len() <= 2000 && !text.contains('\0')
        })
        && value["text"].as_str().is_some_and(|text| {
            !text.trim().is_empty() && text.len() <= 16000 && !text.contains('\0')
        })
        && value["text_html"]
            .as_str()
            .is_some_and(|text| text.len() <= 16000 && !text.contains('\0'))
        && (value["external_url"].is_null()
            || value["external_url"]
                .as_str()
                .is_some_and(|url| url.len() <= 2048 && !url.chars().any(char::is_control)))
        && value["comments"].as_array().is_some_and(Vec::is_empty)
        && value["comment_ids"].as_array().is_some_and(|ids| {
            let mut seen = std::collections::HashSet::new();
            ids.len() <= 5
                && ids.iter().all(|value| {
                    value.as_u64().is_some_and(|child| {
                        child > 0
                            && child <= 9_007_199_254_740_991
                            && child != id
                            && seen.insert(child)
                    })
                })
        })
}
