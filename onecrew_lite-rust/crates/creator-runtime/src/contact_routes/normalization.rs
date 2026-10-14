use url::Url;

use crate::{ContactChannel, ContactLocatorInput, DiscoveredContactRoute, LocatorKind};

pub(crate) fn normalize_discovered_contact_route(
    observation: &DiscoveredContactRoute,
) -> Result<ContactLocatorInput, String> {
    match observation.channel {
        ContactChannel::Email => normalize_email(&observation.value),
        ContactChannel::Wechat => {
            let value = observation.value.trim();
            if value.is_empty() || value.len() > 128 || value.chars().any(|c| c.is_whitespace() || c.is_control()) {
                return Err("invalid WeChat contact locator".into());
            }
            Ok(ContactLocatorInput {
                channel: ContactChannel::Wechat, kind: LocatorKind::Handle,
                normalized_value: value.to_ascii_lowercase(), display_value: value.into(),
            })
        }
        ContactChannel::X | ContactChannel::Instagram | ContactChannel::TikTok => {
            normalize_social_handle(observation.channel, &observation.value)
        }
        ContactChannel::Telegram => normalize_telegram(&observation.value),
        ContactChannel::YouTube | ContactChannel::Website | ContactChannel::LinkHub => {
            normalize_url(&observation.value, false).map(|mut locator| {
                locator.channel = observation.channel;
                locator.kind = LocatorKind::Url;
                locator
            })
        }
    }
}

pub(crate) fn locator_key(locator: &ContactLocatorInput) -> String {
    format!(
        "{}:{}:{}",
        locator.channel.as_str(),
        locator.kind.as_str(),
        locator.normalized_value
    )
}

fn normalize_email(value: &str) -> Result<ContactLocatorInput, String> {
    let value = value.trim().trim_start_matches("mailto:");
    let value = value.split(['?', '#']).next().unwrap_or_default().trim();
    let normalized = value.to_ascii_lowercase();
    let mut parts = normalized.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if local.is_empty() || domain.is_empty() || parts.next().is_some() || !domain.contains('.') {
        return Err(format!("invalid email contact locator: {value}"));
    }
    Ok(ContactLocatorInput {
        channel: ContactChannel::Email,
        kind: LocatorKind::EmailAddress,
        normalized_value: normalized.clone(),
        display_value: normalized,
    })
}

fn normalize_social_handle(
    channel: ContactChannel,
    value: &str,
) -> Result<ContactLocatorInput, String> {
    let display = extract_handle(value)?;
    Ok(ContactLocatorInput {
        channel,
        kind: LocatorKind::Handle,
        normalized_value: display.to_ascii_lowercase(),
        display_value: display,
    })
}

fn normalize_telegram(value: &str) -> Result<ContactLocatorInput, String> {
    let trimmed = value.trim();
    if looks_like_url(trimmed) {
        let url = normalize_url(trimmed, false)?;
        let parsed = Url::parse(&url.normalized_value).map_err(|error| error.to_string())?;
        let host = parsed
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_ascii_lowercase();
        if matches!(host.as_str(), "t.me" | "telegram.me" | "telegram.dog") {
            let segments = parsed
                .path_segments()
                .map(|segments| {
                    segments
                        .filter(|segment| !segment.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if segments.len() == 1
                && !segments[0].starts_with('+')
                && !matches!(segments[0], "joinchat" | "c" | "s" | "share" | "proxy")
            {
                return normalize_social_handle(ContactChannel::Telegram, segments[0]);
            }
            let invite = segments
                .first()
                .is_some_and(|segment| segment.starts_with('+'))
                || segments
                    .first()
                    .is_some_and(|segment| *segment == "joinchat");
            return Ok(ContactLocatorInput {
                channel: ContactChannel::Telegram,
                kind: if invite {
                    LocatorKind::InviteLink
                } else {
                    LocatorKind::Url
                },
                normalized_value: url.normalized_value,
                display_value: url.display_value,
            });
        }
    }
    normalize_social_handle(ContactChannel::Telegram, trimmed)
}

fn extract_handle(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let candidate = if looks_like_url(trimmed) {
        let locator = normalize_url(trimmed, false)?;
        let parsed = Url::parse(&locator.normalized_value).map_err(|error| error.to_string())?;
        parsed
            .path_segments()
            .and_then(|mut segments| segments.find(|segment| !segment.is_empty()))
            .unwrap_or_default()
            .to_owned()
    } else {
        trimmed
            .trim_start_matches('@')
            .trim_start_matches("t.me/")
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    let candidate = candidate.trim_start_matches('@').trim();
    if candidate.is_empty()
        || !candidate
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.'))
    {
        return Err(format!("invalid social handle contact locator: {value}"));
    }
    Ok(candidate.to_owned())
}

pub(crate) fn normalize_url(
    value: &str,
    require_link_hub: bool,
) -> Result<ContactLocatorInput, String> {
    let value = value.trim();
    let with_scheme = if looks_like_url(value) {
        value.to_owned()
    } else {
        format!("https://{value}")
    };
    let mut url =
        Url::parse(&with_scheme).map_err(|_| format!("invalid URL contact locator: {value}"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(format!("unsupported URL contact locator: {value}"));
    }
    if require_link_hub && !is_link_hub_host(url.host_str().unwrap_or_default()) {
        return Err(format!("unsupported link hub contact locator: {value}"));
    }
    url.set_fragment(None);
    let retained = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !key.starts_with("utm_") && !matches!(key.as_str(), "fbclid" | "gclid")
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect::<Vec<_>>();
    url.set_query(None);
    if !retained.is_empty() {
        url.query_pairs_mut().extend_pairs(retained);
    }
    if url.path() == "/" {
        url.set_path("");
    }
    let normalized = url.to_string();
    Ok(ContactLocatorInput {
        channel: ContactChannel::Website,
        kind: LocatorKind::Url,
        normalized_value: normalized.clone(),
        display_value: normalized,
    })
}

fn looks_like_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn is_link_hub_host(value: &str) -> bool {
    let host = value.trim_start_matches("www.").to_ascii_lowercase();
    matches!(
        host.as_str(),
        "linktr.ee"
            | "beacons.ai"
            | "bio.link"
            | "campsite.bio"
            | "hoo.be"
            | "lnk.bio"
            | "solo.to"
            | "taplink.cc"
    )
}

#[cfg(test)]
mod tests {
    use crate::{ContactChannel, DiscoveredContactRoute, LocatorKind, SourceKind};

    use super::normalize_discovered_contact_route;

    fn observation(channel: ContactChannel, value: &str) -> DiscoveredContactRoute {
        DiscoveredContactRoute {
            channel,
            kind: LocatorKind::Handle,
            value: value.into(),
            source: SourceKind::ProfileBio,
            source_url: Some("https://example.test/profile".into()),
        }
    }

    #[test]
    fn normalizes_cross_platform_locator_keys() {
        let email = normalize_discovered_contact_route(&observation(
            ContactChannel::Email,
            " Hello@Example.COM ",
        ))
        .unwrap();
        assert_eq!(email.normalized_value, "hello@example.com");

        let x = normalize_discovered_contact_route(&observation(
            ContactChannel::X,
            "https://x.com/Creator_One?ref=profile",
        ))
        .unwrap();
        assert_eq!(x.normalized_value, "creator_one");

        let telegram = normalize_discovered_contact_route(&observation(
            ContactChannel::Telegram,
            "https://t.me/Creator_One",
        ))
        .unwrap();
        assert_eq!(telegram.kind, LocatorKind::Handle);
        assert_eq!(telegram.normalized_value, "creator_one");

        let invite = normalize_discovered_contact_route(&observation(
            ContactChannel::Telegram,
            "https://t.me/+InviteToken",
        ))
        .unwrap();
        assert_eq!(invite.kind, LocatorKind::InviteLink);
    }

    #[test]
    fn removes_tracking_and_fragment_from_urls() {
        let locator = normalize_discovered_contact_route(&observation(
            ContactChannel::Website,
            "https://Example.com/path/?utm_source=x&keep=1#bio",
        ))
        .unwrap();
        assert_eq!(locator.normalized_value, "https://example.com/path/?keep=1");
    }
}
