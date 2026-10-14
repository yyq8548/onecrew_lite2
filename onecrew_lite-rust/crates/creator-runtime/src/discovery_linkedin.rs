//! LinkedIn profile handles locate a page; native account IDs bind authorship.
//! A profile biography never substitutes for the author's Activity body.
use serde_json::Value;

#[cfg(test)]
mod tests;

pub(crate) fn valid_handle(handle: &str) -> bool {
    (1..=100).contains(&handle.len())
        && handle.as_bytes()[0].is_ascii_alphanumeric()
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        && !matches!(handle.to_ascii_lowercase().as_str(), "me" | "self")
}
pub(crate) fn valid_account(account: &str) -> bool {
    account.strip_prefix("ACo").is_some_and(|body| {
        (16..=125).contains(&body.len())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}
pub(crate) fn activity_id(source: &str) -> Option<&str> {
    source
        .strip_prefix("https://www.linkedin.com/feed/update/urn:li:activity:")?
        .strip_suffix('/')
        .filter(|id| {
            (1..=20).contains(&id.len())
                && !id.starts_with('0')
                && id.bytes().all(|b| b.is_ascii_digit())
        })
}
fn keys(value: &Value, fields: &[&str]) -> bool {
    value.as_object().is_some_and(|row| {
        row.len() == fields.len() && fields.iter().all(|field| row.contains_key(*field))
    })
}
fn text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|text| text.len() <= max && !text.contains('\0'))
}
fn exact_followers(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let number = text
        .strip_suffix(" followers")
        .or_else(|| text.strip_suffix(" follower"))?;
    let groups: Vec<_> = number.split(',').collect();
    if groups
        .iter()
        .any(|group| group.is_empty() || !group.bytes().all(|b| b.is_ascii_digit()))
        || (groups.len() > 1
            && (groups[0].len() > 3 || groups.iter().skip(1).any(|group| group.len() != 3)))
        || (number.starts_with('0') && number != "0")
    {
        return None;
    }
    number
        .replace(',', "")
        .parse::<u64>()
        .ok()
        .filter(|n| *n <= 9_007_199_254_740_991)
}
pub(crate) fn valid_profile(value: &Value) -> bool {
    let handle = value["author_handle"].as_str().unwrap_or_default();
    let profile = format!("https://www.linkedin.com/in/{handle}/");
    keys(
        value,
        &[
            "adapter_version",
            "author_handle",
            "account_id",
            "author_label",
            "profile_url",
            "follower_count",
            "follower_text",
            "headline",
            "bio",
            "public_links",
            "contact_links",
            "evidence_source",
            "bio_truncated",
        ],
    ) && valid_handle(handle)
        && value["account_id"].as_str().is_some_and(valid_account)
        && value["adapter_version"] == "1.0.0"
        && value["profile_url"] == profile
        && value["author_label"].as_str().is_some_and(|name| {
            !name.trim().is_empty() && name.len() <= 512 && !name.chars().any(char::is_control)
        })
        && text(&value["headline"], 2000)
        && text(&value["bio"], 8000)
        && text(&value["follower_text"], 100)
        && (value["follower_count"].is_null() || value["follower_count"].as_u64().is_some())
        && value["follower_count"].as_u64()
            == value["follower_text"].as_str().and_then(exact_followers)
        && value["public_links"].as_array().is_some_and(|links| {
            links.len() <= 10
                && links.iter().all(|link| {
                    link.as_str()
                        .is_some_and(crate::discovery_pinterest::valid_link)
                })
        })
        && value["contact_links"].as_array().is_some_and(|links| {
            links.len() <= 1
                && links.iter().all(|link| {
                    link.as_str() == Some(format!("{profile}overlay/contact-info/").as_str())
                })
        })
        && value["evidence_source"] == "visible_profile_header"
        && value["bio_truncated"] == true
}
pub(crate) fn valid_content(value: &Value, handle: &str, requested_profile: &str) -> bool {
    let post = value["post_id"].as_str().unwrap_or_default();
    keys(
        value,
        &[
            "account_id",
            "author_handle",
            "profile_url",
            "post_id",
            "source_url",
            "text",
            "text_truncated",
            "evidence_source",
        ],
    ) && valid_handle(handle)
        && value["author_handle"].as_str() == Some(handle)
        && value["account_id"].as_str().is_some_and(valid_account)
        && requested_profile == format!("https://www.linkedin.com/in/{handle}/")
        && value["profile_url"].as_str() == Some(requested_profile)
        && (1..=20).contains(&post.len())
        && !post.starts_with('0')
        && post.bytes().all(|b| b.is_ascii_digit())
        && value["source_url"]
            == format!("https://www.linkedin.com/feed/update/urn:li:activity:{post}/")
        && text(&value["text"], 16000)
        && value["text"]
            .as_str()
            .is_some_and(|text| !text.trim().is_empty())
        && value["text_truncated"] == true
        && value["evidence_source"] == "visible_author_activity"
}
