//! One transactional native-author/caption/profile receipt, without synthetic IDs.
use super::{SqliteRepository, now};
use crate::{
    DiscoverySourceAuthorClaim, DiscoverySourceAuthorRead, FinishDiscoverySourceAuthorInput,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

pub(super) fn next_hit(
    repository: &SqliteRepository,
    campaign: &str,
    excluded: &[String],
) -> Result<Option<String>, String> {
    if excluded.iter().any(|platform| platform == "douyin") {
        return Ok(None);
    }
    let db = repository.read_connection()?;
    let mut statement = db.prepare("SELECT h.id,h.source_url FROM discovery_search_hits h
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      LEFT JOIN discovery_search_authors a ON a.search_hit_id=h.id
      LEFT JOIN discovery_douyin_author_reads p ON p.search_hit_id=h.id
      WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL
        AND (c.max_calls IS NULL OR c.used_calls<c.max_calls) AND h.platform='douyin'
        AND r.status='completed' AND a.search_hit_id IS NULL AND p.search_hit_id IS NULL
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([campaign], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    // Older search receipts can predate canonical-video validation. Preserve
    // them, but never dispatch an invalid source or charge a synthetic attempt.
    // Use the claim validator here too so scheduling cannot select a source
    // that the owner will immediately reject.
    for row in rows {
        let (id, source) = row.map_err(|e| e.to_string())?;
        if crate::discovery_douyin::post_id(&source).is_some() {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(super) fn claim(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoverySourceAuthorClaim, String> {
    let mut db = repository.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let (used,maximum):(u32,Option<u32>)=tx.query_row("SELECT c.used_calls,c.max_calls FROM discovery_campaigns c
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL",
      params![campaign,revision], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("Source author read is not approved")?;
    if maximum.is_some_and(|max| used >= max) {
        return Err("Campaign read budget is exhausted".into());
    }
    let (profile_ref,source_url):(String,String)=tx.query_row("SELECT h.profile_ref,h.source_url FROM discovery_search_hits h
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='douyin' AND r.status='completed'
        AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_douyin_author_reads p WHERE p.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=h.discovery_campaign_id AND b.status='searching')",
      params![hit,campaign], |r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("Source author was attempted, is known, or the platform is stopped")?;
    let post_id = crate::discovery_douyin::post_id(&source_url)
        .ok_or("Source author requires its canonical matched video")?
        .to_string();
    let call_number = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    let timestamp = now();
    let claim = DiscoverySourceAuthorClaim {
        campaign_id: campaign.into(),
        expected_revision: revision,
        search_hit_id: hit.into(),
        platform: "douyin".into(),
        profile_ref,
        source_url,
        post_id,
        call_number,
    };
    tx.execute(
        "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
        params![campaign, call_number, timestamp],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?3,stop_code=NULL,updated_at=?4
      WHERE discovery_campaign_id=?1 AND platform='douyin' AND profile_ref=?2",
      params![campaign,claim.profile_ref,format!("source_author:{hit}:{call_number}"),timestamp]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO discovery_douyin_author_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,?7,'reading',NULL,NULL,NULL,NULL,?8)",
      params![hit,campaign,claim.profile_ref,claim.source_url,claim.post_id,revision,call_number,timestamp]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn get(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoverySourceAuthorRead, String> {
    let db = repository.read_connection()?;
    load(&db, campaign, hit)
}
fn load(
    db: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoverySourceAuthorRead, String> {
    db.query_row(
        "SELECT p.expected_revision,p.platform,p.profile_ref,p.source_url,p.post_id,p.call_number,
        p.status,p.author_id,p.profile_url,p.duplicate_of,p.stop_code,p.updated_at
      FROM discovery_douyin_author_reads p JOIN discovery_search_hits h ON h.id=p.search_hit_id
      WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2 AND h.discovery_campaign_id=?1
        AND h.platform=p.platform AND h.profile_ref=p.profile_ref AND h.source_url=p.source_url",
        params![campaign, hit],
        |r| {
            Ok(DiscoverySourceAuthorRead {
                claim: DiscoverySourceAuthorClaim {
                    campaign_id: campaign.into(),
                    search_hit_id: hit.into(),
                    expected_revision: r.get(0)?,
                    platform: r.get(1)?,
                    profile_ref: r.get(2)?,
                    source_url: r.get(3)?,
                    post_id: r.get(4)?,
                    call_number: r.get(5)?,
                },
                status: r.get(6)?,
                author_id: r.get(7)?,
                profile_url: r.get(8)?,
                duplicate_of: r.get(9)?,
                stop_code: r.get(10)?,
                captured_at: r.get(11)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or("Source author read not found at its saved source".into())
}

pub(super) fn finish(
    repository: &SqliteRepository,
    input: FinishDiscoverySourceAuthorInput,
) -> Result<DiscoverySourceAuthorRead, String> {
    crate::discovery_douyin::validate_result(&input)?;
    let mut db = repository.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let prior = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != *claim || prior.status != "reading" {
        return Err("Source author claim is stale".into());
    }
    let timestamp = now();
    let changed=tx.execute("UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
      WHERE discovery_campaign_id=?1 AND platform='douyin' AND profile_ref=?2 AND status='searching' AND active_query=?3
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v
          ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
          WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND v.approved_at IS NOT NULL)",
      params![claim.campaign_id,claim.profile_ref,format!("source_author:{}:{}",claim.search_hit_id,claim.call_number),
        crate::discovery::read_platform_status(input.stop_code.as_deref()),
        input.stop_code.as_deref().filter(|code| !crate::discovery::source_local_read_failure(Some(code))),timestamp,claim.expected_revision]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Source author claim authorization changed".into());
    }
    let mut author: Option<String> = None;
    let mut profile_url: Option<String> = None;
    let mut duplicate: Option<String> = None;
    if let (Some(profile), Some(content)) = (&input.native_profile, &input.native_content) {
        let id = profile["author_id"]
            .as_str()
            .ok_or("Native author missing")?;
        let url = profile["profile_url"]
            .as_str()
            .ok_or("Native profile URL missing")?;
        duplicate = tx
            .query_row(
                "SELECT search_hit_id FROM discovery_profile_reads
          WHERE discovery_campaign_id=?1 AND platform='douyin' AND profile_ref=?2 AND author_id=?3
            AND expected_revision=?4 AND status='completed' AND stop_code IS NULL",
                params![
                    claim.campaign_id,
                    claim.profile_ref,
                    id,
                    claim.expected_revision
                ],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO discovery_search_authors VALUES(?1,?2,?3,?4)",
            params![claim.search_hit_id, id, url, timestamp],
        )
        .map_err(|e| e.to_string())?;
        if duplicate.is_none() {
            tx.execute("INSERT INTO discovery_profile_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,'completed',?7,NULL,?8)",
              params![claim.search_hit_id,claim.campaign_id,claim.profile_ref,id,claim.expected_revision,claim.call_number,profile.to_string(),timestamp]).map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO discovery_content_reads VALUES(?1,?2,'douyin',?3,?4,?5,?6,?7,'completed',?8,NULL,?9)",
              params![claim.search_hit_id,claim.campaign_id,claim.profile_ref,id,claim.source_url,claim.expected_revision,claim.call_number,content.to_string(),timestamp]).map_err(|e|e.to_string())?;
            let profile_receipt = super::discovery_profiles::load_profile_row(
                &tx,
                &claim.campaign_id,
                &claim.search_hit_id,
            )?;
            super::discovery_profiles::insert_profile_candidate(&tx, &profile_receipt)?;
        }
        author = Some(id.into());
        profile_url = Some(url.into());
    }
    tx.execute("UPDATE discovery_douyin_author_reads SET status=?3,author_id=?4,profile_url=?5,duplicate_of=?6,stop_code=?7,updated_at=?8
      WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'",
      params![claim.search_hit_id,claim.call_number,if input.stop_code.is_some() {"failed"} else {"completed"},author,profile_url,duplicate,input.stop_code,timestamp]).map_err(|e|e.to_string())?;
    let receipt = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(receipt)
}
