use std::collections::BTreeSet;

use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    AccountRefId, CommitContactTargetResolutionInput, CommitContactTargetResolutionResult,
    ConflictState, ContactLocatorId, CreatorAccountLinkId, CreatorId, CreatorTargetResolutionError,
    IdentityConflict, IdentityConflictId, LocatorResolution, LocatorResolutionId,
    LocatorResolutionWriteResult, Platform, PlatformAccountId, ResolutionState,
    ResolvedLocatorTarget, ResolvedTarget, ResolvedTargetId, TargetKind,
};

use super::{
    super::{SqliteRepository, json, now},
    resolved_targets::load_resolved_target,
};

pub(super) fn commit_contact_target_resolution(
    repository: &SqliteRepository,
    input: CommitContactTargetResolutionInput,
) -> Result<CommitContactTargetResolutionResult, CreatorTargetResolutionError> {
    let external_target_id = input.external_target_id.trim();
    if external_target_id.is_empty() {
        return Err(CreatorTargetResolutionError::InvalidEvidence);
    }

    let mut connection = repository
        .lock()
        .map_err(|_| CreatorTargetResolutionError::Storage)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|_| CreatorTargetResolutionError::Storage)?;
    let timestamp = now();
    let route_matches = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM creators creator JOIN creator_relationships relationship ON relationship.creator_id = creator.id
         JOIN creator_contact_routes route ON route.creator_id = creator.id JOIN contact_locators locator ON locator.id = route.locator_id
         WHERE creator.id = ?1 AND route.id = ?2 AND locator.id = ?3
           AND creator.archived_at IS NULL AND relationship.stage != 'archived')",
        params![input.creator_id.as_str(), input.contact_route_id.as_str(), input.locator_id.as_str()], |row| row.get::<_, bool>(0),
    ).map_err(|_| CreatorTargetResolutionError::Storage)?;
    if !route_matches {
        return Err(CreatorTargetResolutionError::ContextUnavailable);
    }
    let operator_platform = transaction.query_row(
        "SELECT platform FROM account_refs WHERE id = ?1 AND active = 1 AND availability = 'available'",
        [input.account_ref_id.as_str()], |row| row.get::<_, String>(0),
    ).optional().map_err(|_| CreatorTargetResolutionError::Storage)?.ok_or(CreatorTargetResolutionError::ContextUnavailable)?;
    if operator_platform != input.platform.as_str() {
        return Err(CreatorTargetResolutionError::InvalidEvidence);
    }

    let result = (|| -> Result<CommitContactTargetResolutionResult, String> {
    let existing_target = transaction
        .query_row(
            "SELECT id, created_at FROM resolved_targets
                 WHERE account_ref_id = ?1 AND platform = ?2
                   AND target_kind = ?3 AND external_target_id = ?4",
            params![
                input.account_ref_id.as_str(),
                input.platform.as_str(),
                input.target_kind.as_str(),
                external_target_id
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (target_id, target_created_at) = existing_target
        .map(|(id, created_at)| (ResolvedTargetId::new(id), created_at))
        .unwrap_or_else(|| (ResolvedTargetId::new(Uuid::new_v4().to_string()), timestamp));
    transaction
        .execute(
            "INSERT INTO resolved_targets
                   (id, account_ref_id, platform, target_kind, external_target_id,
                    platform_account_id, metadata_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?7, ?8)
                 ON CONFLICT(account_ref_id, platform, target_kind, external_target_id)
                 DO UPDATE SET metadata_json = excluded.metadata_json,
                   updated_at = excluded.updated_at",
            params![
                target_id.as_str(),
                input.account_ref_id.as_str(),
                input.platform.as_str(),
                input.target_kind.as_str(),
                external_target_id,
                json(&input.metadata)?,
                target_created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    let mut attempted_target = load_resolved_target(&transaction, &target_id)?
        .ok_or_else(|| "resolved target was not recorded".to_string())?;

    let active = load_active_resolution(&transaction, &input.locator_id, &input.account_ref_id)?;
    if let Some(active) = active.as_ref()
        && active.target.id != target_id
    {
        let conflict_key = format!(
            "locator_resolution:{}:{}:{}:{}",
            input.locator_id.as_str(),
            input.account_ref_id.as_str(),
            active.target.id.as_str(),
            target_id.as_str()
        );
        transaction
            .execute(
                "INSERT INTO identity_conflicts
                       (id, conflict_key, kind, creator_id, locator_id, resolved_target_id,
                        evidence_json, state, created_at)
                     VALUES (?1, ?2, 'locator_target_changed', ?3, ?4, ?5, ?6, 'open', ?7)
                     ON CONFLICT(conflict_key) DO NOTHING",
                params![
                    Uuid::new_v4().to_string(),
                    conflict_key,
                    input.creator_id.as_str(),
                    input.locator_id.as_str(),
                    target_id.as_str(),
                    json(&serde_json::json!({
                        "previous_target_id": active.target.id.as_str(),
                        "attempted_target_id": target_id.as_str(),
                    }))?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        let conflict = load_identity_conflict(&transaction, &conflict_key)?
            .ok_or_else(|| "identity conflict was not recorded".to_string())?;
        let active = active.clone();

        return Ok(CommitContactTargetResolutionResult::LocatorConflict {
            conflict,
            active: Box::new(active),
            attempted_target,
        });
    }

    if input.attach_to_creator {
        let existing_account = transaction
            .query_row(
                "SELECT id, created_at FROM platform_accounts
                     WHERE platform = ?1 AND target_kind = ?2 AND external_id = ?3",
                params![
                    input.platform.as_str(),
                    input.target_kind.as_str(),
                    external_target_id
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let existing_account_id = existing_account
            .as_ref()
            .map(|(id, _)| PlatformAccountId::new(id.clone()));
        let mut owners_statement = transaction
            .prepare(
                "SELECT link.creator_id
                     FROM platform_accounts account
                     JOIN creator_account_links link ON link.platform_account_id = account.id
                     WHERE account.platform = ?1 AND account.target_kind = ?2
                       AND account.external_id = ?3 AND link.state = 'confirmed'",
            )
            .map_err(|error| error.to_string())?;
        let owner_ids = owners_statement
            .query_map(
                params![
                    input.platform.as_str(),
                    input.target_kind.as_str(),
                    external_target_id
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<BTreeSet<_>, _>>()?;
        drop(owners_statement);
        let conflicting = owner_ids
            .into_iter()
            .filter(|creator_id| creator_id != input.creator_id.as_str())
            .map(CreatorId::new)
            .collect::<Vec<_>>();
        if !conflicting.is_empty() {
            let conflict_key = format!(
                "creator_target_owner:{}:{}:{}:{}",
                input.creator_id.as_str(),
                input.platform.as_str(),
                input.target_kind.as_str(),
                external_target_id
            );
            transaction
                .execute(
                    "INSERT INTO identity_conflicts
                           (id, conflict_key, kind, creator_id, platform_account_id,
                            locator_id, resolved_target_id, evidence_json, state, created_at)
                         VALUES (?1, ?2, 'platform_account_already_confirmed', ?3, ?4,
                                 ?5, ?6, ?7, 'open', ?8)
                         ON CONFLICT(conflict_key) DO NOTHING",
                    params![
                        Uuid::new_v4().to_string(),
                        conflict_key,
                        input.creator_id.as_str(),
                        existing_account_id.as_ref().map(PlatformAccountId::as_str),
                        input.locator_id.as_str(),
                        target_id.as_str(),
                        json(&serde_json::json!({
                            "conflicting_creator_ids": conflicting
                                .iter()
                                .map(CreatorId::as_str)
                                .collect::<Vec<_>>(),
                        }))?,
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
            let conflict = load_identity_conflict(&transaction, &conflict_key)?
                .ok_or_else(|| "identity conflict was not recorded".to_string())?;

            return Ok(CommitContactTargetResolutionResult::CreatorConflict {
                conflict,
                creator_ids: conflicting,
                attempted_target,
            });
        }

        let (account_id, account_created_at) = existing_account
            .map(|(id, created_at)| (PlatformAccountId::new(id), created_at))
            .unwrap_or_else(|| {
                (
                    PlatformAccountId::new(Uuid::new_v4().to_string()),
                    timestamp,
                )
            });
        transaction
            .execute(
                "INSERT INTO platform_accounts
                       (id, platform, target_kind, external_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(platform, target_kind, external_id) DO UPDATE SET
                       updated_at = excluded.updated_at",
                params![
                    account_id.as_str(),
                    input.platform.as_str(),
                    input.target_kind.as_str(),
                    external_target_id,
                    account_created_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        let existing_link = transaction
            .query_row(
                "SELECT id, created_at FROM creator_account_links
                     WHERE creator_id = ?1 AND platform_account_id = ?2",
                params![input.creator_id.as_str(), account_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let (link_id, link_created_at) = existing_link
            .map(|(id, created_at)| (CreatorAccountLinkId::new(id), created_at))
            .unwrap_or_else(|| {
                (
                    CreatorAccountLinkId::new(Uuid::new_v4().to_string()),
                    timestamp,
                )
            });
        transaction
            .execute(
                "INSERT INTO creator_account_links
                       (id, creator_id, platform_account_id, state, confidence, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'confirmed', 'exact', ?4, ?5)
                     ON CONFLICT(creator_id, platform_account_id) DO UPDATE SET
                       state = 'confirmed', confidence = 'exact', updated_at = excluded.updated_at",
                params![
                    link_id.as_str(),
                    input.creator_id.as_str(),
                    account_id.as_str(),
                    link_created_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE resolved_targets SET platform_account_id = ?1, updated_at = ?2
                     WHERE id = ?3",
                params![account_id.as_str(), timestamp, target_id.as_str()],
            )
            .map_err(|error| error.to_string())?;
        attempted_target.platform_account_id = Some(account_id);
        attempted_target.updated_at = timestamp;
    }

    let resolution = if let Some(mut active) = active {
        transaction
            .execute(
                "UPDATE locator_resolutions SET resolved_at = ?1 WHERE id = ?2",
                params![timestamp, active.resolution.id.as_str()],
            )
            .map_err(|error| error.to_string())?;
        active.resolution.resolved_at = timestamp;
        active.resolution
    } else {
        let resolution = LocatorResolution {
            id: LocatorResolutionId::new(Uuid::new_v4().to_string()),
            locator_id: input.locator_id,
            account_ref_id: input.account_ref_id,
            resolved_target_id: target_id.clone(),
            state: ResolutionState::Active,
            resolved_at: timestamp,
            invalidated_at: None,
        };
        transaction
            .execute(
                "INSERT INTO locator_resolutions
                       (id, locator_id, account_ref_id, resolved_target_id, state,
                        resolved_at, invalidated_at)
                     VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL)",
                params![
                    resolution.id.as_str(),
                    resolution.locator_id.as_str(),
                    resolution.account_ref_id.as_str(),
                    resolution.resolved_target_id.as_str(),
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        resolution
    };

    Ok(CommitContactTargetResolutionResult::Active {
        value: ResolvedLocatorTarget {
            resolution,
            target: attempted_target,
        },
    })
    })().map_err(|_| CreatorTargetResolutionError::Storage)?;
    transaction
        .commit()
        .map_err(|_| CreatorTargetResolutionError::CommitUnknown)?;
    Ok(result)
}

pub(super) fn active_locator_resolution(
    repository: &SqliteRepository,
    locator_id: &ContactLocatorId,
    account_ref_id: &AccountRefId,
) -> Result<Option<ResolvedLocatorTarget>, String> {
    let connection = repository.read_connection()?;
    load_active_resolution(&connection, locator_id, account_ref_id)
}

pub(super) fn record_locator_resolution(
    repository: &SqliteRepository,
    locator_id: &ContactLocatorId,
    account_ref_id: &AccountRefId,
    resolved_target_id: &ResolvedTargetId,
) -> Result<LocatorResolutionWriteResult, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let timestamp = now();
    let attempted_target = load_resolved_target(&transaction, resolved_target_id)?
        .ok_or_else(|| "resolved target not found".to_string())?;
    if attempted_target.account_ref_id != *account_ref_id {
        return Err("resolved target belongs to another operator account".into());
    }
    let active = load_active_resolution(&transaction, locator_id, account_ref_id)?;

    if let Some(mut active) = active {
        if active.target.id == *resolved_target_id {
            transaction
                .execute(
                    "UPDATE locator_resolutions SET resolved_at = ?1
                         WHERE id = ?2",
                    params![timestamp, active.resolution.id.as_str()],
                )
                .map_err(|error| error.to_string())?;
            active.resolution.resolved_at = timestamp;
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(LocatorResolutionWriteResult::Active { value: active });
        }
        let conflict_key = format!(
            "locator_resolution:{}:{}:{}:{}",
            locator_id.as_str(),
            account_ref_id.as_str(),
            active.target.id.as_str(),
            resolved_target_id.as_str()
        );
        let conflict_id = IdentityConflictId::new(Uuid::new_v4().to_string());
        transaction
            .execute(
                "INSERT INTO identity_conflicts
                       (id, conflict_key, kind, locator_id, resolved_target_id,
                        evidence_json, state, created_at)
                     VALUES (?1, ?2, 'locator_target_changed', ?3, ?4, ?5, 'open', ?6)
                    ON CONFLICT(conflict_key) DO NOTHING",
                params![
                    conflict_id.as_str(),
                    conflict_key,
                    locator_id.as_str(),
                    resolved_target_id.as_str(),
                    json(&serde_json::json!({
                        "previous_target_id": active.target.id.as_str(),
                        "attempted_target_id": resolved_target_id.as_str(),
                    }))?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        let conflict = load_identity_conflict(&transaction, &conflict_key)?
            .ok_or_else(|| "identity conflict was not recorded".to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(LocatorResolutionWriteResult::Conflict {
            conflict,
            active: Box::new(active),
            attempted_target,
        });
    }

    let id = LocatorResolutionId::new(Uuid::new_v4().to_string());
    transaction
        .execute(
            "INSERT INTO locator_resolutions
                   (id, locator_id, account_ref_id, resolved_target_id, state,
                    resolved_at, invalidated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id.as_str(),
                locator_id.as_str(),
                account_ref_id.as_str(),
                resolved_target_id.as_str(),
                ResolutionState::Active.as_str(),
                timestamp,
                Option::<u64>::None
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(LocatorResolutionWriteResult::Active {
        value: ResolvedLocatorTarget {
            resolution: LocatorResolution {
                id,
                locator_id: locator_id.clone(),
                account_ref_id: account_ref_id.clone(),
                resolved_target_id: resolved_target_id.clone(),
                state: ResolutionState::Active,
                resolved_at: timestamp,
                invalidated_at: None,
            },
            target: attempted_target,
        },
    })
}

pub(super) fn invalidate_locator_resolution(
    repository: &SqliteRepository,
    locator_id: &ContactLocatorId,
    account_ref_id: &AccountRefId,
) -> Result<Option<LocatorResolution>, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let Some(mut active) = load_active_resolution(&transaction, locator_id, account_ref_id)? else {
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(None);
    };
    let timestamp = now();
    transaction
        .execute(
            "UPDATE locator_resolutions
                 SET state = 'invalidated', invalidated_at = ?1
                 WHERE id = ?2 AND state = 'active'",
            params![timestamp, active.resolution.id.as_str()],
        )
        .map_err(|error| error.to_string())?;
    active.resolution.state = ResolutionState::Invalidated;
    active.resolution.invalidated_at = Some(timestamp);
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(Some(active.resolution))
}

fn load_active_resolution(
    connection: &rusqlite::Connection,
    locator_id: &ContactLocatorId,
    account_ref_id: &AccountRefId,
) -> Result<Option<ResolvedLocatorTarget>, String> {
    connection
        .query_row(
            "SELECT resolution.id, resolution.locator_id, resolution.account_ref_id,
                    resolution.resolved_target_id, resolution.state, resolution.resolved_at,
                    resolution.invalidated_at,
                    target.id, target.account_ref_id, target.platform, target.target_kind,
                    target.external_target_id, target.platform_account_id, target.metadata_json,
                    target.created_at, target.updated_at
             FROM locator_resolutions resolution
             JOIN resolved_targets target ON target.id = resolution.resolved_target_id
             WHERE resolution.locator_id = ?1 AND resolution.account_ref_id = ?2
               AND resolution.state = 'active'",
            params![locator_id.as_str(), account_ref_id.as_str()],
            |row| {
                let state: String = row.get(4)?;
                let platform: String = row.get(9)?;
                let target_kind: String = row.get(10)?;
                let metadata: String = row.get(13)?;
                Ok(ResolvedLocatorTarget {
                    resolution: LocatorResolution {
                        id: LocatorResolutionId::new(row.get::<_, String>(0)?),
                        locator_id: ContactLocatorId::new(row.get::<_, String>(1)?),
                        account_ref_id: AccountRefId::new(row.get::<_, String>(2)?),
                        resolved_target_id: ResolvedTargetId::new(row.get::<_, String>(3)?),
                        state: ResolutionState::parse(&state)
                            .map_err(super::super::creators::to_sql_conversion_error)?,
                        resolved_at: row.get(5)?,
                        invalidated_at: row.get(6)?,
                    },
                    target: ResolvedTarget {
                        id: ResolvedTargetId::new(row.get::<_, String>(7)?),
                        account_ref_id: AccountRefId::new(row.get::<_, String>(8)?),
                        platform: Platform::parse(&platform)
                            .map_err(super::super::creators::to_sql_conversion_error)?,
                        target_kind: TargetKind::parse(&target_kind)
                            .map_err(super::super::creators::to_sql_conversion_error)?,
                        external_target_id: row.get(11)?,
                        platform_account_id: row
                            .get::<_, Option<String>>(12)?
                            .map(PlatformAccountId::new),
                        metadata: serde_json::from_str(&metadata).map_err(|error| {
                            super::super::creators::to_sql_conversion_error(error.to_string())
                        })?,
                        created_at: row.get(14)?,
                        updated_at: row.get(15)?,
                    },
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn load_identity_conflict(
    connection: &rusqlite::Connection,
    conflict_key: &str,
) -> Result<Option<IdentityConflict>, String> {
    connection
        .query_row(
            "SELECT id, conflict_key, kind, creator_id, platform_account_id, locator_id,
                    resolved_target_id, evidence_json, state, created_at, resolved_at
             FROM identity_conflicts WHERE conflict_key = ?1",
            [conflict_key],
            |row| {
                let evidence: String = row.get(7)?;
                let state: String = row.get(8)?;
                Ok(IdentityConflict {
                    id: IdentityConflictId::new(row.get::<_, String>(0)?),
                    conflict_key: row.get(1)?,
                    kind: row.get(2)?,
                    creator_id: row.get::<_, Option<String>>(3)?.map(crate::CreatorId::new),
                    platform_account_id: row
                        .get::<_, Option<String>>(4)?
                        .map(PlatformAccountId::new),
                    locator_id: row.get::<_, Option<String>>(5)?.map(ContactLocatorId::new),
                    resolved_target_id: row.get::<_, Option<String>>(6)?.map(ResolvedTargetId::new),
                    evidence: serde_json::from_str(&evidence).map_err(|error| {
                        super::super::creators::to_sql_conversion_error(error.to_string())
                    })?,
                    state: ConflictState::parse(&state)
                        .map_err(super::super::creators::to_sql_conversion_error)?,
                    created_at: row.get(9)?,
                    resolved_at: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}
