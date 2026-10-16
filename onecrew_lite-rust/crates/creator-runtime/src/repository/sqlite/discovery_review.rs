use super::SqliteRepository;
use crate::{
    DiscoveryPublicPageRead, DiscoveryReviewBasis, DiscoveryReviewSubmissionBasis,
    DiscoverySearchHit, DiscoveryTranscriptRead, PreparedDiscoveryReview, StoredDiscoveryReview,
    discovery_review::{DiscoveryNativeProfileObservation, MAX_REVIEW_BYTES, MAX_REVIEW_PAGES},
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde_json::Value;

fn bounded_profile_text(profile: &Value, key: &str, max_chars: usize) -> Option<String> {
    profile[key]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.chars().count() <= max_chars)
        .map(str::to_owned)
}

/// Only a persisted, source-bound supported review can carry a proof into a
/// later candidate update or review. Old or malformed fields fail closed.
pub(super) fn strip_unaudited_proofs(
    connection: &rusqlite::Connection,
    evidence: &mut Value,
    candidate_id: &str,
    campaign_id: &str,
    revision: u32,
    native_author_id: &str,
) -> Result<(), String> {
    let object = evidence.as_object_mut().ok_or("review_evidence_invalid")?;
    for (key, criterion) in [
        ("reviewed_apparel_content", "apparel_business_content"),
        ("reviewed_category_content", "category_content"),
        ("reviewed_residence", "creator_residence_outside_mainland"),
    ] {
        let Some(proof) = object.get(key) else {
            continue;
        };
        let source = proof["source_url"].as_str().unwrap_or_default();
        let code = proof["country_code"].as_str();
        let locally_valid = proof["verified"] == true
            && proof["native_author_id"].as_str() == Some(native_author_id)
            && (if key == "reviewed_category_content" {
                proof["policy_version"] == "category-content-review-v1"
                    && proof["category"]
                        .as_str()
                        .is_some_and(|category| !category.is_empty() && category != "服装")
            } else {
                proof["policy_version"] == "apparel-residence-review-v1"
                    || proof["policy_version"] == "apparel-native-review-v2"
            })
            && object["source_urls"]
                .as_array()
                .is_some_and(|urls| urls.iter().any(|url| url.as_str() == Some(source)))
            && (key != "reviewed_residence"
                || code.is_some_and(|value| {
                    value.len() == 2
                        && value != "CN"
                        && value.bytes().all(|byte| byte.is_ascii_uppercase())
                }));
        let audited = if locally_valid {
            connection
                .query_row(
                    "SELECT EXISTS(
                      SELECT 1 FROM discovery_evidence_reviews r
                      JOIN discovery_search_hits h ON h.id=r.search_hit_id
                      JOIN json_each(r.findings_json) f
                      WHERE r.id=?1 AND r.candidate_id=?2 AND r.discovery_campaign_id=?3
                        AND r.criteria_revision=?4 AND r.native_author_id=?5
                        AND r.material_hash=?6 AND h.source_url=?7
                        AND ((r.policy_version='apparel-residence-review-v1'
                              AND json_extract(r.material_json,'$.format_version')=1
                              AND h.platform='youtube')
                          OR (r.policy_version='apparel-native-review-v2'
                              AND json_extract(r.material_json,'$.format_version')=2
                              AND h.platform IN ('twitter','tiktok'))
                          OR (r.policy_version IN ('category-content-review-v1','language-match-review-v1')
                              AND ((json_extract(r.material_json,'$.format_version')=1
                                    AND h.platform='youtube')
                                OR (json_extract(r.material_json,'$.format_version')=2
                                    AND h.platform IN ('twitter','tiktok'))
                                OR (json_extract(r.material_json,'$.format_version')=2
                                    AND h.platform='bilibili'))))
                        AND json_extract(f.value,'$.criterion')=?8
                        AND json_extract(f.value,'$.outcome')='supported'
                        AND (?8!='creator_residence_outside_mainland'
                             OR json_extract(f.value,'$.country_code')=?9)
                        AND (?8!='category_content'
                             OR json_extract(r.material_json,'$.criteria.category')=?10)
                    )",
                    params![
                        proof["review_id"].as_str().unwrap_or_default(),
                        candidate_id,
                        campaign_id,
                        revision,
                        native_author_id,
                        proof["material_hash"].as_str().unwrap_or_default(),
                        source,
                        criterion,
                        code,
                        proof["category"].as_str(),
                    ],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(|error| error.to_string())?
        } else {
            false
        };
        if !audited {
            object.remove(key);
        }
    }
    for (key, criterion) in [
        ("reviewed_language", "content_language"),
        ("reviewed_match", "match_fit"),
    ] {
        let Some(proof) = object.get(key) else {
            continue;
        };
        let source = proof["source_url"].as_str().unwrap_or_default();
        let locally_valid = proof["verified"] == true
            && (proof["policy_version"] == "language-match-review-v1"
                || (key == "reviewed_match"
                    && proof["policy_version"] == "initial-match-evaluation-v1"))
            && proof["native_author_id"].as_str() == Some(native_author_id)
            && object["source_urls"]
                .as_array()
                .is_some_and(|urls| urls.iter().any(|url| url.as_str() == Some(source)));
        let audited = locally_valid && connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_evidence_reviews r
             JOIN discovery_search_hits h ON h.id=r.search_hit_id JOIN json_each(r.findings_json) f
             WHERE r.id=?1 AND r.candidate_id=?2 AND r.discovery_campaign_id=?3
             AND r.criteria_revision=?4 AND r.native_author_id=?5 AND r.material_hash=?6
             AND h.source_url=?7 AND r.policy_version=?13
             AND ((h.platform='youtube' AND json_extract(r.material_json,'$.format_version')=1)
               OR (h.platform IN ('twitter','tiktok','bilibili','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin','douyin') AND json_extract(r.material_json,'$.format_version')=2)
               OR (h.platform IN ('google','yahoo') AND json_extract(r.material_json,'$.format_version')=2
                 AND json_extract(r.material_json,'$.native_platform') IN ('youtube','twitter','douyin') AND EXISTS(
                   SELECT 1 FROM discovery_web_source_reads web WHERE web.search_hit_id=h.id
                     AND web.discovery_campaign_id=r.discovery_campaign_id AND web.expected_revision=r.criteria_revision
                     AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref AND web.source_url=h.source_url
                     AND web.status='completed' AND web.stop_code IS NULL AND web.target_platform=json_extract(r.material_json,'$.native_platform')
                     AND web.native_author_id=r.native_author_id
                     AND json_extract(web.native_material_json,CASE WHEN web.target_platform IN ('twitter','douyin') THEN '$.content.text' ELSE '$.description' END)=json_extract(r.material_json,'$.video_description'))))
             AND json_extract(f.value,'$.criterion')=?8
             AND json_extract(f.value,'$.outcome')=?9
             AND json_extract(f.value,'$.rationale')=?10
             AND (?8!='content_language' OR json_extract(r.material_json,'$.criteria.language')=?11)
             AND (?8!='match_fit' OR (json_extract(f.value,'$.outcome')='supported' AND json_extract(f.value,'$.match_level')=?12)
               OR (r.policy_version='initial-match-evaluation-v1' AND json_extract(f.value,'$.outcome')='insufficient'
                   AND json_extract(f.value,'$.match_level') IS NULL AND ?12 IS NULL)))",
            params![proof["review_id"].as_str().unwrap_or_default(), candidate_id, campaign_id, revision,
                native_author_id, proof["material_hash"].as_str().unwrap_or_default(), source, criterion,
                proof["outcome"].as_str(), proof["reason"].as_str(), proof["language"].as_str(), proof["level"].as_str(), proof["policy_version"].as_str()],
            |row| row.get::<_, bool>(0)).map_err(|error| error.to_string())?;
        if !audited {
            object.remove(key);
        }
    }
    if let Some(proofs) = object.get("reviewed_requirements").cloned() {
        let mut verified = Vec::new();
        if let Some(proofs) = proofs.as_array() {
            for proof in proofs {
                let audited = proof["native_author_id"].as_str() == Some(native_author_id)
                    && proof["criteria_revision"].as_u64() == Some(u64::from(revision))
                    && connection.query_row(
                        "SELECT EXISTS(SELECT 1 FROM discovery_evidence_reviews r
                         JOIN discovery_search_hits h ON h.id=r.search_hit_id
                         JOIN json_each(r.findings_json) f
                         WHERE r.id=?1 AND r.candidate_id=?2 AND r.discovery_campaign_id=?3
                           AND r.criteria_revision=?4 AND r.native_author_id=?5
                           AND r.material_hash=?6 AND h.source_url=?7
                           AND json_extract(f.value,'$.criterion.requirement')=?8
                           AND json_extract(f.value,'$.outcome')=?9
                           AND json_extract(f.value,'$.rationale')=?10
                           AND json_extract(r.material_json,'$.criteria.requirements[' || ?8 || ']')=json(?11))",
                        params![proof["review_id"].as_str(), candidate_id, campaign_id,
                            revision, native_author_id, proof["material_hash"].as_str(),
                            proof["source_url"].as_str(), proof["index"].as_u64(),
                            proof["outcome"].as_str(), proof["reason"].as_str(),
                            proof["requirement"].to_string()],
                        |row| row.get::<_, bool>(0),
                    ).map_err(|error| error.to_string())?;
                verified.push(if audited { proof.clone() } else { Value::Null });
            }
        }
        object.insert("reviewed_requirements".into(), Value::Array(verified));
    }
    Ok(())
}

pub(super) fn load_basis(
    tx: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryReviewBasis, String> {
    load_basis_for(tx, campaign, revision, hit, false)
}

/// Contact navigation consumes bounded complete receipts; accumulated contact
/// reads must not turn the already audited score into an oversized review.
pub(super) fn load_contact_basis(
    tx: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryReviewBasis, String> {
    load_basis_for(tx, campaign, revision, hit, true)
}

fn load_basis_for(
    tx: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
    hit: &str,
    contact_navigation: bool,
) -> Result<DiscoveryReviewBasis, String> {
    if campaign.is_empty()
        || campaign.len() > 128
        || hit.is_empty()
        || hit.len() > 128
        || revision == 0
    {
        return Err("review_material_input_invalid".into());
    }
    let platform: String = tx
        .query_row(
            "SELECT platform FROM discovery_search_hits WHERE discovery_campaign_id=?1 AND id=?2",
            params![campaign, hit],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or("review_source_unavailable")?;
    let (source_join, candidate_join) = match platform.as_str() {
        "google" | "yahoo" => (
            "JOIN discovery_web_source_reads web ON web.search_hit_id=h.id AND web.discovery_campaign_id=d.id
              AND web.expected_revision=d.current_revision AND web.origin_platform=h.platform AND web.profile_ref=h.profile_ref
              AND web.source_url=h.source_url AND web.target_platform IN ('youtube','twitter','douyin') AND web.status='completed' AND web.stop_code IS NULL
             JOIN discovery_video_completions v ON v.search_hit_id=h.id
             JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.discovery_campaign_id=d.id
              AND b.criteria_revision=d.current_revision AND b.native_author_id=web.native_author_id",
            "JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=d.id
              AND c.criteria_revision=d.current_revision AND c.platform=web.target_platform AND c.source_platform=h.platform
              AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=web.native_author_id
              AND json_extract(c.evidence_json,'$.source_urls[0]')=h.source_url",
        ),
        "youtube" => (
            "JOIN discovery_video_completions v ON v.search_hit_id=h.id
         JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id
           AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision",
            "JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=b.native_author_id",
        ),
        "twitter" => (
            "JOIN discovery_content_reads v ON v.search_hit_id=h.id
           AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
           AND v.platform=h.platform AND v.profile_ref=h.profile_ref
           AND v.source_url=h.source_url AND v.status='completed' AND v.stop_code IS NULL
         JOIN discovery_content_identity_bindings b ON b.search_hit_id=h.id
           AND b.identity_search_hit_id=h.id AND b.profile_search_hit_id=h.id",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=b.account_id",
        ),
        "douyin" => (
            "JOIN discovery_search_authors s ON s.search_hit_id=h.id
         JOIN discovery_douyin_author_reads joint ON joint.search_hit_id=h.id
           AND joint.discovery_campaign_id=d.id AND joint.expected_revision=d.current_revision
           AND joint.platform=h.platform AND joint.profile_ref=h.profile_ref AND joint.source_url=h.source_url
           AND joint.status='completed' AND joint.stop_code IS NULL AND joint.duplicate_of IS NULL
           AND joint.author_id=s.author_id AND joint.profile_url=s.author_url
         JOIN discovery_profile_reads p ON p.search_hit_id=h.id
           AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
           AND p.status='completed' AND p.stop_code IS NULL AND p.call_number=joint.call_number
           AND json_extract(p.native_profile_json,'$.author_id')=s.author_id
           AND json_extract(p.native_profile_json,'$.profile_url')=s.author_url
         JOIN discovery_content_reads v ON v.search_hit_id=h.id
           AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
           AND v.platform=h.platform AND v.profile_ref=h.profile_ref AND v.author_id=s.author_id
           AND v.source_url=h.source_url AND v.status='completed' AND v.stop_code IS NULL AND v.call_number=joint.call_number",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id
            AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        ),
        "linkedin" => (
            "JOIN discovery_search_authors s ON s.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.search_hit_id=h.id
           AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
           AND p.status='completed' AND p.stop_code IS NULL
           AND json_extract(p.native_profile_json,'$.author_handle')=s.author_id
           AND json_extract(p.native_profile_json,'$.profile_url')=s.author_url AND s.author_url=h.source_url
         JOIN discovery_content_reads v ON v.search_hit_id=h.id
           AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
           AND v.platform=h.platform AND v.profile_ref=h.profile_ref AND v.author_id=s.author_id
           AND v.source_url=h.source_url AND v.status='completed' AND v.stop_code IS NULL
           AND json_extract(v.native_content_json,'$.account_id')=json_extract(p.native_profile_json,'$.account_id')",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=json_extract(p.native_profile_json,'$.account_id')
            AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        ),
        "pinterest" => (
            "JOIN discovery_search_authors s ON s.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.search_hit_id=h.id
           AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
           AND p.status='completed' AND p.stop_code IS NULL
           AND json_extract(p.native_profile_json,'$.author_id')=s.author_id",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id
            AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        ),
        "tiktok" | "instagram" => (
            "JOIN discovery_search_authors s ON s.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.search_hit_id=h.id
           AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
           AND p.status='completed' AND p.stop_code IS NULL
           AND json_extract(p.native_profile_json,'$.id')=s.author_id",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id
            AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        ),
        "bilibili" | "reddit" | "facebook" | "weibo" | "bluesky" | "zhihu" | "hackernews" | "pixiv" => (
            "JOIN discovery_search_authors s ON s.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.search_hit_id=h.id
           AND p.discovery_campaign_id=d.id AND p.expected_revision=d.current_revision
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=s.author_id
           AND p.status='completed' AND p.stop_code IS NULL
           AND json_extract(p.native_profile_json,CASE WHEN h.platform='bilibili' THEN '$.mid' WHEN h.platform IN ('zhihu','hackernews','pixiv') THEN '$.author_id' WHEN h.platform='bluesky' THEN '$.did' ELSE '$.id' END)=s.author_id
         JOIN discovery_content_reads v ON v.search_hit_id=h.id
           AND v.discovery_campaign_id=d.id AND v.expected_revision=d.current_revision
           AND v.platform=h.platform AND v.profile_ref=h.profile_ref
           AND v.author_id=s.author_id AND (v.source_url=h.source_url OR (h.platform='facebook' AND EXISTS(
             SELECT 1 FROM discovery_facebook_content_targets t WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=d.id
               AND t.expected_revision=d.current_revision AND t.profile_ref=h.profile_ref AND t.author_id=s.author_id
               AND t.profile_url=s.author_url AND t.status='completed' AND t.stop_code IS NULL AND t.call_number<v.call_number
               AND EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE l.value=v.source_url))))
           AND v.status='completed' AND v.stop_code IS NULL",
            "JOIN discovery_candidates c ON c.discovery_campaign_id=d.id
            AND c.criteria_revision=d.current_revision AND c.platform=h.platform
            AND c.source_profile_ref=h.profile_ref AND c.platform_account_id=s.author_id
            AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')",
        ),
        _ => return Err("review_platform_unsupported".into()),
    };
    let sql = format!("SELECT a.criteria_json,a.scope_json,a.approved_at,h.profile_ref,h.search_keyword,h.source_url,
           h.title,h.author_label,h.captured_at,c.id,c.platform_account_id,c.evidence_json
         FROM discovery_campaigns d
         JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
         JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.id=?3 AND h.platform=?4
         {source_join}
         {candidate_join}
         WHERE d.id=?1 AND d.current_revision=?2 AND d.phase='active' AND a.approved_at IS NOT NULL");
    let mut basis = tx
        .query_row(&sql, params![campaign, revision, hit, platform], |r| {
            Ok(DiscoveryReviewBasis {
                campaign_id: campaign.into(),
                revision,
                criteria_json: r.get(0)?,
                scopes_json: r.get(1)?,
                approved_at: r.get(2)?,
                hit: DiscoverySearchHit {
                    id: hit.into(),
                    campaign_id: campaign.into(),
                    platform: platform.clone(),
                    profile_ref: r.get(3)?,
                    search_keyword: r.get(4)?,
                    source_url: r.get(5)?,
                    title: r.get(6)?,
                    author_label: r.get(7)?,
                    captured_at: r.get(8)?,
                    author_id: None,
                    author_url: None,
                    author_observed_at: None,
                },
                candidate_id: r.get(9)?,
                native_author_id: r.get(10)?,
                native_followers: None,
                native_follower_estimate: None,
                native_profile: None,
                evidence_json: r.get(11)?,
                transcript: None,
                source_description: None,
                native_content: None,
                language_samples: Vec::new(),
                pages: vec![],
            })
        })
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("review_material_author_binding_unavailable")?;
    // Bind this material to its admitted query, without changing older material
    // hashes when an unrelated subsequent round is added.
    let mut scopes: Vec<crate::DiscoveryPlatformScope> =
        serde_json::from_str(&basis.scopes_json).map_err(|e| e.to_string())?;
    if let Some(scope) = scopes.iter_mut().find(|s| {
        s.platform == basis.hit.platform && s.profile_ref == basis.hit.profile_ref
    }) && !scope.keywords.contains(&basis.hit.search_keyword)
    {
        let admitted = super::discovery_query_rounds::effective_scopes(
            tx, campaign, revision, &basis.scopes_json,
        )?;
        if admitted.iter().any(|s| {
            s.platform == scope.platform
                && s.profile_ref == scope.profile_ref
                && s.keywords.contains(&basis.hit.search_keyword)
        }) {
            scope.keywords.push(basis.hit.search_keyword.clone());
            basis.scopes_json = serde_json::to_string(&scopes).map_err(|e| e.to_string())?;
        }
    }
    let mut evidence: Value =
        serde_json::from_str(&basis.evidence_json).map_err(|_| "review_evidence_invalid")?;
    if matches!(basis.hit.platform.as_str(), "google" | "yahoo") {
        let read = super::discovery_web_sources::get_source_row(tx, campaign, hit)?;
        crate::discovery_web_sources::validate_result(&crate::FinishDiscoveryWebSourceInput {
            claim: read.claim.clone(),
            native_material: read.native_material.clone(),
            stop_code: read.stop_code.clone(),
        })?;
        if !read.is_primary_source
            || read.status != "completed"
            || read.claim.expected_revision != revision
            || read.claim.origin_platform != basis.hit.platform
            || read.claim.profile_ref != basis.hit.profile_ref
            || read.claim.source_url != basis.hit.source_url
            || read.candidate_id.as_deref() != Some(&basis.candidate_id)
            || read.native_author_id.as_deref() != Some(&basis.native_author_id)
            || evidence
                .pointer("/web_origin/target_platform")
                .and_then(Value::as_str)
                != Some(read.claim.target_platform.as_str())
        {
            return Err("review_native_web_source_binding_unavailable".into());
        }
        let native = read
            .native_material
            .as_ref()
            .ok_or("review_native_web_source_binding_unavailable")?;
        let twitter = read.claim.target_platform == "twitter";
        let douyin = read.claim.target_platform == "douyin";
        basis.source_description = if twitter || douyin {
            native.pointer("/content/text")
        } else {
            native.get("description")
        }
        .and_then(Value::as_str)
        .map(str::to_owned);
        basis.hit.author_label = if douyin {
            native.pointer("/profile/author_label")
        } else if twitter {
            native.pointer("/profile/name")
        } else {
            native.get("author")
        }
        .and_then(Value::as_str)
        .ok_or("review_native_web_source_binding_unavailable")?
        .to_owned();
        basis.hit.author_id = Some(basis.native_author_id.clone());
        basis.hit.author_url = Some(if twitter || douyin {
            native["profile"]["profile_url"]
                .as_str()
                .ok_or("review_native_web_source_binding_unavailable")?
                .to_owned()
        } else {
            format!("https://www.youtube.com/channel/{}", basis.native_author_id)
        });
        basis.hit.author_observed_at = Some(read.captured_at);
        if twitter || douyin {
            basis.native_followers = native["profile"]["follower_count"].as_u64();
            basis.native_profile = Some(DiscoveryNativeProfileObservation {
                web_text: None,
                profile_url: basis
                    .hit
                    .author_url
                    .clone()
                    .ok_or("review_native_web_source_binding_unavailable")?,
                observed_at: read.captured_at,
                bio: if douyin && native["profile"]["bio_truncated"] == true {
                    None
                } else {
                    native["profile"]["bio"].as_str().map(str::to_owned)
                },
                self_declared_location: native["profile"]["self_declared_location"]
                    .as_str()
                    .map(str::to_owned),
            });
        }
    }
    strip_unaudited_proofs(
        tx,
        &mut evidence,
        &basis.candidate_id,
        campaign,
        revision,
        &basis.native_author_id,
    )?;
    basis.evidence_json = evidence.to_string();
    if matches!(basis.hit.platform.as_str(), "pinterest" | "instagram") {
        let profile = super::discovery_profiles::load_profile_row(tx, campaign, hit)?;
        if profile.status != "completed"
            || profile.stop_code.is_some()
            || profile.claim.expected_revision != revision
            || profile.claim.platform != basis.hit.platform
            || profile.claim.profile_ref != basis.hit.profile_ref
            || profile.claim.author_id != basis.native_author_id
            || profile.claim.source_url != basis.hit.source_url
            || (basis.hit.platform == "instagram"
                && profile.claim.author_url != basis.hit.source_url)
            || evidence["source_urls"][0].as_str() != Some(basis.hit.source_url.as_str())
        {
            return Err("review_native_profile_binding_unavailable".into());
        }
        crate::discovery_profiles::validate_profile_result(&crate::FinishDiscoveryProfileInput {
            claim: profile.claim.clone(),
            native_profile: profile.native_profile.clone(),
            stop_code: None,
        })
        .map_err(|_| "review_native_profile_binding_unavailable")?;
        let native = profile
            .native_profile
            .as_ref()
            .ok_or("review_native_profile_binding_unavailable")?;
        if basis.hit.platform == "instagram" && native["private_account"] == true {
            return Err("review_source_unavailable".into());
        }
        basis.native_followers = native["follower_count"].as_u64();
        basis.native_profile = Some(DiscoveryNativeProfileObservation {
            web_text: bounded_profile_text(native, "web_text", 4000),
            profile_url: profile.claim.author_url.clone(),
            observed_at: profile.captured_at,
            bio: bounded_profile_text(
                native,
                if basis.hit.platform == "instagram" {
                    "biography"
                } else {
                    "bio"
                },
                8_000,
            ),
            self_declared_location: None,
        });
        // Legacy profile-only material stays readable. A present content receipt
        // must match the saved account, feed URL, browser, revision and call order.
        if tx.query_row("SELECT EXISTS(SELECT 1 FROM discovery_content_reads WHERE discovery_campaign_id=?1 AND search_hit_id=?2)",
            params![campaign, hit], |row| row.get::<_, bool>(0)).map_err(|e| e.to_string())? {
            let content = super::discovery_content::load_content_row(tx, campaign, hit)?;
            if content.status == "completed" && content.stop_code.is_none() {
                if content.claim.expected_revision != revision || content.claim.platform != basis.hit.platform
                    || content.claim.profile_ref != basis.hit.profile_ref || content.claim.author_id != basis.native_author_id
                    || content.claim.source_url != profile.claim.source_url || content.claim.call_number <= profile.claim.call_number
                    || (basis.hit.platform == "pinterest" && content.native_content.as_ref().and_then(|v| v["profile_url"].as_str()) != Some(profile.claim.author_url.as_str())) {
                    return Err("review_native_content_binding_unavailable".into());
                }
                crate::discovery_content::validate_content_result(&crate::FinishDiscoveryContentInput {
                    claim: content.claim.clone(), native_content: content.native_content.clone(), stop_code: None,
                }).map_err(|_| "review_native_content_binding_unavailable")?;
                basis.native_content = Some(content);
            }
        }
        basis.hit.author_id = Some(profile.claim.author_id.clone());
        basis.hit.author_url = Some(profile.claim.author_url.clone());
        basis.hit.author_observed_at = Some(profile.captured_at);
    }
    if basis.hit.platform == "tiktok" {
        let (author_id, author_url, observed_at, profile_json, profile_observed_at): (String, String, i64, String, i64) = tx
            .query_row(
                "SELECT s.author_id,s.author_url,s.observed_at,p.native_profile_json,p.updated_at
                 FROM discovery_search_authors s JOIN discovery_profile_reads p ON p.search_hit_id=s.search_hit_id
                 WHERE s.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3
                   AND p.platform='tiktok' AND p.profile_ref=?4 AND p.author_id=s.author_id
                   AND p.status='completed' AND p.stop_code IS NULL",
                params![hit, campaign, revision, basis.hit.profile_ref],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|_| "review_native_profile_binding_unavailable")?;
        let profile: Value = serde_json::from_str(&profile_json)
            .map_err(|_| "review_native_profile_binding_unavailable")?;
        let username = profile["username"].as_str().unwrap_or_default();
        let expected_profile = format!("https://www.tiktok.com/@{username}");
        let video_id = basis
            .hit
            .source_url
            .strip_prefix(&format!("{expected_profile}/video/"))
            .unwrap_or_default();
        if author_id != basis.native_author_id
            || profile["id"].as_str() != Some(author_id.as_str())
            || profile["profile_url"].as_str() != Some(expected_profile.as_str())
            || author_url != expected_profile
            || username != basis.hit.author_label
            || !(1..=25).contains(&video_id.len())
            || video_id.starts_with('0')
            || !video_id.bytes().all(|byte| byte.is_ascii_digit())
            || basis.hit.title.trim().is_empty()
            || basis.hit.title.chars().count() > 500
            || evidence["source_urls"][0].as_str() != Some(basis.hit.source_url.as_str())
        {
            return Err("review_native_profile_binding_unavailable".into());
        }
        basis.native_followers = Some(
            profile["follower_count"]
                .as_u64()
                .filter(|value| *value <= 9_007_199_254_740_991)
                .ok_or("review_native_followers_invalid")?,
        );
        basis.native_profile = Some(DiscoveryNativeProfileObservation {
            web_text: None,
            profile_url: expected_profile,
            observed_at: profile_observed_at,
            bio: bounded_profile_text(&profile, "signature", 1_000),
            self_declared_location: None,
        });
        basis.hit.author_id = Some(author_id);
        basis.hit.author_url = Some(author_url);
        basis.hit.author_observed_at = Some(observed_at);
    }
    if matches!(
        basis.hit.platform.as_str(),
        "reddit"
            | "facebook"
            | "weibo"
            | "bluesky"
            | "zhihu"
            | "hackernews"
            | "pixiv"
            | "linkedin"
            | "douyin"
    ) {
        let profile = super::discovery_profiles::load_profile_row(tx, campaign, hit)?;
        let content = super::discovery_content::load_content_row(tx, campaign, hit)?;
        let author_url = &profile.claim.author_url;
        if profile.status != "completed"
            || profile.stop_code.is_some()
            || content.status != "completed"
            || content.stop_code.is_some()
            || profile.claim.expected_revision != revision
            || content.claim.expected_revision != revision
            || profile.claim.platform != basis.hit.platform
            || content.claim.platform != basis.hit.platform
            || profile.claim.profile_ref != basis.hit.profile_ref
            || content.claim.profile_ref != basis.hit.profile_ref
            || (if basis.hit.platform == "linkedin" {
                profile
                    .native_profile
                    .as_ref()
                    .and_then(|p| p["account_id"].as_str())
                    != Some(basis.native_author_id.as_str())
            } else {
                profile.claim.author_id != basis.native_author_id
            })
            || content.claim.author_id != profile.claim.author_id
            || profile.claim.source_url != basis.hit.source_url
            || (basis.hit.platform != "facebook"
                && content.claim.source_url != basis.hit.source_url)
            || (if basis.hit.platform == "douyin" {
                profile.claim.call_number != content.claim.call_number
            } else {
                profile.claim.call_number >= content.claim.call_number
            })
            || evidence["source_urls"][0].as_str() != Some(basis.hit.source_url.as_str())
            || evidence["source_urls"][1].as_str() != Some(author_url.as_str())
        {
            return Err("review_native_content_binding_unavailable".into());
        }
        crate::discovery_profiles::validate_profile_result(&crate::FinishDiscoveryProfileInput {
            claim: profile.claim.clone(),
            native_profile: profile.native_profile.clone(),
            stop_code: None,
        })
        .map_err(|_| "review_native_profile_binding_unavailable")?;
        crate::discovery_content::validate_content_result(&crate::FinishDiscoveryContentInput {
            claim: content.claim.clone(),
            native_content: content.native_content.clone(),
            stop_code: None,
        })
        .map_err(|_| "review_native_content_binding_unavailable")?;
        let native = profile
            .native_profile
            .as_ref()
            .ok_or("review_native_profile_binding_unavailable")?;
        basis.native_followers = native["follower_count"].as_u64();
        if basis.hit.platform == "douyin" {
            basis.native_follower_estimate = native["follower_text"]
                .as_str()
                .and_then(crate::discovery_douyin::follower_estimate);
        }
        basis.native_profile = Some(DiscoveryNativeProfileObservation {
            web_text: None,
            profile_url: author_url.clone(),
            observed_at: profile.captured_at,
            bio: bounded_profile_text(
                native,
                if matches!(
                    basis.hit.platform.as_str(),
                    "weibo" | "zhihu" | "pixiv" | "linkedin" | "douyin"
                ) {
                    "bio"
                } else {
                    "biography"
                },
                if matches!(
                    basis.hit.platform.as_str(),
                    "bluesky" | "zhihu" | "hackernews" | "pixiv" | "linkedin" | "douyin"
                ) {
                    8000
                } else {
                    2000
                },
            ),
            self_declared_location: None,
        });
        basis.hit.author_id = Some(if basis.hit.platform == "linkedin" {
            basis.native_author_id.clone()
        } else {
            profile.claim.author_id
        });
        basis.hit.author_url = Some(author_url.clone());
        basis.native_content = Some(content);
    }
    if basis.hit.platform == "bilibili" {
        let (author_id, author_url, observed_at, profile_json, profile_observed_at): (String, String, i64, String, i64) = tx
            .query_row(
                "SELECT s.author_id,s.author_url,s.observed_at,p.native_profile_json,p.updated_at
                 FROM discovery_search_authors s JOIN discovery_profile_reads p ON p.search_hit_id=s.search_hit_id
                 WHERE s.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3
                   AND p.platform='bilibili' AND p.profile_ref=?4 AND p.author_id=s.author_id
                   AND p.status='completed' AND p.stop_code IS NULL",
                params![hit, campaign, revision, basis.hit.profile_ref],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .map_err(|_| "review_native_profile_binding_unavailable")?;
        let profile: Value = serde_json::from_str(&profile_json)
            .map_err(|_| "review_native_profile_binding_unavailable")?;
        let expected_profile = format!("https://space.bilibili.com/{author_id}");
        let content = super::discovery_content::load_content_row(tx, campaign, hit)?;
        let native = content
            .native_content
            .as_ref()
            .ok_or("review_native_content_binding_unavailable")?;
        if author_id != basis.native_author_id
            || profile["mid"].as_str() != Some(author_id.as_str())
            || profile["profile_url"].as_str() != Some(expected_profile.as_str())
            || author_url != expected_profile
            || content.status != "completed"
            || content.stop_code.is_some()
            || content.claim.expected_revision != revision
            || content.claim.platform != "bilibili"
            || content.claim.profile_ref != basis.hit.profile_ref
            || content.claim.author_id != basis.native_author_id
            || content.claim.source_url != basis.hit.source_url
            || native["author_id"].as_str() != Some(author_id.as_str())
            || native["source_url"].as_str() != Some(basis.hit.source_url.as_str())
            || native["bvid"].as_str()
                != basis
                    .hit
                    .source_url
                    .strip_prefix("https://www.bilibili.com/video/")
            || evidence["source_urls"][0].as_str() != Some(basis.hit.source_url.as_str())
        {
            return Err("review_native_content_binding_unavailable".into());
        }
        basis.native_followers = Some(
            profile["follower_count"]
                .as_u64()
                .filter(|value| *value <= 9_007_199_254_740_991)
                .ok_or("review_native_followers_invalid")?,
        );
        basis.native_profile = Some(DiscoveryNativeProfileObservation {
            web_text: None,
            profile_url: expected_profile,
            observed_at: profile_observed_at,
            bio: bounded_profile_text(&profile, "sign", 1_000),
            self_declared_location: None,
        });
        basis.hit.author_id = Some(author_id);
        basis.hit.author_url = Some(author_url);
        basis.hit.author_observed_at = Some(observed_at);
        basis.native_content = Some(content);
    }
    if basis.hit.platform == "twitter" {
        let (profile_json, profile_observed_at): (String, i64) = tx
            .query_row(
                "SELECT p.native_profile_json,p.updated_at
             FROM discovery_profile_reads p
             JOIN discovery_profile_identity_matches m ON m.search_hit_id=p.search_hit_id
               AND m.identity_search_hit_id=p.search_hit_id
             WHERE p.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.expected_revision=?3
               AND p.platform='twitter' AND p.profile_ref=?4 AND p.status='completed'
               AND p.stop_code IS NULL AND m.account_id=?5
               AND json_extract(p.native_profile_json,'$.account_id')=m.account_id",
                params![
                    hit,
                    campaign,
                    revision,
                    basis.hit.profile_ref,
                    basis.native_author_id
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or("review_native_profile_binding_unavailable")?;
        let profile: Value = serde_json::from_str(&profile_json)
            .map_err(|_| "review_native_profile_binding_unavailable")?;
        let followers = profile["follower_count"]
            .as_u64()
            .filter(|value| *value <= 9_007_199_254_740_991)
            .ok_or("review_native_followers_invalid")?;
        basis.native_followers = Some(followers);
        let content = super::discovery_content::load_content_row(tx, campaign, hit)?;
        if content.status != "completed"
            || content.stop_code.is_some()
            || content.claim.expected_revision != revision
            || content.claim.platform != "twitter"
            || content.claim.profile_ref != basis.hit.profile_ref
            || content.claim.source_url != basis.hit.source_url
            || content
                .native_content
                .as_ref()
                .and_then(|value| value.pointer("/identity/account_id"))
                .and_then(Value::as_str)
                != Some(basis.native_author_id.as_str())
        {
            return Err("review_native_content_binding_unavailable".into());
        }
        let native_profile_url = content
            .native_content
            .as_ref()
            .and_then(|value| value.pointer("/identity/profile_url"))
            .and_then(Value::as_str)
            .ok_or("review_native_profile_binding_unavailable")?;
        if profile["profile_url"].as_str() != Some(native_profile_url) {
            return Err("review_native_profile_binding_unavailable".into());
        }
        basis.native_profile = Some(DiscoveryNativeProfileObservation {
            web_text: None,
            profile_url: native_profile_url.to_owned(),
            observed_at: profile_observed_at,
            bio: bounded_profile_text(&profile, "bio", 1_000),
            self_declared_location: bounded_profile_text(&profile, "self_declared_location", 256),
        });
        basis.native_content = Some(content);
    }
    basis.language_samples =
        super::discovery_language::samples(tx, campaign, revision, &basis.candidate_id)?;
    let mut bytes = serde_json::to_vec(&basis.language_samples)
        .map_err(|e| e.to_string())?
        .len()
        + basis.criteria_json.len()
        + basis.scopes_json.len()
        + basis.evidence_json.len()
        + basis.source_description.as_ref().map_or(0, String::len)
        + basis.hit.source_url.len()
        + basis.hit.title.len()
        + basis.hit.author_label.len();
    if let Some(content) = &basis.native_content {
        bytes += serde_json::to_vec(content)
            .map_err(|error| error.to_string())?
            .len();
    }
    if let Some(profile) = &basis.native_profile {
        bytes += serde_json::to_vec(profile)
            .map_err(|error| error.to_string())?
            .len();
    }
    if bytes > MAX_REVIEW_BYTES {
        return Err("review_material_too_large".into());
    }
    basis.transcript = tx
        .query_row(
            "SELECT status,segments_json,truncated,stop_code,call_number,updated_at
         FROM discovery_transcript_reads WHERE search_hit_id=?1 AND candidate_id=?2",
            params![hit, basis.candidate_id],
            |r| {
                Ok((
                    DiscoveryTranscriptRead {
                        search_hit_id: hit.into(),
                        candidate_id: basis.candidate_id.clone(),
                        source_url: basis.hit.source_url.clone(),
                        profile_ref: basis.hit.profile_ref.clone(),
                        platform_account_id: basis.native_author_id.clone(),
                        status: r.get(0)?,
                        segments: vec![],
                        truncated: r.get(2)?,
                        stop_code: r.get(3)?,
                        call_number: r.get(4)?,
                        captured_at: r.get(5)?,
                    },
                    r.get::<_, String>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some((_, json)) = &basis.transcript {
        bytes += json.len();
    }
    {
        let mut statement = tx.prepare(
            "SELECT requested_url,source_excerpt,call_number,status,document_url,title,markdown_excerpt,truncated,stop_code,updated_at
             FROM discovery_public_page_reads WHERE search_hit_id=?1 AND candidate_id=?2
             ORDER BY CASE WHEN ?3 AND requested_url LIKE 'https://www.youtube.com/channel/%' THEN 0 ELSE 1 END,
               CASE WHEN ?3 THEN -call_number ELSE 0 END,requested_url LIMIT 21"
        ).map_err(|e|e.to_string())?;
        let rows = statement
            .query_map(params![hit, basis.candidate_id, contact_navigation], |r| {
                Ok(DiscoveryPublicPageRead {
                    candidate_id: basis.candidate_id.clone(),
                    requested_url: r.get(0)?,
                    search_hit_id: hit.into(),
                    source_url: basis.hit.source_url.clone(),
                    source_excerpt: r.get(1)?,
                    profile_ref: basis.hit.profile_ref.clone(),
                    platform_account_id: basis.native_author_id.clone(),
                    call_number: r.get(2)?,
                    status: r.get(3)?,
                    document_url: r.get(4)?,
                    title: r.get(5)?,
                    markdown_excerpt: r.get(6)?,
                    truncated: r.get(7)?,
                    stop_code: r.get(8)?,
                    captured_at: r.get(9)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let page = row.map_err(|e| e.to_string())?;
            let page_bytes = page.requested_url.len()
                + page.source_excerpt.len()
                + page.title.len()
                + page.markdown_excerpt.len()
                + page.document_url.as_ref().map_or(0, String::len);
            if basis.pages.len() == MAX_REVIEW_PAGES || bytes + page_bytes > MAX_REVIEW_BYTES {
                if contact_navigation {
                    continue;
                }
                return Err("review_material_too_large".into());
            }
            bytes += page_bytes;
            basis.pages.push(page);
        }
    }
    Ok(basis)
}

pub(super) fn read_basis(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryReviewBasis, String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let basis = load_basis(&tx, campaign, revision, hit)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(basis)
}

pub(super) fn load_pair_basis(
    connection: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
    left: &str,
    right: &str,
) -> Result<[DiscoveryReviewBasis; 2], String> {
    Ok([
        load_basis(connection, campaign, revision, left)?,
        load_basis(connection, campaign, revision, right)?,
    ])
}

pub(super) fn read_pair_basis(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    left: &str,
    right: &str,
) -> Result<[DiscoveryReviewBasis; 2], String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    // One WAL snapshot for both accounts; independent reads can mix evidence epochs.
    let basis = load_pair_basis(&tx, campaign, revision, left, right)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(basis)
}

fn find_record(
    connection: &rusqlite::Connection,
    account: &str,
    campaign: &str,
    request: &str,
) -> Result<Option<StoredDiscoveryReview>, String> {
    connection
        .query_row(
            "SELECT request_hash,assessment_json FROM discovery_evidence_reviews
         WHERE operator_account_id=?1 AND discovery_campaign_id=?2 AND request_id=?3",
            params![account, campaign, request],
            |r| {
                Ok(StoredDiscoveryReview {
                    request_hash: r.get(0)?,
                    receipt_json: r.get(1)?,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())
}

pub(super) fn find(
    repository: &SqliteRepository,
    account: &str,
    campaign: &str,
    request: &str,
) -> Result<Option<StoredDiscoveryReview>, String> {
    let connection = repository.read_connection()?;
    find_record(&connection, account, campaign, request)
}

pub(super) fn read_submission(
    repository: &SqliteRepository,
    account: &str,
    campaign: &str,
    request: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryReviewSubmissionBasis, String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let result = match find_record(&tx, account, campaign, request)? {
        Some(stored) => DiscoveryReviewSubmissionBasis::Committed(stored),
        None => DiscoveryReviewSubmissionBasis::Material(Box::new(load_basis(
            &tx, campaign, revision, hit,
        )?)),
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn commit(
    repository: &SqliteRepository,
    prepared: &PreparedDiscoveryReview,
) -> Result<StoredDiscoveryReview, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let receipt = &prepared.receipt;
    // Reconcile first: another identical submit may have committed after preparation.
    if let Some(stored) = find_record(
        &tx,
        &receipt.operator_account_id,
        &receipt.campaign_id,
        &receipt.request_id,
    )? {
        if stored.request_hash != prepared.request_hash {
            return Err("review_idempotency_conflict".into());
        }
        return Ok(stored);
    }
    if load_basis(
        &tx,
        &prepared.basis.campaign_id,
        prepared.basis.revision,
        &prepared.basis.hit.id,
    )? != prepared.basis
    {
        return Err("review_material_changed".into());
    }
    tx.execute("INSERT INTO discovery_evidence_reviews
        (id,operator_account_id,discovery_campaign_id,criteria_revision,search_hit_id,candidate_id,native_author_id,profile_ref,
         request_id,request_hash,material_hash,material_json,policy_version,reviewer_json,findings_json,verdict,assessment_json,created_at)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![receipt.review_id,receipt.operator_account_id,receipt.campaign_id,receipt.criteria_revision,receipt.search_hit_id,
            receipt.candidate_id,receipt.native_author_id,receipt.profile_ref,receipt.request_id,prepared.request_hash,receipt.material_hash,
            prepared.material_json,receipt.policy_version,prepared.reviewer_json,prepared.findings_json,receipt.verdict,prepared.receipt_json,receipt.created_at]
    ).map_err(|e|e.to_string())?;
    let changed = tx
        .execute(
            "UPDATE discovery_candidates SET verdict=?1,evidence_json=?2,updated_at=?3
        WHERE id=?4 AND discovery_campaign_id=?5 AND criteria_revision=?6 AND source_platform=?9
          AND platform_account_id=?7 AND source_profile_ref=?8",
            params![
                receipt.verdict,
                prepared.evidence_json,
                receipt.created_at,
                receipt.candidate_id,
                receipt.campaign_id,
                receipt.criteria_revision,
                receipt.native_author_id,
                receipt.profile_ref,
                prepared.basis.hit.platform,
            ],
        )
        .map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("review_material_changed".into());
    }
    super::discovery_contacts::observe_review(&tx, prepared)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(StoredDiscoveryReview {
        request_hash: prepared.request_hash.clone(),
        receipt_json: prepared.receipt_json.clone(),
    })
}

#[cfg(test)]
mod identity_snapshot_tests;
