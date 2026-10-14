use super::{SqliteRepository, now};
use crate::{DiscoveryFacebookTargetsRead, DiscoveryProfileClaim};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) fn next_hit(db: &Connection, campaign: &str) -> Result<Option<String>, String> {
    db.query_row("SELECT p.search_hit_id FROM discovery_profile_reads p
      JOIN discovery_campaigns c ON c.id=p.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_search_hits h ON h.id=p.search_hit_id
      JOIN discovery_search_authors a ON a.search_hit_id=h.id
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=p.platform AND r.profile_ref=p.profile_ref
      WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
      AND p.platform='facebook' AND p.expected_revision=c.current_revision AND p.status='completed' AND p.stop_code IS NULL
      AND h.discovery_campaign_id=c.id AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND a.author_id=p.author_id
      AND a.author_url='https://www.facebook.com/profile.php?id=' || p.author_id
      AND (h.source_url=a.author_url OR (json_extract(p.native_profile_json,'$.adapter_version')='2.0.0'
        AND lower(json_extract(p.native_profile_json,'$.observed_profile_url'))=lower(rtrim(h.source_url,'/'))
        AND EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id AND i.status='completed'
          AND i.discovery_campaign_id=c.id AND i.profile_ref=p.profile_ref AND i.expected_revision=c.current_revision
          AND i.source_url=h.source_url AND json_extract(i.native_identity_json,'$.id')=p.author_id)))
      AND json_extract(p.native_profile_json,'$.id')=p.author_id
      AND r.status='completed'
      AND NOT EXISTS(SELECT 1 FROM discovery_facebook_content_targets t WHERE t.search_hit_id=h.id)
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1", [campaign], |r| r.get(0)).optional().map_err(|e| e.to_string())
}

pub(super) fn claim(
    repo: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryProfileClaim, String> {
    let mut db = repo.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    if next_hit(&tx, campaign)?.as_deref() != Some(hit) {
        return Err("Facebook targets require an unattempted, approved completed profile".into());
    }
    let profile = super::discovery_profiles::load_profile_row(&tx, campaign, hit)?;
    if profile.claim.expected_revision != revision {
        return Err("Facebook target revision changed".into());
    }
    let mut claim = profile.claim;
    claim.call_number = tx.query_row("UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=?3
      WHERE id=?1 AND current_revision=?2 AND phase='active' AND (max_calls IS NULL OR used_calls<max_calls) RETURNING used_calls",
      params![campaign, revision, now()], |r| r.get(0)).map_err(|e| e.to_string())?;
    tx.execute("INSERT INTO discovery_facebook_content_targets VALUES(?1,?2,'facebook',?3,?4,?5,?6,?7,'reading',NULL,NULL,?8)",
      params![hit, campaign, claim.profile_ref, claim.author_id, claim.author_url, revision, claim.call_number, now()]).map_err(|e| e.to_string())?;
    tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?3,stop_code=NULL,updated_at=?4
      WHERE discovery_campaign_id=?1 AND platform='facebook' AND profile_ref=?2 AND status='completed'",
      params![campaign, claim.profile_ref, format!("targets:{hit}:{}", claim.call_number), now()]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn load(
    db: &Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryFacebookTargetsRead, String> {
    let (mut receipt, json): (DiscoveryFacebookTargetsRead, Option<String>) = db.query_row(
      "SELECT t.expected_revision,t.profile_ref,t.author_id,t.profile_url,t.call_number,t.status,t.source_urls_json,t.stop_code,t.updated_at,h.source_url
       FROM discovery_facebook_content_targets t JOIN discovery_search_hits h ON h.id=t.search_hit_id
       WHERE t.discovery_campaign_id=?1 AND t.search_hit_id=?2 AND h.discovery_campaign_id=t.discovery_campaign_id
         AND h.platform=t.platform AND h.profile_ref=t.profile_ref",
      params![campaign, hit], |r| Ok((DiscoveryFacebookTargetsRead {
        claim: DiscoveryProfileClaim { campaign_id: campaign.into(), search_hit_id: hit.into(), expected_revision:r.get(0)?,
          platform:"facebook".into(), profile_ref:r.get(1)?, author_id:r.get(2)?, author_url:r.get(3)?, source_url:r.get(9)?, call_number:r.get(4)? },
        status:r.get(5)?, source_urls:None, stop_code:r.get(7)?, captured_at:r.get(8)?,
      }, r.get(6)?))).map_err(|e| e.to_string())?;
    receipt.source_urls = json
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(receipt)
}

pub(super) fn finish(
    repo: &SqliteRepository,
    claim: DiscoveryProfileClaim,
    urls: Option<Vec<String>>,
    stop: Option<String>,
) -> Result<DiscoveryFacebookTargetsRead, String> {
    match (&urls, &stop) {
        (Some(urls), None) if (1..=3).contains(&urls.len()) => {
            let mut seen = std::collections::HashSet::new();
            if !urls
                .iter()
                .all(|url| crate::discovery_facebook::valid_reel(url) && seen.insert(url))
            {
                return Err("Facebook target locators are invalid or duplicated".into());
            }
        }
        (None, Some(code)) => crate::discovery_content::validate_content_result(
            &crate::FinishDiscoveryContentInput {
                claim: crate::DiscoveryContentClaim {
                    campaign_id: claim.campaign_id.clone(),
                    expected_revision: claim.expected_revision,
                    search_hit_id: claim.search_hit_id.clone(),
                    platform: claim.platform.clone(),
                    profile_ref: claim.profile_ref.clone(),
                    author_id: claim.author_id.clone(),
                    source_url: claim.source_url.clone(),
                    call_number: claim.call_number,
                },
                native_content: None,
                stop_code: Some(code.clone()),
            },
        )?,
        _ => return Err("Facebook target result must contain locators or a definite stop".into()),
    }
    let mut db = repo.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let prior = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != claim {
        return Err("Facebook target claim changed".into());
    }
    let authorized: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v
      ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision WHERE c.id=?1 AND c.current_revision=?2
      AND c.phase='active' AND v.approved_at IS NOT NULL)", params![claim.campaign_id, claim.expected_revision], |r| r.get(0)).map_err(|e| e.to_string())?;
    if !authorized {
        return Err("Facebook target authorization changed".into());
    }
    if prior.status != "reading" {
        return if prior.source_urls == urls && prior.stop_code == stop {
            Ok(prior)
        } else {
            Err("Facebook targets already finished".into())
        };
    }
    let status = crate::discovery::read_platform_status(stop.as_deref());
    let run_stop = stop
        .as_deref()
        .filter(|code| !crate::discovery::source_local_read_failure(Some(code)));
    let changed = tx.execute("UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
      WHERE discovery_campaign_id=?1 AND platform='facebook' AND profile_ref=?2 AND status='searching' AND active_query=?3",
      params![claim.campaign_id, claim.profile_ref, format!("targets:{}:{}",claim.search_hit_id,claim.call_number),status,run_stop,now()]).map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("Facebook target run changed".into());
    }
    tx.execute("UPDATE discovery_facebook_content_targets SET status=?3,source_urls_json=?4,stop_code=?5,updated_at=?6
      WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'",
      params![claim.search_hit_id,claim.call_number,if urls.is_some(){"completed"}else{"failed"},urls.as_ref().map(serde_json::to_string).transpose().map_err(|e|e.to_string())?,stop,now()]).map_err(|e|e.to_string())?;
    let result = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
