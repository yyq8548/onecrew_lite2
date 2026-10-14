use super::{SqliteRepository, now};
use crate::{AppendDiscoveryQueries, DiscoveryPlatformScope};
use rusqlite::{TransactionBehavior, params};

pub(super) fn effective_scopes(
    db: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
    json: &str,
) -> Result<Vec<DiscoveryPlatformScope>, String> {
    let mut scopes: Vec<DiscoveryPlatformScope> =
        serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut stmt = db.prepare("SELECT scopes_json FROM discovery_query_rounds WHERE discovery_campaign_id=?1 AND revision=?2 ORDER BY round").map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![campaign, revision], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let additions: Vec<DiscoveryPlatformScope> =
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        for addition in additions {
            let scope = scopes
                .iter_mut()
                .find(|s| s.platform == addition.platform && s.profile_ref == addition.profile_ref)
                .ok_or("query_round_scope_corrupt")?;
            scope.keywords.extend(addition.keywords);
        }
    }
    Ok(scopes)
}

pub(super) fn append(
    owner: &SqliteRepository,
    account: &str,
    input: &AppendDiscoveryQueries,
) -> Result<(), String> {
    let mut db = owner.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let (json, used): (String, u32) = tx.query_row("SELECT r.scope_json,c.used_calls FROM discovery_campaigns c JOIN discovery_campaign_revisions r ON r.discovery_campaign_id=c.id AND r.revision=c.current_revision WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND r.approved_at IS NOT NULL AND json_extract(r.criteria_json,'$.automatic_query_expansion')=1 AND (c.max_calls IS NULL OR c.used_calls<c.max_calls) AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs p WHERE p.discovery_campaign_id=c.id AND p.status='searching')", params![input.campaign_id,input.revision], |r| Ok((r.get(0)?,r.get(1)?))).map_err(|_| "query_round_campaign_changed".to_string())?;
    if effective_scopes(&tx, &input.campaign_id, input.revision, &json)? != input.expected_scopes {
        return Err("query_round_campaign_changed".into());
    }
    for scope in &input.additions {
        let prior = input
            .expected_scopes
            .iter()
            .find(|s| s.platform == scope.platform && s.profile_ref == scope.profile_ref)
            .ok_or("query_round_outside_scope")?;
        if super::discovery::next_search_keyword(&tx, &input.campaign_id, prior)?.is_some() {
            return Err("query_round_not_exhausted".into());
        }
        let status: String = tx.query_row("SELECT status FROM discovery_platform_runs WHERE discovery_campaign_id=?1 AND platform=?2 AND profile_ref=?3", params![input.campaign_id,scope.platform,scope.profile_ref], |r| r.get(0)).map_err(|e| e.to_string())?;
        if status != "completed" {
            return Err("query_round_platform_stopped".into());
        }
    }
    tx.execute("INSERT INTO discovery_query_rounds(discovery_campaign_id,revision,round,operator_account_id,scopes_json,used_calls,created_at) SELECT ?1,?2,COALESCE(MAX(round),0)+1,?3,?4,?5,?6 FROM discovery_query_rounds WHERE discovery_campaign_id=?1 AND revision=?2", params![input.campaign_id,input.revision,account,serde_json::to_string(&input.additions).map_err(|e| e.to_string())?,used,now()]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
