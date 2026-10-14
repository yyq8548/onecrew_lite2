use url::Url;

/// Extract a public X profile handle from a handle or profile URL.
/// Content, navigation, foreign-host and credential-bearing URLs return `None`.
pub fn parse_profile_handle(value: &str) -> Option<String> {
    let value = value.trim();
    if !value.contains(['/', ':']) {
        return normalized_handle(value);
    }
    let candidate = if value.contains("://") {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let url = Url::parse(&candidate).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let host = url.host_str()?.trim_start_matches("www.");
    if !matches!(host, "x.com" | "twitter.com") {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|part| !part.is_empty());
    let handle = parts.next()?;
    if parts.next().is_some()
        || [
            "about",
            "compose",
            "explore",
            "hashtag",
            "home",
            "i",
            "intent",
            "login",
            "messages",
            "notifications",
            "search",
            "settings",
            "share",
        ]
        .iter()
        .any(|reserved| handle.eq_ignore_ascii_case(reserved))
    {
        return None;
    }
    normalized_handle(handle)
}

fn normalized_handle(value: &str) -> Option<String> {
    let handle = value.trim_start_matches('@');
    (!handle.is_empty()
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')))
    .then(|| handle.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_profile_forms_resolve_the_same_handle() {
        for value in [
            "creator",
            "@creator",
            " https://x.com/creator/ ",
            "twitter.com/creator",
            "https://www.x.com/creator?ref=profile",
        ] {
            assert_eq!(parse_profile_handle(value).as_deref(), Some("creator"));
        }
    }

    #[test]
    fn navigation_posts_and_other_hosts_are_not_profiles() {
        for value in [
            "",
            "https://example.com/creator",
            "https://x.com/intent",
            "https://x.com/creator/status/123",
            "https://user@x.com/creator",
            "ftp://x.com/creator",
        ] {
            assert!(parse_profile_handle(value).is_none(), "{value}");
        }
    }
}
