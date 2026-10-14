use std::time::Instant;

use super::super::{ResultBinding, ResultRef, ResultState, ResultStoreError};
use super::{ResultMap, ResultRecord, StoreLimits};

pub(super) fn validate_binding(
    record: &ResultRecord,
    binding: &ResultBinding,
) -> Result<(), ResultStoreError> {
    validate_identity(record, binding)?;
    if record.revision != binding.revision {
        return Err(ResultStoreError::RevisionConflict {
            current_revision: record.revision,
        });
    }
    Ok(())
}

pub(super) fn validate_identity(
    record: &ResultRecord,
    binding: &ResultBinding,
) -> Result<(), ResultStoreError> {
    if binding.revision == 0 {
        return Err(ResultStoreError::InvalidInput("result_revision"));
    }
    if record.view != binding.view {
        return Err(ResultStoreError::ViewMismatch);
    }
    if record.origin != binding.origin {
        return Err(ResultStoreError::OriginMismatch);
    }
    if record.fingerprint != binding.fingerprint {
        return Err(ResultStoreError::FingerprintMismatch);
    }
    Ok(())
}

pub(super) fn require_user_action_available(record: &ResultRecord) -> Result<(), ResultStoreError> {
    if !record.presented {
        return Err(ResultStoreError::InvalidState("result_not_presented"));
    }
    if !matches!(record.state, ResultState::Available) {
        return Err(ResultStoreError::Replay("result_action_consumed"));
    }
    if record.pending_action.is_none() {
        return Err(ResultStoreError::InvalidState("result_read_only"));
    }
    Ok(())
}

pub(super) fn ensure_capacity(
    map: &ResultMap,
    limits: StoreLimits,
    added_payload_bytes: usize,
) -> Result<(), ResultStoreError> {
    if limits.max_results == 0 || map.records.len() >= limits.max_results {
        return Err(ResultStoreError::Capacity("result_row_limit"));
    }
    let total = map
        .total_payload_bytes
        .checked_add(added_payload_bytes)
        .ok_or(ResultStoreError::Capacity("result_payload_limit"))?;
    if total > limits.max_total_payload_bytes {
        return Err(ResultStoreError::Capacity("result_payload_limit"));
    }
    Ok(())
}

pub(super) fn expire_target(
    map: &mut ResultMap,
    result_ref: &ResultRef,
    limits: StoreLimits,
    now: Instant,
) -> Result<(), ResultStoreError> {
    let Some(record) = map.records.get(result_ref) else {
        return Ok(());
    };
    if !retention_expired(record, limits, now) {
        if !prepared_expired(record, now) {
            return Ok(());
        }
        if has_completed(record) {
            return super::expiry::expire_review(map, result_ref, limits, now);
        }
    }
    if let Some(record) = map.records.remove(result_ref) {
        map.total_payload_bytes = map.total_payload_bytes.saturating_sub(record.payload_bytes);
    }
    Err(ResultStoreError::Expired)
}

pub(super) fn cleanup_locked(
    map: &mut ResultMap,
    limits: StoreLimits,
    now: Instant,
) -> Result<usize, ResultStoreError> {
    let expired: Vec<_> = map
        .records
        .iter()
        .filter_map(|(result_ref, record)| {
            (retention_expired(record, limits, now)
                || (prepared_expired(record, now) && !has_completed(record)))
            .then_some(result_ref.clone())
        })
        .collect();
    for result_ref in &expired {
        if let Some(record) = map.records.remove(result_ref) {
            map.total_payload_bytes = map.total_payload_bytes.saturating_sub(record.payload_bytes);
        }
    }
    let revoked: Vec<_> = map
        .records
        .iter()
        .filter_map(|(reference, record)| {
            (prepared_expired(record, now) && has_completed(record)).then_some(reference.clone())
        })
        .collect();
    for reference in revoked {
        super::expiry::expire_review(map, &reference, limits, now)?;
    }
    Ok(expired.len())
}

fn prepared_expired(record: &ResultRecord, now: Instant) -> bool {
    matches!(record.state, ResultState::Available)
        && record.pending_action.as_ref().is_some_and(|action| {
            action
                .payload()
                .prepared_deadline()
                .is_some_and(|deadline| now >= deadline)
        })
}

fn has_completed(record: &ResultRecord) -> bool {
    record.completed_sections_len > 0
        || record
            .batch
            .outcomes
            .iter()
            .any(|call| matches!(call.disposition, super::super::CallDisposition::Succeeded))
}

fn retention_expired(record: &ResultRecord, limits: StoreLimits, now: Instant) -> bool {
    let deadline_basis_and_ttl = match record.state {
        ResultState::Available => Some((record.last_accessed_at, limits.available_ttl)),
        ResultState::Resolved(_) | ResultState::Failed(_) => {
            Some((record.state_changed_at, limits.terminal_ttl))
        }
        ResultState::Submitted { .. } | ResultState::Unknown(_) => None,
    };
    deadline_basis_and_ttl.is_some_and(|(basis, ttl)| {
        now.checked_duration_since(basis)
            .is_some_and(|elapsed| elapsed >= ttl)
    })
}
