use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{
    DiscoverySearchHit, DiscoveryTranscriptClaim, DiscoveryTranscriptRead,
    FinishDiscoveryTranscriptInput,
};

pub(super) fn next_transcript_hit(
    repository: &SqliteRepository,
    campaign: &str,
) -> Result<Option<DiscoverySearchHit>, String> {
    next_transcript_hit_excluding(repository, campaign, &[])
}

pub(super) fn next_transcript_hit_excluding(
    repository: &SqliteRepository,
    campaign: &str,
    held_hits: &[String],
) -> Result<Option<DiscoverySearchHit>, String> {
    let connection = repository.read_connection()?;
    connection.query_row(
        "SELECT h.id,h.discovery_campaign_id,h.platform,h.profile_ref,h.search_keyword,
          h.source_url,h.title,h.author_label,h.captured_at,p.author_id,p.author_url,p.observed_at
         FROM discovery_search_hits h
         LEFT JOIN discovery_search_authors p ON p.search_hit_id=h.id
         JOIN discovery_video_completions v ON v.search_hit_id=h.id
         JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id
           AND r.platform=h.platform AND r.profile_ref=h.profile_ref
         JOIN discovery_campaigns d ON d.id=h.discovery_campaign_id
         JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
         WHERE d.id=?1 AND h.id NOT IN (SELECT value FROM json_each(?2)) AND d.phase='active' AND a.approved_at IS NOT NULL
           AND h.platform='youtube' AND COALESCE(json_extract(a.criteria_json,'$.requires_ai_tool_demo'),1)=1
           AND EXISTS(SELECT 1 FROM discovery_candidates c
             JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id AND b.search_hit_id=h.id
               AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision
               AND b.native_author_id=c.platform_account_id
             WHERE c.discovery_campaign_id=d.id
             AND c.platform=h.platform AND c.source_profile_ref=h.profile_ref
             AND c.criteria_revision=d.current_revision AND c.verdict='pending'
             AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.source_urls') s WHERE s.value=h.source_url))
           AND ((r.status='completed' AND NOT EXISTS(SELECT 1 FROM discovery_transcript_reads t WHERE t.search_hit_id=h.id))
             OR (r.status='technical_stop' AND r.stop_code='interrupted_transcript_read'
               AND EXISTS(SELECT 1 FROM discovery_transcript_reads t WHERE t.search_hit_id=h.id
                 AND t.status='reading' AND r.active_query='transcript:' || h.id || ':' || t.call_number)))
         ORDER BY CASE WHEN r.status='technical_stop' THEN 0 ELSE 1 END,h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id LIMIT 1",
        params![campaign, serde_json::to_string(held_hits).map_err(|e|e.to_string())?], |row| Ok(DiscoverySearchHit {
            id: row.get(0)?, campaign_id: row.get(1)?, platform: row.get(2)?, profile_ref: row.get(3)?,
            search_keyword: row.get(4)?, source_url: row.get(5)?, title: row.get(6)?, author_label: row.get(7)?, captured_at: row.get(8)?,
            author_id: row.get(9)?, author_url: row.get(10)?, author_observed_at: row.get(11)?,
        })).optional().map_err(|error| error.to_string())
}

pub(super) fn claim_transcript_read(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    profile: &str,
    hit: &str,
) -> Result<DiscoveryTranscriptClaim, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let (used, maximum): (u32,Option<u32>) = transaction.query_row(
        "SELECT c.used_calls,c.max_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions a
           ON a.discovery_campaign_id=c.id AND a.revision=c.current_revision
         WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND a.approved_at IS NOT NULL
           AND COALESCE(json_extract(a.criteria_json,'$.requires_ai_tool_demo'),1)=1",
        params![campaign,revision], |row| Ok((row.get(0)?,row.get(1)?))).optional().map_err(|e|e.to_string())?
        .ok_or("Transcript read is not approved at this Campaign revision")?;
    if maximum.is_some_and(|max| used >= max) {
        return Err("Campaign read budget is exhausted".into());
    }
    let not_ready: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND status='searching')",
        [campaign], |row|row.get(0)).map_err(|e|e.to_string())?;
    if not_ready {
        return Err("A Campaign read is active".into());
    }
    let source: String = transaction.query_row(
        "SELECT h.source_url FROM discovery_search_hits h JOIN discovery_video_completions v ON v.search_hit_id=h.id
         WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='youtube' AND h.profile_ref=?3",
        params![hit,campaign,profile], |row|row.get(0)).optional().map_err(|e|e.to_string())?
        .ok_or("Transcript source is not a metadata-completed hit in the approved scope")?;
    let candidates = {
        let mut statement = transaction.prepare(
            "SELECT c.id FROM discovery_candidates c
             JOIN discovery_video_identity_bindings b ON b.candidate_id=c.id AND b.search_hit_id=?5
               AND b.discovery_campaign_id=?1 AND b.criteria_revision=?3
               AND b.native_author_id=c.platform_account_id
             WHERE c.discovery_campaign_id=?1 AND c.platform='youtube'
             AND c.source_profile_ref=?2 AND c.criteria_revision=?3 AND c.verdict='pending'
             AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.source_urls') s WHERE s.value=?4) LIMIT 2"
        ).map_err(|e|e.to_string())?;
        statement
            .query_map(params![campaign, profile, revision, source, hit], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    };
    let [candidate] = candidates.as_slice() else {
        return Err("Transcript needs exactly one pending source owner".into());
    };
    let (status, stop, active): (String,Option<String>,Option<String>) = transaction.query_row(
        "SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2",
        params![campaign,profile], |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).map_err(|e|e.to_string())?;
    let prior: Option<(u32,String,String)> = transaction.query_row(
        "SELECT call_number,status,candidate_id FROM discovery_transcript_reads WHERE search_hit_id=?1", [hit],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(|e|e.to_string())?;
    let interrupted = prior.as_ref().is_some_and(|(call, state, owner)| {
        state == "reading"
            && owner == candidate
            && status == "technical_stop"
            && stop.as_deref() == Some("interrupted_transcript_read")
            && active.as_deref() == Some(format!("transcript:{hit}:{call}").as_str())
    });
    if !interrupted && (status != "completed" || prior.is_some()) {
        return Err("Transcript was already attempted or the platform is stopped".into());
    }
    let call = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    let timestamp = now();
    transaction
        .execute(
            "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
            params![campaign, call, timestamp],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute(
        "UPDATE discovery_platform_runs SET status='searching',active_query=?3,stop_code=NULL,updated_at=?4
         WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2",
        params![campaign,profile,format!("transcript:{hit}:{call}"),timestamp]).map_err(|e|e.to_string())?;
    transaction.execute(
        "INSERT INTO discovery_transcript_reads VALUES(?1,?2,?3,'reading','[]',0,NULL,?4)
         ON CONFLICT(search_hit_id) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at",
        params![hit,candidate,call,timestamp]).map_err(|e|e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(DiscoveryTranscriptClaim {
        campaign_id: campaign.into(),
        expected_revision: revision,
        profile_ref: profile.into(),
        search_hit_id: hit.into(),
        candidate_id: candidate.clone(),
        source_url: source,
        call_number: call,
    })
}

pub(super) fn finish_transcript_read(
    repository: &SqliteRepository,
    input: FinishDiscoveryTranscriptInput,
) -> Result<DiscoveryTranscriptRead, String> {
    crate::discovery_transcripts::validate_transcript_result(&input)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let timestamp = now();
    let platform_status = crate::discovery::read_platform_status(input.stop_code.as_deref());
    let run_stop = input
        .stop_code
        .as_deref()
        .filter(|code| !crate::discovery::source_local_read_failure(Some(code)));
    let affected = transaction.execute(
        "UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
         WHERE discovery_campaign_id=?1 AND platform='youtube' AND profile_ref=?2 AND status='searching' AND active_query=?3
           AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions a
             ON a.discovery_campaign_id=c.id AND a.revision=c.current_revision
             WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND a.approved_at IS NOT NULL)
           AND EXISTS(SELECT 1 FROM discovery_search_hits h WHERE h.id=?8 AND h.discovery_campaign_id=?1
             AND h.platform='youtube' AND h.profile_ref=?2 AND h.source_url=?9)
           AND EXISTS(SELECT 1 FROM discovery_video_identity_bindings b
             JOIN discovery_candidates c ON c.id=b.candidate_id
             WHERE b.search_hit_id=?8 AND b.discovery_campaign_id=?1 AND b.criteria_revision=?7
               AND b.candidate_id=?10 AND b.native_author_id=c.platform_account_id
               AND c.discovery_campaign_id=?1 AND c.criteria_revision=?7
               AND c.platform='youtube' AND c.source_profile_ref=?2)",
        params![claim.campaign_id,claim.profile_ref,format!("transcript:{}:{}",claim.search_hit_id,claim.call_number),
            platform_status,run_stop,timestamp,claim.expected_revision,claim.search_hit_id,claim.source_url,claim.candidate_id]
    ).map_err(|e|e.to_string())?;
    if affected != 1 {
        return Err("Transcript claim is stale or Campaign authorization changed".into());
    }
    let status = if input.stop_code.is_some() {
        "failed"
    } else if input.segments.is_empty() {
        "unavailable"
    } else {
        "completed"
    };
    let affected = transaction.execute(
        "UPDATE discovery_transcript_reads SET status=?4,segments_json=?5,truncated=?6,stop_code=?7,updated_at=?8
         WHERE search_hit_id=?1 AND candidate_id=?2 AND call_number=?3 AND status='reading'",
        params![claim.search_hit_id,claim.candidate_id,claim.call_number,status,
            serde_json::to_string(&input.segments).map_err(|e|e.to_string())?,input.truncated,input.stop_code,timestamp]
    ).map_err(|e|e.to_string())?;
    if affected != 1 {
        return Err("Transcript receipt is stale".into());
    }
    let result = load_transcript_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn get_transcript_read(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryTranscriptRead, String> {
    let connection = repository.read_connection()?;
    load_transcript_row(&connection, campaign, hit)
}

fn load_transcript_row(
    connection: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryTranscriptRead, String> {
    let (mut result, json): (DiscoveryTranscriptRead,String) = connection.query_row(
        "SELECT t.search_hit_id,t.candidate_id,h.source_url,h.profile_ref,c.platform_account_id,
           t.status,t.segments_json,t.truncated,t.stop_code,t.call_number,t.updated_at
         FROM discovery_transcript_reads t JOIN discovery_search_hits h ON h.id=t.search_hit_id
         JOIN discovery_candidates c ON c.id=t.candidate_id
         WHERE h.discovery_campaign_id=?1 AND h.id=?2 AND c.discovery_campaign_id=?1",
        params![campaign,hit], |row|Ok((DiscoveryTranscriptRead {
            search_hit_id:row.get(0)?,candidate_id:row.get(1)?,source_url:row.get(2)?,profile_ref:row.get(3)?,platform_account_id:row.get(4)?,
            status:row.get(5)?,segments:vec![],truncated:row.get(7)?,stop_code:row.get(8)?,call_number:row.get(9)?,captured_at:row.get(10)?,
        }, row.get(6)?))).optional().map_err(|e|e.to_string())?.ok_or("Transcript read not found in this Campaign")?;
    result.segments = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(result)
}
