use std::sync::LazyLock;

use crate::{ContactChannel, LocatorKind, Platform, PlatformContactObservation, SourceKind};
use regex::Regex;
use url::Url;

static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b[A-Z0-9._%+\-]+@[A-Z0-9.\-]+\.[A-Z]{2,}\b").expect("email regex")
});
static URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:https?://|www\.)[^\s<>\[\]{}"']+|(?:x\.com|twitter\.com|instagram\.com|t\.me|telegram\.me|youtube\.com)/@?[A-Z0-9_+\-.]+"#,
    )
    .expect("url regex")
});
/// A Telegram handle referenced in plain text: "contact me on telegram
/// @harry_dgf", "TG: kripto_kanal". A bare @mention is the host platform's own
/// mention syntax, so the handle only counts with an explicit Telegram marker
/// and either an @ prefix or a separator colon — "Telegram channel below" must
/// not read "channel" as a handle. Telegram usernames are 5-32 characters of
/// [A-Za-z0-9_] starting with a letter.
static TELEGRAM_MENTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:telegram|tg|电报)\b(?:[\s,，\-–—]*@|\s*[:：=]\s*@?)([A-Za-z][A-Za-z0-9_]{4,31})",
    )
    .expect("telegram mention regex")
});

#[derive(PartialEq, Eq)]
pub struct CreatorContactProfileField<'a> {
    pub source: SourceKind,
    pub source_url: Option<&'a str>,
    pub text: &'a str,
}

#[derive(PartialEq, Eq)]
pub struct CreatorContactProfileLink<'a> {
    pub source: SourceKind,
    pub source_url: Option<&'a str>,
    pub display_text: &'a str,
    pub target_url: &'a str,
}

/// Extract each public observation independently. Locator grouping and
/// cross-source deduplication belong to `creator-runtime`. Each fact carries
/// its actual public source; unknown sources remain unknown.
pub fn discover_creator_contacts(
    owner_platform: Platform,
    fields: &[CreatorContactProfileField<'_>],
    links: &[CreatorContactProfileLink<'_>],
) -> Vec<PlatformContactObservation> {
    let mut observations = Vec::new();

    for field in fields {
        for matched in EMAIL.find_iter(field.text) {
            let value = matched.as_str();
            // Retina/image filenames and email-shaped path fragments are
            // assets rather than independently published contact addresses.
            if is_image_name(value)
                || URL
                    .find_iter(field.text)
                    .any(|url| url.start() <= matched.start() && matched.end() <= url.end())
            {
                continue;
            }
            push_observation(
                &mut observations,
                ContactChannel::Email,
                LocatorKind::EmailAddress,
                value.to_owned(),
                field.source,
                field.source_url.map(str::to_owned),
            );
        }

        // Upload prose includes sponsors, affiliates and community mentions.
        // Only independently published email addresses are contact evidence
        // there; a channel's own bio and curated links retain their rules.
        if email_only_source(owner_platform, field.source) {
            continue;
        }

        if !is_owner_channel(owner_platform, ContactChannel::Telegram) {
            for captures in TELEGRAM_MENTION.captures_iter(field.text) {
                let handle = captures.get(1).expect("telegram handle capture");
                // A handle glued to more token characters is part of something
                // longer — an email, a domain, a path, or an over-long name —
                // that the other extractors own or that no Telegram username
                // can be.
                let following = field.text[handle.end()..].chars().next();
                if matches!(following, Some(character) if character == '.'
                    || character == '@'
                    || character == '/'
                    || character == '_'
                    || character.is_ascii_alphanumeric())
                {
                    continue;
                }
                push_observation(
                    &mut observations,
                    ContactChannel::Telegram,
                    LocatorKind::Handle,
                    handle.as_str().to_owned(),
                    field.source,
                    field.source_url.map(str::to_owned),
                );
            }
        }

        for matched in URL.find_iter(field.text) {
            let raw = trim_url_punctuation(matched.as_str());
            if raw.is_empty() {
                continue;
            }
            let normalized = if raw.starts_with("http://") || raw.starts_with("https://") {
                raw.to_owned()
            } else {
                format!("https://{raw}")
            };
            if is_short_url(&normalized) {
                continue;
            }
            let Some((channel, kind, value)) = classify_contact_route(&normalized) else {
                continue;
            };
            if is_owner_channel(owner_platform, channel) {
                continue;
            }
            push_observation(
                &mut observations,
                channel,
                kind,
                value,
                field.source,
                field.source_url.map(str::to_owned),
            );
        }
    }

    for link in links {
        if email_only_source(owner_platform, link.source) {
            continue;
        }
        if let Ok(target) = Url::parse(link.target_url.trim())
            && target.scheme() == "mailto"
        {
            // A curated mailto contributes only one complete recipient. Its
            // subject/body and any other URL fields are not contact evidence.
            let recipient = target.path();
            if EMAIL
                .find(recipient)
                .is_some_and(|matched| matched.as_str() == recipient)
                && !is_image_name(recipient)
            {
                push_observation(
                    &mut observations,
                    ContactChannel::Email,
                    LocatorKind::EmailAddress,
                    recipient.to_owned(),
                    link.source,
                    link.source_url.map(str::to_owned),
                );
            }
            continue;
        }
        let visible = normalize_visible_url(link.display_text);
        let target = normalize_visible_url(link.target_url);
        let candidate = visible
            .as_deref()
            .filter(|value| !is_short_url(value))
            .or_else(|| target.as_deref().filter(|value| !is_short_url(value)));
        let Some(candidate) = candidate else { continue };
        let Some((channel, kind, value)) = classify_contact_route(candidate) else {
            continue;
        };
        if is_owner_channel(owner_platform, channel) {
            continue;
        }
        push_observation(
            &mut observations,
            channel,
            kind,
            value,
            link.source,
            link.source_url.map(str::to_owned),
        );
    }

    observations
}

pub(crate) fn email_only_source(platform: Platform, source: SourceKind) -> bool {
    platform == Platform::YouTube && source == SourceKind::VideoDescription
}

fn is_owner_channel(platform: Platform, channel: ContactChannel) -> bool {
    matches!(
        (platform, channel),
        (Platform::X, ContactChannel::X)
            | (Platform::Instagram, ContactChannel::Instagram)
            | (Platform::Telegram, ContactChannel::Telegram)
            | (Platform::YouTube, ContactChannel::YouTube)
    )
}

fn is_short_url(value: &str) -> bool {
    Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|host| matches!(host.as_str(), "t.co" | "www.t.co"))
}

fn normalize_visible_url(value: &str) -> Option<String> {
    let value = trim_url_punctuation(value.trim());
    if value.is_empty() {
        return None;
    }
    let normalized = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_owned()
    } else if value.contains('.') && !value.contains(char::is_whitespace) {
        format!("https://{value}")
    } else {
        return None;
    };
    let parsed = Url::parse(&normalized).ok()?;
    (parsed.username().is_empty() && parsed.password().is_none()).then_some(normalized)
}

pub(crate) fn push_observation(
    observations: &mut Vec<PlatformContactObservation>,
    channel: ContactChannel,
    kind: LocatorKind,
    value: String,
    source: SourceKind,
    source_url: Option<String>,
) {
    if !value.trim().is_empty() {
        observations.push(PlatformContactObservation {
            channel,
            kind,
            value,
            source,
            source_url,
        });
    }
}

pub(crate) fn classify_contact_route(value: &str) -> Option<(ContactChannel, LocatorKind, String)> {
    let url = Url::parse(value).ok()?;
    if !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    if !matches!(url.scheme(), "http" | "https") || is_image_asset(&url) {
        return None;
    }
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let segments = url
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let first_segment = segments
        .first()
        .copied()
        .unwrap_or_default()
        .trim_start_matches('@')
        .to_owned();

    match host.as_str() {
        "x.com" | "twitter.com" if !first_segment.is_empty() => {
            Some((ContactChannel::X, LocatorKind::Handle, first_segment))
        }
        "instagram.com" if !first_segment.is_empty() => Some((
            ContactChannel::Instagram,
            LocatorKind::Handle,
            first_segment,
        )),
        "t.me" | "telegram.me" if !first_segment.is_empty() => {
            let invite = first_segment.starts_with('+') || first_segment == "joinchat";
            if segments.len() == 1 && !invite {
                Some((ContactChannel::Telegram, LocatorKind::Handle, first_segment))
            } else {
                Some((
                    ContactChannel::Telegram,
                    if invite {
                        LocatorKind::InviteLink
                    } else {
                        LocatorKind::Url
                    },
                    value.to_owned(),
                ))
            }
        }
        "youtube.com" | "youtu.be" => {
            Some((ContactChannel::YouTube, LocatorKind::Url, value.to_owned()))
        }
        host if is_link_hub(host) => {
            Some((ContactChannel::LinkHub, LocatorKind::Url, value.to_owned()))
        }
        // TikTok has no transport integration, but a discovered TikTok
        // profile is still saveable contact data like any third-party site.
        _ => Some((ContactChannel::Website, LocatorKind::Url, value.to_owned())),
    }
}

pub(crate) fn is_link_hub(host: &str) -> bool {
    matches!(
        host,
        "linktr.ee"
            | "beacons.ai"
            | "bio.link"
            | "solo.to"
            | "lnk.bio"
            | "taplink.cc"
            | "hoo.be"
            | "stan.store"
            | "carrd.co"
            | "allmylinks.com"
            | "campsite.bio"
    )
}

fn trim_url_punctuation(value: &str) -> &str {
    value.trim_end_matches(|character: char| {
        matches!(
            character,
            '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '}'
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_telegram_handle_mentioned_in_plain_bio_text() {
        let observations = discover_creator_contacts(
            Platform::X,
            &[CreatorContactProfileField {
                source: SourceKind::ProfileBio,
                source_url: Some("https://x.com/Harry_DgFex"),
                text: "Currently seeking cooperation with KOLs in crypto industry.\n\
                       Feel free to contact me on telegram @harry_dgf\n\
                       Email: harry@digifinex-partners.online",
            }],
            &[],
        );

        assert!(observations.iter().any(|observation| {
            observation.channel == ContactChannel::Telegram
                && observation.kind == LocatorKind::Handle
                && observation.value == "harry_dgf"
                && observation.source == SourceKind::ProfileBio
        }));
        assert!(observations.iter().any(|observation| {
            observation.channel == ContactChannel::Email
                && observation.value == "harry@digifinex-partners.online"
        }));
    }

    #[test]
    fn telegram_mentions_require_a_marker_and_a_real_handle_shape() {
        let none = |text: &str| {
            let observations = discover_creator_contacts(
                Platform::X,
                &[CreatorContactProfileField {
                    source: SourceKind::ProfileBio,
                    source_url: Some("https://x.com/creator"),
                    text,
                }],
                &[],
            );
            assert!(
                !observations
                    .iter()
                    .any(|observation| observation.channel == ContactChannel::Telegram),
                "{text:?} must not yield a Telegram observation: {observations:?}"
            );
        };
        // A bare mention is the host platform's own syntax.
        none("DM @someone for collabs");
        // Ordinary prose after the marker is not a handle.
        none("Telegram channel below");
        none("Join my telegram group for signals");
        // Too short for a Telegram username.
        none("telegram @ab");
        // Glued tokens belong to the email/url extractors.
        none("telegram @harry_dgf.eth holder");

        let colon_form = discover_creator_contacts(
            Platform::X,
            &[CreatorContactProfileField {
                source: SourceKind::ProfileBio,
                source_url: Some("https://x.com/creator"),
                text: "TG: kripto_kanal \u{2014} weekly signals",
            }],
            &[],
        );
        assert!(colon_form.iter().any(|observation| {
            observation.channel == ContactChannel::Telegram
                && observation.kind == LocatorKind::Handle
                && observation.value == "kripto_kanal"
        }));
    }

    #[test]
    fn telegram_mentions_are_dropped_on_the_owner_platform() {
        let observations = discover_creator_contacts(
            Platform::Telegram,
            &[CreatorContactProfileField {
                source: SourceKind::ProfileBio,
                source_url: Some("https://t.me/kripto_kanal"),
                text: "backup telegram @kripto_backup",
            }],
            &[],
        );
        assert!(
            !observations
                .iter()
                .any(|observation| observation.channel == ContactChannel::Telegram)
        );
    }

    #[test]
    fn retains_same_locator_from_distinct_sources() {
        let observations = discover_creator_contacts(
            Platform::X,
            &[
                CreatorContactProfileField {
                    source: SourceKind::ProfileBio,
                    source_url: Some("https://x.com/creator"),
                    text: "TG t.me/creator",
                },
                CreatorContactProfileField {
                    source: SourceKind::ProfileLocation,
                    source_url: Some("https://x.com/creator"),
                    text: "t.me/creator",
                },
            ],
            &[],
        );

        assert_eq!(
            observations
                .iter()
                .filter(|observation| observation.channel == ContactChannel::Telegram)
                .count(),
            2
        );
        assert!(observations.iter().any(|observation| {
            observation.channel == ContactChannel::Telegram
                && observation.source == SourceKind::ProfileBio
        }));
        assert!(observations.iter().any(|observation| {
            observation.channel == ContactChannel::Telegram
                && observation.source == SourceKind::ProfileLocation
        }));
    }

    #[test]
    fn prefers_visible_locator_over_opaque_short_url() {
        let observations = discover_creator_contacts(
            Platform::X,
            &[CreatorContactProfileField {
                source: SourceKind::ProfileBio,
                source_url: Some("https://x.com/theKriptolik"),
                text: "Verified Author https://t.co/fXIsBs0E4E",
            }],
            &[CreatorContactProfileLink {
                source: SourceKind::ProfileWebsite,
                source_url: Some("https://x.com/theKriptolik"),
                display_text: "t.me/theKriptolik",
                target_url: "https://t.co/fXIsBs0E4E",
            }],
        );

        assert!(observations.iter().any(|observation| {
            observation.channel == ContactChannel::Telegram
                && observation.value.eq_ignore_ascii_case("theKriptolik")
        }));
        assert!(!observations.iter().any(|observation| {
            observation.value.contains("t.co")
                || observation
                    .source_url
                    .as_deref()
                    .is_some_and(|url| url.contains("t.co"))
        }));
    }

    #[test]
    fn a_curated_mailto_link_exposes_only_its_single_recipient() {
        let source_url = "https://www.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw/about";
        let targets = [
            "mailto:business@example.com?subject=Partner&body=ignore-body@example.com",
            "mailto:first@example.com,second@example.com",
            "mailto:?body=ignore-body@example.com",
            "mailto:logo@2x.png",
            "mailto:not-an-address",
        ];
        let links: Vec<_> = targets
            .iter()
            .map(|target_url| CreatorContactProfileLink {
                source: SourceKind::ProfileWebsite,
                source_url: Some(source_url),
                display_text: "Business email",
                target_url,
            })
            .collect();
        let observations = discover_creator_contacts(Platform::YouTube, &[], &links);
        assert_eq!(observations.len(), 1, "{observations:?}");
        assert_eq!(observations[0].channel, ContactChannel::Email);
        assert_eq!(observations[0].value, "business@example.com");
        assert_eq!(observations[0].source_url.as_deref(), Some(source_url));
    }

    #[test]
    fn a_business_email_outside_a_video_url_survives_the_same_address_inside_that_url() {
        let observations = discover_creator_contacts(
            Platform::YouTube,
            &[CreatorContactProfileField {
                source: SourceKind::VideoDescription,
                source_url: Some("https://www.youtube.com/watch?v=original"),
                text: "Business: business@example.com. Image: https://assets.example.com/business@example.com/banner.jpg",
            }],
            &[],
        );
        assert_eq!(observations.len(), 1, "{observations:?}");
        assert_eq!(observations[0].channel, ContactChannel::Email);
        assert_eq!(observations[0].value, "business@example.com");
        assert_eq!(
            observations[0].source_url.as_deref(),
            Some("https://www.youtube.com/watch?v=original")
        );
    }

    #[test]
    fn rejects_email_shaped_urls() {
        assert_eq!(normalize_visible_url("creator@example.com"), None);
        assert_eq!(normalize_visible_url("https://creator@example.com"), None);
        assert_eq!(classify_contact_route("https://creator@example.com"), None);
    }
}

fn is_image_asset(url: &Url) -> bool {
    is_image_name(url.path())
        || url.host_str().is_some_and(|host| {
            matches!(
                host,
                "pbs.twimg.com"
                    | "abs.twimg.com"
                    | "scontent.cdninstagram.com"
                    | "yt3.ggpht.com"
                    | "yt3.googleusercontent.com"
            )
        })
}

pub(crate) fn is_image_name(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".avif", ".ico", ".bmp", ".tiff",
    ]
    .iter()
    .any(|extension| value.ends_with(extension))
}

#[cfg(test)]
mod image_evidence_tests {
    use super::*;

    #[test]
    fn asset_links_and_retina_filenames_do_not_become_contacts() {
        let observations = discover_creator_contacts(
            Platform::X,
            &[CreatorContactProfileField {
                source: SourceKind::ProfileBio,
                source_url: Some("https://x.com/creator"),
                text: "logo@2x.png https://assets.example.com/contact@example.com/banner.jpg https://pbs.twimg.com/media/123?format=jpg Real: hello@example.com",
            }],
            &[
                CreatorContactProfileLink {
                    source: SourceKind::ProfileWebsite,
                    source_url: Some("https://x.com/creator"),
                    display_text: "Brand image",
                    target_url: "https://example.com/brand.PNG?v=2",
                },
                CreatorContactProfileLink {
                    source: SourceKind::ProfileWebsite,
                    source_url: Some("https://x.com/creator"),
                    display_text: "Website",
                    target_url: "https://example.com/contact",
                },
            ],
        );
        assert_eq!(observations.len(), 2, "{observations:?}");
        assert!(
            observations
                .iter()
                .any(|observation| observation.channel == ContactChannel::Email
                    && observation.value == "hello@example.com")
        );
        assert!(
            observations
                .iter()
                .any(|observation| observation.channel == ContactChannel::Website
                    && observation.value == "https://example.com/contact")
        );
    }
}
