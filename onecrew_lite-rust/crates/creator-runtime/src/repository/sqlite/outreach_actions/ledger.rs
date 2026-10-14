//! Version-one, body-free outreach receipts in the existing idempotency table.

use super::super::{SqliteRepository, now};
use crate::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const ENVELOPE_VERSION: u8 = 1;
const MAX_ENVELOPE_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u8,
    state: CreatorOutreachActionState,
}

pub(in super::super) fn read(
    repository: &SqliteRepository,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<Option<CreatorOutreachActionState>, CreatorOutreachError> {
    let connection = repository.read_connection().map_err(storage)?;
    let state = load_in(&connection, key)?;
    if let Some(state) = &state {
        matches_intent(state, intent)?;
    }
    Ok(state)
}

pub(in super::super) fn status(
    repository: &SqliteRepository,
    key: &str,
) -> Result<Option<CreatorOutreachStatus>, CreatorOutreachError> {
    let connection = repository.read_connection().map_err(storage)?;
    let row = connection.query_row("SELECT action, state, CASE WHEN json_valid(result_json) THEN json_extract(result_json, '$.kind') END FROM idempotency_records WHERE key = ?1", [key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))).optional().map_err(storage)?;
    let Some((action, phase, marker)) = row else {
        return Ok(None);
    };
    if let Some(operation) = legacy_operation(&action) {
        let marked = marker.as_deref() == Some("retired_action_unresolved");
        // Retirement closes the old database row as failed without proving
        // that its remote effect failed. The marker carries that distinction.
        let phase = if marked {
            CreatorRetiredOutreachPhase::RetiredActionUnresolved
        } else {
            match phase.as_str() {
                "completed" => CreatorRetiredOutreachPhase::Completed,
                "failed" => CreatorRetiredOutreachPhase::Failed,
                _ => CreatorRetiredOutreachPhase::RetiredActionUnresolved,
            }
        };
        return Ok(Some(CreatorOutreachStatus::Retired {
            operation,
            phase,
            marked,
        }));
    }
    Ok(
        load_in(&connection, key)?.map(|state| CreatorOutreachStatus::Current {
            state: Box::new(state),
        }),
    )
}

pub(in super::super) fn validate_new(
    repository: &SqliteRepository,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<(), CreatorOutreachError> {
    let mut connection = repository.read_connection().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(storage)?;
    validate_intent(intent)?;
    validate_ownership_in(&transaction, intent.identity(), true)?;
    transaction.commit().map_err(storage)
}

pub(in super::super) fn claim(
    repository: &SqliteRepository,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<CreatorOutreachClaim, CreatorOutreachError> {
    validate_intent(intent)?;
    if key.trim().is_empty() {
        return Err(CreatorOutreachError::InvalidInput);
    }
    let mut connection = repository.lock().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    // Durable recovery precedes current admission: a disable/archive cannot
    // erase an accepted or verified effect that still needs recording.
    if let Some(state) = load_in(&transaction, key)? {
        matches_intent(&state, intent)?;
        return Ok(CreatorOutreachClaim {
            is_new: false,
            state,
        });
    }
    validate_ownership_in(&transaction, intent.identity(), true)?;
    let state = CreatorOutreachActionState {
        operation: intent.operation(),
        identity: intent.identity().clone(),
        fingerprint: intent.fingerprint().to_owned(),
        claimed_at: now(),
        phase: CreatorOutreachPhase::RemoteStarted,
        receipt: None,
        verification: None,
        record: None,
    };
    transaction.execute("INSERT INTO idempotency_records(key, action, contact_route_id, request_fingerprint, state, result_json, created_at, updated_at) VALUES(?1, ?2, ?3, ?4, 'remote_started', ?5, ?6, ?6)", params![key, state.operation.action_name(), state.identity.contact_route_id.as_ref().map(CreatorContactRouteId::as_str), state.fingerprint, encode(&state)?, state.claimed_at]).map_err(storage)?;
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::CommitUnknown)?;
    Ok(CreatorOutreachClaim {
        is_new: true,
        state,
    })
}

pub(in super::super) fn accept(
    repository: &SqliteRepository,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
    receipt: CreatorOutreachReceipt,
) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
    let mut connection = repository.lock().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let mut state = require_intent(&transaction, key, intent)?;
    validate_receipt(&state.identity, &receipt)?;
    if state.phase != CreatorOutreachPhase::RemoteStarted {
        if state.receipt.as_ref() == Some(&receipt) && state.phase != CreatorOutreachPhase::Refused
        {
            return Ok(state);
        }
        return Err(CreatorOutreachError::IntentConflict);
    }
    state.receipt = Some(receipt);
    state.phase = CreatorOutreachPhase::RemoteAccepted;
    persist_phase(&transaction, key, "remote_started", &state)?;
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::CommitUnknown)?;
    Ok(state)
}

pub(in super::super) fn verify(
    repository: &SqliteRepository,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
    verification: CreatorOutreachVerification,
) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
    let mut connection = repository.lock().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let mut state = require_intent(&transaction, key, intent)?;
    let receipt = verified_receipt(&state, &verification)?;
    if matches!(
        state.phase,
        CreatorOutreachPhase::RecordPending | CreatorOutreachPhase::Completed
    ) {
        if state.verification.as_ref() == Some(&verification) {
            return Ok(state);
        }
        return Err(CreatorOutreachError::IntentConflict);
    }
    if !matches!(
        state.phase,
        CreatorOutreachPhase::RemoteStarted | CreatorOutreachPhase::RemoteAccepted
    ) {
        return Err(CreatorOutreachError::ActionUnavailable);
    }
    let previous = database_phase(state.phase);
    state.receipt = Some(receipt);
    state.verification = Some(verification);
    state.phase = CreatorOutreachPhase::RecordPending;
    persist_phase(&transaction, key, previous, &state)?;
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::CommitUnknown)?;
    Ok(state)
}

pub(in super::super) fn refuse(
    repository: &SqliteRepository,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
    let mut connection = repository.lock().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(storage)?;
    let mut state = require_intent(&transaction, key, intent)?;
    if state.phase == CreatorOutreachPhase::Refused {
        return Ok(state);
    }
    if state.phase != CreatorOutreachPhase::RemoteStarted {
        return Err(CreatorOutreachError::ActionUnavailable);
    }
    state.phase = CreatorOutreachPhase::Refused;
    persist_phase(&transaction, key, "remote_started", &state)?;
    transaction
        .commit()
        .map_err(|_| CreatorOutreachError::CommitUnknown)?;
    Ok(state)
}

pub(super) fn load_in(
    connection: &Connection,
    key: &str,
) -> Result<Option<CreatorOutreachActionState>, CreatorOutreachError> {
    let row = connection.query_row("SELECT action, request_fingerprint, state, result_json, contact_route_id, created_at FROM idempotency_records WHERE key = ?1", [key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, u64>(5)?))).optional().map_err(storage)?;
    let Some((action, fingerprint, phase, payload, route_id, claimed_at)) = row else {
        return Ok(None);
    };
    if legacy_operation(&action).is_some() {
        return Err(CreatorOutreachError::RetiredActionUnresolved);
    }
    let operation = operation(&action).ok_or(CreatorOutreachError::IntentConflict)?;
    let payload = payload.ok_or(CreatorOutreachError::Storage)?;
    if payload.len() > MAX_ENVELOPE_BYTES {
        return Err(CreatorOutreachError::Storage);
    }
    let envelope: Envelope = serde_json::from_str(&payload).map_err(storage)?;
    let state = envelope.state;
    if envelope.version != ENVELOPE_VERSION
        || state.operation != operation
        || state.identity.operation != operation
        || fingerprint.as_deref() != Some(state.fingerprint.as_str())
        || !valid_fingerprint(&state.fingerprint)
        || phase != database_phase(state.phase)
        || claimed_at != state.claimed_at
        || route_id.as_deref()
            != state
                .identity
                .contact_route_id
                .as_ref()
                .map(CreatorContactRouteId::as_str)
    {
        return Err(CreatorOutreachError::Storage);
    }
    validate_identity(&state.identity).map_err(|_| CreatorOutreachError::Storage)?;
    validate_state(&state).map_err(|_| CreatorOutreachError::Storage)?;
    Ok(Some(state))
}

fn validate_state(state: &CreatorOutreachActionState) -> Result<(), CreatorOutreachError> {
    let shape = match state.phase {
        CreatorOutreachPhase::RemoteStarted | CreatorOutreachPhase::Refused => {
            state.receipt.is_none() && state.verification.is_none() && state.record.is_none()
        }
        CreatorOutreachPhase::RemoteAccepted => {
            state.receipt.is_some() && state.verification.is_none() && state.record.is_none()
        }
        CreatorOutreachPhase::RecordPending => {
            state.receipt.is_some() && state.verification.is_some() && state.record.is_none()
        }
        CreatorOutreachPhase::Completed => {
            state.receipt.is_some() && state.verification.is_some() && state.record.is_some()
        }
    };
    if !shape {
        return Err(CreatorOutreachError::Storage);
    }
    if let Some(receipt) = &state.receipt {
        validate_receipt(&state.identity, receipt)?;
    }
    if let Some(verification) = &state.verification {
        verified_receipt(state, verification)?;
    }
    if let Some(record) = &state.record {
        let valid = match (state.operation, record) {
            (
                CreatorOutreachOperation::RelationshipUpdate,
                CreatorOutreachRecordIds::Relationship {
                    relationship_id,
                    can_message_id,
                    touchpoint_id,
                    audit_event_id,
                },
            ) => {
                !relationship_id.as_str().is_empty()
                    && !can_message_id.as_str().is_empty()
                    && !touchpoint_id.as_str().is_empty()
                    && !audit_event_id.as_str().is_empty()
            }
            (
                CreatorOutreachOperation::ContentReplySend,
                CreatorOutreachRecordIds::ContentReply {
                    external_reply_id,
                    touchpoint_id,
                    audit_event_id,
                },
            ) => {
                matches!(&state.verification, Some(CreatorOutreachVerification::ContentReply { external_reply_id: verified }) if verified == external_reply_id)
                    && !touchpoint_id.as_str().is_empty()
                    && !audit_event_id.as_str().is_empty()
            }
            (
                CreatorOutreachOperation::DirectMessageSend,
                CreatorOutreachRecordIds::DirectMessage {
                    conversation_id,
                    message_id,
                    touchpoint_id,
                    audit_event_id,
                },
            ) => {
                !conversation_id.as_str().is_empty()
                    && !message_id.as_str().is_empty()
                    && !touchpoint_id.as_str().is_empty()
                    && !audit_event_id.as_str().is_empty()
            }
            _ => false,
        };
        if !valid {
            return Err(CreatorOutreachError::Storage);
        }
    }
    Ok(())
}

pub(in super::super) fn require_verified(
    connection: &Connection,
    key: &str,
    expected: CreatorOutreachOperation,
) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
    let state = load_in(connection, key)?.ok_or(CreatorOutreachError::ActionUnavailable)?;
    if state.operation != expected {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    if !matches!(
        state.phase,
        CreatorOutreachPhase::RecordPending | CreatorOutreachPhase::Completed
    ) {
        return Err(CreatorOutreachError::ActionUnavailable);
    }
    if state.verification.is_none() {
        return Err(CreatorOutreachError::Storage);
    }
    Ok(state)
}

pub(in super::super) fn complete_in(
    transaction: &Transaction<'_>,
    key: &str,
    mut state: CreatorOutreachActionState,
    record: CreatorOutreachRecordIds,
) -> Result<(), CreatorOutreachError> {
    if state.phase != CreatorOutreachPhase::RecordPending {
        return Err(CreatorOutreachError::ActionUnavailable);
    }
    state.phase = CreatorOutreachPhase::Completed;
    state.record = Some(record);
    persist_phase(transaction, key, "remote_verified", &state)
}

fn require_intent(
    connection: &Connection,
    key: &str,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<CreatorOutreachActionState, CreatorOutreachError> {
    let state = load_in(connection, key)?.ok_or(CreatorOutreachError::ActionUnavailable)?;
    matches_intent(&state, intent)?;
    Ok(state)
}

fn matches_intent(
    state: &CreatorOutreachActionState,
    intent: &PreparedCreatorOutreachIntent,
) -> Result<(), CreatorOutreachError> {
    validate_intent(intent)?;
    if state.fingerprint != intent.fingerprint() || state.identity != *intent.identity() {
        return Err(CreatorOutreachError::IntentConflict);
    }
    Ok(())
}

fn validate_intent(intent: &PreparedCreatorOutreachIntent) -> Result<(), CreatorOutreachError> {
    validate_identity(intent.identity())?;
    if crate::outreach::intents::fingerprint_v1(intent.identity(), intent.body())?
        != intent.fingerprint()
    {
        return Err(CreatorOutreachError::IntentConflict);
    }
    Ok(())
}

fn validate_identity(identity: &CreatorOutreachIdentity) -> Result<(), CreatorOutreachError> {
    if identity.creator_id.as_str().trim().is_empty()
        || identity.operator_account_ref_id.as_str().trim().is_empty()
        || identity
            .target_platform_account_id
            .as_str()
            .trim()
            .is_empty()
        || identity.external_target_id.trim().is_empty()
    {
        return Err(CreatorOutreachError::InvalidInput);
    }
    let route = identity
        .contact_route_id
        .as_ref()
        .is_some_and(|id| !id.as_str().trim().is_empty())
        && identity
            .resolved_target_id
            .as_ref()
            .is_some_and(|id| !id.as_str().trim().is_empty());
    let valid = match identity.operation {
        CreatorOutreachOperation::RelationshipUpdate => {
            route
                && identity.relationship_action.is_some()
                && identity.content_id.is_none()
                && identity.reply_to_id.is_none()
                && identity.conversation.is_none()
                && !identity.allow_unfollowed_recipient
                && matches!(identity.platform, Platform::X | Platform::Instagram)
        }
        CreatorOutreachOperation::ContentReplySend => {
            !route
                && identity.contact_route_id.is_none()
                && identity.resolved_target_id.is_none()
                && identity.relationship_action.is_none()
                && identity
                    .content_id
                    .as_ref()
                    .is_some_and(|id| !id.trim().is_empty())
                && identity.conversation.is_none()
                && !identity.allow_unfollowed_recipient
                && identity.platform == Platform::X
                && identity.target_kind == TargetKind::User
        }
        CreatorOutreachOperation::DirectMessageSend => {
            route
                && identity.relationship_action.is_none()
                && identity.content_id.is_none()
                && identity.reply_to_id.is_none()
                && matches!(
                    identity.platform,
                    Platform::X | Platform::Instagram | Platform::Telegram
                )
        }
    };
    if !valid
        || identity
            .reply_to_id
            .as_deref()
            .is_some_and(|id| id.trim().is_empty())
        || identity.conversation.as_ref().is_some_and(|conversation| {
            conversation.conversation_id.as_str().trim().is_empty()
                || conversation.provider_thread_id.trim().is_empty()
        })
    {
        return Err(CreatorOutreachError::InvalidInput);
    }
    Ok(())
}

pub(in super::super) fn validate_ownership_in(
    connection: &Connection,
    identity: &CreatorOutreachIdentity,
    require_active: bool,
) -> Result<(), CreatorOutreachError> {
    validate_identity(identity)?;
    let owned = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM creators creator JOIN creator_relationships relationship ON relationship.creator_id = creator.id
         JOIN creator_account_links link ON link.creator_id = creator.id AND link.platform_account_id = ?3
         JOIN platform_accounts target ON target.id = link.platform_account_id
         JOIN account_refs operator ON operator.id = ?2
         WHERE creator.id = ?1 AND link.state = 'confirmed' AND target.platform = ?4 AND operator.platform = ?4 AND operator.kind = 'profile' AND target.target_kind = ?5 AND target.external_id = ?6
           AND (?7 = 0 OR (creator.archived_at IS NULL AND relationship.stage != 'archived' AND operator.active = 1 AND operator.availability = 'available')))",
        params![identity.creator_id.as_str(), identity.operator_account_ref_id.as_str(), identity.target_platform_account_id.as_str(), identity.platform.as_str(), identity.target_kind.as_str(), identity.external_target_id, require_active], |row| row.get::<_, bool>(0),
    ).map_err(storage)?;
    if !owned {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    if let (Some(route), Some(target)) = (&identity.contact_route_id, &identity.resolved_target_id)
    {
        let matches = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM creator_contact_routes route JOIN locator_resolutions resolution ON resolution.locator_id = route.locator_id
             JOIN resolved_targets target ON target.id = resolution.resolved_target_id WHERE route.id = ?1 AND route.creator_id = ?2
               AND resolution.account_ref_id = ?3 AND target.id = ?4 AND target.account_ref_id = ?3 AND target.platform_account_id = ?5
               AND target.platform = ?6 AND target.target_kind = ?7 AND target.external_target_id = ?8 AND (?9 = 0 OR resolution.state = 'active'))",
            params![route.as_str(),identity.creator_id.as_str(),identity.operator_account_ref_id.as_str(),target.as_str(),identity.target_platform_account_id.as_str(),identity.platform.as_str(),identity.target_kind.as_str(),identity.external_target_id,require_active], |row| row.get::<_, bool>(0),
        ).map_err(storage)?;
        if !matches {
            return Err(CreatorOutreachError::ScopeMismatch);
        }
    }
    if let Some(conversation) = &identity.conversation {
        let matches = connection.query_row("SELECT EXISTS(SELECT 1 FROM conversations conversation JOIN conversation_contact_routes link ON link.conversation_id = conversation.id WHERE conversation.id = ?1 AND conversation.provider_thread_id = ?2 AND conversation.account_ref_id = ?3 AND conversation.resolved_target_id = ?4 AND link.contact_route_id = ?5 AND conversation.channel = ?6)", params![conversation.conversation_id.as_str(),conversation.provider_thread_id,identity.operator_account_ref_id.as_str(),identity.resolved_target_id.as_ref().map(ResolvedTargetId::as_str),identity.contact_route_id.as_ref().map(CreatorContactRouteId::as_str),identity.platform.as_str()], |row| row.get::<_, bool>(0)).map_err(storage)?;
        if !matches {
            return Err(CreatorOutreachError::ScopeMismatch);
        }
    }
    Ok(())
}

fn validate_receipt(
    identity: &CreatorOutreachIdentity,
    receipt: &CreatorOutreachReceipt,
) -> Result<(), CreatorOutreachError> {
    let valid = match (identity.operation, receipt) {
        (CreatorOutreachOperation::RelationshipUpdate, CreatorOutreachReceipt::Relationship) => {
            true
        }
        (
            CreatorOutreachOperation::ContentReplySend,
            CreatorOutreachReceipt::ContentReply { external_reply_id },
        ) => !external_reply_id.trim().is_empty(),
        (
            CreatorOutreachOperation::DirectMessageSend,
            CreatorOutreachReceipt::DirectMessage {
                external_message_id,
                provider_thread_id,
            },
        ) => {
            !external_message_id.trim().is_empty()
                && !provider_thread_id
                    .as_deref()
                    .is_some_and(|id| id.trim().is_empty())
                && identity.conversation.as_ref().is_none_or(|conversation| {
                    provider_thread_id
                        .as_deref()
                        .is_none_or(|id| id == conversation.provider_thread_id)
                })
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(CreatorOutreachError::ScopeMismatch)
    }
}

fn verified_receipt(
    state: &CreatorOutreachActionState,
    verification: &CreatorOutreachVerification,
) -> Result<CreatorOutreachReceipt, CreatorOutreachError> {
    let receipt = match (state.operation, verification) {
        (
            CreatorOutreachOperation::RelationshipUpdate,
            CreatorOutreachVerification::Relationship { following, .. },
        ) => {
            let expected = match state.identity.relationship_action {
                Some(RelationshipMutation::Follow) => RelationshipValue::Yes,
                Some(RelationshipMutation::Unfollow) => RelationshipValue::No,
                None => return Err(CreatorOutreachError::ScopeMismatch),
            };
            if *following != expected {
                return Err(CreatorOutreachError::ScopeMismatch);
            }
            CreatorOutreachReceipt::Relationship
        }
        (
            CreatorOutreachOperation::ContentReplySend,
            CreatorOutreachVerification::ContentReply { external_reply_id },
        ) => {
            if state.phase == CreatorOutreachPhase::RemoteStarted {
                return Err(CreatorOutreachError::ActionUnavailable);
            }
            CreatorOutreachReceipt::ContentReply {
                external_reply_id: external_reply_id.clone(),
            }
        }
        (
            CreatorOutreachOperation::DirectMessageSend,
            CreatorOutreachVerification::DirectMessage {
                external_message_id,
                provider_thread_id,
                sent_at,
            },
        ) => {
            if state.phase == CreatorOutreachPhase::RemoteStarted && *sent_at < state.claimed_at {
                return Err(CreatorOutreachError::ScopeMismatch);
            }
            CreatorOutreachReceipt::DirectMessage {
                external_message_id: external_message_id.clone(),
                provider_thread_id: Some(provider_thread_id.clone()),
            }
        }
        _ => return Err(CreatorOutreachError::ScopeMismatch),
    };
    validate_receipt(&state.identity, &receipt)?;
    if let Some(accepted) = &state.receipt {
        let matches = match (accepted, &receipt) {
            (
                CreatorOutreachReceipt::DirectMessage {
                    external_message_id: left,
                    provider_thread_id: thread,
                },
                CreatorOutreachReceipt::DirectMessage {
                    external_message_id: right,
                    provider_thread_id: verified_thread,
                },
            ) => {
                left == right
                    && thread
                        .as_ref()
                        .is_none_or(|thread| Some(thread) == verified_thread.as_ref())
            }
            _ => accepted == &receipt,
        };
        if !matches {
            return Err(CreatorOutreachError::ScopeMismatch);
        }
    }
    Ok(receipt)
}

fn persist_phase(
    transaction: &Transaction<'_>,
    key: &str,
    previous: &str,
    state: &CreatorOutreachActionState,
) -> Result<(), CreatorOutreachError> {
    let changed = transaction.execute("UPDATE idempotency_records SET state = ?4, result_json = ?5, updated_at = ?6 WHERE key = ?1 AND action = ?2 AND request_fingerprint = ?3 AND state = ?7", params![key,state.operation.action_name(),state.fingerprint,database_phase(state.phase),encode(state)?,now(),previous]).map_err(storage)?;
    if changed != 1 {
        return Err(CreatorOutreachError::IntentConflict);
    }
    Ok(())
}

fn encode(state: &CreatorOutreachActionState) -> Result<String, CreatorOutreachError> {
    validate_state(state)?;
    let value = serde_json::to_string(&Envelope {
        version: ENVELOPE_VERSION,
        state: state.clone(),
    })
    .map_err(storage)?;
    if value.len() > MAX_ENVELOPE_BYTES {
        return Err(CreatorOutreachError::InvalidInput);
    }
    Ok(value)
}
fn valid_fingerprint(value: &str) -> bool {
    value.strip_prefix("sha256:v1:").is_some_and(|hash| {
        hash.len() == 64
            && hash
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}
fn database_phase(phase: CreatorOutreachPhase) -> &'static str {
    match phase {
        CreatorOutreachPhase::RemoteStarted => "remote_started",
        CreatorOutreachPhase::RemoteAccepted => "remote_accepted",
        CreatorOutreachPhase::RecordPending => "remote_verified",
        CreatorOutreachPhase::Completed => "completed",
        CreatorOutreachPhase::Refused => "failed",
    }
}
fn operation(action: &str) -> Option<CreatorOutreachOperation> {
    match action {
        "relationship_update_v1" => Some(CreatorOutreachOperation::RelationshipUpdate),
        "content_reply_send_v1" => Some(CreatorOutreachOperation::ContentReplySend),
        "direct_message_send_v1" => Some(CreatorOutreachOperation::DirectMessageSend),
        _ => None,
    }
}
fn legacy_operation(action: &str) -> Option<CreatorOutreachOperation> {
    match action {
        "relationship_update" => Some(CreatorOutreachOperation::RelationshipUpdate),
        "content_reply_send" => Some(CreatorOutreachOperation::ContentReplySend),
        "direct_message_send" => Some(CreatorOutreachOperation::DirectMessageSend),
        _ => None,
    }
}
fn storage(_: impl std::fmt::Display) -> CreatorOutreachError {
    CreatorOutreachError::Storage
}
