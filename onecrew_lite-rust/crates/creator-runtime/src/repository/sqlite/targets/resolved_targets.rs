use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    AccountRefId, Platform, PlatformAccountId, ResolvedTarget, ResolvedTargetId, TargetKind,
};

use super::{
    super::{SqliteRepository, json, now},
    account_refs,
};

pub(super) fn upsert_resolved_target(
    repository: &SqliteRepository,
    account_ref_id: &AccountRefId,
    platform: Platform,
    target_kind: TargetKind,
    external_target_id: &str,
    platform_account_id: Option<&PlatformAccountId>,
    metadata: serde_json::Value,
) -> Result<ResolvedTarget, String> {
    let connection = repository.lock()?;
    account_refs::require_operable_account_ref_in(&connection, account_ref_id)?;
    let external_target_id = external_target_id.trim();
    if external_target_id.is_empty() {
        return Err("resolved target external id is required".into());
    }
    let timestamp = now();
    let existing = connection
        .query_row(
            "SELECT id, created_at FROM resolved_targets
                 WHERE account_ref_id = ?1 AND platform = ?2
                   AND target_kind = ?3 AND external_target_id = ?4",
            params![
                account_ref_id.as_str(),
                platform.as_str(),
                target_kind.as_str(),
                external_target_id
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (id, created_at) = existing
        .map(|(id, created_at)| (ResolvedTargetId::new(id), created_at))
        .unwrap_or_else(|| (ResolvedTargetId::new(Uuid::new_v4().to_string()), timestamp));
    connection
        .execute(
            "INSERT INTO resolved_targets
                   (id, account_ref_id, platform, target_kind, external_target_id,
                    platform_account_id, metadata_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(account_ref_id, platform, target_kind, external_target_id)
                 DO UPDATE SET platform_account_id = excluded.platform_account_id,
                   metadata_json = excluded.metadata_json, updated_at = excluded.updated_at",
            params![
                id.as_str(),
                account_ref_id.as_str(),
                platform.as_str(),
                target_kind.as_str(),
                external_target_id,
                platform_account_id.map(PlatformAccountId::as_str),
                json(&metadata)?,
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(ResolvedTarget {
        id,
        account_ref_id: account_ref_id.clone(),
        platform,
        target_kind,
        external_target_id: external_target_id.to_owned(),
        platform_account_id: platform_account_id.cloned(),
        metadata,
        created_at,
        updated_at: timestamp,
    })
}

pub(super) fn get_resolved_target(
    repository: &SqliteRepository,
    resolved_target_id: &ResolvedTargetId,
) -> Result<ResolvedTarget, String> {
    let connection = repository.read_connection()?;
    load_resolved_target(&connection, resolved_target_id)?
        .ok_or_else(|| "resolved target not found".into())
}

pub(in super::super) fn load_resolved_target(
    connection: &rusqlite::Connection,
    resolved_target_id: &ResolvedTargetId,
) -> Result<Option<ResolvedTarget>, String> {
    connection
        .query_row(
            "SELECT id, account_ref_id, platform, target_kind, external_target_id,
                    platform_account_id, metadata_json, created_at, updated_at
             FROM resolved_targets WHERE id = ?1",
            [resolved_target_id.as_str()],
            resolved_target_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn resolved_target_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResolvedTarget> {
    let platform: String = row.get(2)?;
    let target_kind: String = row.get(3)?;
    let metadata: String = row.get(6)?;
    Ok(ResolvedTarget {
        id: ResolvedTargetId::new(row.get::<_, String>(0)?),
        account_ref_id: AccountRefId::new(row.get::<_, String>(1)?),
        platform: Platform::parse(&platform)
            .map_err(super::super::creators::to_sql_conversion_error)?,
        target_kind: TargetKind::parse(&target_kind)
            .map_err(super::super::creators::to_sql_conversion_error)?,
        external_target_id: row.get(4)?,
        platform_account_id: row.get::<_, Option<String>>(5)?.map(PlatformAccountId::new),
        metadata: serde_json::from_str(&metadata)
            .map_err(|error| super::super::creators::to_sql_conversion_error(error.to_string()))?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}
