//! Transactional source observations; no synthetic native searches or candidates.
use super::{SqliteRepository, now};
use crate::{DiscoveryWebSourceClaim, DiscoveryWebSourceRead, FinishDiscoveryWebSourceInput};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

const ELIGIBLE: &str = "FROM discovery_search_hits h
 JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
 JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
 JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
 WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL
 AND h.platform IN ('google','yahoo') AND r.status='completed'
 AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
 AND (EXISTS(SELECT 1 FROM json_each(v.scope_json) s WHERE json_extract(s.value,'$.platform')=h.platform
   AND json_extract(s.value,'$.profile_ref')=h.profile_ref
   AND EXISTS(SELECT 1 FROM json_each(s.value,'$.keywords') k WHERE k.value=h.search_keyword))
 OR EXISTS(SELECT 1 FROM discovery_query_rounds q, json_each(q.scopes_json) s
   WHERE q.discovery_campaign_id=c.id AND q.revision=c.current_revision
   AND json_extract(s.value,'$.platform')=h.platform
   AND json_extract(s.value,'$.profile_ref')=h.profile_ref
   AND EXISTS(SELECT 1 FROM json_each(s.value,'$.keywords') k WHERE k.value=h.search_keyword)))
 AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads p WHERE p.search_hit_id=h.id)
 AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs busy WHERE busy.discovery_campaign_id=c.id AND busy.status='searching')";

pub(super) fn next_hit(
    repository: &SqliteRepository,
    campaign: &str,
    excluded: &[String],
) -> Result<Option<String>, String> {
    let db = repository.read_connection()?;
    let mut statement = db
        .prepare(&format!(
            "SELECT h.id,h.platform,h.source_url {ELIGIBLE} ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id"
        ))
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([campaign], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (hit, origin, source) = row.map_err(|e| e.to_string())?;
        if let Some((target, _, cost)) = crate::discovery_web_sources::source_target(&source)
            && !excluded.contains(&origin)
            && !excluded.iter().any(|p| p == target)
            && target_ready(&db, campaign, &hit, target, cost)?
        {
            return Ok(Some(hit));
        }
    }
    Ok(None)
}

fn target_ready(
    db: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
    target: &str,
    cost: u32,
) -> Result<bool, String> {
    db.query_row("SELECT EXISTS(SELECT 1 FROM discovery_search_hits h JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      WHERE c.id=?1 AND h.id=?2 AND (c.max_calls IS NULL OR c.used_calls+?4<=c.max_calls)
      AND NOT EXISTS(SELECT 1 FROM discovery_web_source_reads p WHERE p.discovery_campaign_id=c.id AND p.profile_ref=h.profile_ref
        AND p.target_platform=?3 AND p.stop_code IN ('opencli_auth_required','opencli_risk_control'))
      AND NOT EXISTS(SELECT 1 FROM discovery_language_samples stopped JOIN discovery_search_hits sh ON sh.id=stopped.search_hit_id
        JOIN discovery_candidates sc ON sc.id=stopped.candidate_id WHERE stopped.discovery_campaign_id=c.id AND sh.profile_ref=h.profile_ref
        AND sc.platform=?3 AND stopped.stop_code IN ('opencli_auth_required','opencli_risk_control'))
      AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=c.id AND r.profile_ref=h.profile_ref
        AND r.platform=?3 AND r.status IN ('unauthenticated','risk_control_stop')))",params![campaign,hit,target,cost],|r|r.get(0)).map_err(|e|e.to_string())
}

pub(super) fn claim(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryWebSourceClaim, String> {
    let mut db = repository.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let (origin,profile_ref,source_url,used):(String,String,String,u32)=tx.query_row(
        &format!("SELECT h.platform,h.profile_ref,h.source_url,c.used_calls {ELIGIBLE} AND c.current_revision=?2 AND h.id=?3"),
        params![campaign,revision,hit],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?.ok_or("Web source is outside current approval, attempted, or stopped")?;
    let (target, video_id, cost) = crate::discovery_web_sources::source_target(&source_url)
        .ok_or("Web source is not supported native content")?;
    if !target_ready(&tx, campaign, hit, target, cost)? {
        return Err("Native source is stopped or outside remaining budget".into());
    }
    let call_number = used
        .checked_add(cost)
        .ok_or("Campaign call counter overflow")?;
    let claim = DiscoveryWebSourceClaim {
        campaign_id: campaign.into(),
        expected_revision: revision,
        search_hit_id: hit.into(),
        origin_platform: origin,
        profile_ref,
        source_url,
        target_platform: target.into(),
        video_id,
        call_number,
    };
    let timestamp = now();
    tx.execute(
        "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
        params![campaign, call_number, timestamp],
    )
    .map_err(|e| e.to_string())?;
    tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?4,stop_code=NULL,updated_at=?5
      WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",params![campaign,claim.origin_platform,claim.profile_ref,format!("web_source:{hit}:{call_number}"),timestamp]).map_err(|e|e.to_string())?;
    tx.execute("INSERT INTO discovery_web_source_reads (search_hit_id,discovery_campaign_id,origin_platform,profile_ref,source_url,expected_revision,call_number,target_platform,video_id,status,updated_at)
      VALUES(?1,?2,?3,?4,?5,?6,?7,?10,?8,'reading',?9)",params![hit,campaign,claim.origin_platform,claim.profile_ref,claim.source_url,revision,call_number,claim.video_id,timestamp,claim.target_platform]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn get_source_row(
    db: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryWebSourceRead, String> {
    db.query_row("SELECT p.expected_revision,p.origin_platform,p.profile_ref,p.source_url,p.target_platform,p.video_id,p.call_number,
      p.status,p.native_author_id,p.native_material_json,p.stop_code,p.updated_at,b.candidate_id,
      COALESCE(c.source_platform=p.origin_platform AND c.source_profile_ref=p.profile_ref
        AND c.criteria_revision=p.expected_revision AND json_extract(c.evidence_json,'$.source_urls[0]')=p.source_url,0)
      FROM discovery_web_source_reads p
      JOIN discovery_search_hits h ON h.id=p.search_hit_id AND h.discovery_campaign_id=p.discovery_campaign_id
        AND h.platform=p.origin_platform AND h.profile_ref=p.profile_ref AND h.source_url=p.source_url
      LEFT JOIN discovery_video_identity_bindings b ON b.search_hit_id=p.search_hit_id
        AND b.discovery_campaign_id=p.discovery_campaign_id AND b.criteria_revision=p.expected_revision AND b.native_author_id=p.native_author_id
      LEFT JOIN discovery_candidates c ON c.id=b.candidate_id AND c.discovery_campaign_id=p.discovery_campaign_id
        AND c.platform=p.target_platform AND c.platform_account_id=p.native_author_id
      WHERE p.discovery_campaign_id=?1 AND p.search_hit_id=?2",params![campaign,hit],|r|{
      let material:Option<String>=r.get(9)?;
      let native_material=material.map(|text|serde_json::from_str(&text).map_err(|e|rusqlite::Error::FromSqlConversionFailure(9,rusqlite::types::Type::Text,Box::new(e)))).transpose()?;
      Ok(DiscoveryWebSourceRead {claim:DiscoveryWebSourceClaim{campaign_id:campaign.into(),search_hit_id:hit.into(),expected_revision:r.get(0)?,origin_platform:r.get(1)?,
        profile_ref:r.get(2)?,source_url:r.get(3)?,target_platform:r.get(4)?,video_id:r.get(5)?,call_number:r.get(6)?},status:r.get(7)?,native_author_id:r.get(8)?,native_material,stop_code:r.get(10)?,captured_at:r.get(11)?,candidate_id:r.get(12)?,is_primary_source:r.get(13)?})
    }).optional().map_err(|e|e.to_string())?.ok_or("Web source receipt not found at its actual origin".into())
}
pub(super) fn get(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryWebSourceRead, String> {
    let db = repository.read_connection()?;
    get_source_row(&db, campaign, hit)
}

pub(super) fn finish(
    repository: &SqliteRepository,
    input: FinishDiscoveryWebSourceInput,
) -> Result<DiscoveryWebSourceRead, String> {
    crate::discovery_web_sources::validate_result(&input)?;
    let mut db = repository.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let prior = get_source_row(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != *claim || prior.status != "reading" {
        return Err("Web source claim is stale".into());
    }
    let timestamp = now();
    // Login/risk belongs to the native target, not to the discovering engine.
    // The failed native receipt quarantines that target/browser in ELIGIBLE.
    let native_stop = matches!(
        input.stop_code.as_deref(),
        Some("opencli_auth_required" | "opencli_risk_control")
    );
    let status = if native_stop {
        "completed"
    } else {
        crate::discovery::read_platform_status(input.stop_code.as_deref())
    };
    let changed=tx.execute("UPDATE discovery_platform_runs SET status=?5,active_query=NULL,stop_code=?6,updated_at=?7
      WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3 AND status='searching' AND active_query=?4
      AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
        WHERE c.id=?1 AND c.current_revision=?8 AND c.phase='active' AND c.used_calls>=?9 AND v.approved_at IS NOT NULL)",
      params![claim.campaign_id,claim.origin_platform,claim.profile_ref,format!("web_source:{}:{}",claim.search_hit_id,claim.call_number),status,
        if status=="completed" {None} else {input.stop_code.as_deref()},timestamp,claim.expected_revision,claim.call_number]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Web source approval or reservation changed".into());
    }
    let author = input
        .native_material
        .as_ref()
        .and_then(|m| crate::discovery_web_sources::native_author(&claim.target_platform, m));
    tx.execute("UPDATE discovery_web_source_reads SET status=?2,native_author_id=?3,native_material_json=?4,stop_code=?5,updated_at=?6 WHERE search_hit_id=?1",
      params![claim.search_hit_id,if input.native_material.is_some(){"completed"}else{"failed"},author,input.native_material.as_ref().map(serde_json::Value::to_string),input.stop_code,timestamp]).map_err(|e|e.to_string())?;
    if let Some(native) = &input.native_material {
        bind_candidate(&tx, claim, native, timestamp)?;
    }
    let result = get_source_row(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

fn bind_candidate(
    db: &rusqlite::Connection,
    claim: &DiscoveryWebSourceClaim,
    native: &serde_json::Value,
    timestamp: u64,
) -> Result<(), String> {
    let author = crate::discovery_web_sources::native_author(&claim.target_platform, native)
        .ok_or("Native source author is absent")?;
    let raw:String=db.query_row("SELECT criteria_json FROM discovery_campaign_revisions WHERE discovery_campaign_id=?1 AND revision=?2",params![claim.campaign_id,claim.expected_revision],|r|r.get(0)).map_err(|e|e.to_string())?;
    let criteria = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let profile = if matches!(claim.target_platform.as_str(), "twitter" | "douyin") {
        native["profile"]["profile_url"]
            .as_str()
            .ok_or("Native profile is absent")?
            .to_owned()
    } else {
        format!("https://www.youtube.com/channel/{author}")
    };
    let name = if claim.target_platform == "douyin" {
        &native["profile"]["author_label"]
    } else if claim.target_platform == "twitter" {
        &native["profile"]["name"]
    } else {
        &native["author"]
    };
    // Original text stays in its full source receipt. No search title or guessed profile facts.
    let evidence = serde_json::json!({"source_urls":[claim.source_url,profile],"captured_at":timestamp,
      "subscribers_raw":native["subscribers"],"subscribers_source_url":claim.source_url,"author_identity_kind":"native_id",
      "web_origin":{"search_hit_id":claim.search_hit_id,"origin_platform":claim.origin_platform,"target_platform":claim.target_platform,"call_number":claim.call_number}});
    let assessed = crate::discovery_qualification::assess(&criteria, &evidence, author);
    let evidence = crate::discovery_qualification::assessed_evidence(evidence, &assessed);
    db.execute("INSERT INTO discovery_candidates(id,discovery_campaign_id,criteria_revision,platform,source_profile_ref,platform_account_id,display_name,profile_url,verdict,evidence_json,created_at,updated_at,source_platform)
      VALUES(?1,?2,?3,?12,?4,?5,?6,?7,?8,?9,?10,?10,?11)
      ON CONFLICT(discovery_campaign_id,platform,platform_account_id) DO NOTHING",
      params![uuid::Uuid::new_v4().to_string(),claim.campaign_id,claim.expected_revision,claim.profile_ref,author,name.as_str().ok_or("Native author name is absent")?,profile,assessed.verdict,evidence.to_string(),timestamp,claim.origin_platform,claim.target_platform]).map_err(|e|e.to_string())?;
    db.execute(
        "INSERT INTO discovery_video_completions(search_hit_id,completed_at) VALUES(?1,?2)",
        params![claim.search_hit_id, timestamp],
    )
    .map_err(|e| e.to_string())?;
    let changed=db.execute("INSERT INTO discovery_video_identity_bindings(search_hit_id,discovery_campaign_id,criteria_revision,candidate_id,native_author_id,created_at)
      SELECT ?1,discovery_campaign_id,criteria_revision,id,platform_account_id,?2 FROM discovery_candidates
      WHERE discovery_campaign_id=?3 AND criteria_revision=?4 AND platform=?6 AND platform_account_id=?5",
      params![claim.search_hit_id,timestamp,claim.campaign_id,claim.expected_revision,author,claim.target_platform]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Native source candidate is outside the current revision".into());
    }
    Ok(())
}
