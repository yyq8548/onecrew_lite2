use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{AccountAvailability, AccountKind, AccountRef, AccountRefId};

use super::super::creators::to_sql_conversion_error;

pub(super) fn validate_key(key: &str) -> Result<&str, String> {
    let key = key.trim();
    if key.is_empty() || key.len() > 128 {
        return Err("idempotency_key must contain 1 to 128 characters".into());
    }
    Ok(key)
}

pub(super) fn load_record(
    connection: &rusqlite::Connection,
    key: &str,
) -> Result<(String, Option<String>, String, Option<String>), String> {
    connection
        .query_row(
            "SELECT action, request_fingerprint, state, result_json
             FROM idempotency_records WHERE key = ?1",
            [key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "operator account disconnect claim not found".into())
}

pub(super) fn load_account(
    connection: &rusqlite::Connection,
    id: &AccountRefId,
) -> Result<AccountRef, String> {
    connection
        .query_row(
            "SELECT id, platform, kind, label, active, availability, created_at, updated_at
             FROM account_refs WHERE id = ?1",
            [id.as_str()],
            |row| {
                let platform: String = row.get(1)?;
                let kind: String = row.get(2)?;
                let availability: String = row.get(5)?;
                Ok(AccountRef {
                    id: AccountRefId::new(row.get::<_, String>(0)?),
                    platform: crate::Platform::parse(&platform).map_err(to_sql_conversion_error)?,
                    kind: AccountKind::parse(&kind).map_err(to_sql_conversion_error)?,
                    label: row.get(3)?,
                    active: row.get(4)?,
                    availability: AccountAvailability::parse(&availability)
                        .map_err(to_sql_conversion_error)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "account reference not found".into())
}

pub(super) fn insert_audit(
    transaction: &rusqlite::Transaction<'_>,
    kind: &str,
    account_ref_id: &AccountRefId,
    payload: serde_json::Value,
    timestamp: u64,
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO audit_events
               (id, kind, account_ref_id, payload_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                kind,
                account_ref_id.as_str(),
                payload.to_string(),
                timestamp
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}
