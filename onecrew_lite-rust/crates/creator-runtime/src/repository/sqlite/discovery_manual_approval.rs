use super::{SqliteRepository, now};
use crate::{ApproveDiscoveryCandidateInput, DiscoveryManualApproval};
use rusqlite::{TransactionBehavior, params};

pub(super) fn approve(
    repository: &SqliteRepository,
    input: ApproveDiscoveryCandidateInput,
) -> Result<DiscoveryManualApproval, String> {
    let mut connection = repository.lock()?;
    let tx = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let current: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM discovery_candidates candidate
         JOIN discovery_campaigns campaign ON campaign.id=candidate.discovery_campaign_id
         JOIN discovery_campaign_revisions revision ON revision.discovery_campaign_id=campaign.id
           AND revision.revision=campaign.current_revision
         WHERE campaign.id=?1 AND campaign.current_revision=?2 AND campaign.phase='active'
           AND revision.approved_at IS NOT NULL AND candidate.id=?3
           AND candidate.criteria_revision=?2)",
            params![
                input.campaign_id,
                input.expected_revision,
                input.candidate_id
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if !current {
        return Err("manual_approval_candidate_or_revision_stale".into());
    }
    let basis =
        super::discovery_library::load_basis(&tx, &input.campaign_id, input.expected_revision)?;
    let candidate = basis
        .rows
        .iter()
        .filter_map(|source| source.candidate.as_ref())
        .find(|candidate| candidate.id == input.candidate_id)
        .ok_or("manual_approval_source_binding_missing")?;
    // Existing receipts remain idempotent; new approvals must pass current policy.
    if candidate.manual_approved_at.is_none() {
        if basis.contact_required {
            return Err("manual_approval_retired_for_contact_recommendations".into());
        }
        let criteria = serde_json::from_str(&basis.criteria_json)
            .map_err(|_| "manual_approval_criteria_invalid")?;
        let evidence = serde_json::from_str(&candidate.evidence_json)
            .map_err(|_| "manual_approval_evidence_invalid")?;
        if !crate::discovery_manual_approval::language_only_pending(
            &criteria,
            &evidence,
            &candidate.native_author_id,
            &candidate.verdict,
        ) {
            return Err("manual_approval_requires_language_only_pending".into());
        }
    }
    tx.execute(
        "INSERT INTO discovery_manual_approvals
        (discovery_campaign_id,criteria_revision,candidate_id,approved_at) VALUES (?1,?2,?3,?4)
        ON CONFLICT(discovery_campaign_id,criteria_revision,candidate_id) DO NOTHING",
        params![
            input.campaign_id,
            input.expected_revision,
            input.candidate_id,
            now()
        ],
    )
    .map_err(|error| error.to_string())?;
    let approved_at = tx
        .query_row(
            "SELECT approved_at FROM discovery_manual_approvals
        WHERE discovery_campaign_id=?1 AND criteria_revision=?2 AND candidate_id=?3",
            params![
                input.campaign_id,
                input.expected_revision,
                input.candidate_id
            ],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    Ok(DiscoveryManualApproval {
        campaign_id: input.campaign_id,
        criteria_revision: input.expected_revision,
        candidate_id: input.candidate_id,
        approved_at,
    })
}
