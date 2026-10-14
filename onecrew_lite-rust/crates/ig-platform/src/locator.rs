use url::Url;

/// Extract a public Instagram account handle from a handle or profile URL.
/// Media, navigation, foreign-host and credential-bearing URLs return `None`.
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
    if url.host_str()?.trim_start_matches("www.") != "instagram.com" {
        return None;
    }
    let mut parts = url.path_segments()?.filter(|part| !part.is_empty());
    let handle = parts.next()?;
    if parts.next().is_some()
        || [
            "about",
            "accounts",
            "developer",
            "direct",
            "explore",
            "legal",
            "p",
            "reel",
            "reels",
            "stories",
            "web",
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
    fn accepts_accounts_without_treating_media_or_navigation_as_people() {
        for value in [
            "@creator.name",
            "creator.name",
            "https://www.instagram.com/creator.name/",
        ] {
            assert_eq!(parse_profile_handle(value).as_deref(), Some("creator.name"));
        }
        for value in [
            "",
            "https://x.com/creator",
            "https://instagram.com/p/post-id",
            "https://instagram.com/reel/123",
            "https://instagram.com/direct",
            "https://user@instagram.com/creator",
        ] {
            assert!(parse_profile_handle(value).is_none(), "{value}");
        }
    }
}
