//! Native account policy. A profile never proves ownership of a saved Pin.
use serde_json::Value;
#[cfg(test)]
mod tests;
pub(crate) fn valid_id(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
pub(crate) fn valid_username(name: &str) -> bool {
    (1..=30).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

pub(crate) fn pin_id(source: &str) -> Option<&str> {
    source
        .strip_prefix("https://www.pinterest.com/pin/")
        .and_then(|path| path.strip_suffix('/'))
        .filter(|id| valid_id(id))
}
pub(crate) fn valid_profile(value: &Value) -> bool {
    let keys = [
        "author_id",
        "username",
        "author_label",
        "profile_url",
        "follower_count",
        "bio",
        "public_links",
    ];
    let id = value["author_id"].as_str().unwrap_or_default();
    let username = value["username"].as_str().unwrap_or_default();
    value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
        && valid_id(id)
        && valid_username(username)
        && value["profile_url"] == format!("https://www.pinterest.com/{username}/")
        && value["author_label"].as_str().is_some_and(|name| {
            !name.trim().is_empty() && name.len() <= 512 && !name.chars().any(char::is_control)
        })
        && value["bio"]
            .as_str()
            .is_some_and(|text| text.len() <= 8000 && !text.contains('\0'))
        && (value["follower_count"].is_null()
            || value["follower_count"]
                .as_u64()
                .is_some_and(|count| count <= 9_007_199_254_740_991))
        && value["public_links"].as_array().is_some_and(|links| {
            links.len() <= 1
                && links
                    .iter()
                    .all(|value| value.as_str().is_some_and(valid_link))
        })
}
pub(crate) fn valid_link(source: &str) -> bool {
    if source.len() > 2048 || source.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    url::Url::parse(source).is_ok_and(|url| {
        let Some(url::Host::Domain(host)) = url.host() else {
            return false;
        };
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && host.contains('.')
            && !host.ends_with('.')
            && ![
                "localhost",
                "local",
                "internal",
                "test",
                "invalid",
                "onion",
                "home.arpa",
            ]
            .iter()
            .any(|suffix| host == *suffix || host.ends_with(&format!(".{suffix}")))
    })
}

/// Validate the observed caption separately from the author-feed request URL.
/// A native pinner binding certifies this public caption, not the image's origin.
pub(crate) fn valid_content(value: &Value, author: &str, requested_profile: &str) -> bool {
    let keys = [
        "pin_id",
        "author_id",
        "author_username",
        "profile_url",
        "source_url",
        "text",
        "text_truncated",
    ];
    let pin = value["pin_id"].as_str().unwrap_or_default();
    let username = value["author_username"].as_str().unwrap_or_default();
    value
        .as_object()
        .is_some_and(|row| row.len() == keys.len() && keys.iter().all(|key| row.contains_key(*key)))
        && valid_id(pin)
        && valid_id(author)
        && valid_username(username)
        && value["author_id"].as_str() == Some(author)
        && value["profile_url"] == format!("https://www.pinterest.com/{username}/")
        && (value["profile_url"].as_str() == Some(requested_profile)
            || pin_id(requested_profile) == Some(pin))
        && value["source_url"] == format!("https://www.pinterest.com/pin/{pin}/")
        && value["text"].as_str().is_some_and(|text| {
            !text.trim().is_empty() && text.len() <= 16000 && !text.contains('\0')
        })
        && value["text_truncated"] == true
}
