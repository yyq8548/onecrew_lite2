use rusqlite::{OptionalExtension, params};

use crate::{AccountAvailability, AccountKind, AccountRef, AccountRefId, Platform};

use super::super::{SqliteRepository, now};

pub(super) fn upsert_account_ref(
    repository: &SqliteRepository,
    account_ref_id: AccountRefId,
    platform: Platform,
    kind: &str,
    label: &str,
) -> Result<AccountRef, String> {
    upsert_account_ref_with_state(
        repository,
        account_ref_id,
        platform,
        AccountKind::parse(kind)?,
        label,
        AccountAvailability::Available,
    )
}

pub(super) fn upsert_account_ref_with_state(
    repository: &SqliteRepository,
    account_ref_id: AccountRefId,
    platform: Platform,
    kind: AccountKind,
    label: &str,
    availability: AccountAvailability,
) -> Result<AccountRef, String> {
    if account_ref_id.as_str().trim().is_empty() || label.trim().is_empty() {
        return Err("account reference id and label are required".into());
    }
    let connection = repository.lock()?;
    let timestamp = now();
    let created_at = connection
        .query_row(
            "SELECT created_at FROM account_refs WHERE id = ?1",
            [account_ref_id.as_str()],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(timestamp);
    connection
        .execute(
            "INSERT INTO account_refs
                   (id, platform, kind, label, active, availability, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET platform = excluded.platform,
                   kind = excluded.kind, label = excluded.label,
                   availability = excluded.availability, updated_at = excluded.updated_at",
            params![
                account_ref_id.as_str(),
                platform.as_str(),
                kind.as_str(),
                label.trim(),
                availability.as_str(),
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    load_account_ref(&connection, &account_ref_id)
}

pub(super) fn get_account_ref(
    repository: &SqliteRepository,
    account_ref_id: &AccountRefId,
) -> Result<AccountRef, String> {
    let connection = repository.read_connection()?;
    load_account_ref(&connection, account_ref_id)
}

pub(in super::super) fn load_account_ref(
    connection: &rusqlite::Connection,
    account_ref_id: &AccountRefId,
) -> Result<AccountRef, String> {
    connection
        .query_row(
            "SELECT id, platform, kind, label, active, availability, created_at, updated_at
                 FROM account_refs WHERE id = ?1",
            [account_ref_id.as_str()],
            |row| {
                let platform: String = row.get(1)?;
                Ok(AccountRef {
                    id: AccountRefId::new(row.get::<_, String>(0)?),
                    platform: Platform::parse(&platform)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    kind: AccountKind::parse(&row.get::<_, String>(2)?)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    label: row.get(3)?,
                    active: row.get(4)?,
                    availability: AccountAvailability::parse(&row.get::<_, String>(5)?)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "account reference not found".into())
}

pub(super) fn list_account_refs(
    repository: &SqliteRepository,
    platform: Platform,
    kind: AccountKind,
) -> Result<Vec<AccountRef>, String> {
    let connection = repository.read_connection()?;
    let mut statement = connection
        .prepare(
            "SELECT id, platform, kind, label, active, availability, created_at, updated_at
                 FROM account_refs WHERE platform = ?1 AND kind = ?2 ORDER BY created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map(params![platform.as_str(), kind.as_str()], |row| {
            Ok(AccountRef {
                id: AccountRefId::new(row.get::<_, String>(0)?),
                platform,
                kind,
                label: row.get(3)?,
                active: row.get(4)?,
                availability: AccountAvailability::parse(&row.get::<_, String>(5)?)
                    .map_err(super::super::creators::to_sql_conversion_error)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub(super) fn require_operable_account_ref(
    repository: &SqliteRepository,
    account_ref_id: &AccountRefId,
) -> Result<AccountRef, String> {
    let connection = repository.read_connection()?;
    require_operable_account_ref_in(&connection, account_ref_id)
}

pub(super) fn require_operable_account_ref_in(
    connection: &rusqlite::Connection,
    account_ref_id: &AccountRefId,
) -> Result<AccountRef, String> {
    let account = load_account_ref(connection, account_ref_id)?;
    if !account.active || account.availability != AccountAvailability::Available {
        return Err("operator account is inactive or unavailable".into());
    }
    Ok(account)
}

pub(super) fn require_operable_account_ref_for_read(
    repository: &SqliteRepository,
    account_ref_id: &AccountRefId,
) -> Result<AccountRef, crate::repository::OperableAccountRefError> {
    use crate::repository::OperableAccountRefError;

    let connection = repository
        .read_connection()
        .map_err(|_| OperableAccountRefError::Internal)?;
    let account = connection
        .query_row(
            "SELECT id, platform, kind, label, active, availability, created_at, updated_at
                 FROM account_refs WHERE id = ?1",
            [account_ref_id.as_str()],
            |row| {
                let platform: String = row.get(1)?;
                Ok(AccountRef {
                    id: AccountRefId::new(row.get::<_, String>(0)?),
                    platform: Platform::parse(&platform)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    kind: AccountKind::parse(&row.get::<_, String>(2)?)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    label: row.get(3)?,
                    active: row.get(4)?,
                    availability: AccountAvailability::parse(&row.get::<_, String>(5)?)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|_| OperableAccountRefError::Internal)?
        .ok_or(OperableAccountRefError::Unavailable)?;
    if !account.active || account.availability != AccountAvailability::Available {
        return Err(OperableAccountRefError::Unavailable);
    }
    Ok(account)
}
