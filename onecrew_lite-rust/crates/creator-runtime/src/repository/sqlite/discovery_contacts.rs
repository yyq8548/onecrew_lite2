//! Persist only contacts extracted from a validated saved review basis.
use crate::{
    DiscoveryContact, DiscoveryContactAttribution, DiscoveryContactKind, PreparedDiscoveryReview,
};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn observe_review(
    tx: &Connection,
    prepared: &PreparedDiscoveryReview,
) -> Result<(), String> {
    let evidence = serde_json::from_str(&prepared.evidence_json).map_err(|e| e.to_string())?;
    if !crate::discovery_contacts::score_passes(
        &prepared.receipt.verdict,
        &evidence,
        &prepared.receipt.native_author_id,
    ) {
        return Ok(());
    }
    let material = crate::discovery_review::build_material(prepared.basis.clone())?;
    let mut contacts = Vec::new();
    let mut add = |text: &str, source: &str, captured: i64, attribution| {
        contacts.extend(crate::discovery_contacts::extract(
            text,
            source,
            &material.native_author_id,
            captured,
            attribution,
        ));
    };
    let public_fields = profile_contact_fields(tx, &material)?;
    if let Some(profile) = &material.native_profile {
        add(
            &public_fields,
            &profile.profile_url,
            profile.observed_at,
            DiscoveryContactAttribution::AuthorPublished,
        );
    } else if material.native_platform.as_deref() == Some("youtube") {
        add(
            &public_fields,
            &material.hit.source_url,
            material
                .hit
                .author_observed_at
                .ok_or("contact_native_source_time_unavailable")?,
            DiscoveryContactAttribution::AuthorPublished,
        );
    }
    if let Some(description) = &material.video_description {
        let saved: serde_json::Value =
            serde_json::from_str(&prepared.basis.evidence_json).map_err(|e| e.to_string())?;
        if let Some(captured) = saved["captured_at"].as_i64() {
            add(
                description,
                &material.hit.source_url,
                captured,
                DiscoveryContactAttribution::AuthorPublished,
            );
        }
    }
    if let Some(profile) = &material.native_profile {
        for text in [
            profile.bio.as_deref(),
            profile.self_declared_location.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            add(
                text,
                &profile.profile_url,
                profile.observed_at,
                DiscoveryContactAttribution::AuthorPublished,
            );
        }
    }
    if let Some(read) = &material.native_content
        && let Some(content) = &read.native_content
    {
        let key = if read.claim.platform == "bilibili" {
            "description"
        } else {
            "text"
        };
        if let Some(text) = content[key].as_str() {
            add(
                text,
                read.observed_source_url()
                    .ok_or("contact_native_source_unavailable")?,
                read.captured_at,
                DiscoveryContactAttribution::AuthorPublished,
            );
        }
    }
    for page in &material.public_pages {
        if page.status == "completed"
            && page.stop_code.is_none()
            && lookup_page_matches_material(tx, &material, page)?
            && let Some(url) = &page.document_url
        {
            add(
                &page.markdown_excerpt,
                url,
                page.captured_at,
                DiscoveryContactAttribution::Unconfirmed,
            );
        }
    }
    // These saved native author accounts are themselves contact routes.
    // Material construction and score_passes above bind it to the reviewed author.
    if matches!(
        material.author_platform(),
        "twitter"
            | "instagram"
            | "tiktok"
            | "douyin"
            | "linkedin"
            | "reddit"
            | "facebook"
            | "youtube"
    ) {
        let profile = material.native_profile.as_ref();
        let youtube_url = format!(
            "https://www.youtube.com/channel/{}",
            material.native_author_id
        );
        let url = profile
            .map(|p| p.profile_url.as_str())
            .or(material.hit.author_url.as_deref())
            .or((material.author_platform() == "youtube").then_some(youtube_url.as_str()));
        let captured = profile
            .map(|p| p.observed_at)
            .or(material.hit.author_observed_at)
            .or((material.author_platform() == "youtube").then_some(material.hit.captured_at));
        if let (Some(url), Some(captured_at)) = (url, captured)
            && crate::discovery_contacts::social_profile(url)
            && crate::discovery_contacts::contact_platform(url) == Some(material.author_platform())
        {
            contacts.push(DiscoveryContact {
                kind: DiscoveryContactKind::SocialProfile,
                original_value: url.into(),
                normalized_value: url.into(),
                source_url: url.into(),
                source_excerpt: "Verified native author account".into(),
                captured_at,
                native_author_id: material.native_author_id.clone(),
                attribution: DiscoveryContactAttribution::AuthorPublished,
            });
        }
    }
    store(
        tx,
        &material.campaign_id,
        material.criteria_revision,
        &material.candidate_id,
        contacts,
    )
}

fn lookup_page_matches_material(
    tx: &Connection,
    material: &crate::DiscoveryReviewMaterial,
    page: &crate::DiscoveryPublicPageRead,
) -> Result<bool, String> {
    if crate::contact_navigation::non_contact(&page.requested_url)
        || crate::contact_navigation::promotional(&page.requested_url, &page.source_excerpt)
        || page
            .document_url
            .as_deref()
            .is_some_and(crate::contact_navigation::non_contact)
    {
        return Ok(false);
    }
    if !super::discovery_contact_lookup::links(
        tx,
        &material.candidate_id,
        material.criteria_revision,
    )?
    .iter()
    .any(|(link, _)| link == &page.requested_url)
    {
        return Ok(true);
    }
    if crate::contact_navigation::non_contact(&page.requested_url) {
        return Ok(false);
    }
    // Search summaries and matching display names cannot bind a mailbox to an
    // author. Require the saved primary profile/channel URL in the page itself.
    let bound_urls = [
        material.hit.author_url.as_deref(),
        material
            .native_profile
            .as_ref()
            .map(|p| p.profile_url.as_str()),
    ];
    if bound_urls.into_iter().flatten().any(|url| {
        crate::discovery_public_pages::description_observes_link(&page.markdown_excerpt, url)
    }) {
        return Ok(true);
    }
    if let Some(url) = youtube_profile_url(material)
        && crate::discovery_public_pages::description_observes_link(&page.markdown_excerpt, &url)
    {
        return Ok(true);
    }
    Ok(false)
}

fn store(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    contacts: Vec<DiscoveryContact>,
) -> Result<(), String> {
    let platform: String = tx.query_row("SELECT platform FROM discovery_candidates WHERE id=?1 AND discovery_campaign_id=?2 AND criteria_revision=?3", params![candidate,campaign,revision], |r|r.get(0)).map_err(|e|e.to_string())?;
    for contact in contacts
        .into_iter()
        .filter(|c| {
            c.kind != DiscoveryContactKind::SocialProfile
                || matches!(
                    platform.as_str(),
                    "twitter"
                        | "instagram"
                        | "tiktok"
                        | "douyin"
                        | "linkedin"
                        | "reddit"
                        | "facebook"
                        | "youtube"
                )
                || crate::discovery_contacts::contact_platform(&c.normalized_value)
                    != Some(platform.as_str())
        })
        .take(100)
    {
        tx.execute("INSERT INTO discovery_contact_observations
            (candidate_id,discovery_campaign_id,criteria_revision,native_author_id,kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
            ON CONFLICT(candidate_id,criteria_revision,normalized_value,source_url) DO NOTHING",
            params![candidate, campaign, revision, contact.native_author_id,
                match contact.kind { DiscoveryContactKind::Email => "email", DiscoveryContactKind::Telegram => "telegram", DiscoveryContactKind::HiddenEmail => "hidden_email", DiscoveryContactKind::SocialProfile => "social_profile", DiscoveryContactKind::Whatsapp => "whatsapp", DiscoveryContactKind::Wechat => "wechat", DiscoveryContactKind::WechatOfficial => "wechat_official" },
                contact.original_value, contact.normalized_value, contact.source_url, contact.source_excerpt, contact.captured_at,
                match contact.attribution { DiscoveryContactAttribution::AuthorPublished => "author_published", DiscoveryContactAttribution::Unconfirmed => "unconfirmed" }]
        ).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// A backend score field alone cannot authorize contact lookup.
pub(super) fn eligible_author(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
) -> Result<Option<String>, String> {
    let saved: Option<(String, String, String)> = tx.query_row(
        "SELECT c.platform_account_id,c.verdict,c.evidence_json FROM discovery_candidates c
         JOIN discovery_recommendation_policies p ON p.discovery_campaign_id=c.discovery_campaign_id AND p.criteria_revision=c.criteria_revision
         WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3
           AND p.policy='score_and_contact_v1' AND c.verdict='qualified'",
        params![candidate,campaign,revision], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional().map_err(|e| e.to_string())?;
    let Some((author, verdict, raw)) = saved else {
        return Ok(None);
    };
    let mut evidence = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    super::discovery_review::strip_unaudited_proofs(
        tx,
        &mut evidence,
        candidate,
        campaign,
        revision,
        &author,
    )?;
    Ok(crate::discovery_contacts::score_passes(&verdict, &evidence, &author).then_some(author))
}

/// Reuse only author-bound profile/content receipts. Audience comments are excluded.
pub(super) fn native_link_source(
    tx: &Connection,
    input: &crate::DiscoveryPublicPageInput,
) -> Result<Option<(String, String, String, String)>, String> {
    let Some(material) = native_material(tx, input)? else {
        return Ok(None);
    };
    if super::discovery_contact_lookup::blocked_host(tx, &input.campaign_id, &input.requested_url)?
        || crate::contact_navigation::non_contact(&input.requested_url)
    {
        return Ok(None);
    }
    if youtube_profile_url(&material).as_deref() == Some(&input.requested_url) {
        return Ok(Some((
            material.hit.id,
            material.hit.source_url,
            input.requested_url.clone(),
            material.hit.platform,
        )));
    }
    let public_fields = profile_contact_fields(tx, &material)?;
    let bio = material
        .native_profile
        .as_ref()
        .and_then(|p| p.bio.as_deref());
    let content = material
        .native_content
        .as_ref()
        .and_then(|r| r.native_content.as_ref())
        .and_then(|c| {
            c[if material.hit.platform == "bilibili" {
                "description"
            } else {
                "text"
            }]
            .as_str()
        });
    for text in [
        bio,
        content,
        material.video_description.as_deref(),
        Some(public_fields.as_str()),
    ]
    .into_iter()
    .flatten()
    {
        if crate::discovery_public_pages::description_observes_link(text, &input.requested_url)
            && !crate::contact_navigation::promotional(&input.requested_url, text)
            && !crate::contact_navigation::third_party_link(&input.requested_url, text)
        {
            return Ok(Some((
                material.hit.id,
                material.hit.source_url,
                text.into(),
                material.hit.platform,
            )));
        }
    }
    // Only one additional hop, from an already completed author-linked page.
    // The source excerpt is saved with the claim, so resume cannot change it.
    for page in &material.public_pages {
        // A separately reserved native channel biography can publish an
        // external website. Only its exact observed links authorize a read.
        if page.status == "completed"
            && page.stop_code.is_none()
            && youtube_profile_url(&material).as_deref() == Some(&page.requested_url)
            && crate::discovery_public_pages::description_observes_link(
                &page.markdown_excerpt,
                &input.requested_url,
            )
            && !crate::contact_navigation::promotional(&input.requested_url, &page.markdown_excerpt)
            && !crate::contact_navigation::third_party_link(
                &input.requested_url,
                &page.markdown_excerpt,
            )
        {
            return Ok(Some((
                material.hit.id.clone(),
                material.hit.source_url.clone(),
                page.markdown_excerpt.clone(),
                material.hit.platform.clone(),
            )));
        }
        if page.status == "completed"
            && page.stop_code.is_none()
            && author_observes_link(tx, &material, &page.requested_url)?
            && page.document_url.as_deref().is_some_and(|source| {
                crate::discovery_public_pages::contact_page_links(source, &page.markdown_excerpt)
                    .contains(&input.requested_url)
            })
        {
            return Ok(Some((
                material.hit.id.clone(),
                material.hit.source_url.clone(),
                page.markdown_excerpt.clone(),
                material.hit.platform.clone(),
            )));
        }
    }
    for (link, query) in
        super::discovery_contact_lookup::links(tx, &input.candidate_id, input.expected_revision)?
    {
        if link == input.requested_url {
            return Ok(Some((
                material.hit.id,
                material.hit.source_url,
                query,
                material.hit.platform,
            )));
        }
    }
    Ok(None)
}

/// Read only optional public fields from the exact validated native receipt.
fn profile_contact_fields(
    tx: &Connection,
    material: &crate::DiscoveryReviewMaterial,
) -> Result<String, String> {
    if matches!(material.hit.platform.as_str(), "google" | "yahoo") {
        let read = super::discovery_web_sources::get_source_row(
            tx,
            &material.campaign_id,
            &material.hit.id,
        )?;
        if !read.is_primary_source
            || read.status != "completed"
            || read.stop_code.is_some()
            || read.claim.expected_revision != material.criteria_revision
            || read.claim.profile_ref != material.hit.profile_ref
            || read.claim.source_url != material.hit.source_url
            || read.candidate_id.as_deref() != Some(&material.candidate_id)
            || read.native_author_id.as_deref() != Some(&material.native_author_id)
        {
            return Err("contact_native_web_source_binding_unavailable".into());
        }
        let twitter = read.claim.target_platform == "twitter";
        let douyin = read.claim.target_platform == "douyin";
        crate::discovery_web_sources::validate_result(&crate::FinishDiscoveryWebSourceInput {
            claim: read.claim,
            native_material: read.native_material.clone(),
            stop_code: None,
        })?;
        let native = read
            .native_material
            .ok_or("contact_native_web_source_binding_unavailable")?;
        let original = if twitter || douyin {
            native.pointer("/content/text")
        } else {
            native.get("description")
        }
        .and_then(serde_json::Value::as_str);
        if original != material.video_description.as_deref() {
            return Err("contact_native_web_source_binding_unavailable".into());
        }
        if twitter || douyin {
            if material.native_profile.as_ref().is_none_or(|profile| {
                Some(profile.profile_url.as_str()) != native["profile"]["profile_url"].as_str()
            }) {
                return Err("contact_native_web_source_binding_unavailable".into());
            }
            return Ok(if douyin {
                native["profile"]["public_links"]
                    .as_array()
                    .ok_or("contact_native_web_source_links_unavailable")?
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                String::new()
            });
        }
        return Ok(native["links"]
            .as_array()
            .ok_or("contact_native_web_source_links_unavailable")?
            .iter()
            .filter_map(|link| link["url"].as_str())
            .collect::<Vec<_>>()
            .join("\n"));
    }
    if !matches!(
        material.hit.platform.as_str(),
        "instagram" | "facebook" | "pinterest" | "pixiv" | "linkedin" | "douyin"
    ) {
        return Ok(String::new());
    }
    let profile =
        super::discovery_profiles::load_profile_row(tx, &material.campaign_id, &material.hit.id)?;
    if profile.status != "completed"
        || profile.stop_code.is_some()
        || profile.claim.expected_revision != material.criteria_revision
        || profile.claim.platform != material.hit.platform
        || profile.claim.profile_ref != material.hit.profile_ref
        || (if material.hit.platform == "linkedin" {
            profile
                .native_profile
                .as_ref()
                .and_then(|p| p["account_id"].as_str())
                != Some(material.native_author_id.as_str())
        } else {
            profile.claim.author_id != material.native_author_id
        })
        || material
            .native_profile
            .as_ref()
            .is_none_or(|p| p.profile_url != profile.claim.author_url)
    {
        return Err("contact_native_profile_binding_unavailable".into());
    }
    crate::discovery_profiles::validate_profile_result(&crate::FinishDiscoveryProfileInput {
        claim: profile.claim.clone(),
        native_profile: profile.native_profile.clone(),
        stop_code: None,
    })?;
    let native = profile
        .native_profile
        .ok_or("contact_native_profile_binding_unavailable")?;
    let mut fields = Vec::new();
    if let Some(email) = native["public_email"].as_str() {
        fields.push(email.to_owned());
    }
    // LinkedIn overlays use the native facade and recheck identity in the
    // visible dialog. Generic reads remain limited to observed public websites.
    let links_key = if matches!(
        material.hit.platform.as_str(),
        "pinterest" | "pixiv" | "linkedin" | "douyin"
    ) {
        "public_links"
    } else {
        "contact_links"
    };
    if let Some(links) = native[links_key].as_array() {
        fields.extend(
            links
                .iter()
                .filter_map(|link| link.as_str().map(str::to_owned)),
        );
    }
    if material.hit.platform == "linkedin"
        && let Some(links) = native["contact_links"].as_array()
    {
        fields.extend(
            links
                .iter()
                .filter_map(|link| link.as_str().map(str::to_owned)),
        );
    }
    Ok(fields.join("\n"))
}

fn native_texts(material: &crate::DiscoveryReviewMaterial) -> [Option<&str>; 3] {
    [
        material
            .native_profile
            .as_ref()
            .and_then(|p| p.bio.as_deref()),
        material
            .native_content
            .as_ref()
            .and_then(|r| r.native_content.as_ref())
            .and_then(|c| {
                c[if material.hit.platform == "bilibili" {
                    "description"
                } else {
                    "text"
                }]
                .as_str()
            }),
        material.video_description.as_deref(),
    ]
}

fn youtube_profile_url(material: &crate::DiscoveryReviewMaterial) -> Option<String> {
    (material.author_platform() == "youtube"
        && material.native_author_id.starts_with("UC")
        && (10..=64).contains(&material.native_author_id.len())
        && material
            .native_author_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then(|| {
        format!(
            "https://www.youtube.com/channel/{}",
            material.native_author_id
        )
    })
}

fn author_observes_link(
    tx: &Connection,
    material: &crate::DiscoveryReviewMaterial,
    url: &str,
) -> Result<bool, String> {
    let public_fields = profile_contact_fields(tx, material)?;
    Ok(
        crate::discovery_public_pages::description_observes_link(&public_fields, url)
            || native_texts(material).into_iter().flatten().any(|text| {
                crate::discovery_public_pages::description_observes_link(text, url)
                    && !crate::contact_navigation::promotional(url, text)
                    && !crate::contact_navigation::third_party_link(url, text)
            })
            || material.public_pages.iter().any(|page| {
                page.status == "completed"
                    && page.stop_code.is_none()
                    && youtube_profile_url(material).as_deref() == Some(&page.requested_url)
                    && crate::discovery_public_pages::description_observes_link(
                        &page.markdown_excerpt,
                        url,
                    )
                    && !crate::contact_navigation::promotional(url, &page.markdown_excerpt)
                    && !crate::contact_navigation::third_party_link(url, &page.markdown_excerpt)
            }),
    )
}

pub(super) fn native_material(
    tx: &Connection,
    input: &crate::DiscoveryPublicPageInput,
) -> Result<Option<crate::DiscoveryReviewMaterial>, String> {
    let Some(author) = eligible_author(
        tx,
        &input.campaign_id,
        input.expected_revision,
        &input.candidate_id,
    )?
    else {
        return Ok(None);
    };
    if !list(
        tx,
        &input.campaign_id,
        input.expected_revision,
        &input.candidate_id,
        &author,
    )?
    .is_empty()
        || has_hidden_email(
            tx,
            &input.campaign_id,
            input.expected_revision,
            &input.candidate_id,
            &author,
        )?
    {
        return Ok(None);
    }
    let hit: Option<String> = tx.query_row(
        "SELECT h.id FROM discovery_search_hits h JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id
         AND c.source_platform=h.platform AND c.source_profile_ref=h.profile_ref
         WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3 AND c.source_profile_ref=?4
           AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        params![input.candidate_id,input.campaign_id,input.expected_revision,input.profile_ref], |r|r.get(0),
    ).optional().map_err(|e|e.to_string())?;
    let Some(hit) = hit else { return Ok(None) };
    let material =
        crate::discovery_review::build_material(super::discovery_review::load_contact_basis(
            tx,
            &input.campaign_id,
            input.expected_revision,
            &hit,
        )?)?;
    if material.candidate_id != input.candidate_id || material.native_author_id != author {
        return Err("contact_source_author_mismatch".into());
    }
    Ok(Some(material))
}

pub(super) fn next_native_link(
    tx: &Connection,
    campaign: &str,
    excluded_platforms: &[String],
) -> Result<Option<crate::DiscoveryPublicPageInput>, String> {
    let mut query = tx.prepare(
        "SELECT c.id,c.criteria_revision,c.source_profile_ref,p.status,p.active_query FROM discovery_candidates c
         JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
         JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
         JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.source_platform AND p.profile_ref=c.source_profile_ref
         JOIN discovery_recommendation_policies policy ON policy.discovery_campaign_id=d.id AND policy.criteria_revision=d.current_revision
         WHERE d.id=?1 AND d.phase='active' AND c.criteria_revision=d.current_revision AND a.approved_at IS NOT NULL
           AND NOT EXISTS(SELECT 1 FROM json_each(?2) excluded WHERE excluded.value IN (c.platform,c.source_platform))
           AND policy.policy='score_and_contact_v1' AND c.verdict='qualified' AND (d.max_calls IS NULL OR d.used_calls<d.max_calls)
           AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching')
           AND (p.status='completed' OR (p.status='technical_stop' AND p.stop_code='interrupted_public_page_read'))
         ORDER BY c.id").map_err(|e|e.to_string())?;
    let rows = query
        .query_map(
            params![
                campaign,
                serde_json::to_string(excluded_platforms).map_err(|e| e.to_string())?
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (candidate_id, expected_revision, profile_ref, status, active) =
            row.map_err(|e| e.to_string())?;
        let input = crate::DiscoveryPublicPageInput {
            campaign_id: campaign.into(),
            candidate_id: candidate_id.clone(),
            expected_revision,
            profile_ref: profile_ref.clone(),
            requested_url: String::new(),
        };
        let Some(material) = native_material(tx, &input)? else {
            continue;
        };
        let public_fields = profile_contact_fields(tx, &material)?;
        let strings = [
            material
                .native_profile
                .as_ref()
                .and_then(|p| p.bio.as_deref()),
            material
                .native_content
                .as_ref()
                .and_then(|r| r.native_content.as_ref())
                .and_then(|c| {
                    c[if material.hit.platform == "bilibili" {
                        "description"
                    } else {
                        "text"
                    }]
                    .as_str()
                }),
        ];
        let mut links = Vec::new();
        if let Some(profile_url) = youtube_profile_url(&material) {
            links.push(profile_url);
        }
        for text in strings
            .into_iter()
            .chain([
                material.video_description.as_deref(),
                Some(public_fields.as_str()),
            ])
            .flatten()
        {
            for token in text.split_whitespace() {
                let url = token.trim_matches(|c: char| {
                    matches!(
                        c,
                        '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.' | ';' | '!' | '"' | '\''
                    )
                });
                if !url.starts_with("https://")
                    || url.len() > 2048
                    || url.contains("...")
                    || url.contains('…')
                    || crate::discovery_public_pages::is_community_invite_link(url)
                    || crate::contact_navigation::third_party_link(url, text)
                {
                    continue;
                }
                links.push(url.to_string());
            }
        }
        // A channel read is separately reserved after scoring; no second
        // provider call is hidden inside the original video read.
        for page in &material.public_pages {
            if page.status == "completed"
                && page.stop_code.is_none()
                && youtube_profile_url(&material).as_deref() == Some(&page.requested_url)
            {
                for token in page.markdown_excerpt.split_whitespace() {
                    let url = token.trim_matches(|c: char| {
                        matches!(
                            c,
                            '(' | ')' | '[' | ']' | '<' | '>' | ',' | '.' | ';' | '!' | '"' | '\''
                        )
                    });
                    if url.starts_with("https://")
                        && url.len() <= 2048
                        && !url.contains("...")
                        && !url.contains('…')
                        && !crate::discovery_public_pages::is_community_invite_link(url)
                        && !crate::contact_navigation::third_party_link(url, &page.markdown_excerpt)
                    {
                        links.push(url.to_string());
                    }
                }
            }
            if page.status == "completed"
                && page.stop_code.is_none()
                && author_observes_link(tx, &material, &page.requested_url)?
                && let Some(source) = &page.document_url
            {
                links.extend(crate::discovery_public_pages::contact_page_links(
                    source,
                    &page.markdown_excerpt,
                ));
            }
        }
        links.extend(
            super::discovery_contact_lookup::links(tx, &candidate_id, expected_revision)?
                .into_iter()
                .map(|(link, _)| link),
        );
        let native_profile = youtube_profile_url(&material);
        links.sort_by_key(|link| {
            crate::contact_navigation::priority(link, native_profile.as_deref())
        });
        let mut seen = std::collections::HashSet::new();
        for url in links {
            if !seen.insert(crate::contact_navigation::read_key(&url))
                || crate::contact_navigation::non_contact(&url)
                || material.public_pages.iter().any(|page| {
                    page.status != "reading"
                        && (crate::contact_navigation::read_key(&page.requested_url)
                            == crate::contact_navigation::read_key(&url)
                            || page.document_url.as_deref().is_some_and(|document| {
                                crate::contact_navigation::read_key(document)
                                    == crate::contact_navigation::read_key(&url)
                            }))
                })
                || (crate::contact_navigation::priority(&url, native_profile.as_deref()) >= 4
                    && strings
                        .into_iter()
                        .chain([material.video_description.as_deref()])
                        .flatten()
                        .any(|text| crate::contact_navigation::promotional(&url, text)))
            {
                continue;
            }
            let input = crate::DiscoveryPublicPageInput {
                campaign_id: campaign.into(),
                candidate_id: candidate_id.clone(),
                expected_revision,
                profile_ref: profile_ref.clone(),
                requested_url: url.clone(),
            };
            let prior: Option<(String,u32)> = tx.query_row("SELECT status,call_number FROM discovery_public_page_reads WHERE candidate_id=?1 AND requested_url=?2",params![candidate_id,url], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
            let eligible = match prior {
                None => status == "completed",
                Some((state, call)) => {
                    state == "reading"
                        && status == "technical_stop"
                        && active.as_deref() == Some(format!("page:{candidate_id}:{call}").as_str())
                }
            };
            if !eligible {
                continue;
            }
            if native_link_source(tx, &input)?.is_some() {
                return Ok(Some(input));
            }
        }
    }
    Ok(None)
}

/// Called only after the public-page owner has committed a source-bound receipt.
pub(super) fn observe_public_page(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    page: &crate::DiscoveryPublicPageRead,
) -> Result<(), String> {
    if page.status != "completed" || page.stop_code.is_some() {
        return Ok(());
    }
    let Some(author) = eligible_author(tx, campaign, revision, &page.candidate_id)? else {
        return Ok(());
    };
    if author != page.platform_account_id {
        return Err("contact_page_author_mismatch".into());
    }
    let Some(source) = &page.document_url else {
        return Ok(());
    };
    if crate::contact_navigation::non_contact(&page.requested_url)
        || crate::contact_navigation::promotional(&page.requested_url, &page.source_excerpt)
        || crate::contact_navigation::non_contact(source)
    {
        return Ok(());
    }
    // An unrelated search hit or same-name profile cannot provide this author's contact.
    if super::discovery_contact_lookup::links(tx, &page.candidate_id, revision)?
        .iter()
        .any(|(link, _)| link == &page.requested_url)
    {
        let input = crate::DiscoveryPublicPageInput {
            campaign_id: campaign.into(),
            expected_revision: revision,
            candidate_id: page.candidate_id.clone(),
            profile_ref: page.profile_ref.clone(),
            requested_url: page.requested_url.clone(),
        };
        let Some(material) = native_material(tx, &input)? else {
            return Ok(());
        };
        if !lookup_page_matches_material(tx, &material, page)? {
            return Ok(());
        }
    }
    let contacts = crate::discovery_contacts::extract(
        &page.markdown_excerpt,
        source,
        &author,
        page.captured_at,
        DiscoveryContactAttribution::Unconfirmed,
    );
    store(tx, campaign, revision, &page.candidate_id, contacts)
}

pub(super) fn list(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    author: &str,
) -> Result<Vec<DiscoveryContact>, String> {
    observations(tx, campaign, revision, candidate, author, false)
}

pub(super) fn social_accounts(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    author: &str,
) -> Result<Vec<DiscoveryContact>, String> {
    observations(tx, campaign, revision, candidate, author, true)
}

/// Reproject newly supported routes from saved native receipts without provider calls.
pub(super) fn recommendation_routes(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    author: &str,
    hit: &str,
) -> Result<Vec<DiscoveryContact>, String> {
    let mut routes = list(tx, campaign, revision, candidate, author)?;
    if eligible_author(tx, campaign, revision, candidate)?.as_deref() != Some(author) {
        return Ok(routes);
    }
    let Ok(basis) = super::discovery_review::load_contact_basis(tx, campaign, revision, hit) else {
        return Ok(routes);
    };
    let Ok(material) = crate::discovery_review::build_material(basis) else {
        return Ok(routes);
    };
    if material.candidate_id != candidate || material.native_author_id != author {
        return Err("contact_reprojection_author_mismatch".into());
    }
    if let Some(profile) = &material.native_profile {
        for bio in [
            profile.bio.as_deref(),
            profile.self_declared_location.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            routes.extend(crate::discovery_contacts::extract(
                bio,
                &profile.profile_url,
                author,
                profile.observed_at,
                DiscoveryContactAttribution::AuthorPublished,
            ));
        }
    }
    if matches!(
        material.author_platform(),
        "twitter"
            | "instagram"
            | "tiktok"
            | "douyin"
            | "linkedin"
            | "reddit"
            | "facebook"
            | "youtube"
    ) {
        let profile = material.native_profile.as_ref();
        let youtube_url = format!(
            "https://www.youtube.com/channel/{}",
            material.native_author_id
        );
        let url = profile
            .map(|p| p.profile_url.as_str())
            .or(material.hit.author_url.as_deref())
            .or((material.author_platform() == "youtube").then_some(youtube_url.as_str()));
        let captured = profile
            .map(|p| p.observed_at)
            .or(material.hit.author_observed_at)
            .or((material.author_platform() == "youtube").then_some(material.hit.captured_at));
        if let (Some(url), Some(captured_at)) = (url, captured)
            && crate::discovery_contacts::social_profile(url)
            && crate::discovery_contacts::contact_platform(url) == Some(material.author_platform())
        {
            routes.push(DiscoveryContact {
                kind: DiscoveryContactKind::SocialProfile,
                original_value: url.into(),
                normalized_value: url.into(),
                source_url: url.into(),
                source_excerpt: "Verified native author account".into(),
                captured_at,
                native_author_id: author.into(),
                attribution: DiscoveryContactAttribution::AuthorPublished,
            });
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    routes
        .retain(|route| seen.insert((format!("{:?}", route.kind), route.normalized_value.clone())));
    Ok(routes)
}

/// A confirmed hidden entry is a contact route, not a disclosed mailbox.
pub(super) fn has_hidden_email(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    author: &str,
) -> Result<bool, String> {
    tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_contact_observations o JOIN discovery_campaigns d ON d.id=o.discovery_campaign_id AND d.current_revision=o.criteria_revision WHERE o.candidate_id=?1 AND o.discovery_campaign_id=?2 AND o.criteria_revision=?3 AND o.native_author_id=?4 AND o.kind='hidden_email' AND o.attribution='author_published')",
        params![candidate, campaign, revision, author], |r| r.get(0),
    ).map_err(|e| e.to_string())
}

fn observations(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
    author: &str,
    social_only: bool,
) -> Result<Vec<DiscoveryContact>, String> {
    let mut statement = tx.prepare("SELECT kind,original_value,normalized_value,source_url,source_excerpt,captured_at,attribution
        FROM discovery_contact_observations WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND native_author_id=?4
        AND ((?5=1 AND kind='social_profile' AND attribution='author_published') OR (?5=0 AND (kind IN ('email','telegram') OR (kind IN ('hidden_email','social_profile','whatsapp','wechat','wechat_official') AND attribution='author_published'))))
        ORDER BY normalized_value,source_url LIMIT 100").map_err(|e| e.to_string())?;
    statement
        .query_map(
            params![campaign, revision, candidate, author, social_only],
            |row| {
                let kind: String = row.get(0)?;
                let attribution: String = row.get(6)?;
                Ok(DiscoveryContact {
                    kind: match kind.as_str() {
                        "email" => DiscoveryContactKind::Email,
                        "telegram" => DiscoveryContactKind::Telegram,
                        "hidden_email" => DiscoveryContactKind::HiddenEmail,
                        "social_profile" => DiscoveryContactKind::SocialProfile,
                        "whatsapp" => DiscoveryContactKind::Whatsapp,
                        "wechat" => DiscoveryContactKind::Wechat,
                        "wechat_official" => DiscoveryContactKind::WechatOfficial,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    },
                    original_value: row.get(1)?,
                    normalized_value: row.get(2)?,
                    source_url: row.get(3)?,
                    source_excerpt: row.get(4)?,
                    captured_at: row.get(5)?,
                    native_author_id: author.into(),
                    attribution: if attribution == "author_published" {
                        DiscoveryContactAttribution::AuthorPublished
                    } else {
                        DiscoveryContactAttribution::Unconfirmed
                    },
                })
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map(|contacts| {
            contacts
                .into_iter()
                .filter(|contact| {
                    contact.attribution == DiscoveryContactAttribution::AuthorPublished
                        || !crate::contact_navigation::non_contact(&contact.source_url)
                })
                .collect()
        })
        .map_err(|e| e.to_string())
}

/// Typed native adapter evidence, not a claim inferred from free page text.
pub(super) fn observe_hidden_email(
    tx: &Connection,
    campaign: &str,
    revision: u32,
    page: &crate::DiscoveryPublicPageRead,
) -> Result<(), String> {
    if page.status != "completed" || page.stop_code.is_some() {
        return Err("hidden_email_invalid_receipt".into());
    }
    let Some(author) = eligible_author(tx, campaign, revision, &page.candidate_id)? else {
        return Ok(());
    };
    let expected = format!("https://www.youtube.com/channel/{author}");
    if !author.starts_with("UC")
        || author.len() != 24
        || page.requested_url != expected
        || page.document_url.as_deref() != Some(expected.as_str())
        || author != page.platform_account_id
    {
        return Err("hidden_email_author_mismatch".into());
    }
    store(tx, campaign, revision, &page.candidate_id, vec![DiscoveryContact {
        kind: DiscoveryContactKind::HiddenEmail, original_value: "有隐藏邮箱，需手动查看".into(),
        normalized_value: expected.clone(), source_url: expected,
        source_excerpt: "YouTube native businessEmailLabel/businessEmailButton entry observed; address not disclosed".into(),
        captured_at: page.captured_at, native_author_id: author, attribution: DiscoveryContactAttribution::AuthorPublished,
    }])
}

#[cfg(test)]
mod fashion_contact_regressions {
    #[test]
    fn legacy_brand_mailboxes_do_not_enter_contact_results() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE discovery_contact_observations (candidate_id TEXT, discovery_campaign_id TEXT, criteria_revision INTEGER, native_author_id TEXT, kind TEXT, original_value TEXT, normalized_value TEXT, source_url TEXT, source_excerpt TEXT, captured_at INTEGER, attribution TEXT);
        INSERT INTO discovery_contact_observations VALUES ('candidate','campaign',1,'author','email','media@aritzia.com','media@aritzia.com','https://www.aritzia.com/us/en/contact-us','',1,'unconfirmed');
        INSERT INTO discovery_contact_observations VALUES ('candidate','campaign',1,'author','email','creator@example.com','creator@example.com','https://www.youtube.com/watch?v=example','',1,'author_published');").unwrap();
        let contacts = super::list(&db, "campaign", 1, "candidate", "author").unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].normalized_value, "creator@example.com");
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM discovery_contact_observations",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
    #[test]
    fn author_bound_social_and_hidden_email_are_recommendation_contact_routes() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE discovery_contact_observations (candidate_id TEXT, discovery_campaign_id TEXT, criteria_revision INTEGER, native_author_id TEXT, kind TEXT, original_value TEXT, normalized_value TEXT, source_url TEXT, source_excerpt TEXT, captured_at INTEGER, attribution TEXT);
        INSERT INTO discovery_contact_observations VALUES ('candidate','campaign',1,'author','social_profile','https://instagram.com/creator','https://instagram.com/creator','https://youtube.com/watch?v=example','',1,'author_published');
        INSERT INTO discovery_contact_observations VALUES ('candidate','campaign',1,'author','hidden_email','Email hidden','https://youtube.com/channel/author','https://youtube.com/channel/author','',1,'author_published');
        INSERT INTO discovery_contact_observations VALUES ('candidate','campaign',1,'author','telegram','https://t.me/creator_contact','https://t.me/creator_contact','https://youtube.com/watch?v=example','',1,'author_published');").unwrap();
        let contacts = super::list(&db, "campaign", 1, "candidate", "author").unwrap();
        assert_eq!(contacts.len(), 3);
        assert!(
            contacts
                .iter()
                .any(|c| c.normalized_value == "https://t.me/creator_contact")
        );
        let accounts = super::social_accounts(&db, "campaign", 1, "candidate", "author").unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(
            accounts[0].normalized_value,
            "https://instagram.com/creator"
        );
        assert!(
            super::social_accounts(&db, "campaign", 2, "candidate", "author")
                .unwrap()
                .is_empty()
        );
        assert!(
            super::social_accounts(&db, "campaign", 1, "candidate", "another_author")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM discovery_contact_observations",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            3
        );
        db.execute("UPDATE discovery_contact_observations SET attribution='unconfirmed' WHERE kind IN ('social_profile','hidden_email')", []).unwrap();
        assert_eq!(
            super::list(&db, "campaign", 1, "candidate", "author")
                .unwrap()
                .len(),
            1
        );
        assert!(
            super::social_accounts(&db, "campaign", 1, "candidate", "author")
                .unwrap()
                .is_empty()
        );
        assert!(
            super::list(&db, "campaign", 2, "candidate", "author")
                .unwrap()
                .is_empty()
        );
        assert!(
            super::list(&db, "campaign", 1, "candidate", "another_author")
                .unwrap()
                .is_empty()
        );
    }
}

/// Manual-only labels from an eligible author's native biography. Never a formal contact.
pub(super) fn manual_wechat(
    tx: &Connection,
    candidate: &crate::DiscoveryCandidate,
) -> Result<Vec<serde_json::Value>, String> {
    let current: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_campaigns WHERE id=?1 AND current_revision=?2)",
            params![candidate.campaign_id, candidate.criteria_revision],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !current
        || eligible_author(
            tx,
            &candidate.campaign_id,
            candidate.criteria_revision,
            &candidate.id,
        )?
        .as_deref()
            != Some(candidate.platform_account_id.as_str())
    {
        return Ok(vec![]);
    }
    let hit: Option<String> = tx.query_row("SELECT id FROM discovery_search_hits WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND source_url=?4",
        params![candidate.campaign_id,candidate.platform,candidate.source_profile_ref,candidate.evidence["source_urls"][0].as_str()], |r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let Some(hit) = hit else { return Ok(vec![]) };
    let material =
        crate::discovery_review::build_material(super::discovery_review::load_contact_basis(
            tx,
            &candidate.campaign_id,
            candidate.criteria_revision,
            &hit,
        )?)?;
    if material.native_author_id != candidate.platform_account_id
        || material.candidate_id != candidate.id
    {
        return Ok(vec![]);
    }
    let Some(profile) = material.native_profile else {
        return Ok(vec![]);
    };
    let Some(bio) = profile.bio else {
        return Ok(vec![]);
    };
    Ok(manual_wechat_labels(&bio).into_iter().map(|(kind,value)|serde_json::json!({"kind":kind,"value":value,"source_url":profile.profile_url,"native_author_id":material.native_author_id})).collect())
}

fn manual_wechat_labels(bio: &str) -> Vec<(&'static str, String)> {
    use std::sync::LazyLock;
    static PERSONAL: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"(?:个人微信|商务微信|合作微信|我的微信|微信号)\s*[:：]\s*([A-Za-z][A-Za-z0-9_-]{5,19})(?:[^A-Za-z0-9_-]|$)").unwrap()
    });
    static PUBLIC: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r#"(?:我的公众号|微信公众号|公众号)(?:叫|是|名称)?\s*[:：]?\s*[「“【]([^」”】\r\n<>]{1,40})[」”】]"#).unwrap()
    });
    let mut rows = Vec::new();
    for (kind, re) in [("wechat", &*PERSONAL), ("wechat_official", &*PUBLIC)] {
        for capture in re.captures_iter(bio) {
            let item = (kind, capture[1].trim().to_owned());
            if !rows.contains(&item) {
                rows.push(item)
            }
        }
    }
    rows.truncate(10);
    rows
}

#[cfg(test)]
mod manual_wechat_tests {
    use super::*;
    #[test]
    fn explicit_biography_labels_only() {
        assert_eq!(
            manual_wechat_labels(
                "公众号叫「一只马丁靴」，个人微信：DINGDINGCATHERINE读者分析请详见公众号"
            ),
            vec![
                ("wechat", "DINGDINGCATHERINE".into()),
                ("wechat_official", "一只马丁靴".into())
            ]
        );
        assert!(
            manual_wechat_labels("看过微信文章，公众号很多，其他作者 test123，https://example.com")
                .is_empty()
        );
        assert!(manual_wechat_labels("微信号：abcdefghijklmnopqrstuvwxyz").is_empty());
        assert_eq!(
            manual_wechat_labels("个人微信：abc123\n个人微信：abc123").len(),
            1
        );
    }
}
