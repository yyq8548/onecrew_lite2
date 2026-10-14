use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::SqliteRepository;
use crate::{
    DiscoveryIdentityMergeMaterialInput, DiscoveryIdentitySubmissionBasis,
    PreparedDiscoveryIdentityReview, StoredDiscoveryIdentityReview,
};

fn find_record(
    connection: &rusqlite::Connection,
    campaign: &str,
    decision: &str,
) -> Result<Option<StoredDiscoveryIdentityReview>, String> {
    connection.query_row(
        "SELECT request_hash,receipt_json FROM discovery_identity_reviews WHERE discovery_campaign_id=?1 AND decision_id=?2",
        params![campaign,decision], |row| Ok(StoredDiscoveryIdentityReview { request_hash: row.get(0)?, receipt_json: row.get(1)? })
    ).optional().map_err(|e| e.to_string())
}

pub(super) fn find(
    repository: &SqliteRepository,
    campaign: &str,
    decision: &str,
) -> Result<Option<StoredDiscoveryIdentityReview>, String> {
    let connection = repository.read_connection()?;
    find_record(&connection, campaign, decision)
}

pub(super) fn read_submission(
    repository: &SqliteRepository,
    pair: &DiscoveryIdentityMergeMaterialInput,
    decision: &str,
) -> Result<DiscoveryIdentitySubmissionBasis, String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let result = match find_record(&tx, &pair.campaign_id, decision)? {
        Some(stored) => DiscoveryIdentitySubmissionBasis::Committed(stored),
        None => DiscoveryIdentitySubmissionBasis::Material(Box::new(
            super::discovery_review::load_pair_basis(
                &tx,
                &pair.campaign_id,
                pair.expected_revision,
                &pair.left_search_hit_id,
                &pair.right_search_hit_id,
            )?,
        )),
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn read_revocation(
    repository: &SqliteRepository,
    input: &crate::RevokeDiscoveryIdentityReviewInput,
) -> Result<crate::DiscoveryIdentityRevocationBasis, String> {
    let mut connection = repository.read_connection()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|e| e.to_string())?;
    let result = if let Some(stored) = find_record(&tx, &input.campaign_id, &input.decision_id)? {
        crate::DiscoveryIdentityRevocationBasis::Committed(stored)
    } else {
        let stored = find_record(&tx, &input.campaign_id, &input.revokes_decision_id)?
            .ok_or("identity_review_not_found")?;
        let material_json = tx.query_row(
            "SELECT material_json FROM discovery_identity_reviews WHERE discovery_campaign_id=?1 AND decision_id=?2",
            params![input.campaign_id, input.revokes_decision_id], |row| row.get(0),
        ).map_err(|e| e.to_string())?;
        crate::DiscoveryIdentityRevocationBasis::Original {
            stored,
            material_json,
        }
    };
    tx.commit().map_err(|e| e.to_string())?;
    Ok(result)
}

pub(super) fn commit_revocation(
    repository: &SqliteRepository,
    prepared: &crate::PreparedDiscoveryIdentityRevocation,
) -> Result<StoredDiscoveryIdentityReview, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let receipt = &prepared.receipt;
    // An uncertain or concurrent identical write is reconciled before state checks.
    if let Some(stored) = find_record(&tx, &receipt.campaign_id, &receipt.decision_id)? {
        if stored.request_hash != prepared.request_hash {
            return Err("identity_review_idempotency_conflict".into());
        }
        return Ok(stored);
    }
    let current: Option<(u32, String)> = tx
        .query_row(
            "SELECT current_revision,phase FROM discovery_campaigns WHERE id=?1",
            [&receipt.campaign_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (revision, status) = current.ok_or("campaign_not_found")?;
    if revision != receipt.criteria_revision {
        return Err("identity_review_revision_changed".into());
    }
    if status != "active" {
        return Err("identity_review_campaign_inactive".into());
    }
    let target = receipt
        .revokes_decision_id
        .as_ref()
        .ok_or("identity_review_input_invalid")?;
    let original =
        find_record(&tx, &receipt.campaign_id, target)?.ok_or("identity_review_not_found")?;
    if original.request_hash != prepared.original_request_hash
        || original.receipt_json != prepared.original_receipt_json
    {
        return Err("identity_review_original_changed".into());
    }
    let revoked: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM discovery_identity_reviews WHERE discovery_campaign_id=?1 AND kind='revoke' AND revokes_decision_id=?2)",
        params![receipt.campaign_id, target], |row| row.get(0),
    ).map_err(|e| e.to_string())?;
    if revoked {
        return Err("identity_review_already_revoked".into());
    }
    tx.execute("INSERT INTO discovery_identity_reviews (
        discovery_campaign_id,decision_id,operator_account_id,criteria_revision,kind,
        left_candidate_id,right_candidate_id,material_hash,request_hash,input_json,material_json,receipt_json,revokes_decision_id,created_at
        ) VALUES(?1,?2,?3,?4,'revoke',?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![receipt.campaign_id,receipt.decision_id,receipt.operator_account_id,receipt.criteria_revision,
            receipt.candidate_ids[0],receipt.candidate_ids[1],receipt.material_hash,prepared.request_hash,
            prepared.input_json,prepared.material_json,prepared.receipt_json,target,receipt.created_at]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(StoredDiscoveryIdentityReview {
        request_hash: prepared.request_hash.clone(),
        receipt_json: prepared.receipt_json.clone(),
    })
}

pub(super) fn commit(
    repository: &SqliteRepository,
    prepared: &PreparedDiscoveryIdentityReview,
) -> Result<StoredDiscoveryIdentityReview, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| e.to_string())?;
    let receipt = &prepared.receipt;
    // Reconcile a concurrent or uncertain completed submission before reading changed sources.
    if let Some(stored) = find_record(&tx, &receipt.campaign_id, &receipt.decision_id)? {
        if stored.request_hash != prepared.request_hash {
            return Err("identity_review_idempotency_conflict".into());
        }
        return Ok(stored);
    }
    if super::discovery_review::load_pair_basis(
        &tx,
        &receipt.campaign_id,
        receipt.criteria_revision,
        &prepared.basis[0].hit.id,
        &prepared.basis[1].hit.id,
    )? != prepared.basis
    {
        return Err("identity_review_material_changed".into());
    }
    let mut statement = tx.prepare("SELECT r.left_candidate_id,r.right_candidate_id,r.kind
        FROM discovery_identity_reviews r WHERE r.discovery_campaign_id=?1 AND r.criteria_revision=?2
        AND r.kind IN ('same_author','different_authors')
        AND NOT EXISTS(SELECT 1 FROM discovery_identity_reviews x WHERE x.discovery_campaign_id=r.discovery_campaign_id
          AND x.kind='revoke' AND x.revokes_decision_id=r.decision_id) LIMIT 10001").map_err(|e| e.to_string())?;
    let relationships = statement
        .query_map(
            params![receipt.campaign_id, receipt.criteria_revision],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if relationships.len() > 10000 {
        return Err("identity_review_ledger_too_large".into());
    }
    crate::discovery_identity_merge::relationships::validate_addition(
        &relationships,
        receipt.kind,
        &receipt.candidate_ids,
    )?;
    drop(statement);
    tx.execute("INSERT INTO discovery_identity_reviews (
        discovery_campaign_id,decision_id,operator_account_id,criteria_revision,kind,
        left_candidate_id,right_candidate_id,material_hash,request_hash,input_json,material_json,receipt_json,revokes_decision_id,created_at
        ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,NULL,?13)",
        params![receipt.campaign_id,receipt.decision_id,receipt.operator_account_id,receipt.criteria_revision,receipt.kind.as_str(),
            receipt.candidate_ids[0],receipt.candidate_ids[1],receipt.material_hash,prepared.request_hash,
            prepared.input_json,prepared.material_json,prepared.receipt_json,receipt.created_at]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(StoredDiscoveryIdentityReview {
        request_hash: prepared.request_hash.clone(),
        receipt_json: prepared.receipt_json.clone(),
    })
}
