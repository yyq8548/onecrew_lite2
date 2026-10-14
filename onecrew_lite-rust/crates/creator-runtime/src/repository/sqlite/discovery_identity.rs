use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};
use crate::{DiscoveryIdentityClaim, DiscoveryIdentityRead, FinishDiscoveryIdentityInput};

/// Planning only. The selected profile hit already obeys source, author
/// deduplication and stop rules; claiming still checks current authorization.
pub(super) fn profile_needs_identity(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<bool, String> {
    repository
        .read_connection()?
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_search_hits h
         WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='twitter'
           AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id))",
            params![hit, campaign],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())
}

pub(super) fn claim_identity_read(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryIdentityClaim, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let used: u32 = transaction.query_row(
        "SELECT c.used_calls FROM discovery_campaigns c
         JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
         WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL
           AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
           AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs r WHERE r.discovery_campaign_id=c.id AND r.status='searching')",
        params![campaign,revision], |r|r.get(0)).optional().map_err(|e| e.to_string())?
        .ok_or("Post identity needs approved, idle Campaign and available budget")?;
    let mut claim = transaction.query_row(
        "SELECT h.profile_ref,a.author_id,h.source_url FROM discovery_search_hits h
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         JOIN discovery_platform_runs r ON r.discovery_campaign_id=h.discovery_campaign_id
           AND r.platform=h.platform AND r.profile_ref=h.profile_ref
         WHERE h.id=?1 AND h.discovery_campaign_id=?2 AND h.platform='twitter' AND r.status='completed'
           AND NOT EXISTS(SELECT 1 FROM discovery_identity_reads i WHERE i.search_hit_id=h.id)",
        params![hit,campaign], |r|Ok(DiscoveryIdentityClaim {
            campaign_id:campaign.into(),expected_revision:revision,search_hit_id:hit.into(),
            profile_ref:r.get(0)?,author_handle:r.get(1)?,source_url:r.get(2)?,post_id:String::new(),call_number:0
        })).optional().map_err(|e| e.to_string())?.ok_or("Post identity source unavailable or already attempted")?;
    claim.post_id =
        crate::discovery_identity::post_id_for_source(&claim.author_handle, &claim.source_url)?;
    claim.call_number = used
        .checked_add(1)
        .ok_or("Campaign call counter overflow")?;
    let timestamp = now();
    transaction
        .execute(
            "UPDATE discovery_campaigns SET used_calls=?2,updated_at=?3 WHERE id=?1",
            params![campaign, claim.call_number, timestamp],
        )
        .map_err(|e| e.to_string())?;
    transaction.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?3,stop_code=NULL,updated_at=?4
        WHERE discovery_campaign_id=?1 AND platform='twitter' AND profile_ref=?2",
        params![campaign,claim.profile_ref,format!("identity:{hit}:{}",claim.call_number),timestamp]).map_err(|e| e.to_string())?;
    transaction.execute("INSERT INTO discovery_identity_reads VALUES(?1,?2,'twitter',?3,?4,?5,?6,?7,?8,'reading',NULL,NULL,NULL,?9)",
        params![hit,campaign,claim.profile_ref,claim.author_handle,claim.source_url,claim.post_id,revision,claim.call_number,timestamp])
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

pub(super) fn finish_identity_read(
    repository: &SqliteRepository,
    input: FinishDiscoveryIdentityInput,
) -> Result<DiscoveryIdentityRead, String> {
    crate::discovery_identity::validate_identity_result(&input)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let prior = load_identity_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != *claim || prior.status != "reading" {
        return Err("Post identity claim is stale or its source changed".into());
    }
    let platform_status = crate::discovery::read_platform_status(input.stop_code.as_deref());
    let run_stop = input
        .stop_code
        .as_deref()
        .filter(|code| !crate::discovery::source_local_read_failure(Some(code)));
    let timestamp = now();
    let changed = transaction.execute("UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
        WHERE discovery_campaign_id=?1 AND platform='twitter' AND profile_ref=?2 AND status='searching' AND active_query=?3
          AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v
            ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
            WHERE c.id=?1 AND c.phase='active' AND c.current_revision=?7 AND v.approved_at IS NOT NULL)",
        params![claim.campaign_id,claim.profile_ref,format!("identity:{}:{}",claim.search_hit_id,claim.call_number),
            platform_status,run_stop,timestamp,claim.expected_revision]).map_err(|e| e.to_string())?;
    if changed != 1 {
        return Err("Post identity authorization changed or claim interrupted".into());
    }
    transaction.execute("UPDATE discovery_identity_reads SET status=?2,account_id=?3,observed_source_url=?4,stop_code=?5,updated_at=?6
        WHERE search_hit_id=?1",
        params![claim.search_hit_id,if input.stop_code.is_some() {"failed"} else {"completed"},
            input.account_id,input.observed_source_url,input.stop_code,timestamp]).map_err(|e| e.to_string())?;
    let receipt = load_identity_row(&transaction, &claim.campaign_id, &claim.search_hit_id)?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(receipt)
}

pub(super) fn get_identity_read(
    repository: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryIdentityRead, String> {
    let connection = repository.read_connection()?;
    load_identity_row(&connection, campaign, hit)
}

pub(super) fn load_identity_row(
    connection: &rusqlite::Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryIdentityRead, String> {
    connection.query_row(
        "SELECT i.expected_revision,i.profile_ref,i.author_handle,i.source_url,i.post_id,i.call_number,
           i.status,i.account_id,i.observed_source_url,i.stop_code,i.updated_at
         FROM discovery_identity_reads i
         JOIN discovery_search_hits h ON h.id=i.search_hit_id
         JOIN discovery_search_authors a ON a.search_hit_id=h.id
         WHERE i.discovery_campaign_id=?1 AND i.search_hit_id=?2 AND h.discovery_campaign_id=?1
           AND h.platform=i.platform AND h.profile_ref=i.profile_ref AND h.source_url=i.source_url AND a.author_id=i.author_handle",
        params![campaign,hit], |r|Ok(DiscoveryIdentityRead {
            claim:DiscoveryIdentityClaim {campaign_id:campaign.into(),search_hit_id:hit.into(),
                expected_revision:r.get(0)?,profile_ref:r.get(1)?,author_handle:r.get(2)?,source_url:r.get(3)?,
                post_id:r.get(4)?,call_number:r.get(5)?},
            status:r.get(6)?,account_id:r.get(7)?,observed_source_url:r.get(8)?,stop_code:r.get(9)?,captured_at:r.get(10)?
        })).optional().map_err(|e| e.to_string())?.ok_or("Post identity read not found in this Campaign".into())
}
