use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{DiscoveryProfileClaim, DiscoveryProfileRead, FinishDiscoveryProfileInput};

pub(super) fn next_profile_hit(
    repository: &SqliteRepository,
    campaign: &str,
    excluded_platforms: &[String],
) -> Result<Option<String>, String> {
    next_profile_hit_excluding(repository, campaign, excluded_platforms, &[])
}

pub(super) fn next_profile_hit_excluding(
    repository: &SqliteRepository,
    campaign: &str,
    excluded_platforms: &[String],
    held_hits: &[String],
) -> Result<Option<String>, String> {
    let connection = repository.read_connection()?;
    let mut held_hits = held_hits.to_vec();
    held_hits.extend(super::discovery_read_health::excluded_hits(
        &connection,
        campaign,
        "profile",
    )?);
    let held_hits = held_hits.as_slice();
    connection.query_row(
        "SELECT h.id FROM discovery_search_hits h
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
         JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
         JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
         LEFT JOIN discovery_profile_reads p ON p.discovery_campaign_id=c.id AND p.platform=h.platform
           AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
         WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
           AND h.platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin')
           AND h.platform NOT IN (SELECT value FROM json_each(?2))
           AND h.id NOT IN (SELECT value FROM json_each(?3))
           AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id AND i.status!='completed')
           AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
           AND ((r.status='completed' AND p.search_hit_id IS NULL)
             OR (r.status='technical_stop' AND r.stop_code='interrupted_profile_read' AND p.status='reading'
               AND p.search_hit_id=h.id AND p.expected_revision=c.current_revision
               AND r.active_query='profile:' || h.id || ':' || p.call_number))
         ORDER BY CASE WHEN p.status='reading' THEN 0 ELSE 1 END,h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1",
        params![campaign, serde_json::to_string(excluded_platforms).map_err(|error| error.to_string())?,serde_json::to_string(held_hits).map_err(|e| e.to_string())?], |row|row.get(0)).optional().map_err(|e|e.to_string())
}

pub(super) fn claim_profile_read(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryProfileClaim, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    super::discovery_read_health::check(&transaction, campaign, "profile", hit)?;
    let (used, maximum): (u32,Option<u32>) = transaction.query_row(
        "SELECT c.used_calls,c.max_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions v
         ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
         WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL",
        params![campaign,revision], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?
        .ok_or("Profile read is not approved at this Campaign revision")?;
    if maximum.is_some_and(|max| used >= max) {
        return Err("Campaign read budget is exhausted".into());
    }
    let not_ready: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND status='searching')",
        [campaign], |r|r.get(0)).map_err(|e|e.to_string())?;
    if not_ready {
        return Err("A Campaign read is active".into());
    }
    let mut claim = transaction.query_row(
        "SELECT h.platform,h.profile_ref,a.author_id,a.author_url,h.source_url FROM discovery_search_hits h
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform IN ('bilibili','tiktok','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pinterest','pixiv','linkedin')",
        params![hit,campaign], |r|Ok(DiscoveryProfileClaim {
            campaign_id:campaign.into(),expected_revision:revision,search_hit_id:hit.into(),
            platform:r.get(0)?,profile_ref:r.get(1)?,author_id:r.get(2)?,author_url:r.get(3)?,source_url:r.get(4)?,call_number:0,
        })).optional().map_err(|e|e.to_string())?.ok_or("Profile source is not an observed author in this Campaign")?;
    let (status,stop,active): (String,Option<String>,Option<String>) = transaction.query_row(
        "SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
        params![campaign,claim.platform,claim.profile_ref], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
    let prior: Option<(String, u32, String, u32)> = transaction
        .query_row(
            "SELECT search_hit_id,call_number,status,expected_revision FROM discovery_profile_reads
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND author_id=?4",
            params![campaign, claim.platform, claim.profile_ref, claim.author_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let interrupted = prior
        .as_ref()
        .is_some_and(|(original, call, state, approved)| {
            original == hit
                && state == "reading"
                && *approved == revision
                && status == "technical_stop"
                && stop.as_deref() == Some("interrupted_profile_read")
                && active.as_deref() == Some(format!("profile:{hit}:{call}").as_str())
        });
    if !interrupted && (status != "completed" || prior.is_some()) {
        return Err("Author profile was already attempted or the platform is stopped".into());
    }
    claim.call_number = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    if claim.platform == "twitter" {
        matching_post_identity(&transaction, &claim)?;
    }
    if claim.platform == "facebook"
        && crate::discovery_facebook_identity::valid_source(&claim.source_url)
    {
        let matched:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i
            WHERE i.search_hit_id=?1 AND i.discovery_campaign_id=?2 AND i.profile_ref=?3 AND i.expected_revision=?4
              AND i.source_url=?5 AND i.status='completed' AND i.stop_code IS NULL AND i.call_number<?6
              AND json_extract(i.native_identity_json,'$.id') IS ?7)",
            params![claim.search_hit_id,claim.campaign_id,claim.profile_ref,claim.expected_revision,claim.source_url,claim.call_number,claim.author_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if !matched {
            return Err("Facebook vanity profile requires its completed source identity".into());
        }
    }
    let timestamp = now();
    transaction
        .execute(
            "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
            params![campaign, claim.call_number, timestamp],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute(
        "UPDATE discovery_platform_runs SET status='searching',active_query=?4,stop_code=NULL,updated_at=?5
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
        params![campaign,claim.platform,claim.profile_ref,format!("profile:{hit}:{}",claim.call_number),timestamp]).map_err(|e|e.to_string())?;
    transaction.execute(
        "INSERT INTO discovery_profile_reads VALUES(?1,?2,?3,?4,?5,?6,?7,'reading',NULL,NULL,?8)
         ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at",
        params![hit,campaign,claim.platform,claim.profile_ref,claim.author_id,revision,claim.call_number,timestamp]).map_err(|e|e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn finish_profile_read(
    repository: &SqliteRepository,
    input: FinishDiscoveryProfileInput,
) -> Result<DiscoveryProfileRead, String> {
    crate::discovery_profiles::validate_profile_result(&input)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let prior = load_profile_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim == *claim
        && prior.status != "reading"
        && prior.native_profile == input.native_profile
        && prior.stop_code == input.stop_code
    {
        // Returning an identical saved receipt is read-only. Never replay a
        // provider call, budget reservation or candidate insertion on delivery retry.
        return Ok(prior);
    }
    if prior.claim != *claim || prior.status != "reading" {
        return Err("Profile claim is stale or its source changed".into());
    }
    let matched_id = if claim.platform == "twitter" && input.native_profile.is_some() {
        let account = matching_post_identity(&transaction, claim)?;
        if input
            .native_profile
            .as_ref()
            .and_then(|value| value.get("account_id"))
            .and_then(serde_json::Value::as_str)
            != Some(account.as_str())
        {
            return Err("Profile native identity does not match its observed post author".into());
        }
        Some(account)
    } else {
        None
    };
    let platform_status = crate::discovery::read_platform_status(input.stop_code.as_deref());
    let run_stop = input
        .stop_code
        .as_deref()
        .filter(|code| !crate::discovery::source_local_read_failure(Some(code)));
    let timestamp = now();
    let changed = transaction.execute(
        "UPDATE discovery_platform_runs SET status=?5,active_query=NULL,stop_code=?6,updated_at=?7
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4
         AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v
           ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
           WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?8 AND v.approved_at IS NOT NULL)",
        params![claim.campaign_id,claim.platform,claim.profile_ref,format!("profile:{}:{}",claim.search_hit_id,claim.call_number),
            platform_status,run_stop,timestamp,claim.expected_revision]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Profile claim is stale or Campaign authorization changed".into());
    }
    let json = input
        .native_profile
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| e.to_string())?;
    let changed = transaction.execute(
        "UPDATE discovery_profile_reads SET status=?3,native_profile_json=?4,stop_code=?5,updated_at=?6
         WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'",
        params![claim.search_hit_id,claim.call_number,if input.stop_code.is_some() {"failed"} else {"completed"},json,input.stop_code,timestamp]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Profile receipt is stale".into());
    }
    let receipt = load_profile_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    if let Some(account) = matched_id {
        transaction
            .execute(
                "INSERT INTO discovery_profile_identity_matches VALUES(?1,?1,?2,?3)",
                params![claim.search_hit_id, account, timestamp],
            )
            .map_err(|e| e.to_string())?;
    }
    insert_profile_candidate(&transaction, &receipt)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(receipt)
}

fn matching_post_identity(
    connection: &rusqlite::Connection,
    claim: &DiscoveryProfileClaim,
) -> Result<String, String> {
    let identity = super::discovery_identity::load_identity_row(
        connection,
        &claim.campaign_id,
        &claim.search_hit_id,
    )?;
    if identity.status != "completed"
        || identity.claim.expected_revision != claim.expected_revision
        || identity.claim.profile_ref != claim.profile_ref
        || identity.claim.author_handle != claim.author_id
        || identity.claim.source_url != claim.source_url
        || identity.claim.call_number >= claim.call_number
    {
        return Err("Profile requires completed post identity at its approved source".into());
    }
    identity
        .account_id
        .ok_or("Completed post identity has no native account ID".into())
}

/// Current-schema candidate projection, in the receipt's transaction. This does
/// not save a canonical Creator or promote profile text to content proof.
pub(super) fn insert_profile_candidate(
    connection: &rusqlite::Connection,
    receipt: &DiscoveryProfileRead,
) -> Result<(), String> {
    use crate::discovery_qualification::{
        Assessment, assess_profile, assess_profile_without_count, assessed_evidence,
    };
    use serde_json::{Value, json};

    let Some(native) = receipt
        .native_profile
        .as_ref()
        .filter(|_| receipt.status == "completed")
    else {
        return Ok(());
    };
    let claim = &receipt.claim;
    let name_key = match claim.platform.as_str() {
        "bilibili" | "weibo" => "name",
        "tiktok" | "facebook" | "instagram" | "bluesky" => "display_name",
        "twitter" => "name",
        "reddit" | "hackernews" => "username",
        "zhihu" | "pinterest" | "pixiv" | "linkedin" | "douyin" => "author_label",
        _ => return Err("Unsupported profile candidate platform".into()),
    };
    // Older valid receipts need not contain a usable display name. Preserve the
    // native receipt without inventing a name or changing its durable format.
    let Some(name) = native
        .get(name_key)
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty() && !name.chars().any(char::is_control))
    else {
        return Ok(());
    };
    let name: String = name.chars().take(200).collect();
    let followers = native.get("follower_count").and_then(Value::as_u64);
    let follower_text = if claim.platform == "douyin" {
        native["follower_text"].as_str().map(str::to_owned)
    } else {
        followers.map(|count| count.to_string())
    };
    if !matches!(
        claim.platform.as_str(),
        "twitter"
            | "reddit"
            | "facebook"
            | "instagram"
            | "weibo"
            | "bluesky"
            | "zhihu"
            | "hackernews"
            | "pinterest"
            | "pixiv"
            | "linkedin"
            | "douyin"
    ) && followers.is_none()
    {
        return Err("Profile receipt has no observed follower count".into());
    }
    let criteria_json: String = connection
        .query_row(
            "SELECT criteria_json FROM discovery_campaign_revisions
         WHERE discovery_campaign_id=?1 AND revision=?2 AND approved_at IS NOT NULL",
            params![claim.campaign_id, claim.expected_revision],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    let criteria = serde_json::from_str(&criteria_json).map_err(|e| e.to_string())?;
    let account_id = if claim.platform == "twitter" {
        connection
            .query_row(
                "SELECT account_id FROM discovery_profile_identity_matches WHERE search_hit_id=?1",
                [&claim.search_hit_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())?
    } else if claim.platform == "linkedin" {
        native["account_id"]
            .as_str()
            .ok_or("LinkedIn profile has no native account ID")?
            .into()
    } else {
        claim.author_id.clone()
    };
    let evidence = json!({
        "source_urls": [claim.source_url, claim.author_url],
        "subscribers_raw": follower_text,
        "subscribers_source_url": claim.author_url,
        "author_identity_kind": "native_id",
        "author_identity_search_hit_id": (claim.platform == "twitter").then_some(&claim.search_hit_id),
        "captured_at": receipt.captured_at,
    });
    let assessment = if claim.platform == "instagram" && native["private_account"] == true {
        Assessment {
            verdict: "rejected",
            reasons: vec!["private_account"],
            subscribers: followers,
        }
    } else if claim.platform == "douyin" && followers.is_none() {
        crate::discovery_qualification::assess_douyin_profile(
            &criteria,
            &evidence,
            &account_id,
            native["follower_text"].as_str(),
        )
    } else {
        match followers {
            Some(count) => assess_profile(&criteria, &evidence, &account_id, count),
            None => assess_profile_without_count(&criteria, &evidence, &account_id),
        }
    };
    let evidence = assessed_evidence(evidence, &assessment);
    if evidence.to_string().len() > 16_384 {
        return Err("Assessed discovery evidence exceeds the size limit".into());
    }
    connection
        .execute(
            "INSERT INTO discovery_candidates
         (id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,
          platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?4)
         ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO NOTHING",
            params![
                uuid::Uuid::new_v4().to_string(),
                claim.campaign_id,
                claim.expected_revision,
                claim.platform,
                claim.profile_ref,
                account_id,
                name,
                claim.author_url,
                assessment.verdict,
                evidence.to_string(),
                receipt.captured_at
            ],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn get_profile_read(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryProfileRead, String> {
    let connection = repository.read_connection()?;
    load_profile_row(&connection, campaign, hit)
}

pub(super) fn load_profile_row(
    connection: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryProfileRead, String> {
    let (mut result,json): (DiscoveryProfileRead,Option<String>) = connection.query_row(
        "SELECT p.expected_revision,p.platform,p.profile_ref,p.author_id,a.author_url,h.source_url,
           p.call_number,p.status,p.native_profile_json,p.stop_code,p.updated_at
         FROM discovery_profile_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2 AND h.discovery_campaign_id=?1
           AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND a.author_id=p.author_id",
        params![campaign,hit], |r|Ok((DiscoveryProfileRead {
            claim:DiscoveryProfileClaim {campaign_id:campaign.into(),search_hit_id:hit.into(),expected_revision:r.get(0)?,
              platform:r.get(1)?,profile_ref:r.get(2)?,author_id:r.get(3)?,author_url:r.get(4)?,source_url:r.get(5)?,call_number:r.get(6)?},
            status:r.get(7)?,native_profile:None,stop_code:r.get(9)?,captured_at:r.get(10)?,
        },r.get(8)?))).optional().map_err(|e|e.to_string())?.ok_or("Profile read not found in this Campaign")?;
    result.native_profile = json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(result)
}
