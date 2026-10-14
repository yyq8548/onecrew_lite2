use rusqlite::{TransactionBehavior, params};
use std::collections::BTreeMap;

use super::SqliteRepository;
use crate::{
    DiscoveryIdentityGroupsBasis, SubmitDiscoveryIdentityReviewInput,
    discovery_identity_merge::groups::IdentityGroupReviewSource,
};

const MAX_RECORDS: usize = 10_000;
const MAX_BYTES: usize = 32 * 1024 * 1024;

fn source_bytes(pair: &[crate::DiscoveryReviewBasis; 2]) -> Result<usize, String> {
    let mut bytes = 0;
    for basis in pair {
        bytes += serde_json::to_vec(&(
            &basis.campaign_id,
            basis.revision,
            &basis.criteria_json,
            &basis.scopes_json,
            &basis.hit,
            &basis.candidate_id,
            &basis.native_author_id,
            &basis.native_profile,
            &basis.evidence_json,
            &basis.transcript,
            &basis.source_description,
            &basis.native_content,
            &basis.pages,
        ))
        .map_err(|_| "identity_groups_source_invalid")?
        .len();
    }
    Ok(bytes)
}

pub(super) fn read(
    repository: &SqliteRepository,
    campaign: &str,
    revision: u32,
) -> Result<DiscoveryIdentityGroupsBasis, String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let basis = read_snapshot(&tx, campaign, revision)?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(basis)
}

pub(super) fn read_snapshot(
    connection: &rusqlite::Connection,
    campaign: &str,
    revision: u32,
) -> Result<DiscoveryIdentityGroupsBasis, String> {
    let library = super::discovery_library::load_basis(connection, campaign, revision)?;
    let mut bytes = library.criteria_json.len()
        + library.scopes_json.len()
        + serde_json::to_vec(&library.rows)
            .map_err(|_| "identity_groups_source_invalid")?
            .len();
    if bytes > MAX_BYTES {
        return Err("identity_groups_snapshot_too_large".into());
    }
    let mut native_details = BTreeMap::new();
    let mut statement = connection.prepare(
        "SELECT id,display_name,profile_url FROM discovery_candidates WHERE discovery_campaign_id=?1 AND criteria_revision=?2 ORDER BY id LIMIT 10001",
    ).map_err(|e| e.to_string())?;
    let records = statement
        .query_map(params![campaign, revision], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for record in records {
        let (id, name, url) = record.map_err(|e| e.to_string())?;
        bytes += id.len() + name.len() + url.len();
        if native_details.len() == MAX_RECORDS || bytes > MAX_BYTES {
            return Err("identity_groups_snapshot_too_large".into());
        }
        native_details.insert(id, (name, url));
    }
    drop(statement);
    let mut statement = connection.prepare(
        "SELECT r.decision_id,r.operator_account_id,r.criteria_revision,r.kind,r.left_candidate_id,r.right_candidate_id,
         r.material_hash,r.request_hash,r.input_json,r.receipt_json,r.revokes_decision_id,r.created_at,
         EXISTS(SELECT 1 FROM discovery_identity_reviews x WHERE x.discovery_campaign_id=r.discovery_campaign_id
           AND x.kind='revoke' AND x.revokes_decision_id=r.decision_id)
         FROM discovery_identity_reviews r WHERE r.discovery_campaign_id=?1 ORDER BY r.decision_id LIMIT 10001",
    ).map_err(|e| e.to_string())?;
    let records = statement
        .query_map([campaign], |row| {
            Ok((
                IdentityGroupReviewSource {
                    decision_id: row.get(0)?,
                    account: row.get(1)?,
                    revision: row.get(2)?,
                    kind: row.get(3)?,
                    candidate_ids: [row.get(4)?, row.get(5)?],
                    material_hash: row.get(6)?,
                    request_hash: row.get(7)?,
                    input_json: row.get(8)?,
                    receipt_json: row.get(9)?,
                    revokes_decision_id: row.get(10)?,
                    created_at: row.get(11)?,
                    current_pair: None,
                },
                row.get::<_, bool>(12)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut reviews = Vec::new();
    for record in records {
        let (mut source, revoked) = record.map_err(|e| e.to_string())?;
        bytes += source.decision_id.len()
            + source.account.len()
            + source.input_json.len()
            + source.receipt_json.len();
        if reviews.len() == MAX_RECORDS || bytes > MAX_BYTES {
            return Err("identity_groups_snapshot_too_large".into());
        }
        if !revoked
            && source.revision == revision
            && matches!(
                source.kind.as_str(),
                "same_author" | "different_authors" | "insufficient"
            )
        {
            let input: SubmitDiscoveryIdentityReviewInput =
                serde_json::from_str(&source.input_json)
                    .map_err(|_| "identity_groups_ledger_invalid")?;
            if input.pair.campaign_id != campaign || input.pair.expected_revision != revision {
                return Err("identity_groups_ledger_invalid".into());
            }
            match super::discovery_review::load_pair_basis(
                connection,
                campaign,
                revision,
                &input.pair.left_search_hit_id,
                &input.pair.right_search_hit_id,
            ) {
                Ok(pair) => {
                    bytes += source_bytes(&pair)?;
                    if bytes > MAX_BYTES {
                        return Err("identity_groups_snapshot_too_large".into());
                    }
                    source.current_pair = Some(Box::new(pair));
                }
                Err(code)
                    if matches!(
                        code.as_str(),
                        "review_source_unavailable"
                            | "review_material_author_binding_unavailable"
                            | "review_native_profile_binding_unavailable"
                            | "review_native_content_binding_unavailable"
                    ) => {}
                Err(code) => return Err(code),
            }
        }
        reviews.push(source);
    }
    Ok(DiscoveryIdentityGroupsBasis {
        library,
        native_details,
        reviews,
    })
}

#[cfg(test)]
mod tests;
