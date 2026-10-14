use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{
    DiscoveryPublicPageClaim, DiscoveryPublicPageInput, DiscoveryPublicPageRead,
    FinishDiscoveryPublicPageInput,
};

/// Planning is local and does not reserve a call. The existing claim repeats
/// source, revision, platform and budget guards immediately before execution.
pub(super) fn next_public_page(
    repository: &SqliteRepository,
    campaign: &str,
    excluded_platforms: &[String],
) -> Result<Option<DiscoveryPublicPageInput>, String> {
    let connection = repository.read_connection()?;
    let mut statement = connection.prepare(
        "SELECT c.id,c.source_profile_ref,c.criteria_revision,l.value,
                json_extract(c.evidence_json,'$.video_description'),c.verdict
         FROM discovery_candidates c
         JOIN discovery_campaigns d ON d.id=c.discovery_campaign_id
         JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
         JOIN discovery_platform_runs p ON p.discovery_campaign_id=d.id AND p.platform=c.platform AND p.profile_ref=c.source_profile_ref
         JOIN discovery_search_hits h ON h.discovery_campaign_id=d.id AND h.platform=c.platform AND h.profile_ref=c.source_profile_ref
           AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
         JOIN discovery_video_completions v ON v.search_hit_id=h.id
         JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id
           AND b.discovery_campaign_id=d.id AND b.criteria_revision=d.current_revision
           AND b.native_author_id=c.platform_account_id
         JOIN json_each(c.evidence_json,'$.public_links') l
         WHERE d.id=?1 AND d.phase='active' AND (d.max_calls IS NULL OR d.used_calls<d.max_calls)
           AND a.approved_at IS NOT NULL AND c.criteria_revision=d.current_revision
           AND NOT EXISTS(SELECT 1 FROM json_each(?2) excluded WHERE excluded.value=c.platform)
           AND ((c.verdict='pending' AND (COALESCE(json_extract(a.criteria_json,'$.requires_public_tool_url'),1)=1
                OR json_extract(a.criteria_json,'$.region')='中国大陆以外'))
             OR (c.verdict='qualified' AND EXISTS(SELECT 1 FROM discovery_recommendation_policies policy
                 WHERE policy.discovery_campaign_id=d.id AND policy.criteria_revision=d.current_revision AND policy.policy='score_and_contact_v1')
                 AND NOT EXISTS(SELECT 1 FROM discovery_contact_observations contact WHERE contact.candidate_id=c.id AND contact.criteria_revision=c.criteria_revision)))
           AND json_type(c.evidence_json,'$.video_description')='text' AND l.type='text'
           AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=d.id AND r.status='searching')
           AND ((p.status='completed' AND NOT EXISTS(
                 SELECT 1 FROM discovery_public_page_reads r WHERE r.candidate_id=c.id AND r.requested_url=l.value))
             OR (p.status='technical_stop' AND p.stop_code='interrupted_public_page_read' AND EXISTS(
                 SELECT 1 FROM discovery_public_page_reads r WHERE r.candidate_id=c.id AND r.requested_url=l.value
                   AND r.status='reading' AND r.search_hit_id=h.id
                   AND r.source_excerpt=json_extract(c.evidence_json,'$.video_description')
                   AND p.active_query='page:'||c.id||':'||r.call_number)))
         ORDER BY c.id,l.key",
    ).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(
            params![
                campaign,
                serde_json::to_string(excluded_platforms).map_err(|e| e.to_string())?
            ],
            |row| {
                Ok((
                    DiscoveryPublicPageInput {
                        campaign_id: campaign.into(),
                        candidate_id: row.get(0)?,
                        profile_ref: row.get(1)?,
                        expected_revision: row.get(2)?,
                        requested_url: row.get(3)?,
                    },
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (input, description, verdict) = row.map_err(|e| e.to_string())?;
        // Qualified authors use the contact planner, which prioritizes the
        // native biography and filters product destinations.
        if verdict == "qualified" {
            continue;
        }
        if input.requested_url.len() <= 2048
            && input.requested_url.starts_with("https://")
            && !crate::discovery_public_pages::is_community_invite_link(&input.requested_url)
            && crate::discovery_public_pages::description_observes_link(
                &description,
                &input.requested_url,
            )
        {
            if !contact_lookup_score_valid(&connection, &input)? {
                continue;
            }
            return Ok(Some(input));
        }
    }
    super::discovery_contacts::next_native_link(&connection, campaign, excluded_platforms)
}

fn contact_lookup_score_valid(
    connection: &rusqlite::Connection,
    input: &DiscoveryPublicPageInput,
) -> Result<bool, String> {
    let verdict: String = connection.query_row("SELECT verdict FROM discovery_candidates WHERE id=?1 AND discovery_campaign_id=?2 AND criteria_revision=?3",
        params![input.candidate_id,input.campaign_id,input.expected_revision], |r| r.get(0)).map_err(|e| e.to_string())?;
    Ok(verdict == "pending"
        || super::discovery_contacts::eligible_author(
            connection,
            &input.campaign_id,
            input.expected_revision,
            &input.candidate_id,
        )?
        .is_some())
}

pub(super) fn claim_public_page(
    repository: &SqliteRepository,
    input: DiscoveryPublicPageInput,
) -> Result<DiscoveryPublicPageClaim, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let (used, maximum): (u32,Option<u32>) = transaction.query_row(
        "SELECT d.used_calls,d.max_calls FROM discovery_campaigns d JOIN discovery_campaign_revisions a
         ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
         WHERE d.id=?1 AND d.current_revision=?2 AND d.phase='active' AND a.approved_at IS NOT NULL
",
        params![input.campaign_id,input.expected_revision],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?
        .ok_or("Public page read is not approved at this Campaign revision")?;
    if maximum.is_some_and(|max| used >= max) {
        return Err("Campaign read budget is exhausted".into());
    }
    let not_ready: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND status='searching')",
        [&input.campaign_id],|r|r.get(0)).map_err(|e|e.to_string())?;
    if not_ready {
        return Err("A Campaign read is active".into());
    }
    let saved: Option<(String,String,String,String)> = transaction.query_row(
        "SELECT h.id,h.source_url,json_extract(c.evidence_json,'$.video_description'),c.platform
         FROM discovery_candidates c JOIN discovery_search_hits h ON h.discovery_campaign_id=c.discovery_campaign_id
           AND h.platform=c.platform AND h.profile_ref=c.source_profile_ref
           AND h.source_url=json_extract(c.evidence_json,'$.source_urls[0]')
         JOIN discovery_video_completions v ON v.search_hit_id=h.id
         JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id
           AND b.discovery_campaign_id=c.discovery_campaign_id AND b.criteria_revision=c.criteria_revision
           AND b.native_author_id=c.platform_account_id
         WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3 AND c.source_profile_ref=?4
           AND ((c.verdict='pending' AND EXISTS(SELECT 1 FROM discovery_campaign_revisions a
                WHERE a.discovery_campaign_id=c.discovery_campaign_id AND a.revision=c.criteria_revision
                  AND (COALESCE(json_extract(a.criteria_json,'$.requires_public_tool_url'),1)=1 OR json_extract(a.criteria_json,'$.region')='中国大陆以外')))
             OR (c.verdict='qualified' AND EXISTS(SELECT 1 FROM discovery_recommendation_policies policy
                 WHERE policy.discovery_campaign_id=c.discovery_campaign_id AND policy.criteria_revision=c.criteria_revision AND policy.policy='score_and_contact_v1')
                 AND NOT EXISTS(SELECT 1 FROM discovery_contact_observations contact WHERE contact.candidate_id=c.id AND contact.criteria_revision=c.criteria_revision)))
           AND json_type(c.evidence_json,'$.video_description')='text'
           AND EXISTS(SELECT 1 FROM json_each(c.evidence_json,'$.public_links') l WHERE l.type='text' AND l.value=?5)",
        params![input.candidate_id,input.campaign_id,input.expected_revision,input.profile_ref,input.requested_url],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
    let (hit, source, description, platform) = match saved {
        Some(source) => source,
        None => super::discovery_contacts::native_link_source(&transaction, &input)?
            .ok_or("Public link has no pending metadata source in this Campaign scope")?,
    };
    if input.requested_url.len() > 2048
        || !input.requested_url.starts_with("https://")
        || crate::discovery_public_pages::is_community_invite_link(&input.requested_url)
        || (!crate::discovery_public_pages::description_observes_link(
            &description,
            &input.requested_url,
        ) && super::discovery_contacts::native_link_source(&transaction, &input)?
            != Some((
                hit.clone(),
                source.clone(),
                description.clone(),
                platform.clone(),
            )))
    {
        return Err(
            "Public link is not an eligible exact token in the observed source description".into(),
        );
    }
    if !contact_lookup_score_valid(&transaction, &input)? {
        return Err("contact_lookup_score_not_audited".into());
    }
    let (status,stop,active): (String,Option<String>,Option<String>) = transaction.query_row(
        "SELECT status,stop_code,active_query FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
        params![input.campaign_id,platform,input.profile_ref],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).map_err(|e|e.to_string())?;
    let prior: Option<(u32,String,String,String)> = transaction.query_row(
        "SELECT call_number,status,search_hit_id,source_excerpt FROM discovery_public_page_reads WHERE candidate_id=?1 AND requested_url=?2",
        params![input.candidate_id,input.requested_url],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
    let interrupted = prior
        .as_ref()
        .is_some_and(|(call, state, old_hit, excerpt)| {
            state == "reading"
                && old_hit == &hit
                && excerpt == &description
                && status == "technical_stop"
                && stop.as_deref() == Some("interrupted_public_page_read")
                && active.as_deref() == Some(format!("page:{}:{call}", input.candidate_id).as_str())
        });
    if !interrupted && (status != "completed" || prior.is_some()) {
        return Err("Public page already attempted or platform stopped".into());
    }
    let call = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    let timestamp = now();
    transaction
        .execute(
            "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
            params![input.campaign_id, call, timestamp],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute(
        "UPDATE discovery_platform_runs SET status='searching',active_query=?4,stop_code=NULL,updated_at=?5
         WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3",
        params![input.campaign_id,platform,input.profile_ref,format!("page:{}:{call}",input.candidate_id),timestamp]).map_err(|e|e.to_string())?;
    transaction.execute(
        "INSERT INTO discovery_public_page_reads VALUES(?1,?2,?3,?4,'reading',?5,NULL,'','',0,NULL,?6)
         ON CONFLICT(candidate_id,requested_url) DO UPDATE SET call_number=excluded.call_number,updated_at=excluded.updated_at",
        params![input.candidate_id,input.requested_url,hit,call,description,timestamp]).map_err(|e|e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(DiscoveryPublicPageClaim {
        input,
        search_hit_id: hit,
        source_url: source,
        call_number: call,
    })
}

pub(super) fn finish_public_page(
    repository: &SqliteRepository,
    input: FinishDiscoveryPublicPageInput,
) -> Result<DiscoveryPublicPageRead, String> {
    crate::discovery_public_pages::validate_page_result(&input)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let scope = &claim.input;
    let platform: String = transaction
        .query_row(
            "SELECT platform FROM discovery_search_hits WHERE id=?1 AND discovery_campaign_id=?2",
            params![claim.search_hit_id, scope.campaign_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if input.stop_code.is_none()
        && (platform != "youtube"
            || super::discovery_contacts::eligible_author(
                &transaction,
                &scope.campaign_id,
                scope.expected_revision,
                &scope.candidate_id,
            )?
            .is_some())
    {
        let source = super::discovery_contacts::native_link_source(&transaction, scope)?
            .ok_or("contact_source_no_longer_eligible")?;
        if source.0 != claim.search_hit_id || source.1 != claim.source_url {
            return Err("contact_source_claim_mismatch".into());
        }
        let excerpt: String = transaction.query_row("SELECT source_excerpt FROM discovery_public_page_reads WHERE candidate_id=?1 AND requested_url=?2",
            params![scope.candidate_id,scope.requested_url], |r|r.get(0)).map_err(|e|e.to_string())?;
        if excerpt != source.2 {
            return Err("contact_source_changed".into());
        }
    }
    let timestamp = now();
    // Failure cleanup checks the exact durable reservation and approval below,
    // without rebuilding the material that caused the failure. It admits no
    // evidence, contact or recommendation.
    let contact_lookup_page: bool = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_candidates c JOIN discovery_recommendation_policies p
         ON p.discovery_campaign_id=c.discovery_campaign_id AND p.criteria_revision=c.criteria_revision
         WHERE c.id=?1 AND c.discovery_campaign_id=?2 AND c.criteria_revision=?3
         AND c.verdict='qualified' AND p.policy='score_and_contact_v1')",
        params![scope.candidate_id,scope.campaign_id,scope.expected_revision], |row|row.get(0),
    ).map_err(|error|error.to_string())?;
    let lookup_site_failure = contact_lookup_page
        && input.stop_code.as_deref().is_some_and(|code| {
            matches!(
                code,
                "opencli_risk_control"
                    | "opencli_auth_required"
                    | "opencli_command_failed"
                    | "opencli_timeout"
                    | "opencli_invalid_output"
                    | "opencli_invalid_input"
                    | "opencli_output_too_large"
            )
        });
    let local_failure = lookup_site_failure
        || crate::discovery::source_local_read_failure(input.stop_code.as_deref());
    let platform_status = if local_failure {
        "completed"
    } else {
        crate::discovery::read_platform_status(input.stop_code.as_deref())
    };
    let affected=transaction.execute(
        "UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
         WHERE discovery_campaign_id=?1 AND profile_ref=?2 AND status='searching' AND active_query=?3
           AND EXISTS(SELECT 1 FROM discovery_campaigns d JOIN discovery_campaign_revisions a ON a.discovery_campaign_id=d.id AND a.revision=d.current_revision
             WHERE d.id=?1 AND d.phase='active' AND d.current_revision=?7 AND a.approved_at IS NOT NULL)
           AND EXISTS(SELECT 1 FROM discovery_search_hits h JOIN discovery_candidates c ON c.id=?8
             LEFT JOIN discovery_video_identity_bindings b ON b.search_hit_id=h.id AND b.candidate_id=c.id
               AND b.discovery_campaign_id=?1 AND b.criteria_revision=?7
               AND b.native_author_id=c.platform_account_id
             WHERE h.id=?9 AND h.discovery_campaign_id=?1 AND c.discovery_campaign_id=?1
               AND c.criteria_revision=?7 AND (c.platform!='youtube' OR b.search_hit_id IS NOT NULL)
               AND h.platform=discovery_platform_runs.platform AND c.source_platform=h.platform
               AND c.source_profile_ref=?2 AND h.profile_ref=?2 AND h.source_url=?10)",
        params![scope.campaign_id,scope.profile_ref,format!("page:{}:{}",scope.candidate_id,claim.call_number),
            platform_status,if local_failure {None} else {input.stop_code.as_deref()},timestamp,scope.expected_revision,scope.candidate_id,claim.search_hit_id,claim.source_url]).map_err(|e|e.to_string())?;
    if affected != 1 {
        return Err("Public page claim is stale or authorization changed".into());
    }
    let affected=transaction.execute(
        "UPDATE discovery_public_page_reads SET status=?5,document_url=?6,title=?7,markdown_excerpt=?8,truncated=?9,stop_code=?10,updated_at=?11
         WHERE candidate_id=?1 AND requested_url=?2 AND search_hit_id=?3 AND call_number=?4 AND status='reading'",
        params![scope.candidate_id,scope.requested_url,claim.search_hit_id,claim.call_number,
            if input.stop_code.is_some(){"failed"}else{"completed"},input.document_url,input.title,input.markdown_excerpt,input.truncated,input.stop_code,timestamp]).map_err(|e|e.to_string())?;
    if affected != 1 {
        return Err("Public page receipt is stale".into());
    }
    let result = load_public_page_row(
        &transaction,
        &scope.campaign_id,
        &scope.candidate_id,
        &scope.requested_url,
    )?;
    super::discovery_contacts::observe_public_page(
        &transaction,
        &scope.campaign_id,
        scope.expected_revision,
        &result,
    )?;
    if input.hidden_email_available {
        super::discovery_contacts::observe_hidden_email(
            &transaction,
            &scope.campaign_id,
            scope.expected_revision,
            &result,
        )?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn get_public_page(
    repository: &SqliteRepository,
    campaign: &str,
    candidate: &str,
    url: &str,
) -> Result<DiscoveryPublicPageRead, String> {
    let connection = repository.read_connection()?;
    load_public_page_row(&connection, campaign, candidate, url)
}

fn load_public_page_row(
    connection: &rusqlite::Connection,
    campaign: &str,
    candidate: &str,
    url: &str,
) -> Result<DiscoveryPublicPageRead, String> {
    connection.query_row(
        "SELECT p.candidate_id,p.requested_url,p.search_hit_id,h.source_url,p.source_excerpt,h.profile_ref,c.platform_account_id,
           p.call_number,p.status,p.document_url,p.title,p.markdown_excerpt,p.truncated,p.stop_code,p.updated_at
         FROM discovery_public_page_reads p JOIN discovery_candidates c ON c.id=p.candidate_id
         JOIN discovery_search_hits h ON h.id=p.search_hit_id
         WHERE p.candidate_id=?1 AND p.requested_url=?2 AND c.discovery_campaign_id=?3 AND h.discovery_campaign_id=?3",
        params![candidate,url,campaign],|r|Ok(DiscoveryPublicPageRead {
            candidate_id:r.get(0)?,requested_url:r.get(1)?,search_hit_id:r.get(2)?,source_url:r.get(3)?,source_excerpt:r.get(4)?,
            profile_ref:r.get(5)?,platform_account_id:r.get(6)?,call_number:r.get(7)?,status:r.get(8)?,document_url:r.get(9)?,
            title:r.get(10)?,markdown_excerpt:r.get(11)?,truncated:r.get(12)?,stop_code:r.get(13)?,captured_at:r.get(14)?,
        })).optional().map_err(|e|e.to_string())?.ok_or_else(||"Public page read not found in this Campaign".into())
}
