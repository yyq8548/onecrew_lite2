use rusqlite::{OptionalExtension, params};

use crate::{
    AccountKind, AccountLifecycleClaimError, AccountRefId, OperatorAccountDisconnectClaim,
    OperatorAccountDisconnectRecovery, OperatorAccountDisconnectResult,
};

use super::super::{SqliteRepository, now};
use super::shared::{insert_audit, load_account, load_record, validate_key};

const DISCONNECT_ACTION: &str = "operator_account_disconnect";

pub(super) fn begin(
    repository: &SqliteRepository,
    key: &str,
    account_ref_id: &AccountRefId,
) -> Result<OperatorAccountDisconnectClaim, AccountLifecycleClaimError> {
    let key = internal(validate_key(key))?;
    let mut connection = internal(repository.lock())?;
    let transaction = internal(connection.transaction())?;
    if let Some((action, fingerprint, state, result_json)) = transaction
        .query_row(
            "SELECT action, request_fingerprint, state, result_json
                 FROM idempotency_records WHERE key = ?1",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AccountLifecycleClaimError::Internal)?
    {
        ensure_begin_scope(&action, fingerprint.as_deref(), account_ref_id)?;
        let claim = internal(decode_claim(&state, result_json))?;
        internal(transaction.commit())?;
        return Ok(claim);
    }

    let account = internal(load_account(&transaction, account_ref_id))?;
    if internal(super::super::outreach_actions::guards::operator_pending(
        &transaction,
        account_ref_id,
    ))? {
        return Err(AccountLifecycleClaimError::OutreachInProgress);
    }
    if account.kind != AccountKind::Profile {
        return Err(AccountLifecycleClaimError::WrongKind);
    }
    let timestamp = now();
    transaction
        .execute(
            "UPDATE account_refs SET active = 0, updated_at = ?2 WHERE id = ?1",
            params![account_ref_id.as_str(), timestamp],
        )
        .map_err(|_| AccountLifecycleClaimError::Internal)?;
    let recovery = OperatorAccountDisconnectRecovery {
        account_ref_id: account_ref_id.clone(),
    };
    transaction
        .execute(
            "INSERT INTO idempotency_records
                   (key, action, request_fingerprint, state, result_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'in_progress', ?4, ?5, ?5)",
            params![
                key,
                DISCONNECT_ACTION,
                account_ref_id.as_str(),
                serde_json::to_string(&recovery)
                    .map_err(|_| AccountLifecycleClaimError::Internal)?,
                timestamp
            ],
        )
        .map_err(|_| AccountLifecycleClaimError::Internal)?;
    insert_audit(
        &transaction,
        "operator_account_disconnect_started",
        account_ref_id,
        serde_json::json!({
            "previous_availability": account.availability,
        }),
        timestamp,
    )
    .map_err(|_| AccountLifecycleClaimError::Internal)?;
    internal(transaction.commit())?;
    Ok(OperatorAccountDisconnectClaim::Acquired(recovery))
}

pub(super) fn mark_remote_started(
    repository: &SqliteRepository,
    key: &str,
    account_ref_id: &AccountRefId,
) -> Result<OperatorAccountDisconnectRecovery, String> {
    let key = validate_key(key)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let (action, fingerprint, state, result_json) = load_record(&transaction, key)?;
    ensure_scope(&action, fingerprint.as_deref(), account_ref_id)?;
    let recovery = recovery_from_json(result_json)?;
    match state.as_str() {
        "in_progress" => {
            let changed = transaction
                .execute(
                    "UPDATE idempotency_records
                         SET state = 'remote_started', updated_at = ?3
                         WHERE key = ?1 AND action = ?2 AND state = 'in_progress'",
                    params![key, DISCONNECT_ACTION, now()],
                )
                .map_err(|error| error.to_string())?;
            if changed != 1 {
                return Err("operator account disconnect remote-start claim was lost".into());
            }
        }
        "remote_started" | "remote_verified" | "completed" => {}
        _ => {
            return Err(format!(
                "operator account disconnect has invalid state: {state}"
            ));
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(recovery)
}

pub(super) fn mark_remote_failed(
    repository: &SqliteRepository,
    key: &str,
    account_ref_id: &AccountRefId,
) -> Result<OperatorAccountDisconnectRecovery, String> {
    let key = validate_key(key)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let (action, fingerprint, state, result_json) = load_record(&transaction, key)?;
    ensure_scope(&action, fingerprint.as_deref(), account_ref_id)?;
    let recovery = recovery_from_json(result_json)?;
    match state.as_str() {
        "remote_started" => {
            let timestamp = now();
            let changed = transaction
                .execute(
                    "UPDATE idempotency_records
                         SET state = 'failed', updated_at = ?3
                         WHERE key = ?1 AND action = ?2 AND state = 'remote_started'",
                    params![key, DISCONNECT_ACTION, timestamp],
                )
                .map_err(|error| error.to_string())?;
            if changed != 1 {
                return Err("operator account disconnect failure claim was lost".into());
            }
            insert_audit(
                &transaction,
                "operator_account_disconnect_failed",
                account_ref_id,
                serde_json::json!({
                    "reason": "vault_profile_not_deleted",
                }),
                timestamp,
            )?;
        }
        "failed" => {}
        _ => {
            return Err(format!(
                "operator account disconnect has invalid state: {state}"
            ));
        }
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(recovery)
}

pub(super) fn mark_remote_verified(
    repository: &SqliteRepository,
    key: &str,
    account_ref_id: &AccountRefId,
) -> Result<OperatorAccountDisconnectRecovery, String> {
    let key = validate_key(key)?;
    let connection = repository.lock()?;
    let (action, fingerprint, state, result_json) = load_record(&connection, key)?;
    ensure_scope(&action, fingerprint.as_deref(), account_ref_id)?;
    let recovery = recovery_from_json(result_json)?;
    match state.as_str() {
        "remote_started" => {
            let changed = connection
                .execute(
                    "UPDATE idempotency_records
                         SET state = 'remote_verified', updated_at = ?3
                         WHERE key = ?1 AND action = ?2 AND state = 'remote_started'",
                    params![key, DISCONNECT_ACTION, now()],
                )
                .map_err(|error| error.to_string())?;
            if changed != 1 {
                return Err(
                    "service credential deletion remote verification claim was lost".into(),
                );
            }
        }
        "remote_verified" | "completed" => {}
        _ => {
            return Err(format!(
                "operator account disconnect has invalid state: {state}"
            ));
        }
    }
    Ok(recovery)
}

pub(super) fn complete(
    repository: &SqliteRepository,
    key: &str,
    account_ref_id: &AccountRefId,
) -> Result<OperatorAccountDisconnectResult, String> {
    let key = validate_key(key)?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let (action, fingerprint, state, result_json) = load_record(&transaction, key)?;
    ensure_scope(&action, fingerprint.as_deref(), account_ref_id)?;
    if state == "completed" {
        let mut result: OperatorAccountDisconnectResult = serde_json::from_str(
            result_json
                .as_deref()
                .ok_or_else(|| "completed disconnect is missing its result".to_string())?,
        )
        .map_err(|error| error.to_string())?;
        result.replayed = true;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(result);
    }
    if state != "remote_verified" {
        return Err("operator account disconnect vault deletion is not verified".into());
    }
    let recovery = recovery_from_json(result_json)?;
    let timestamp = now();
    transaction
        .execute(
            "UPDATE account_refs
                 SET active = 0, availability = 'unavailable', updated_at = ?2
                 WHERE id = ?1",
            params![account_ref_id.as_str(), timestamp],
        )
        .map_err(|error| error.to_string())?;
    let account_ref = load_account(&transaction, account_ref_id)?;
    let result = OperatorAccountDisconnectResult {
        account_ref,
        replayed: false,
    };
    insert_audit(
        &transaction,
        "operator_account_disconnected",
        account_ref_id,
        serde_json::json!({
            "account_ref_id": recovery.account_ref_id,
            "vault_profile_deleted": true,
        }),
        timestamp,
    )?;
    let changed = transaction
        .execute(
            "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2 AND state = 'remote_verified'",
            params![
                key,
                DISCONNECT_ACTION,
                serde_json::to_string(&result).map_err(|error| error.to_string())?,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("operator account disconnect completion claim was lost".into());
    }
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(result)
}

pub(super) fn pending(
    repository: &SqliteRepository,
) -> Result<Vec<(String, OperatorAccountDisconnectRecovery)>, String> {
    let connection = repository.read_connection()?;
    let mut statement = connection
        .prepare(
            "SELECT key, result_json FROM idempotency_records
                 WHERE action = ?1 AND state IN ('in_progress','remote_started','remote_verified')
                 ORDER BY created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([DISCONNECT_ACTION], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .map(|row| {
            let (key, result_json) = row.map_err(|error| error.to_string())?;
            Ok((
                key,
                serde_json::from_str(&result_json).map_err(|error| error.to_string())?,
            ))
        })
        .collect()
}

fn ensure_scope(action: &str, fingerprint: Option<&str>, id: &AccountRefId) -> Result<(), String> {
    if action != DISCONNECT_ACTION {
        return Err("idempotency key was already used for another action".into());
    }
    if fingerprint != Some(id.as_str()) {
        return Err("idempotency key was already used for another operator account".into());
    }
    Ok(())
}

fn ensure_begin_scope(
    action: &str,
    fingerprint: Option<&str>,
    id: &AccountRefId,
) -> Result<(), AccountLifecycleClaimError> {
    if action != DISCONNECT_ACTION || fingerprint != Some(id.as_str()) {
        return Err(AccountLifecycleClaimError::IdempotencyConflict);
    }
    Ok(())
}

fn internal<T, E>(result: Result<T, E>) -> Result<T, AccountLifecycleClaimError> {
    result.map_err(|_| AccountLifecycleClaimError::Internal)
}

fn recovery_from_json(
    result_json: Option<String>,
) -> Result<OperatorAccountDisconnectRecovery, String> {
    serde_json::from_str(
        result_json
            .as_deref()
            .ok_or_else(|| "operator account disconnect is missing recovery state".to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn decode_claim(
    state: &str,
    result_json: Option<String>,
) -> Result<OperatorAccountDisconnectClaim, String> {
    match state {
        "in_progress" => Ok(OperatorAccountDisconnectClaim::Acquired(
            recovery_from_json(result_json)?,
        )),
        "remote_started" => Ok(OperatorAccountDisconnectClaim::RemoteStarted(
            recovery_from_json(result_json)?,
        )),
        "remote_verified" => Ok(OperatorAccountDisconnectClaim::RemoteVerified(
            recovery_from_json(result_json)?,
        )),
        "completed" => {
            let mut result: OperatorAccountDisconnectResult = serde_json::from_str(
                result_json
                    .as_deref()
                    .ok_or_else(|| "completed disconnect is missing its result".to_string())?,
            )
            .map_err(|error| error.to_string())?;
            result.replayed = true;
            Ok(OperatorAccountDisconnectClaim::Completed(result))
        }
        "failed" => {
            Err("operator account disconnect previously failed to delete the vault profile".into())
        }
        _ => Err(format!(
            "operator account disconnect has invalid state: {state}"
        )),
    }
}
