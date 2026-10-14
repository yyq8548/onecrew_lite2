//! An engine URL is only a locator. Independent native receipts establish authorship.
use serde_json::Value;

pub(super) fn target(source: &str) -> Option<(String, String)> {
    let url = url::Url::parse(source).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("x.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let parts: Vec<_> = url.path_segments()?.collect();
    let [handle, "status", post] = parts.as_slice() else {
        return None;
    };
    if !valid_handle(handle) || !numeric(post) {
        return None;
    }
    Some((handle.to_ascii_lowercase(), (*post).into()))
}
fn numeric(id: &str) -> bool {
    (1..=20).contains(&id.len()) && !id.starts_with('0') && id.bytes().all(|b| b.is_ascii_digit())
}
fn valid_handle(handle: &str) -> bool {
    (1..=15).contains(&handle.len())
        && handle
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
fn keys(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|row| {
        row.len() == expected.len() && expected.iter().all(|key| row.contains_key(*key))
    })
}
pub(super) fn valid(material: &Value, source: &str, post: &str) -> bool {
    let Some((handle, expected_post)) = target(source) else {
        return false;
    };
    let identity = &material["identity"];
    let profile = &material["profile"];
    let content = &material["content"];
    let profile_url = format!("https://x.com/{handle}");
    let native_source_url = format!("{profile_url}/status/{expected_post}");
    let account = identity["account_id"].as_str().unwrap_or_default();
    let bounded = |value: &Value, max, multiline| {
        value.as_str().is_some_and(|s| {
            s.len() <= max
                && !s
                    .chars()
                    .any(|ch| ch.is_control() && !(multiline && ch == '\n'))
        })
    };
    keys(material, &["identity", "profile", "content"])
        && material.to_string().len() <= 65536
        && expected_post == post
        && numeric(account)
        && keys(
            identity,
            &[
                "adapter_version",
                "post_id",
                "account_id",
                "handle",
                "profile_url",
                "source_url",
                "identity_kind",
                "evidence_source",
            ],
        )
        && identity["adapter_version"] == "1.0.0"
        && identity["post_id"] == post
        && identity["handle"] == handle
        && identity["profile_url"] == profile_url
        && identity["source_url"] == native_source_url
        && identity["identity_kind"] == "native_post_author_id"
        && identity["evidence_source"] == "x_tweet_detail"
        && keys(
            profile,
            &[
                "adapter_version",
                "account_id",
                "handle",
                "profile_url",
                "name",
                "bio",
                "self_declared_location",
                "follower_count",
                "identity_kind",
                "evidence_source",
            ],
        )
        && profile["adapter_version"] == "1.0.0"
        && profile["account_id"] == account
        && profile["handle"] == handle
        && profile["profile_url"] == profile_url
        && profile["identity_kind"] == "native_id"
        && profile["evidence_source"] == "x_user_by_screen_name"
        && bounded(&profile["name"], 800, false)
        && profile["name"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
        && bounded(&profile["bio"], 8000, true)
        && bounded(&profile["self_declared_location"], 800, false)
        && (profile["follower_count"].is_null()
            || profile["follower_count"]
                .as_u64()
                .is_some_and(|n| n <= 9_007_199_254_740_991))
        && keys(
            content,
            &[
                "adapter_version",
                "identity",
                "text",
                "text_truncated",
                "text_source",
                "created_at",
                "has_media",
                "quotes_another_post",
                "expanded_urls",
            ],
        )
        && content["adapter_version"] == "1.0.0"
        && content["identity"] == *identity
        && content["text"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 8000 && !s.contains('\0'))
        && content["text_truncated"].is_boolean()
        && content["has_media"].is_boolean()
        && content["quotes_another_post"].is_boolean()
        && matches!(
            content["text_source"].as_str(),
            Some("note_tweet" | "legacy_full_text")
        )
        && bounded(&content["created_at"], 80, false)
        && content["created_at"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty())
        && content["expanded_urls"].as_array().is_some_and(|urls| {
            let mut seen = std::collections::HashSet::new();
            urls.len() <= 20
                && urls.iter().all(|url| {
                    url.as_str().is_some_and(|s| {
                        crate::discovery_pinterest::valid_link(s) && seen.insert(s)
                    })
                })
        })
}
