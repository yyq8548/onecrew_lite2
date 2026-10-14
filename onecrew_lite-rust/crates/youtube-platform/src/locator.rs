use onecrew_youtube_client::accepts_channel_id;
use url::Url;

/// The native address available in a public YouTube locator. A stable channel
/// ID needs no handle lookup; browser-only locations do not identify a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YoutubeChannelLocator {
    ChannelId(String),
    Handle(String),
    BrowserOnly { canonical_url: Option<String> },
}

/// Parse a stable channel ID, channel URL, public handle or handle URL.
/// Bare handles remain accepted. Non-profile YouTube URLs retain their browser
/// URL, while foreign or credential-bearing URLs expose no canonical URL.
pub fn parse_channel_locator(value: &str) -> YoutubeChannelLocator {
    let value = value.trim();
    if accepts_channel_id(value) {
        return YoutubeChannelLocator::ChannelId(value.to_owned());
    }
    if !value.contains(['/', ':']) {
        return match normalized_handle(value) {
            Some(handle) => YoutubeChannelLocator::Handle(handle),
            None => YoutubeChannelLocator::BrowserOnly {
                canonical_url: None,
            },
        };
    }
    let Some(url) = youtube_url(value) else {
        return YoutubeChannelLocator::BrowserOnly {
            canonical_url: None,
        };
    };
    let parts: Vec<_> = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    if let ["channel", channel_id, ..] = parts.as_slice()
        && accepts_channel_id(channel_id)
    {
        return YoutubeChannelLocator::ChannelId((*channel_id).to_owned());
    }
    if let [handle] = parts.as_slice()
        && handle.starts_with('@')
        && let Some(handle) = normalized_handle(handle)
    {
        return YoutubeChannelLocator::Handle(handle);
    }
    YoutubeChannelLocator::BrowserOnly {
        canonical_url: Some(url.to_string()),
    }
}

fn normalized_handle(value: &str) -> Option<String> {
    let handle = value.trim_start_matches('@');
    (!handle.is_empty()
        && handle
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')))
    .then(|| handle.to_owned())
}

fn youtube_url(value: &str) -> Option<Url> {
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
    matches!(
        url.host_str()?.trim_start_matches("www."),
        "youtube.com" | "m.youtube.com"
    )
    .then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_channel_ids_and_channel_pages_keep_the_same_identity() {
        let id = "UC_x5XG1OV2P6uZZ5FSM9Ttw";
        for value in [
            id.to_owned(),
            format!("https://www.youtube.com/channel/{id}"),
            format!("https://m.youtube.com/channel/{id}/about"),
        ] {
            assert_eq!(
                parse_channel_locator(&value),
                YoutubeChannelLocator::ChannelId(id.into())
            );
        }
        assert_eq!(
            parse_channel_locator(&format!("@{id}")),
            YoutubeChannelLocator::Handle(id.into())
        );
    }

    #[test]
    fn handles_are_distinct_from_video_and_custom_name_urls() {
        for value in [
            "@creator",
            "creator",
            "https://youtube.com/@creator",
            "m.youtube.com/@creator",
        ] {
            assert_eq!(
                parse_channel_locator(value),
                YoutubeChannelLocator::Handle("creator".into())
            );
        }
        for value in [
            "https://youtube.com/watch?v=video",
            "https://youtube.com/custom-name",
            "https://youtube.com/channel/UC_too_short",
            "https://youtube.com/@creator/videos",
        ] {
            assert!(
                matches!(
                    parse_channel_locator(value),
                    YoutubeChannelLocator::BrowserOnly {
                        canonical_url: Some(_)
                    }
                ),
                "{value}"
            );
        }
        for value in [
            "https://example.com/@creator",
            "https://user@youtube.com/@creator",
            "ftp://youtube.com/@creator",
            "",
        ] {
            assert_eq!(
                parse_channel_locator(value),
                YoutubeChannelLocator::BrowserOnly {
                    canonical_url: None
                }
            );
        }
    }
}
