//! Native Pixiv ownership policy. Search labels and account bios do not prove
//! authorship or target language; only the separately bound artwork can do so.
use serde_json::Value;
pub(crate) fn valid_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
pub(crate) fn artwork_id(source: &str) -> Option<&str> {
    source
        .strip_prefix("https://www.pixiv.net/artworks/")
        .filter(|id| valid_id(id))
}
pub(crate) fn valid_profile(value: &Value) -> bool {
    let keys = [
        "adapter_version",
        "author_id",
        "author_label",
        "profile_url",
        "follower_count",
        "bio",
        "public_links",
        "evidence_source",
    ];
    let id = value["author_id"].as_str().unwrap_or_default();
    value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
        && valid_id(id)
        && value["adapter_version"] == "1.0.0"
        && value["evidence_source"] == "native_ajax_response"
        && value["profile_url"] == format!("https://www.pixiv.net/users/{id}")
        && value["follower_count"].is_null()
        && value["author_label"].as_str().is_some_and(|name| {
            !name.trim().is_empty() && name.len() <= 512 && !name.chars().any(char::is_control)
        })
        && value["bio"]
            .as_str()
            .is_some_and(|text| text.len() <= 8000 && !text.contains('\0'))
        && value["public_links"].as_array().is_some_and(|links| {
            links.len() <= 10
                && links.iter().all(|link| {
                    link.as_str()
                        .is_some_and(crate::discovery_pinterest::valid_link)
                })
        })
}
pub(crate) fn valid_content(value: &Value, author: &str, source: &str) -> bool {
    let keys = [
        "post_id",
        "author_id",
        "source_url",
        "text",
        "text_truncated",
    ];
    let Some(id) = artwork_id(source) else {
        return false;
    };
    value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
        && valid_id(author)
        && value["author_id"] == author
        && value["post_id"] == id
        && value["source_url"] == source
        && value["text"].as_str().is_some_and(|text| {
            !text.trim().is_empty() && text.len() <= 16000 && !text.contains('\0')
        })
        && value["text_truncated"].is_boolean()
}
#[cfg(test)]
mod tests;
