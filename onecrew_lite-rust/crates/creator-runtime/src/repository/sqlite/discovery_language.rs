use super::{SqliteRepository, now};
use crate::{DiscoveryLanguageClaim, DiscoveryLanguageSample};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) fn samples(
    connection: &Connection,
    campaign: &str,
    revision: u32,
    candidate: &str,
) -> Result<Vec<DiscoveryLanguageSample>, String> {
    let mut query = connection.prepare("SELECT sample_json,sample_format_version FROM discovery_language_samples WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND status='completed' ORDER BY ordinal").map_err(|e| e.to_string())?;
    query
        .query_map(params![campaign, revision, candidate], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))
        })
        .map_err(|e| e.to_string())?
        .map(|row| {
            let (json, format) = row.map_err(|e| e.to_string())?;
            decode_sample(&json, format)
        })
        .collect()
}

// Frozen v1 receipts remain byte-identical on disk. Their commenter identities
// are discarded only when assembling current language evidence.
fn decode_sample(json: &str, format: u32) -> Result<DiscoveryLanguageSample, String> {
    let mut value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    match format {
        1 => {
            let comments = value
                .get_mut("comments")
                .and_then(serde_json::Value::as_array_mut)
                .ok_or("language_sample_invalid")?;
            for comment in comments {
                let object = comment.as_object_mut().ok_or("language_sample_invalid")?;
                if object.remove("author_id").is_none_or(|id| !id.is_string()) {
                    return Err("language_sample_invalid".into());
                }
            }
        }
        2 => {}
        _ => return Err("language_sample_format_unsupported".into()),
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

// A language finding requests comments only after its audited material includes
// the same completed original read. Profile-only or stale reviews cannot spend
// a comments call before original text has actually been evaluated.
fn eligible(connection: &Connection, campaign: &str, hit: &str) -> Result<bool, String> {
    connection.query_row("SELECT EXISTS(
      SELECT 1 FROM discovery_candidates c JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
      JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=d.id AND v.revision=d.current_revision
      JOIN discovery_evidence_reviews review ON review.id=json_extract(c.evidence_json,'$.reviewed_match.review_id') AND review.candidate_id=c.id AND review.criteria_revision=d.current_revision
        AND review.discovery_campaign_id=d.id AND review.search_hit_id=h.id
        AND review.policy_version='initial-match-evaluation-v1'
        AND review.material_hash=json_extract(c.evidence_json,'$.reviewed_match.material_hash')
        AND review.native_author_id=json_extract(c.evidence_json,'$.reviewed_match.native_author_id')
      WHERE d.id=?1 AND h.id=?2 AND h.platform IN ('twitter','youtube','bilibili','tiktok','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','linkedin','douyin','google','yahoo') AND c.verdict='pending'
        AND c.criteria_revision=d.current_revision AND d.phase='active' AND v.approved_at IS NOT NULL
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=d.id AND busy.status='searching')
        AND json_extract(v.criteria_json,'$.language') IS NOT NULL
        AND json_extract(c.evidence_json,'$.reviewed_match.outcome')='insufficient'
        AND (
          (h.platform IN ('google','yahoo') AND c.platform IN ('youtube','twitter','douyin') AND c.source_platform=h.platform
            AND c.source_profile_ref=h.profile_ref
            AND c.platform_account_id=review.native_author_id
            AND json_extract(review.material_json,'$.native_author_id')=review.native_author_id
            AND json_extract(review.material_json,'$.native_platform')=c.platform
            AND json_extract(review.material_json,'$.hit.platform')=h.platform
            AND json_extract(review.material_json,'$.hit.profile_ref')=h.profile_ref
            AND json_extract(review.material_json,'$.hit.source_url')=h.source_url
            AND EXISTS(SELECT 1 FROM discovery_web_source_reads web
              JOIN discovery_video_identity_bindings binding ON binding.search_hit_id=web.search_hit_id
              WHERE web.search_hit_id=h.id AND web.discovery_campaign_id=d.id
                AND web.expected_revision=d.current_revision AND web.origin_platform=h.platform
                AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
                AND web.target_platform=c.platform AND web.status='completed' AND web.stop_code IS NULL
                AND web.native_author_id=review.native_author_id
                AND binding.discovery_campaign_id=d.id AND binding.criteria_revision=d.current_revision
                AND binding.candidate_id=c.id AND binding.native_author_id=web.native_author_id
                AND json_extract(web.native_material_json,CASE WHEN web.target_platform IN ('twitter','douyin') THEN '$.content.text' ELSE '$.description' END)=json_extract(review.material_json,'$.video_description'))
            AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads stopped
              WHERE stopped.discovery_campaign_id=d.id AND stopped.profile_ref=h.profile_ref
                AND stopped.target_platform=c.platform AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control'))
            AND NOT EXISTS(SELECT 1 FROM discovery_language_samples stopped
              JOIN discovery_search_hits sh ON sh.id=stopped.search_hit_id
              JOIN discovery_candidates sc ON sc.id=stopped.candidate_id
              WHERE stopped.discovery_campaign_id=d.id AND sh.profile_ref=h.profile_ref AND sc.platform=c.platform
                AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control')))
          OR
          (h.platform='tiktok'
            AND json_extract(review.material_json,'$.hit.title')=h.title
            AND json_extract(review.material_json,'$.hit.source_url')=h.source_url
            AND json_extract(review.material_json,'$.hit.profile_ref')=h.profile_ref
            AND EXISTS(SELECT 1 FROM discovery_search_authors native
              WHERE native.search_hit_id=h.id AND native.author_id=review.native_author_id
                AND json_extract(review.material_json,'$.hit.author_id')=native.author_id
                AND json_extract(review.material_json,'$.hit.author_url')=native.author_url))
          OR
          (h.platform='youtube'
            AND json_type(review.material_json,'$.video_description')='text'
            AND json_extract(review.material_json,'$.video_description')=json_extract(c.evidence_json,'$.video_description')
            AND (
              (json_type(review.material_json,'$.transcript') IS NULL
                OR json_type(review.material_json,'$.transcript')='null')
              AND NOT EXISTS(SELECT 1 FROM discovery_transcript_reads transcript
                WHERE transcript.search_hit_id=h.id AND transcript.candidate_id=c.id)
              OR EXISTS(SELECT 1 FROM discovery_transcript_reads transcript
                WHERE transcript.search_hit_id=h.id AND transcript.candidate_id=c.id
                  AND json_extract(review.material_json,'$.transcript.search_hit_id')=h.id
                  AND json_extract(review.material_json,'$.transcript.candidate_id')=c.id
                  AND json_extract(review.material_json,'$.transcript.source_url')=h.source_url
                  AND json_extract(review.material_json,'$.transcript.profile_ref')=h.profile_ref
                  AND json_extract(review.material_json,'$.transcript.platform_account_id')=review.native_author_id
                  AND json_extract(review.material_json,'$.transcript.status')=transcript.status
                  AND json_extract(review.material_json,'$.transcript.segments')=json(transcript.segments_json)
                  AND json_extract(review.material_json,'$.transcript.truncated')=transcript.truncated
                  AND json_extract(review.material_json,'$.transcript.stop_code') IS transcript.stop_code
                  AND json_extract(review.material_json,'$.transcript.call_number')=transcript.call_number
                  AND json_extract(review.material_json,'$.transcript.captured_at')=transcript.updated_at)
            )
            AND EXISTS(SELECT 1 FROM discovery_video_completions completed
              JOIN discovery_video_identity_bindings binding ON binding.search_hit_id=completed.search_hit_id
              WHERE completed.search_hit_id=h.id AND binding.discovery_campaign_id=d.id
                AND binding.criteria_revision=d.current_revision AND binding.candidate_id=c.id
                AND binding.native_author_id=review.native_author_id))
          OR EXISTS(SELECT 1 FROM discovery_content_reads original
            WHERE original.search_hit_id=h.id AND original.discovery_campaign_id=d.id
              AND original.expected_revision=d.current_revision AND original.platform=h.platform
              AND original.profile_ref=h.profile_ref AND original.status='completed' AND original.stop_code IS NULL
              AND json_extract(review.material_json,'$.native_content.status')='completed'
              AND json_extract(review.material_json,'$.native_content.stop_code') IS NULL
              AND json_extract(review.material_json,'$.native_content.claim.search_hit_id')=h.id
              AND json_extract(review.material_json,'$.native_content.claim.campaign_id')=d.id
              AND json_extract(review.material_json,'$.native_content.claim.expected_revision')=original.expected_revision
              AND json_extract(review.material_json,'$.native_content.claim.platform')=original.platform
              AND json_extract(review.material_json,'$.native_content.claim.profile_ref')=original.profile_ref
              AND json_extract(review.material_json,'$.native_content.claim.author_id')=original.author_id
              AND json_extract(review.material_json,'$.native_content.claim.source_url')=original.source_url
              AND json_extract(review.material_json,'$.native_content.claim.call_number')=original.call_number
              AND json_extract(review.material_json,'$.native_content.native_content')=json(original.native_content_json))
        )
        AND (d.max_calls IS NULL OR d.used_calls + 1 <= d.max_calls)
        AND EXISTS(SELECT 1 FROM json_each(review.findings_json) f WHERE json_extract(f.value,'$.criterion')='content_language' AND json_extract(f.value,'$.outcome')='insufficient')
        AND coalesce(json_array_length(review.material_json,'$.language_samples'),0) =
            (SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision AND s.status='completed')
        AND (SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision)<1
        AND (h.platform NOT IN ('instagram','weibo','bluesky','zhihu','hackernews') OR NOT EXISTS(SELECT 1 FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision))
        AND (h.platform!='facebook' OR EXISTS(SELECT 1 FROM discovery_facebook_content_targets t
          WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=d.id AND t.expected_revision=d.current_revision
            AND t.status='completed' AND t.stop_code IS NULL
            AND json_array_length(t.source_urls_json)>(SELECT COUNT(*) FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision)))
        AND NOT EXISTS(SELECT 1 FROM discovery_language_samples s WHERE s.candidate_id=c.id AND s.criteria_revision=d.current_revision AND s.status!='completed')
        AND EXISTS(SELECT 1 FROM discovery_platform_runs p WHERE p.discovery_campaign_id=d.id AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.status='completed')
    )", params![campaign, hit], |r| r.get(0)).map_err(|e| e.to_string())
}

pub(super) fn next_hit_excluding(
    connection: &Connection,
    campaign: &str,
    platform: &str,
    held_hits: &[String],
) -> Result<Option<String>, String> {
    if !matches!(
        platform,
        "twitter"
            | "youtube"
            | "bilibili"
            | "tiktok"
            | "reddit"
            | "facebook"
            | "instagram"
            | "weibo"
            | "bluesky"
            | "zhihu"
            | "hackernews"
            | "linkedin"
            | "douyin"
            | "google"
            | "yahoo"
    ) {
        return Ok(None);
    }
    let mut query = connection.prepare("SELECT h.id FROM discovery_search_hits h JOIN discovery_candidates c ON c.discovery_campaign_id=h.discovery_campaign_id AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]') WHERE h.discovery_campaign_id=?1 AND h.platform=?2 AND c.verdict='pending' ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id").map_err(|e| e.to_string())?;
    for hit in query
        .query_map(params![campaign, platform], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
    {
        let hit = hit.map_err(|e| e.to_string())?;
        if !held_hits.contains(&hit) && eligible(connection, campaign, &hit)? {
            return Ok(Some(hit));
        }
    }
    Ok(None)
}

pub(super) fn claim(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<Option<DiscoveryLanguageClaim>, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    super::discovery_read_health::check(&tx, campaign, "content", hit)?;
    if !eligible(&tx, campaign, hit)? {
        return Ok(None);
    }
    let basis = super::discovery_review::load_basis(&tx, campaign, revision, hit)?;
    let previous = samples(&tx, campaign, revision, &basis.candidate_id)?;
    let ordinal = previous.len() as u32 + 1;
    // One matched post, one bounded comments read. Historical extra samples
    // remain saved, but never authorize a new post or another reservation.
    let cost = 1;
    let used: Option<u32> = tx.query_row("UPDATE discovery_campaigns SET used_calls=used_calls+?3,updated_at=?4 WHERE id=?1 AND current_revision=?2 AND phase='active' AND (max_calls IS NULL OR used_calls+?3<=max_calls) RETURNING used_calls", params![campaign, revision, cost, now()], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(call_number) = used else {
        return Ok(None);
    };
    let saved_content_urls = if basis.hit.platform == "facebook" {
        super::discovery_facebook::load(&tx, campaign, hit)?
            .source_urls
            .ok_or("language_author_unavailable")?
    } else {
        Vec::new()
    };
    if basis.hit.platform == "facebook"
        && !saved_content_urls
            .iter()
            .any(|url| !previous.iter().any(|s| &s.source_url == url))
    {
        return Ok(None);
    }
    let (author_handle, root_url) = if basis.hit.platform == "linkedin" {
        let handle = basis
            .hit
            .author_url
            .as_deref()
            .and_then(|url| url.strip_prefix("https://www.linkedin.com/in/"))
            .and_then(|path| path.strip_suffix('/'))
            .filter(|handle| crate::discovery_linkedin::valid_handle(handle))
            .ok_or("language_author_unavailable")?;
        let source = basis
            .native_content
            .as_ref()
            .and_then(|read| read.observed_source_url())
            .filter(|source| crate::discovery_linkedin::activity_id(source).is_some())
            .ok_or("language_author_unavailable")?;
        (handle.to_owned(), source.to_owned())
    } else if basis.hit.platform == "instagram" {
        let handle = basis
            .hit
            .author_url
            .as_deref()
            .and_then(|s| s.strip_prefix("https://www.instagram.com/"))
            .and_then(|s| s.strip_suffix('/'))
            .ok_or("language_author_unavailable")?;
        let source = basis
            .native_content
            .as_ref()
            .and_then(|read| read.observed_source_url())
            .ok_or("language_author_unavailable")?;
        (handle.to_owned(), source.to_owned())
    } else if basis.hit.platform == "facebook" {
        (
            basis.native_author_id.clone(),
            basis
                .native_content
                .as_ref()
                .ok_or("language_author_unavailable")?
                .claim
                .source_url
                .clone(),
        )
    } else if basis.hit.platform == "reddit" {
        let username = basis
            .hit
            .author_url
            .as_deref()
            .and_then(|s| s.strip_prefix("https://www.reddit.com/user/"))
            .and_then(|s| s.strip_suffix('/'))
            .ok_or("language_author_unavailable")?;
        (username.to_owned(), basis.hit.source_url.clone())
    } else if basis.hit.platform == "tiktok" {
        (basis.hit.author_label.clone(), basis.hit.source_url.clone())
    } else if matches!(basis.hit.platform.as_str(), "google" | "yahoo") {
        let read = super::discovery_web_sources::get_source_row(&tx, campaign, hit)?;
        if !read.is_primary_source
            || read.status != "completed"
            || read.stop_code.is_some()
            || read.claim.expected_revision != revision
            || read.claim.profile_ref != basis.hit.profile_ref
            || !matches!(
                read.claim.target_platform.as_str(),
                "youtube" | "twitter" | "douyin"
            )
            || read.native_author_id.as_deref() != Some(&basis.native_author_id)
        {
            return Err("language_native_content_binding_invalid".into());
        }
        if read.claim.target_platform == "twitter" {
            (
                crate::web_source_twitter_target(&read.claim.source_url)
                    .ok_or("language_native_content_binding_invalid")?
                    .0,
                read.claim.source_url,
            )
        } else if read.claim.target_platform == "douyin" {
            (basis.native_author_id.clone(), read.claim.source_url)
        } else {
            (
                basis.native_author_id.clone(),
                format!("https://www.youtube.com/watch?v={}", read.claim.video_id),
            )
        }
    } else if matches!(
        basis.hit.platform.as_str(),
        "youtube" | "bilibili" | "weibo" | "bluesky" | "zhihu" | "hackernews" | "douyin"
    ) {
        (basis.native_author_id.clone(), basis.hit.source_url.clone())
    } else {
        let identity = basis
            .native_content
            .as_ref()
            .and_then(|r| r.native_content.as_ref())
            .ok_or("language_author_unavailable")?;
        let author_handle = identity
            .pointer("/identity/handle")
            .and_then(serde_json::Value::as_str)
            .ok_or("language_author_unavailable")?
            .to_owned();
        let root_url = identity
            .pointer("/identity/source_url")
            .and_then(serde_json::Value::as_str)
            .ok_or("language_author_unavailable")?
            .to_owned();
        (author_handle, root_url)
    };
    let claim = DiscoveryLanguageClaim {
        campaign_id: campaign.into(),
        revision,
        search_hit_id: hit.into(),
        candidate_id: basis.candidate_id,
        platform: basis.hit.platform,
        profile_ref: basis.hit.profile_ref,
        author_id: basis.native_author_id,
        author_handle,
        root_url,
        ordinal,
        call_number,
        previous_urls: previous.into_iter().map(|s| s.source_url).collect(),
        saved_content_urls,
    };
    tx.execute(
        "INSERT INTO discovery_language_samples(discovery_campaign_id,criteria_revision,candidate_id,search_hit_id,ordinal,call_number,status,sample_json,stop_code,created_at,sample_format_version) VALUES(?1,?2,?3,?4,?5,?6,'reading',NULL,NULL,?7,2)",
        params![
            campaign,
            revision,
            claim.candidate_id,
            hit,
            ordinal,
            call_number,
            now()
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?4,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3", params![campaign, claim.platform, claim.profile_ref, format!("language:{hit}:{call_number}"), now()]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(Some(claim))
}

pub(super) fn finish(
    repository: &SqliteRepository,
    claim: DiscoveryLanguageClaim,
    sample: Option<DiscoveryLanguageSample>,
    stop: Option<String>,
) -> Result<(), String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let engine_source = matches!(claim.platform.as_str(), "google" | "yahoo");
    if engine_source {
        let read = super::discovery_web_sources::get_source_row(
            &tx,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        let original = read
            .native_material
            .as_ref()
            .and_then(|native| {
                if matches!(read.claim.target_platform.as_str(), "twitter" | "douyin") {
                    native
                        .pointer("/content/text")
                        .and_then(serde_json::Value::as_str)
                } else {
                    native["description"].as_str()
                }
            })
            .ok_or("language_native_content_unavailable")?;
        let end = original
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(original.len()))
            .take_while(|index| {
                *index
                    <= if read.claim.target_platform == "douyin" {
                        12000
                    } else {
                        8000
                    }
            })
            .last()
            .unwrap_or(0);
        if read.status != "completed"
            || read.stop_code.is_some()
            || !read.is_primary_source
            || read.claim.expected_revision != claim.revision
            || read.claim.origin_platform != claim.platform
            || read.claim.profile_ref != claim.profile_ref
            || !matches!(
                read.claim.target_platform.as_str(),
                "youtube" | "twitter" | "douyin"
            )
            || read.native_author_id.as_deref() != Some(claim.author_id.as_str())
            || read.candidate_id.as_deref() != Some(claim.candidate_id.as_str())
            || (matches!(read.claim.target_platform.as_str(), "youtube" | "douyin")
                && claim.author_handle != claim.author_id)
            || (read.claim.target_platform == "twitter"
                && crate::web_source_twitter_target(&read.claim.source_url)
                    .is_none_or(|(handle, _)| handle != claim.author_handle))
            || read.claim.call_number >= claim.call_number
            || claim.root_url
                != if matches!(read.claim.target_platform.as_str(), "twitter" | "douyin") {
                    read.claim.source_url.clone()
                } else {
                    format!("https://www.youtube.com/watch?v={}", read.claim.video_id)
                }
            || sample
                .as_ref()
                .is_some_and(|sample| sample.text != original[..end] || !sample.valid_for(&claim))
        {
            return Err("language_native_content_binding_invalid".into());
        }
    }
    if claim.platform == "douyin"
        && let Some(sample) = &sample
    {
        // Rebuild the joint source-author basis, rather than accepting a
        // caller's profile or caption. Its profile/content share one receipt.
        let basis = super::discovery_review::load_basis(
            &tx,
            &claim.campaign_id,
            claim.revision,
            &claim.search_hit_id,
        )?;
        let body = basis
            .native_content
            .as_ref()
            .ok_or("language_native_content_unavailable")?;
        let native = body
            .native_content
            .as_ref()
            .ok_or("language_native_content_unavailable")?;
        if basis.candidate_id != claim.candidate_id
            || basis.native_author_id != claim.author_id
            || claim.author_handle != claim.author_id
            || basis.hit.profile_ref != claim.profile_ref
            || basis.hit.platform != claim.platform
            || basis.hit.source_url != claim.root_url
            || body.claim.call_number >= claim.call_number
            || body.observed_source_url() != Some(claim.root_url.as_str())
            || native["text"].as_str() != Some(sample.text.as_str())
            || !sample.valid_for(&claim)
        {
            return Err("language_native_content_binding_invalid".into());
        }
    }
    if claim.platform == "linkedin"
        && let Some(sample) = &sample
    {
        let body = super::discovery_content::load_content_row(
            &tx,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        let profile = super::discovery_profiles::load_profile_row(
            &tx,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        let native = body
            .native_content
            .as_ref()
            .ok_or("language_native_content_unavailable")?;
        let identity = profile
            .native_profile
            .as_ref()
            .ok_or("language_native_profile_unavailable")?;
        let canonical = format!("https://www.linkedin.com/in/{}/", claim.author_handle);
        if body.status != "completed"
            || body.stop_code.is_some()
            || profile.status != "completed"
            || profile.stop_code.is_some()
            || body.claim.expected_revision != claim.revision
            || profile.claim.expected_revision != claim.revision
            || body.claim.platform != claim.platform
            || profile.claim.platform != claim.platform
            || body.claim.profile_ref != claim.profile_ref
            || profile.claim.profile_ref != claim.profile_ref
            || body.claim.author_id != claim.author_handle
            || profile.claim.author_id != claim.author_handle
            || body.claim.source_url != canonical
            || profile.claim.source_url != canonical
            || profile.claim.call_number >= body.claim.call_number
            || body.claim.call_number >= claim.call_number
            || body.observed_source_url() != Some(claim.root_url.as_str())
            || native["account_id"].as_str() != Some(claim.author_id.as_str())
            || identity["account_id"].as_str() != Some(claim.author_id.as_str())
            || native["text"].as_str() != Some(sample.text.as_str())
            || !sample.valid_for(&claim)
        {
            return Err("language_native_content_binding_invalid".into());
        }
    }
    if claim.platform == "hackernews"
        && let Some(sample) = &sample
    {
        let saved = super::discovery_content::load_content_row(
            &tx,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        let native = saved
            .native_content
            .as_ref()
            .ok_or("language_native_content_unavailable")?;
        let ids = native["comment_ids"]
            .as_array()
            .ok_or("language_native_content_unavailable")?;
        if saved.status != "completed"
            || saved.stop_code.is_some()
            || saved.claim.expected_revision != claim.revision
            || saved.claim.platform != claim.platform
            || saved.claim.profile_ref != claim.profile_ref
            || saved.claim.author_id != claim.author_id
            || saved.claim.source_url != sample.source_url
            || saved.claim.call_number >= claim.call_number
            || native["text"].as_str() != Some(sample.text.as_str())
            || ids.len() != sample.comments.len()
            || !ids.iter().zip(&sample.comments).all(|(id, comment)| {
                id.as_u64() == crate::discovery_hackernews::item_id(&comment.source_url)
            })
        {
            return Err("language_native_content_binding_invalid".into());
        }
    }
    let changed = tx.execute("UPDATE discovery_language_samples SET status=?7,sample_json=?8,stop_code=?9 WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3 AND search_hit_id=?4 AND ordinal=?5 AND call_number=?6 AND status='reading' AND EXISTS(SELECT 1 FROM discovery_campaigns c WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active')",
        params![claim.campaign_id, claim.revision, claim.candidate_id, claim.search_hit_id, claim.ordinal, claim.call_number, if sample.is_some() {"completed"} else {"failed"}, sample.map(|s| serde_json::to_string(&s)).transpose().map_err(|e| e.to_string())?, stop]).map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("language_claim_changed".into());
    }
    // The approved engine remains usable when its native destination is blocked.
    // Preserve the failed sample as the target/browser quarantine, without
    // manufacturing a native platform run that was never approved or searched.
    let target_blocked = engine_source
        && matches!(
            stop.as_deref(),
            Some("opencli_auth_required" | "opencli_risk_control")
        );
    if target_blocked {
        tx.execute("UPDATE discovery_platform_runs SET status=?4,stop_code=?3,active_query=NULL,updated_at=?5
          WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2 AND status!='searching'",
          params![claim.campaign_id, claim.profile_ref, stop, crate::discovery::read_platform_status(stop.as_deref()), now()])
          .map_err(|e|e.to_string())?;
    }
    let platform_status = if target_blocked {
        "completed"
    } else {
        crate::discovery::read_platform_status(stop.as_deref())
    };
    let run_stop = stop.as_deref().filter(|code| {
        !target_blocked
            && *code != "language_sources_exhausted"
            && !crate::discovery::source_local_read_failure(Some(code))
    });
    let changed = tx.execute("UPDATE discovery_platform_runs SET status=?6,stop_code=?7,active_query=NULL,updated_at=?5 WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4", params![claim.campaign_id, claim.platform, claim.profile_ref, format!("language:{}:{}", claim.search_hit_id, claim.call_number), now(), platform_status, run_stop]).map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("language_claim_changed".into());
    }
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn historical_ids_are_not_needed_in_current_evidence() {
        let old = r#"{"author_id":"42","source_url":"post","text":"original","comments":[{"author_id":"51","source_url":"comment","text":"中文评论"}]}"#;
        let sample = decode_sample(old, 1).unwrap();
        let current = serde_json::to_value(&sample).unwrap();
        assert_eq!(current["author_id"], "42");
        assert!(current["comments"][0].get("author_id").is_none());
        assert_eq!(decode_sample(&current.to_string(), 2).unwrap(), sample);
        assert!(decode_sample(old, 2).is_err());
        assert!(decode_sample(&current.to_string(), 1).is_err());
        assert!(decode_sample(old, 3).is_err());
    }
}
