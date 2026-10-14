use onecrew_creator_runtime::{
    CreatorSaveContactEvidence, CreatorSaveProfile, Platform, PlatformAccountCandidate, SourceKind,
    TargetKind,
};

use super::ReadCreatorProfile;

pub(super) fn x(user: onecrew_x::User) -> ReadCreatorProfile {
    let profile_url = format!("https://x.com/{}", user.username);
    let mut evidence: Vec<_> = std::iter::once(CreatorSaveContactEvidence::ProfileField {
        source: SourceKind::ProfileBio,
        source_url: Some(profile_url.clone()),
        text: user.bio.clone(),
    })
    .chain(
        user.profile_links
            .iter()
            .map(|link| CreatorSaveContactEvidence::ProfileLink {
                source: SourceKind::ProfileWebsite,
                source_url: Some(profile_url.clone()),
                display_text: link.display_text.clone(),
                target_url: link.target_url.clone(),
            }),
    )
    .collect();
    if !user.location.trim().is_empty() {
        evidence.push(CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::ProfileLocation,
            source_url: Some(profile_url.clone()),
            text: user.location.clone(),
        });
    }
    ReadCreatorProfile {
        observed_at: std::time::SystemTime::now(),
        account: PlatformAccountCandidate {
            platform: Platform::X,
            external_id: Some(user.id),
            target_kind: Some(TargetKind::User),
            handle: Some(user.username),
            display_name: user.name,
            profile_url: Some(profile_url),
        },
        profile: CreatorSaveProfile {
            avatar_url: nonempty(user.profile_image_url.clone()),
            description: Some(user.bio),
            location: nonempty(user.location),
            website_urls: user
                .profile_links
                .into_iter()
                .map(|link| link.target_url)
                .collect(),
            follower_count: Some(user.followers),
        },
        evidence,
        warnings: Vec::new(),
        administrator: None,
    }
}

pub(super) fn instagram(user: onecrew_ig::IgUser) -> ReadCreatorProfile {
    let profile_url = format!("https://www.instagram.com/{}/", user.username);
    let mut website_urls = user.bio_links;
    if let Some(url) = user.external_url
        && !website_urls.contains(&url)
    {
        website_urls.push(url);
    }
    let mut evidence: Vec<_> = website_urls
        .iter()
        .map(|url| CreatorSaveContactEvidence::ProfileLink {
            source: SourceKind::ProfileWebsite,
            source_url: Some(profile_url.clone()),
            display_text: url.clone(),
            target_url: url.clone(),
        })
        .collect();
    if let Some(bio) = &user.biography {
        evidence.push(CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::ProfileBio,
            source_url: Some(profile_url.clone()),
            text: bio.clone(),
        });
    }
    if let Some(email) = user.public_email {
        evidence.push(CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::ProfileBio,
            source_url: Some(profile_url.clone()),
            text: email,
        });
    }
    for location in [user.city_name.as_ref(), user.address_street.as_ref()]
        .into_iter()
        .flatten()
    {
        if !location.trim().is_empty() {
            evidence.push(CreatorSaveContactEvidence::ProfileField {
                source: SourceKind::ProfileLocation,
                source_url: Some(profile_url.clone()),
                text: location.clone(),
            });
        }
    }
    ReadCreatorProfile {
        observed_at: std::time::SystemTime::now(),
        account: PlatformAccountCandidate {
            platform: Platform::Instagram,
            external_id: Some(user.id),
            target_kind: Some(TargetKind::User),
            handle: Some(user.username),
            display_name: user.full_name,
            profile_url: Some(profile_url),
        },
        profile: CreatorSaveProfile {
            avatar_url: user.profile_pic_url.clone(),
            description: user.biography,
            location: user.city_name,
            website_urls,
            follower_count: user
                .follower_count
                .and_then(|count| u64::try_from(count).ok()),
        },
        evidence,
        warnings: Vec::new(),
        administrator: None,
    }
}

pub(super) fn youtube(user: onecrew_youtube::YoutubeCreator) -> ReadCreatorProfile {
    // Preserve source facts here. Creator decides which routes each source
    // supports, including the narrower policy for video descriptions.
    let mut evidence = vec![CreatorSaveContactEvidence::ProfileField {
        source: SourceKind::ChannelDescription,
        source_url: user.description_source_url.clone(),
        text: user.description.clone(),
    }];
    if let Some(country) = &user.country {
        evidence.push(CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::ProfileLocation,
            source_url: Some(user.channel_url.clone()),
            text: country.clone(),
        });
    }
    // Search-result snippets are provider facts for display only. They are
    // not complete video resources and must not become contact evidence.
    evidence.extend(user.recent_videos.into_iter().map(|video| {
        CreatorSaveContactEvidence::ProfileField {
            source: SourceKind::VideoDescription,
            source_url: Some(format!(
                "https://www.youtube.com/watch?v={}",
                video.video_id
            )),
            text: video.description,
        }
    }));
    let website_urls = user
        .about_links
        .iter()
        .map(|link| link.target_url.clone())
        .collect();
    evidence.extend(user.about_links.into_iter().map(|link| {
        CreatorSaveContactEvidence::ProfileLink {
            source: SourceKind::ProfileWebsite,
            source_url: Some(link.source_url),
            display_text: link.target_url.clone(),
            target_url: link.target_url,
        }
    }));
    let warnings = user.unavailable_sections.into_iter().map(|section| {
        match section {
            onecrew_youtube::YoutubeProfileSection::About => "YouTube About information could not be read; this preview may omit public links and description evidence.",
            onecrew_youtube::YoutubeProfileSection::RecentVideos => "YouTube recent videos could not be read; this preview omits their description evidence.",
            onecrew_youtube::YoutubeProfileSection::PublicComments => "YouTube public comments could not be read.",
        }.to_owned()
    }).collect();
    ReadCreatorProfile {
        observed_at: std::time::SystemTime::now(),
        account: PlatformAccountCandidate {
            platform: Platform::YouTube,
            external_id: Some(user.channel_id),
            target_kind: Some(TargetKind::Channel),
            handle: user.custom_url.map(|value| {
                value
                    .trim()
                    .trim_start_matches("https://www.youtube.com/")
                    .trim_start_matches('@')
                    .to_owned()
            }),
            display_name: user.title,
            profile_url: Some(user.channel_url),
        },
        profile: CreatorSaveProfile {
            avatar_url: user.thumbnail_url,
            description: Some(user.description),
            location: user.country,
            website_urls,
            follower_count: user.subscriber_count,
        },
        evidence,
        warnings,
        administrator: None,
    }
}

fn nonempty(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}
