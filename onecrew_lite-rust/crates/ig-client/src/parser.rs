//! Normalize private-API JSON into the lean typed models the app exposes.

use serde_json::Value;

use crate::models::{
    IgComment, IgCommentPage, IgMedia, IgMediaPage, IgMessage, IgThread, IgUser, IgUserPage,
};

pub(crate) fn user(value: &Value) -> IgUser {
    IgUser {
        id: id_of(value, "pk"),
        username: str_of(value, "username"),
        full_name: str_of(value, "full_name"),
        is_private: value.get("is_private").and_then(Value::as_bool),
        is_verified: value.get("is_verified").and_then(Value::as_bool),
        profile_pic_url: opt_str(value, "profile_pic_url"),
        follower_count: value.get("follower_count").and_then(Value::as_i64),
        following_count: value.get("following_count").and_then(Value::as_i64),
        media_count: value.get("media_count").and_then(Value::as_i64),
        biography: opt_str(value, "biography"),
        external_url: opt_str(value, "external_url"),
        bio_links: bio_links(value),
        public_email: opt_str(value, "public_email"),
        city_name: opt_str(value, "city_name"),
        address_street: opt_str(value, "address_street"),
    }
}

fn bio_links(value: &Value) -> Vec<String> {
    let mut links = Vec::new();
    for item in arr(value, "bio_links") {
        let link = opt_str(item, "url").or_else(|| opt_str(item, "lynx_url"));
        if let Some(link) = link.filter(|link| !links.iter().any(|value| value == link)) {
            links.push(link);
        }
    }
    links
}

pub(crate) fn message(value: &Value) -> IgMessage {
    let item_type = str_of(value, "item_type");
    let text = opt_str(value, "text").unwrap_or_else(|| format!("[{item_type}]"));
    IgMessage {
        item_id: str_of(value, "item_id"),
        user_id: id_of(value, "user_id"),
        item_type,
        text,
        timestamp: num_or_str_i64(value, "timestamp"),
    }
}

pub(crate) fn thread(value: &Value) -> IgThread {
    IgThread {
        thread_id: str_of(value, "thread_id"),
        title: str_of(value, "thread_title"),
        users: arr(value, "users").iter().map(user).collect(),
        messages: arr(value, "items").iter().map(message).collect(),
    }
}

pub(crate) fn users(value: &Value) -> Vec<IgUser> {
    arr(value, "users").iter().map(user).collect()
}

pub(crate) fn media(value: &Value) -> IgMedia {
    IgMedia {
        id: id_of(value, "pk"),
        code: str_of(value, "code"),
        media_type: value.get("media_type").and_then(Value::as_i64).unwrap_or(0),
        caption: media_caption(value),
        like_count: value.get("like_count").and_then(Value::as_i64).unwrap_or(0),
        comment_count: value
            .get("comment_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        view_count: value
            .get("view_count")
            .and_then(Value::as_i64)
            .or_else(|| value.get("play_count").and_then(Value::as_i64)),
        taken_at: value.get("taken_at").and_then(Value::as_i64),
        thumbnail_url: media_thumbnail(value),
        video_url: value
            .pointer("/video_versions/0/url")
            .and_then(Value::as_str)
            .map(str::to_owned),
    }
}

fn media_caption(value: &Value) -> String {
    value
        .get("caption")
        .and_then(|caption| {
            caption
                .as_str()
                .or_else(|| caption.get("text").and_then(Value::as_str))
        })
        .or_else(|| value.get("caption_text").and_then(Value::as_str))
        .unwrap_or_default()
        .to_owned()
}

fn media_thumbnail(value: &Value) -> Option<String> {
    value
        .pointer("/image_versions2/candidates/0/url")
        .or_else(|| value.pointer("/carousel_media/0/image_versions2/candidates/0/url"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_owned)
}

pub(crate) fn comment(value: &Value) -> IgComment {
    IgComment {
        id: id_of(value, "pk"),
        text: str_of(value, "text"),
        username: value
            .get("user")
            .map(|u| str_of(u, "username"))
            .unwrap_or_default(),
        like_count: value
            .get("comment_like_count")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        created_at: value.get("created_at").and_then(Value::as_i64),
    }
}

pub(crate) fn media_page(value: &Value) -> IgMediaPage {
    IgMediaPage {
        items: arr(value, "items").iter().map(media).collect(),
        next_max_id: opt_str(value, "next_max_id"),
    }
}

pub(crate) fn user_page(value: &Value) -> IgUserPage {
    IgUserPage {
        users: arr(value, "users").iter().map(user).collect(),
        next_max_id: opt_str(value, "next_max_id"),
    }
}

pub(crate) fn comment_page(value: &Value) -> IgCommentPage {
    IgCommentPage {
        comments: arr(value, "comments").iter().map(comment).collect(),
        next_max_id: opt_str(value, "next_max_id"),
    }
}

fn str_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn opt_str(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Instagram ids (`pk`, `user_id`) come back as either a number or a string.
fn id_of(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn num_or_str_i64(value: &Value, key: &str) -> Option<i64> {
    match value.get(key) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

fn arr<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

#[cfg(test)]
#[test]
fn export_public_platform_read_node_oracle() {
    let directory = std::path::PathBuf::from(std::env::var("ONECREW_PUBLIC_READ_ORACLE_DIRECTORY").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let cases = [
        serde_json::json!({"pk":9223372036854775807_i64,"username":"creator","full_name":"Creator","is_private":false,"is_verified":true,"follower_count":i64::MAX,"following_count":i64::MIN,"media_count":-1,"biography":"Business: public@example.com","external_url":"https://example.com","bio_links":[{"url":"https://example.com"},{"url":"https://example.com"},{"lynx_url":"https://example.net"}],"profile_pic_url":"https://example.com/a","public_email":"public@example.com","city_name":"City","address_street":"Street"}),
        serde_json::json!({"pk":"9007199254740993","username":"minimal","full_name":"","follower_count":u64::MAX,"following_count":"42","biography":"","external_url":null,"is_private":null}),
        serde_json::json!({}),
    ].into_iter().map(|input| serde_json::json!({"input":input,"expected":user(&input)})).collect::<Vec<_>>();
    std::fs::write(directory.join("instagram.json"), serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profile_contact_fields() {
        let parsed = user(&serde_json::json!({
            "pk": "42",
            "username": "creator",
            "full_name": "Creator",
            "biography": "Business: creator@example.com",
            "external_url": "https://creator.example",
            "bio_links": [
                {"url": "https://t.me/creator"},
                {"lynx_url": "https://linktr.ee/creator"},
                {"url": "https://t.me/creator"}
            ],
            "public_email": "team@example.com",
            "city_name": "Istanbul",
            "address_street": "Example street"
        }));

        assert_eq!(
            parsed.external_url.as_deref(),
            Some("https://creator.example")
        );
        assert_eq!(parsed.bio_links.len(), 2);
        assert_eq!(parsed.public_email.as_deref(), Some("team@example.com"));
        assert_eq!(parsed.city_name.as_deref(), Some("Istanbul"));
    }

    #[test]
    fn parses_media_caption_variants_and_preview() {
        for (payload, expected) in [
            (
                serde_json::json!({"pk": "1", "caption": {"text": "object caption"}}),
                "object caption",
            ),
            (
                serde_json::json!({"pk": "2", "caption": "string caption"}),
                "string caption",
            ),
            (
                serde_json::json!({"pk": "3", "caption_text": "flat caption"}),
                "flat caption",
            ),
        ] {
            assert_eq!(media(&payload).caption, expected);
        }

        let parsed = media(&serde_json::json!({
            "pk": "4",
            "media_type": 8,
            "carousel_media": [{
                "image_versions2": {"candidates": [{"url": "https://cdn.example/preview.jpg"}]}
            }]
        }));
        assert_eq!(
            parsed.thumbnail_url.as_deref(),
            Some("https://cdn.example/preview.jpg")
        );
    }
}
