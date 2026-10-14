//! Creator-side native evidence policy; provider transport stays in OpenCLI.
use serde_json::Value;

pub(crate) fn valid_did(did: &str) -> bool {
    did.len() <= 256
        && (did.strip_prefix("did:plc:").is_some_and(|id| {
            id.len() == 24
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
        }) || did.strip_prefix("did:web:").is_some_and(|id| {
            !id.is_empty()
                && id.split(':').all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                })
        }))
}

pub(crate) fn valid_handle(handle: &str) -> bool {
    handle.len() <= 253
        && handle.contains('.')
        && handle.split('.').all(|part| {
            (1..=63).contains(&part.len())
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

pub(crate) fn uri_from_source(source: &str) -> Option<String> {
    let (did, key) = source
        .strip_prefix("https://bsky.app/profile/")?
        .split_once("/post/")?;
    (valid_did(did)
        && (1..=64).contains(&key.len())
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then(|| format!("at://{did}/app.bsky.feed.post/{key}"))
}

fn exact_keys(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.len() == keys.len() && keys.iter().all(|key| v.contains_key(*key)))
}

pub(crate) fn valid_profile(value: &Value) -> bool {
    let did = value["did"].as_str().unwrap_or_default();
    exact_keys(
        value,
        &[
            "did",
            "handle",
            "display_name",
            "biography",
            "follower_count",
            "profile_url",
        ],
    ) && valid_did(did)
        && value["handle"].as_str().is_some_and(valid_handle)
        && value["profile_url"] == format!("https://bsky.app/profile/{did}")
        && value["display_name"].as_str().is_some_and(|s| {
            !s.trim().is_empty() && s.len() <= 800 && !s.chars().any(char::is_control)
        })
        && value["biography"]
            .as_str()
            .is_some_and(|s| s.len() <= 8000 && !s.contains('\0'))
        && (value["follower_count"].is_null()
            || value["follower_count"]
                .as_u64()
                .is_some_and(|n| n <= 9_007_199_254_740_991))
}

pub(crate) fn valid_content(value: &Value, did: &str, source: &str) -> bool {
    exact_keys(
        value,
        &[
            "author_id",
            "author_handle",
            "post_uri",
            "source_url",
            "text",
            "public_links",
            "comments",
        ],
    ) && valid_did(did)
        && value["author_id"] == did
        && value["source_url"] == source
        && source
            .strip_prefix(&format!("https://bsky.app/profile/{did}/post/"))
            .is_some()
        && uri_from_source(source).is_some_and(|uri| value["post_uri"] == uri)
        && value["author_handle"].as_str().is_some_and(valid_handle)
        && value["text"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 16000 && !s.contains('\0'))
        && value["comments"].as_array().is_some_and(Vec::is_empty)
        && value["public_links"].as_array().is_some_and(|links| {
            let mut seen = std::collections::HashSet::new();
            links.len() <= 10
                && links.iter().all(|link| {
                    link.as_str()
                        .is_some_and(|s| valid_contact_link(s) && seen.insert(s))
                })
        })
}

fn valid_contact_link(link: &str) -> bool {
    if link.len() > 2048 || link.chars().any(char::is_control) || link.contains([' ', '\0']) {
        return false;
    }
    let Ok(url) = url::Url::parse(link) else {
        return false;
    };
    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query_pairs().any(|(key, _)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "token"
                    | "access_token"
                    | "api_key"
                    | "authorization"
                    | "cookie"
                    | "session"
                    | "password"
                    | "secret"
                    | "auth"
            )
        })
    {
        return false;
    }
    match url.scheme() {
        "mailto" => {
            url.query().is_none()
                && url.fragment().is_none()
                && url.path().contains('@')
                && !url.path().contains(['/', '%'])
        }
        "https" => {
            matches!(url.host(),Some(url::Host::Domain(host)) if host.contains('.') && !host.ends_with(".local") && !host.ends_with(".internal") && !host.ends_with(".localhost"))
        }
        _ => false,
    }
}
