use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{DiscoveryContentClaim, DiscoveryContentRead, FinishDiscoveryContentInput};

pub(super) fn next_content_hit(
    repository: &SqliteRepository,
    campaign: &str,
    platform: &str,
) -> Result<Option<String>, String> {
    next_content_hit_excluding(repository, campaign, platform, &[])
}

pub(super) fn next_content_hit_excluding(
    repository: &SqliteRepository,
    campaign: &str,
    platform: &str,
    held_hits: &[String],
) -> Result<Option<String>, String> {
    let connection = repository.read_connection()?;
    let mut held = held_hits.to_vec();
    held.extend(super::discovery_read_health::excluded_hits(
        &connection,
        campaign,
        "content",
    )?);
    let held_hits = held.as_slice();
    if matches!(platform, "tiktok" | "douyin" | "google" | "yahoo") {
        return super::discovery_language::next_hit_excluding(
            &connection,
            campaign,
            platform,
            held_hits,
        );
    }
    let next = connection.query_row(
        "SELECT h.id FROM discovery_search_hits h
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.discovery_campaign_id=h.discovery_campaign_id
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
         JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
         JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
         LEFT JOIN discovery_content_reads content ON content.search_hit_id=h.id
         WHERE c.id=?1 AND c.phase='active' AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
           AND h.platform=?2 AND h.id NOT IN (SELECT value FROM json_each(?3)) AND p.status='completed' AND p.expected_revision=c.current_revision
           AND (h.platform!='instagram' OR COALESCE(json_extract(p.native_profile_json,'$.private_account'),0)!=1)
           AND (h.platform!='twitter' OR (p.search_hit_id=h.id AND EXISTS(
             SELECT 1 FROM discovery_profile_identity_matches m JOIN discovery_identity_reads i ON i.search_hit_id=m.identity_search_hit_id
             WHERE m.search_hit_id=h.id AND i.status='completed' AND i.account_id=m.account_id
               AND i.expected_revision=c.current_revision AND i.profile_ref=h.profile_ref
               AND i.discovery_campaign_id=c.id AND i.author_handle=a.author_id AND i.source_url=h.source_url
               AND i.call_number<p.call_number)))
           AND h.id=(SELECT h2.id FROM discovery_search_hits h2
             JOIN discovery_search_authors a2 ON a2.search_hit_id=h2.id
             WHERE h2.discovery_campaign_id=h.discovery_campaign_id AND h2.platform=h.platform
               AND h2.profile_ref=h.profile_ref AND a2.author_id=a.author_id
             ORDER BY h2.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h2.id),9223372036854775807),h2.id LIMIT 1)
           AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=c.id AND busy.status='searching')
           AND (h.platform!='facebook' OR EXISTS(SELECT 1 FROM discovery_facebook_content_targets t
             WHERE t.search_hit_id=h.id AND t.discovery_campaign_id=c.id AND t.expected_revision=c.current_revision
               AND t.profile_ref=h.profile_ref AND t.author_id=a.author_id AND t.profile_url=a.author_url AND t.status='completed'
               AND (json_extract(p.native_profile_json,'$.adapter_version')!='2.0.0' OR EXISTS(
                 SELECT 1 FROM json_each(t.source_urls_json) l WHERE NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.source_url=l.value)))))
           AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.expected_revision=c.current_revision AND f.status='failed' AND f.stop_code IN ('opencli_invalid_output','opencli_output_too_large'))
           AND ((r.status='completed' AND (content.search_hit_id IS NULL OR (h.platform='facebook' AND content.status='failed'
               AND json_extract(p.native_profile_json,'$.adapter_version')='2.0.0' AND EXISTS(
                 SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.call_number=content.call_number
                   AND f.status='failed' AND f.stop_code IN ('opencli_command_failed','opencli_timeout','interrupted_content_read')))))
             OR (r.status='technical_stop' AND r.stop_code='interrupted_content_read'
               AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=h.id AND f.status='reading')
               AND content.status='reading' AND content.expected_revision=c.current_revision
               AND (h.platform!='twitter' OR EXISTS(SELECT 1 FROM discovery_content_identity_bindings b WHERE b.search_hit_id=h.id))
               AND r.active_query='content:' || h.id || ':' || content.call_number))
         ORDER BY CASE WHEN content.status='reading' THEN 0 ELSE 1 END,h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1",
        params![campaign,platform,serde_json::to_string(held_hits).map_err(|e|e.to_string())?], |row| row.get(0)).optional().map_err(|e|e.to_string())?;
    match next {
        Some(hit) => Ok(Some(hit)),
        None => super::discovery_language::next_hit_excluding(
            &connection,
            campaign,
            platform,
            held_hits,
        ),
    }
}

pub(super) fn claim_content_read(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryContentClaim, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    super::discovery_read_health::check(&transaction, campaign, "content", hit)?;
    let (used, maximum): (u32,Option<u32>) = transaction.query_row(
        "SELECT c.used_calls,c.max_calls FROM discovery_campaigns c
         JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
         WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL",
        params![campaign,revision], |r|Ok((r.get(0)?,r.get(1)?)))
        .optional().map_err(|e|e.to_string())?.ok_or("Content read is not approved at this Campaign revision")?;
    if maximum.is_some_and(|max| used >= max) {
        return Err("Campaign read budget is exhausted".into());
    }
    let not_ready: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND status='searching')",
        [campaign], |r|r.get(0)).map_err(|e|e.to_string())?;
    if not_ready {
        return Err("A Campaign read is active".into());
    }
    let mut claim = transaction
        .query_row(
            "SELECT h.platform,h.profile_ref,a.author_id,h.source_url FROM discovery_search_hits h
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         JOIN discovery_profile_reads p ON p.discovery_campaign_id=h.discovery_campaign_id
           AND p.platform=h.platform AND p.profile_ref=h.profile_ref AND p.author_id=a.author_id
           AND p.status='completed' AND p.expected_revision=?3
         WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform IN ('bilibili','twitter','reddit','facebook','instagram','weibo','bluesky','zhihu','hackernews','pixiv','pinterest','linkedin')
           AND (h.platform!='instagram' OR COALESCE(json_extract(p.native_profile_json,'$.private_account'),0)!=1)
           AND h.id=(SELECT h2.id FROM discovery_search_hits h2
             JOIN discovery_search_authors a2 ON a2.search_hit_id=h2.id
             WHERE h2.discovery_campaign_id=h.discovery_campaign_id AND h2.platform=h.platform
               AND h2.profile_ref=h.profile_ref AND a2.author_id=a.author_id
             ORDER BY h2.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h2.id),9223372036854775807),h2.id LIMIT 1)",
            params![hit, campaign, revision],
            |r| {
                Ok(DiscoveryContentClaim {
                    campaign_id: campaign.into(),
                    expected_revision: revision,
                    search_hit_id: hit.into(),
                    platform: r.get(0)?,
                    profile_ref: r.get(1)?,
                    author_id: r.get(2)?,
                    source_url: r.get(3)?,
                    call_number: 0,
                })
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Content source lacks a completed profile observation")?;
    let mut native_facebook = false;
    if claim.platform == "facebook" {
        let targets = super::discovery_facebook::load(&transaction, campaign, hit)?;
        if targets.status != "completed"
            || targets.stop_code.is_some()
            || targets.claim.expected_revision != revision
            || targets.claim.profile_ref != claim.profile_ref
            || targets.claim.author_id != claim.author_id
            || targets.claim.source_url != claim.source_url
        {
            return Err("Facebook content lacks current saved target locators".into());
        }
        native_facebook=transaction.query_row("SELECT json_extract(native_profile_json,'$.adapter_version')='2.0.0' FROM discovery_profile_reads WHERE search_hit_id=?1",[hit],|r|r.get(0)).map_err(|e|e.to_string())?;
        let structurally_failed: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts WHERE search_hit_id=?1 AND expected_revision=?2 AND status='failed' AND stop_code IN ('opencli_invalid_output','opencli_output_too_large'))",
            params![hit, revision], |row| row.get(0)).map_err(|e| e.to_string())?;
        if native_facebook && structurally_failed {
            return Err("Facebook original expansion stopped after structural failure".into());
        }
        claim.source_url = if native_facebook {
            transaction.query_row("SELECT l.value FROM discovery_facebook_content_targets t JOIN json_each(t.source_urls_json) l
              WHERE t.search_hit_id=?1 AND NOT EXISTS(SELECT 1 FROM discovery_facebook_original_attempts f WHERE f.search_hit_id=t.search_hit_id AND f.source_url=l.value)
              ORDER BY l.key LIMIT 1",[hit],|r|r.get(0)).optional().map_err(|e|e.to_string())?.ok_or("All saved Facebook originals were attempted")?
        } else {
            targets
                .source_urls
                .and_then(|urls| urls.into_iter().next())
                .ok_or("Facebook content targets are empty")?
        };
    }
    let (status, stop, active): (String, Option<String>, Option<String>) = transaction
        .query_row(
            "SELECT status,stop_code,active_query FROM discovery_platform_runs
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
            params![campaign, claim.platform, claim.profile_ref],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let prior: Option<(u32,String,u32)> = transaction.query_row(
        "SELECT call_number,status,expected_revision FROM discovery_content_reads WHERE search_hit_id=?1 AND discovery_campaign_id=?2",
        params![hit,campaign], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))
        .optional().map_err(|e|e.to_string())?;
    let interrupted = prior.as_ref().is_some_and(|(call, state, approved)| {
        state == "reading"
            && *approved == revision
            && status == "technical_stop"
            && stop.as_deref() == Some("interrupted_content_read")
            && active.as_deref() == Some(format!("content:{hit}:{call}").as_str())
    });
    let native_retry = if native_facebook && status == "completed" {
        transaction.query_row("SELECT EXISTS(SELECT 1 FROM discovery_content_reads c JOIN discovery_facebook_original_attempts f
          ON f.search_hit_id=c.search_hit_id AND f.call_number=c.call_number AND f.source_url=c.source_url
          WHERE c.search_hit_id=?1 AND c.status='failed' AND f.status='failed'
            AND f.stop_code IN ('opencli_command_failed','opencli_timeout','interrupted_content_read'))",[hit],|r|r.get::<_,bool>(0)).map_err(|e|e.to_string())?
    } else {
        false
    };
    if native_facebook && interrupted {
        return Err("Interrupted Facebook original cannot be replayed".into());
    }
    if !interrupted && (status != "completed" || (prior.is_some() && !native_retry)) {
        return Err("Content was already attempted or the platform is stopped".into());
    }
    claim.call_number = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    let native_account = if claim.platform == "twitter" {
        let account = matching_content_identity(&transaction, &claim)?;
        if interrupted && content_binding(&transaction, &claim)? != account {
            return Err("Historical content cannot acquire a new native binding on retry".into());
        }
        Some(account)
    } else {
        None
    };
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
        params![campaign,claim.platform,claim.profile_ref,format!("content:{hit}:{}",claim.call_number),timestamp]).map_err(|e|e.to_string())?;
    transaction.execute(
        "INSERT INTO discovery_content_reads VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'reading',NULL,NULL,?9)
         ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at,
           source_url=excluded.source_url,status='reading',native_content_json=NULL,stop_code=NULL",
        params![hit,campaign,claim.platform,claim.profile_ref,claim.author_id,claim.source_url,revision,claim.call_number,timestamp])
        .map_err(|e|e.to_string())?;
    if native_facebook {
        transaction.execute("INSERT INTO discovery_facebook_original_attempts VALUES(?1,?2,?3,?4,?5,?6,?7,'reading',NULL,NULL,?8)",
            params![hit,campaign,claim.profile_ref,claim.author_id,claim.source_url,revision,claim.call_number,timestamp]).map_err(|e|e.to_string())?;
    }
    if let Some(account) = native_account.filter(|_| !interrupted) {
        transaction
            .execute(
                "INSERT INTO discovery_content_identity_bindings VALUES(?1,?1,?1,?2,?3)",
                params![hit, account, timestamp],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn finish_content_read(
    repository: &SqliteRepository,
    input: FinishDiscoveryContentInput,
) -> Result<DiscoveryContentRead, String> {
    crate::discovery_content::validate_content_result(&input)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    if claim.platform == "facebook" {
        let saved:Option<(String,Option<String>,Option<String>,i64)>=transaction.query_row("SELECT status,native_content_json,stop_code,updated_at
          FROM discovery_facebook_original_attempts WHERE search_hit_id=?1 AND discovery_campaign_id=?2 AND expected_revision=?3
            AND profile_ref=?4 AND author_id=?5 AND source_url=?6 AND call_number=?7 AND status!='reading'",
          params![claim.search_hit_id,claim.campaign_id,claim.expected_revision,claim.profile_ref,claim.author_id,claim.source_url,claim.call_number],
          |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
        if let Some((status, json, stop_code, captured_at)) = saved {
            let native_content = json
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| e.to_string())?;
            if native_content != input.native_content || stop_code != input.stop_code {
                return Err(
                    "Facebook original attempt already finished with another outcome".into(),
                );
            }
            return Ok(DiscoveryContentRead {
                claim: claim.clone(),
                status,
                native_content,
                stop_code,
                captured_at,
                language_sample: None,
            });
        }
    }
    let prior = load_content_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != *claim || prior.status != "reading" {
        return Err("Content claim is stale or its source changed".into());
    }
    if claim.platform == "pinterest"
        && let Some(native) = &input.native_content
    {
        let profile = super::discovery_profiles::load_profile_row(
            &transaction,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        if profile.status != "completed"
            || profile.stop_code.is_some()
            || profile.claim.expected_revision != claim.expected_revision
            || profile.claim.platform != claim.platform
            || profile.claim.profile_ref != claim.profile_ref
            || profile.claim.author_id != claim.author_id
            || profile.claim.source_url != claim.source_url
            || profile.claim.call_number >= claim.call_number
            || native["profile_url"].as_str() != Some(profile.claim.author_url.as_str())
        {
            return Err("Pinterest content differs from its saved native profile identity".into());
        }
    }
    if claim.platform == "linkedin" && input.native_content.is_some() {
        let profile = super::discovery_profiles::load_profile_row(
            &transaction,
            &claim.campaign_id,
            &claim.search_hit_id,
        )?;
        crate::discovery_profiles::validate_profile_result(&crate::FinishDiscoveryProfileInput {
            claim: profile.claim.clone(),
            native_profile: profile.native_profile.clone(),
            stop_code: None,
        })?;
        if profile.status != "completed"
            || profile.stop_code.is_some()
            || profile.claim.expected_revision != claim.expected_revision
            || profile.claim.platform != claim.platform
            || profile.claim.profile_ref != claim.profile_ref
            || profile.claim.author_id != claim.author_id
            || profile.claim.author_url != claim.source_url
            || profile.claim.call_number >= claim.call_number
            || profile.native_profile.as_ref().map(|p| &p["account_id"])
                != input.native_content.as_ref().map(|c| &c["account_id"])
        {
            return Err("LinkedIn content differs from its saved native profile identity".into());
        }
    }
    if claim.platform == "twitter" {
        let expected = matching_content_identity(&transaction, claim)?;
        if content_binding(&transaction, claim)? != expected
            || input.native_content.as_ref().is_some_and(|value| {
                value["identity"]["account_id"].as_str() != Some(expected.as_str())
            })
        {
            return Err("Content native author differs from its reserved identity".into());
        }
    }
    let platform_status = crate::discovery::read_platform_status(input.stop_code.as_deref());
    let run_stop = input
        .stop_code
        .as_deref()
        .filter(|code| !crate::discovery::source_local_read_failure(Some(code)));
    let timestamp = now();
    let changed=transaction.execute(
        "UPDATE discovery_platform_runs SET status=?5,active_query=NULL,stop_code=?6,updated_at=?7
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4
         AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v
           ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
           WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?8 AND v.approved_at IS NOT NULL)",
        params![claim.campaign_id,claim.platform,claim.profile_ref,format!("content:{}:{}",claim.search_hit_id,claim.call_number),
            platform_status,run_stop,timestamp,claim.expected_revision]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Content claim is stale or Campaign authorization changed".into());
    }
    let json = input
        .native_content
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| e.to_string())?;
    let changed=transaction.execute(
        "UPDATE discovery_content_reads SET status=?3,native_content_json=?4,stop_code=?5,updated_at=?6
         WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'",
        params![claim.search_hit_id,claim.call_number,if input.stop_code.is_some() {"failed"} else {"completed"},json,input.stop_code,timestamp])
        .map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Content receipt is stale".into());
    }
    if claim.platform == "facebook" {
        transaction.execute("UPDATE discovery_facebook_original_attempts SET status=?3,native_content_json=?4,stop_code=?5,updated_at=?6
          WHERE search_hit_id=?1 AND call_number=?2 AND source_url=?7 AND status='reading'",
          params![claim.search_hit_id,claim.call_number,if input.stop_code.is_some(){"failed"}else{"completed"},json,input.stop_code,timestamp,claim.source_url]).map_err(|e|e.to_string())?;
    }
    let receipt = load_content_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(receipt)
}

fn matching_content_identity(
    connection: &rusqlite::Connection,
    claim: &DiscoveryContentClaim,
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
        return Err("Content requires its completed original post identity".into());
    }
    let account = identity
        .account_id
        .ok_or("Original post identity has no native account")?;
    let matched: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_profile_reads p JOIN discovery_profile_identity_matches m ON m.search_hit_id=p.search_hit_id
         WHERE p.search_hit_id=?1 AND p.discovery_campaign_id=?2 AND p.platform='twitter' AND p.profile_ref=?3
           AND p.author_id=?4 AND p.expected_revision=?5 AND p.status='completed' AND m.identity_search_hit_id=?1
           AND m.account_id=?6 AND p.call_number>?7 AND p.call_number<?8)",
        params![claim.search_hit_id,claim.campaign_id,claim.profile_ref,claim.author_id,claim.expected_revision,
            account,identity.claim.call_number,claim.call_number], |row| row.get(0)).map_err(|e|e.to_string())?;
    if !matched {
        return Err("Content requires the same post's native profile match".into());
    }
    Ok(account)
}

fn content_binding(
    connection: &rusqlite::Connection,
    claim: &DiscoveryContentClaim,
) -> Result<String, String> {
    connection.query_row(
        "SELECT account_id FROM discovery_content_identity_bindings WHERE search_hit_id=?1 AND identity_search_hit_id=?1 AND profile_search_hit_id=?1",
        [&claim.search_hit_id], |row| row.get(0)).optional().map_err(|e|e.to_string())?
        .ok_or("Content has no reserved native identity binding".into())
}

pub(super) fn get_content_read(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryContentRead, String> {
    let connection = repository.read_connection()?;
    load_content_row(&connection, campaign, hit)
}

pub(super) fn load_content_row(
    connection: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryContentRead, String> {
    let (mut receipt,json): (DiscoveryContentRead,Option<String>)=connection.query_row(
        "SELECT content.expected_revision,content.platform,content.profile_ref,content.author_id,
           content.source_url,content.call_number,content.status,content.native_content_json,
           content.stop_code,content.updated_at
         FROM discovery_content_reads content
         JOIN discovery_search_hits h ON h.id=content.search_hit_id
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         WHERE content.discovery_campaign_id=?1 AND content.search_hit_id=?2
           AND h.discovery_campaign_id=?1 AND h.platform=content.platform
           AND h.profile_ref=content.profile_ref
           AND (h.source_url=content.source_url OR (content.platform='facebook' AND EXISTS (
             SELECT 1 FROM discovery_facebook_content_targets t
             WHERE t.discovery_campaign_id=content.discovery_campaign_id AND t.search_hit_id=h.id
               AND t.expected_revision=content.expected_revision AND t.profile_ref=content.profile_ref
               AND t.author_id=content.author_id AND t.profile_url=a.author_url
               AND t.status='completed' AND t.stop_code IS NULL
               AND t.call_number<content.call_number
               AND EXISTS(SELECT 1 FROM json_each(t.source_urls_json) l WHERE l.value=content.source_url))))
           AND a.author_id=content.author_id",
        params![campaign,hit], |r|Ok((DiscoveryContentRead {
            claim:DiscoveryContentClaim {campaign_id:campaign.into(),search_hit_id:hit.into(),expected_revision:r.get(0)?,
                platform:r.get(1)?,profile_ref:r.get(2)?,author_id:r.get(3)?,source_url:r.get(4)?,call_number:r.get(5)?},
            status:r.get(6)?,native_content:None,stop_code:r.get(8)?,captured_at:r.get(9)?,language_sample:None,
        },r.get(7)?))).optional().map_err(|e|e.to_string())?.ok_or("Content read not found in this Campaign")?;
    receipt.native_content = json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(receipt)
}
