use super::{SqliteRepository, now};
use crate::{
    DiscoveryFacebookIdentityClaim, DiscoveryFacebookIdentityRead,
    FinishDiscoveryFacebookIdentityInput,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(super) fn next_hit(
    repo: &SqliteRepository,
    campaign: &str,
    excluded: &[String],
) -> Result<Option<String>, String> {
    if excluded.iter().any(|p| p == "facebook") {
        return Ok(None);
    }
    let db = repo.read_connection()?;
    let mut statement=db.prepare("SELECT h.id,h.source_url FROM discovery_search_hits h
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE c.id=?1 AND c.phase='active' AND v.approved_at IS NOT NULL AND (c.max_calls IS NULL OR c.used_calls<c.max_calls)
        AND h.platform='facebook' AND r.status='completed'
        AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')
      ORDER BY h.captured_at,COALESCE((SELECT position FROM discovery_search_positions WHERE search_hit_id=h.id),9223372036854775807),h.id").map_err(|e|e.to_string())?;
    let rows = statement
        .query_map([campaign], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, url) = row.map_err(|e| e.to_string())?;
        if crate::discovery_facebook_identity::valid_source(&url) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(super) fn claim(
    repo: &SqliteRepository,
    campaign: &str,
    revision: u32,
    hit: &str,
) -> Result<DiscoveryFacebookIdentityClaim, String> {
    let mut db = repo.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let(profile_ref,source_url):(String,String)=tx.query_row("SELECT h.profile_ref,h.source_url FROM discovery_search_hits h
      JOIN discovery_campaigns c ON c.id=h.discovery_campaign_id
      JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
      JOIN discovery_platform_runs r ON r.discovery_campaign_id=c.id AND r.platform=h.platform AND r.profile_ref=h.profile_ref
      WHERE c.id=?1 AND c.current_revision=?2 AND c.phase='active' AND v.approved_at IS NOT NULL
        AND (c.max_calls IS NULL OR c.used_calls<c.max_calls) AND h.id=?3 AND h.platform='facebook' AND r.status='completed'
        AND NOT EXISTS(SELECT 1 FROM discovery_search_authors a WHERE a.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_facebook_identity_reads i WHERE i.search_hit_id=h.id)
        AND NOT EXISTS(SELECT 1 FROM discovery_platform_runs b WHERE b.discovery_campaign_id=c.id AND b.status='searching')",
      params![campaign,revision,hit],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?.ok_or("Facebook identity source is unapproved, attempted, bound or stopped")?;
    if !crate::discovery_facebook_identity::valid_source(&source_url) {
        return Err("Facebook identity requires a saved vanity profile".into());
    }
    let timestamp = now();
    let call_number=tx.query_row("UPDATE discovery_campaigns SET used_calls=used_calls+1,updated_at=?2 WHERE id=?1 AND used_calls<4294967295 RETURNING used_calls",params![campaign,timestamp],|r|r.get(0)).map_err(|e|e.to_string())?;
    let claim = DiscoveryFacebookIdentityClaim {
        campaign_id: campaign.into(),
        expected_revision: revision,
        search_hit_id: hit.into(),
        profile_ref,
        source_url,
        call_number,
    };
    let changed=tx.execute("UPDATE discovery_platform_runs SET status='searching',active_query=?3,stop_code=NULL,updated_at=?4 WHERE discovery_campaign_id=?1 AND platform='facebook' AND profile_ref=?2 AND status='completed'",
      params![campaign,claim.profile_ref,format!("facebook_identity:{hit}:{call_number}"),timestamp]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Facebook identity platform changed".into());
    }
    tx.execute("INSERT INTO discovery_facebook_identity_reads VALUES(?1,?2,'facebook',?3,?4,?5,?6,'reading',NULL,NULL,?7)",params![hit,campaign,claim.profile_ref,claim.source_url,revision,call_number,timestamp]).map_err(|e|e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(claim)
}

fn load(
    db: &Connection,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryFacebookIdentityRead, String> {
    let(mut read,json):(DiscoveryFacebookIdentityRead,Option<String>)=db.query_row("SELECT i.expected_revision,i.profile_ref,i.source_url,i.call_number,i.status,i.native_identity_json,i.stop_code,i.updated_at
      FROM discovery_facebook_identity_reads i JOIN discovery_search_hits h ON h.id=i.search_hit_id
      WHERE i.discovery_campaign_id=?1 AND i.search_hit_id=?2 AND h.discovery_campaign_id=i.discovery_campaign_id
        AND h.platform=i.platform AND h.profile_ref=i.profile_ref AND h.source_url=i.source_url",
      params![campaign,hit],|r|Ok((DiscoveryFacebookIdentityRead{claim:DiscoveryFacebookIdentityClaim{campaign_id:campaign.into(),search_hit_id:hit.into(),expected_revision:r.get(0)?,profile_ref:r.get(1)?,source_url:r.get(2)?,call_number:r.get(3)?},status:r.get(4)?,native_identity:None,stop_code:r.get(6)?,captured_at:r.get(7)?},r.get(5)?))).optional().map_err(|e|e.to_string())?.ok_or("Facebook identity receipt not found at its saved source")?;
    read.native_identity = json
        .map(|s| serde_json::from_str(&s))
        .transpose()
        .map_err(|e| e.to_string())?;
    Ok(read)
}
pub(super) fn get(
    repo: &SqliteRepository,
    campaign: &str,
    hit: &str,
) -> Result<DiscoveryFacebookIdentityRead, String> {
    let db = repo.read_connection()?;
    load(&db, campaign, hit)
}

pub(super) fn finish(
    repo: &SqliteRepository,
    input: FinishDiscoveryFacebookIdentityInput,
) -> Result<DiscoveryFacebookIdentityRead, String> {
    crate::discovery_facebook_identity::validate_result(&input)?;
    let mut db = repo.lock()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let claim = &input.claim;
    let prior = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    if prior.claim != *claim {
        return Err("Facebook identity claim changed".into());
    }
    if prior.status != "reading" {
        return if prior.native_identity == input.native_identity
            && prior.stop_code == input.stop_code
        {
            Ok(prior)
        } else {
            Err("Facebook identity already finished differently".into())
        };
    }
    let timestamp = now();
    let changed=tx.execute("UPDATE discovery_platform_runs SET status=?4,active_query=NULL,stop_code=?5,updated_at=?6
      WHERE discovery_campaign_id=?1 AND platform='facebook' AND profile_ref=?2 AND status='searching' AND active_query=?3
        AND EXISTS(SELECT 1 FROM discovery_campaigns c JOIN discovery_campaign_revisions v ON v.discovery_campaign_id=c.id AND v.revision=c.current_revision
          WHERE c.id=?1 AND c.current_revision=?7 AND c.phase='active' AND v.approved_at IS NOT NULL)",
      params![claim.campaign_id,claim.profile_ref,format!("facebook_identity:{}:{}",claim.search_hit_id,claim.call_number),
        crate::discovery::read_platform_status(input.stop_code.as_deref()),input.stop_code.as_deref().filter(|code|!crate::discovery::source_local_read_failure(Some(code))),timestamp,claim.expected_revision]).map_err(|e|e.to_string())?;
    if changed != 1 {
        return Err("Facebook identity authorization changed".into());
    }
    if let Some(native) = &input.native_identity {
        let id = native["id"].as_str().ok_or("Native Facebook ID absent")?;
        // Canonical native locator derives only from the independently observed ID.
        // The actual vanity URL and its binding remain in the immutable receipt.
        tx.execute(
            "INSERT INTO discovery_search_authors VALUES(?1,?2,?3,?4)",
            params![
                claim.search_hit_id,
                id,
                format!("https://www.facebook.com/profile.php?id={id}"),
                timestamp
            ],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute("UPDATE discovery_facebook_identity_reads SET status=?3,native_identity_json=?4,stop_code=?5,updated_at=?6 WHERE search_hit_id=?1 AND call_number=?2 AND status='reading'",
      params![claim.search_hit_id,claim.call_number,if input.native_identity.is_some(){"completed"}else{"failed"},input.native_identity.as_ref().map(|value|value.to_string()),input.stop_code,timestamp]).map_err(|e|e.to_string())?;
    let result = load(&tx, &claim.campaign_id, &claim.search_hit_id)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}
