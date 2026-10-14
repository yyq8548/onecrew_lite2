use std::sync::Arc;

use crate::{
    ConfirmCreatorImportInput, ConfirmCreatorImportResult, CreatorBundle, CreatorImportOperationId,
    CreatorImportReceipt, CreatorTransferPlatform, CreatorTransferTargetKind,
    PrepareCreatorImportResult, creator_import::*,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use super::{SqliteRepository, now};

const IMPORT_OPERATION_TTL_SECONDS: u64 = 30 * 60;
const MAX_CONFLICT_SAMPLES: usize = 50;

pub(super) const ACCOUNT_BASIS_SQL: &str = "WITH input AS (
       SELECT json_extract(value, '$.platform') AS platform,
              json_extract(value, '$.target_kind') AS target_kind,
              json_extract(value, '$.external_id') AS external_id
       FROM json_each(?1)
     )
     SELECT account.platform, account.target_kind, account.external_id,
            account.id, link.creator_id, link.state,
            creator.score, creator.follower_counts_json,
            observation.handle, observation.display_name,
            observation.profile_url, observation.public_json, creator.tags_json
     FROM creator_account_links link
     JOIN platform_accounts account ON account.id = link.platform_account_id
     LEFT JOIN creators creator ON creator.id = link.creator_id
     LEFT JOIN platform_account_observations observation
       ON observation.rowid = (
         SELECT latest.rowid
         FROM platform_account_observations latest
         WHERE latest.platform_account_id = account.id
         ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1
       )
     WHERE link.platform_account_id IN (
       SELECT matched.id FROM input
       CROSS JOIN platform_accounts matched
       WHERE matched.platform = input.platform
         AND matched.target_kind = input.target_kind
         AND matched.external_id = input.external_id
     )
     UNION ALL
     SELECT account.platform, account.target_kind, account.external_id,
            account.id, NULL, NULL, NULL, NULL,
            observation.handle, observation.display_name,
            observation.profile_url, observation.public_json, NULL
     FROM input
     CROSS JOIN platform_accounts account
     LEFT JOIN platform_account_observations observation
       ON observation.rowid = (
         SELECT latest.rowid
         FROM platform_account_observations latest
         WHERE latest.platform_account_id = account.id
         ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1
       )
     WHERE account.platform = input.platform
       AND account.target_kind = input.target_kind
       AND account.external_id = input.external_id
       AND account.id NOT IN (
       SELECT platform_account_id FROM creator_account_links
     )
     ORDER BY 1, 2, 3, 5";

pub(super) const HANDLE_BASIS_SQL: &str = "WITH input AS (
       SELECT json_extract(value, '$.platform') AS platform,
              json_extract(value, '$.target_kind') AS target_kind,
              json_extract(value, '$.handle') AS handle
       FROM json_each(?1)
     )
     SELECT DISTINCT account.platform, account.target_kind,
                     lower(observation.handle), link.creator_id
     FROM creator_account_links link
     JOIN platform_accounts account ON account.id = link.platform_account_id
     JOIN platform_account_observations observation
       ON observation.platform_account_id = account.id
     WHERE link.state != 'rejected'
       AND observation.handle IS NOT NULL
       AND account.platform || ':' || account.target_kind || ':' || lower(observation.handle)
           IN (
             SELECT input.platform || ':' || input.target_kind || ':' || lower(input.handle)
             FROM input
           )
     ORDER BY 1, 2, 3, 4";

pub(super) const CONTACT_BASIS_SQL: &str = "WITH input AS (
       SELECT json_extract(value, '$.channel') AS channel,
              json_extract(value, '$.kind') AS kind,
              json_extract(value, '$.normalized_value') AS normalized_value
       FROM json_each(?1)
     )
     SELECT locator.channel, locator.kind, locator.normalized_value,
            locator.id, route.creator_id
     FROM creator_contact_routes route
     JOIN contact_locators locator ON locator.id = route.locator_id
     WHERE route.locator_id IN (
       SELECT matched.id FROM input
       CROSS JOIN contact_locators matched
       WHERE matched.channel = input.channel
         AND matched.kind = input.kind
         AND matched.normalized_value = input.normalized_value
     )
     UNION ALL
     SELECT input.channel, input.kind, input.normalized_value,
            locator.id, NULL
     FROM input
     CROSS JOIN contact_locators locator
     WHERE locator.channel = input.channel
       AND locator.kind = input.kind
       AND locator.normalized_value = input.normalized_value
       AND locator.id NOT IN (
       SELECT locator_id FROM creator_contact_routes
     )
     ORDER BY 1, 2, 3, 5";

pub(super) fn prepare(
    repository: &SqliteRepository,
    bundle: CreatorBundle,
) -> Result<PrepareCreatorImportResult, String> {
    let artifact = Arc::new(validated_artifact_from_bundle(bundle)?);
    for _ in 0..=2 {
        let basis = load_basis(repository, &artifact)?;
        let work = build_prepare_work(artifact.clone(), basis)?;
        match write_prepare(repository, work)? {
            PrepareImportWriteOutcome::BasisDrift => continue,
            outcome => return decode_prepare_outcome(outcome),
        }
    }
    Err("Creator import basis kept changing; prepare the bundle again".into())
}

pub(super) fn cancel(
    repository: &SqliteRepository,
    input: ConfirmCreatorImportInput,
) -> Result<ConfirmCreatorImportResult, String> {
    let cancelled = ConfirmCreatorImportResult::Cancelled {
        operation_id: input.operation_id.clone(),
    };
    let cancelled_json = serde_json::to_string(&cancelled).map_err(|error| error.to_string())?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let stored = transaction
        .query_row(
            "SELECT status, result_json FROM creator_import_operations WHERE id = ?1",
            [input.operation_id.as_str()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Creator import operation not found".to_string())?;
    let result_json = if stored.0 == "completed" {
        stored
            .1
            .ok_or_else(|| "completed Creator import has no result".to_string())?
    } else {
        complete_operation_json(&transaction, &input.operation_id, &cancelled_json, now())?;
        cancelled_json
    };
    transaction.commit().map_err(|error| error.to_string())?;
    drop(connection);
    serde_json::from_str(&result_json)
        .map_err(|error| format!("stored Creator import result is invalid: {error}"))
}

pub(super) fn confirm(
    repository: &SqliteRepository,
    input: ConfirmCreatorImportInput,
) -> Result<(ConfirmCreatorImportResult, bool), String> {
    let mut operation = load_operation(repository, &input)?;
    if let Some(result) = operation.completed_result()? {
        return Ok((result, false));
    }
    let artifact = Arc::new(operation.take_artifact()?);
    let basis = load_basis(repository, &artifact)?;
    let work = build_confirm_work(operation, artifact, basis)?;
    let (result, changed, _) = write_confirm(repository, work)?.decode()?;
    Ok((result, changed))
}

pub(super) fn load_operation(
    repository: &SqliteRepository,
    input: &ConfirmCreatorImportInput,
) -> Result<ImportOperationSnapshot, String> {
    let mut connection = repository.read_connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| error.to_string())?;
    let operation = load_operation_from_connection(&transaction, &input.operation_id)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(operation)
}

fn load_operation_from_connection(
    connection: &rusqlite::Connection,
    operation_id: &CreatorImportOperationId,
) -> Result<ImportOperationSnapshot, String> {
    connection
        .query_row(
            "SELECT content_sha256, bundle_json, plan_fingerprint, impact_json,
                    status, result_json, expires_at
             FROM creator_import_operations WHERE id = ?1",
            [operation_id.as_str()],
            |row| {
                Ok(ImportOperationSnapshot {
                    operation_id: operation_id.clone(),
                    content_sha256: row.get(0)?,
                    bundle_json: row.get(1)?,
                    plan_fingerprint: row.get(2)?,
                    impact_json: row.get(3)?,
                    status: row.get(4)?,
                    result_json: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Creator import operation not found".to_string())
}

pub(super) fn load_basis(
    repository: &SqliteRepository,
    artifact: &ValidatedImportArtifact,
) -> Result<ImportBasis, String> {
    let mut connection = repository.read_connection()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|error| error.to_string())?;
    let basis = load_basis_from_connection(&transaction, artifact)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(basis)
}

fn load_basis_from_connection(
    connection: &rusqlite::Connection,
    artifact: &ValidatedImportArtifact,
) -> Result<ImportBasis, String> {
    let domain_empty = creator_domain_is_empty(connection)?;
    let accounts = {
        let mut statement = connection
            .prepare_cached(ACCOUNT_BASIS_SQL)
            .map_err(|error| error.to_string())?;
        statement
            .query_map([&artifact.account_lookup_json], |row| {
                Ok(BasisAccountRow {
                    key: AccountNaturalKey {
                        platform: parse_transfer_platform(&row.get::<_, String>(0)?)?,
                        target_kind: parse_transfer_target_kind(&row.get::<_, String>(1)?)?,
                        external_id: row.get(2)?,
                    },
                    account_id: row.get(3)?,
                    creator_id: row.get(4)?,
                    link_state: row.get(5)?,
                    creator_score: row.get(6)?,
                    tags: row
                        .get::<_, Option<String>>(12)?
                        .map(|raw| serde_json::from_str(&raw))
                        .transpose()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                12,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?
                        .unwrap_or_default(),
                    follower_counts_json: row.get(7)?,
                    profile_handle: row.get(8)?,
                    profile_display_name: row.get(9)?,
                    profile_url: row.get(10)?,
                    profile_public_json: row.get(11)?,
                })
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
    };
    let handle_owners = {
        let mut statement = connection
            .prepare_cached(HANDLE_BASIS_SQL)
            .map_err(|error| error.to_string())?;
        statement
            .query_map([&artifact.handle_lookup_json], |row| {
                Ok(BasisHandleOwnerRow {
                    platform: parse_transfer_platform(&row.get::<_, String>(0)?)?,
                    target_kind: parse_transfer_target_kind(&row.get::<_, String>(1)?)?,
                    handle: row.get(2)?,
                    creator_id: row.get(3)?,
                })
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
    };
    let contacts = {
        let mut statement = connection
            .prepare_cached(CONTACT_BASIS_SQL)
            .map_err(|error| error.to_string())?;
        statement
            .query_map([&artifact.contact_lookup_json], |row| {
                Ok(BasisContactRow {
                    key: ContactNaturalKey {
                        channel: row.get(0)?,
                        kind: row.get(1)?,
                        normalized_value: row.get(2)?,
                    },
                    locator_id: row.get(3)?,
                    creator_id: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?
    };
    Ok(ImportBasis {
        domain_empty,
        accounts,
        handle_owners,
        contacts,
    })
}

fn parse_transfer_platform(value: &str) -> rusqlite::Result<CreatorTransferPlatform> {
    match value {
        "x" => Ok(CreatorTransferPlatform::X),
        "instagram" => Ok(CreatorTransferPlatform::Instagram),
        "telegram" => Ok(CreatorTransferPlatform::Telegram),
        "youtube" => Ok(CreatorTransferPlatform::YouTube),
        "tiktok" => Ok(CreatorTransferPlatform::TikTok),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

fn parse_transfer_target_kind(value: &str) -> rusqlite::Result<CreatorTransferTargetKind> {
    match value {
        "user" => Ok(CreatorTransferTargetKind::User),
        "bot" => Ok(CreatorTransferTargetKind::Bot),
        "channel" => Ok(CreatorTransferTargetKind::Channel),
        "supergroup" => Ok(CreatorTransferTargetKind::Supergroup),
        "group" => Ok(CreatorTransferTargetKind::Group),
        "creator" => Ok(CreatorTransferTargetKind::Creator),
        "business" => Ok(CreatorTransferTargetKind::Business),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

pub(super) fn write_prepare(
    repository: &SqliteRepository,
    work: PrepareImportWrite,
) -> Result<PrepareImportWriteOutcome, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let timestamp = now();
    expire_prepared_operations(&transaction, timestamp)?;
    let digests = std::iter::once(&work.artifact.bundle.content_sha256)
        .chain(work.artifact.historical_content_sha256.iter())
        .collect::<Vec<_>>();
    let digests_json = serde_json::to_string(&digests).map_err(|error| error.to_string())?;
    if let Some(receipt) = load_receipt_payload(&transaction, &digests_json)? {
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(PrepareImportWriteOutcome::Finished(receipt));
    }
    if let Some(prepared) = load_prepared_operation_payload(&transaction, &digests_json, timestamp)?
    {
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(PrepareImportWriteOutcome::Finished(prepared));
    }
    if load_basis_from_connection(&transaction, &work.artifact)? != work.basis {
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(PrepareImportWriteOutcome::BasisDrift);
    }
    if !work.planned.conflicts.is_empty() {
        let conflict_count = u64::try_from(work.planned.conflicts.len())
            .map_err(|_| "Creator import conflict count cannot be represented".to_string())?;
        let conflicts = work
            .planned
            .conflicts
            .into_iter()
            .take(MAX_CONFLICT_SAMPLES)
            .collect();
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(PrepareImportWriteOutcome::Finished(
            PrepareOutcomePayload::Direct(PrepareCreatorImportResult::Blocked {
                content_sha256: work.artifact.bundle.content_sha256.clone(),
                conflict_count,
                conflicts,
            }),
        ));
    }
    let plan = work
        .planned
        .plan
        .ok_or_else(|| "Creator import planner returned no plan".to_string())?;
    let plan_fingerprint = work
        .plan_fingerprint
        .ok_or_else(|| "Creator import plan fingerprint is missing".to_string())?;
    let impact_json = work
        .impact_json
        .ok_or_else(|| "Creator import impact encoding is missing".to_string())?;
    let expires_at = timestamp.saturating_add(IMPORT_OPERATION_TTL_SECONDS);
    transaction
        .prepare_cached(
            "INSERT INTO creator_import_operations
               (id, content_sha256, bundle_json, plan_fingerprint, impact_json,
                status, result_json, expires_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', NULL, ?6, ?7, ?7)",
        )
        .map_err(|error| error.to_string())?
        .execute(params![
            work.operation_id.as_str(),
            work.artifact.bundle.content_sha256,
            work.artifact.canonical_json,
            plan_fingerprint,
            impact_json,
            expires_at,
            timestamp
        ])
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(PrepareImportWriteOutcome::Finished(
        PrepareOutcomePayload::Direct(PrepareCreatorImportResult::ConfirmationRequired {
            operation_id: work.operation_id,
            content_sha256: plan.content_sha256,
            impact: plan.impact,
            expires_at,
        }),
    ))
}

fn load_receipt_payload(
    connection: &rusqlite::Connection,
    digests_json: &str,
) -> Result<Option<PrepareOutcomePayload>, String> {
    connection
        .query_row(
            "SELECT format_version, creator_count, impact_json, imported_at, content_sha256
             FROM creator_import_receipts WHERE content_sha256 IN (SELECT value FROM json_each(?1))
             ORDER BY CASE WHEN content_sha256 = json_extract(?1, '$[0]') THEN 0 ELSE 1 END LIMIT 1",
            [digests_json],
            |row| {
                Ok(PrepareOutcomePayload::AlreadyImported {
                    content_sha256: row.get(4)?,
                    format_version: row.get(0)?,
                    creator_count: row.get(1)?,
                    impact_json: row.get(2)?,
                    imported_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn load_prepared_operation_payload(
    connection: &rusqlite::Connection,
    digests_json: &str,
    timestamp: u64,
) -> Result<Option<PrepareOutcomePayload>, String> {
    connection
        .query_row(
            "SELECT id, impact_json, expires_at, content_sha256
             FROM creator_import_operations
             WHERE content_sha256 IN (SELECT value FROM json_each(?1)) AND status = 'prepared' AND expires_at > ?2
             ORDER BY created_at ASC LIMIT 1",
            params![digests_json, timestamp],
            |row| {
                Ok(PrepareOutcomePayload::ExistingPrepared {
                    operation_id: CreatorImportOperationId::new(row.get::<_, String>(0)?),
                    content_sha256: row.get(3)?,
                    impact_json: row.get(1)?,
                    expires_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub(super) fn write_confirm(
    repository: &SqliteRepository,
    work: ConfirmImportWrite,
) -> Result<ConfirmImportWriteOutcome, String> {
    #[cfg(test)]
    let timing_sink = work.timing_sink.clone();
    #[cfg(test)]
    let writer_wait_started = std::time::Instant::now();
    let mut connection = repository.lock()?;
    #[cfg(test)]
    let writer_wait = writer_wait_started.elapsed();
    #[cfg(test)]
    let writer_hold_started = std::time::Instant::now();
    #[cfg(test)]
    let transaction_started = std::time::Instant::now();
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    #[cfg(test)]
    let transaction_begin = transaction_started.elapsed();
    let authoritative = load_operation_from_connection(&transaction, &work.operation.operation_id)?;
    if authoritative.status == "completed" {
        let result_json = authoritative
            .result_json
            .ok_or_else(|| "completed Creator import has no result".to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(ConfirmImportWriteOutcome {
            result_json,
            changed: false,
            publish_change: false,
        });
    }
    let timestamp = now();
    if authoritative.expires_at <= timestamp {
        complete_operation_json(
            &transaction,
            &authoritative.operation_id,
            &work.expired_result_json,
            timestamp,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(ConfirmImportWriteOutcome {
            result_json: work.expired_result_json,
            changed: false,
            publish_change: false,
        });
    }
    if let Some(result_json) = receipt_result_json(
        &transaction,
        &authoritative.operation_id,
        &authoritative.content_sha256,
    )? {
        complete_operation_json(
            &transaction,
            &authoritative.operation_id,
            &result_json,
            timestamp,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(ConfirmImportWriteOutcome {
            result_json,
            changed: false,
            publish_change: false,
        });
    }
    let operation_unchanged = authoritative.content_sha256 == work.operation.content_sha256
        && authoritative.plan_fingerprint == work.operation.plan_fingerprint
        && authoritative.impact_json == work.operation.impact_json;
    let basis_unchanged = load_basis_from_connection(&transaction, &work.artifact)? == work.basis;
    if !operation_unchanged || !basis_unchanged || matches!(work.decision, ConfirmDecision::Stale) {
        complete_operation_json(
            &transaction,
            &authoritative.operation_id,
            &work.stale_result_json,
            timestamp,
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok(ConfirmImportWriteOutcome {
            result_json: work.stale_result_json,
            changed: false,
            publish_change: false,
        });
    }
    let ConfirmDecision::Apply(materialized_apply) = work.decision else {
        unreachable!("stale decisions return before apply")
    };
    let MaterializedApply {
        materialized,
        receipt,
    } = *materialized_apply;
    #[cfg(test)]
    let apply_started = std::time::Instant::now();
    apply_materialized(&transaction, &work.artifact.bundle, &materialized)?;
    import_mutation_failpoint(&materialized, "creator_import_receipts")?;
    insert_receipt_preencoded(
        &transaction,
        &work.artifact.bundle,
        &receipt,
        &work.impact_json,
    )?;
    complete_operation_json(
        &transaction,
        &authoritative.operation_id,
        &work.result_json,
        timestamp,
    )?;
    #[cfg(test)]
    let apply = apply_started.elapsed();
    let changed = receipt.impact.changes_creator_graph();
    #[cfg(test)]
    let commit_started = std::time::Instant::now();
    transaction.commit().map_err(|error| error.to_string())?;
    #[cfg(test)]
    if let Some(sink) = timing_sink {
        let commit = commit_started.elapsed();
        *sink
            .try_lock()
            .expect("Creator import timing sink contended") = Some(ImportWriterTiming {
            writer_wait,
            writer_hold: writer_hold_started.elapsed(),
            transaction_begin,
            apply,
            commit,
        });
    }
    Ok(ConfirmImportWriteOutcome {
        result_json: work.result_json,
        changed,
        publish_change: changed,
    })
}

fn expire_prepared_operations(
    connection: &rusqlite::Connection,
    timestamp: u64,
) -> Result<(), String> {
    connection
        .prepare_cached(
            "UPDATE creator_import_operations
             SET bundle_json = NULL,
                 status = 'completed',
                 result_json = json_object(
                   'status', 'expired',
                   'operation_id', id,
                   'next_action', ?2
                 ),
                 updated_at = ?1
             WHERE status = 'prepared' AND expires_at <= ?1",
        )
        .map_err(|error| error.to_string())?
        .execute(params![timestamp, PREPARE_AGAIN_ACTION])
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn creator_domain_is_empty(connection: &rusqlite::Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM creators) = 0
               AND (SELECT COUNT(*) FROM platform_accounts) = 0
               AND (SELECT COUNT(*) FROM creator_account_links) = 0
               AND (SELECT COUNT(*) FROM platform_account_observations) = 0
               AND (SELECT COUNT(*) FROM contact_locators) = 0
               AND (SELECT COUNT(*) FROM creator_contact_routes) = 0",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn execute_cached<P: rusqlite::Params>(
    connection: &rusqlite::Connection,
    sql: &str,
    params: P,
) -> Result<(), String> {
    connection
        .prepare_cached(sql)
        .map_err(|error| error.to_string())?
        .execute(params)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn import_mutation_failpoint(
    materialized: &MaterializedImport,
    table: &'static str,
) -> Result<(), String> {
    if materialized.forced_rollback_table() == Some(table) {
        return Err(format!("forced Creator import {table} rollback"));
    }
    Ok(())
}

fn insert_materialized_contact_routes(
    connection: &rusqlite::Connection,
    routes_json: &str,
    timestamp: u64,
) -> Result<(), String> {
    let conflict = connection
        .query_row(
            "WITH input AS (
               SELECT json_extract(value, '$.locator_id') AS locator_id,
                      json_extract(value, '$.creator_id') AS creator_id
               FROM json_each(?1)
             )
             SELECT
               EXISTS (
                 SELECT 1 FROM input
                 GROUP BY locator_id HAVING min(creator_id) != max(creator_id)
               )
               OR EXISTS (
                 SELECT 1 FROM creator_contact_routes existing
                 WHERE existing.locator_id IN (SELECT locator_id FROM input)
                   AND existing.locator_id || char(31) || existing.creator_id
                       NOT IN (
                         SELECT locator_id || char(31) || creator_id FROM input
                       )
               )",
            [routes_json],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|error| error.to_string())?;
    if conflict {
        return Err("materialized contact route ownership changed before apply".into());
    }
    execute_cached(
        connection,
        "INSERT INTO creator_contact_routes
           (id, creator_id, locator_id, confidence, created_at, updated_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.creator_id'),
                json_extract(value, '$.locator_id'),
                json_extract(value, '$.confidence'),
                json_extract(value, '$.created_at'), ?2
         FROM json_each(?1)",
        params![routes_json, timestamp],
    )
}

fn apply_materialized(
    connection: &rusqlite::Connection,
    bundle: &CreatorBundle,
    materialized: &MaterializedImport,
) -> Result<(), String> {
    let timestamp = materialized.timestamp;
    import_mutation_failpoint(materialized, "observation_sources")?;
    execute_cached(
        connection,
        "INSERT INTO observation_sources
           (id, kind, platform, source_url, external_ref, account_ref_id,
            metadata_json, observed_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.kind'),
                NULL, NULL, ?2, NULL, ?3, ?4
         FROM json_each(?1)",
        params![
            materialized.sources_json,
            bundle.content_sha256,
            materialized.source_metadata_json,
            timestamp
        ],
    )?;
    import_mutation_failpoint(materialized, "creators")?;
    execute_cached(
        connection,
        "INSERT INTO creators
           (id, display_name, notes, created_at, updated_at, archived_at,
            score, assessment, follower_counts_json, pinned_at, group_id, tags_json)
         SELECT json_extract(value, '$.id'),
                json_extract(value, '$.display_name'),
                json_extract(value, '$.notes'), ?2, ?2, json_extract(value, '$.archived_at'),
                json_extract(value, '$.score'), NULL,
                json_extract(value, '$.follower_counts_json'),
                json_extract(value, '$.pinned_at'), NULL, json_extract(value, '$.tags_json')
         FROM json_each(?1)",
        params![materialized.creator_inserts_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "creator_relationships")?;
    execute_cached(
        connection,
        "INSERT INTO creator_relationships
           (creator_id, stage, last_touch_at, created_at, updated_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.stage'),
                NULL, ?2, ?2
         FROM json_each(?1)",
        params![materialized.creator_inserts_json, timestamp],
    )?;
    execute_cached(
        connection,
        "WITH input AS (
           SELECT json_extract(value, '$.id') AS creator_id,
                  json_extract(value, '$.fill_score') AS fill_score,
                  json_extract(value, '$.tags_json') AS tags_json,
                  json_extract(value, '$.follower_counts_json') AS follower_counts_json
           FROM json_each(?1)
         )
         UPDATE creators
         SET score = COALESCE(creators.score, input.fill_score),
             tags_json = COALESCE(input.tags_json, creators.tags_json),
             follower_counts_json = COALESCE(input.follower_counts_json,
                                             creators.follower_counts_json),
             updated_at = ?2
         FROM input WHERE creators.id = input.creator_id",
        params![materialized.creator_updates_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "platform_accounts")?;
    execute_cached(
        connection,
        "INSERT INTO platform_accounts
           (id, platform, target_kind, external_id, created_at, updated_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.platform'),
                json_extract(value, '$.target_kind'),
                json_extract(value, '$.external_id'),
                json_extract(value, '$.created_at'), ?2
         FROM json_each(?1)
         ORDER BY json_extract(value, '$.capture_order'),
                  json_extract(value, '$.platform'),
                  json_extract(value, '$.target_kind'),
                  json_extract(value, '$.external_id')",
        params![materialized.accounts_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "creator_account_links")?;
    execute_cached(
        connection,
        "INSERT INTO creator_account_links
           (id, creator_id, platform_account_id, state, confidence,
            created_at, updated_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.creator_id'),
                json_extract(value, '$.account_id'), json_extract(value, '$.state'),
                json_extract(value, '$.confidence'), ?2, ?2
         FROM json_each(?1)",
        params![materialized.links_json, timestamp],
    )?;
    execute_cached(
        connection,
        "WITH input AS (
           SELECT json_extract(value, '$.creator_id') AS creator_id,
                  json_extract(value, '$.account_id') AS account_id,
                  json_extract(value, '$.confidence') AS confidence
           FROM json_each(?1)
         )
         UPDATE creator_account_links
         SET state = 'confirmed', confidence = input.confidence, updated_at = ?2
         FROM input
         WHERE creator_account_links.creator_id = input.creator_id
           AND creator_account_links.platform_account_id = input.account_id
           AND creator_account_links.state = 'claimed'",
        params![materialized.link_updates_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "platform_account_observations")?;
    execute_cached(
        connection,
        "INSERT INTO platform_account_observations
           (id, platform_account_id, source_id, handle, display_name,
            profile_url, public_json, observed_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.account_id'),
                json_extract(value, '$.source_id'), json_extract(value, '$.handle'),
                json_extract(value, '$.display_name'),
                json_extract(value, '$.profile_url'),
                json_extract(value, '$.public_json'), ?2
         FROM json_each(?1)",
        params![materialized.profiles_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "contact_locators")?;
    execute_cached(
        connection,
        "INSERT INTO contact_locators
           (id, channel, kind, normalized_value, display_value,
            created_at, updated_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.channel'),
                json_extract(value, '$.kind'), json_extract(value, '$.normalized_value'),
                json_extract(value, '$.display_value'),
                json_extract(value, '$.created_at'), ?2
         FROM json_each(?1)",
        params![materialized.locators_json, timestamp],
    )?;
    import_mutation_failpoint(materialized, "creator_contact_routes")?;
    insert_materialized_contact_routes(connection, &materialized.routes_json, timestamp)?;
    import_mutation_failpoint(materialized, "locator_observations")?;
    execute_cached(
        connection,
        "INSERT INTO locator_observations
           (id, locator_id, source_id, creator_id, platform_account_id,
            observed_value, observed_at)
         SELECT json_extract(value, '$.id'), json_extract(value, '$.locator_id'),
                json_extract(value, '$.source_id'),
                json_extract(value, '$.creator_id'), NULL,
                json_extract(value, '$.observed_value'),
                json_extract(value, '$.observed_at')
         FROM json_each(?1)",
        [&materialized.locator_observations_json],
    )?;
    Ok(())
}

fn insert_receipt_preencoded(
    connection: &rusqlite::Connection,
    bundle: &CreatorBundle,
    receipt: &CreatorImportReceipt,
    impact_json: &str,
) -> Result<(), String> {
    execute_cached(
        connection,
        "INSERT INTO creator_import_receipts
           (content_sha256, format, format_version, profile, snapshot_kind,
            creator_count, impact_json, source_id, imported_at)
         VALUES (?1, ?2, ?3, 'portable_public', 'full', ?4, ?5, NULL, ?6)",
        params![
            receipt.content_sha256,
            bundle.format,
            bundle.format_version,
            bundle.creator_count,
            impact_json,
            receipt.imported_at
        ],
    )
}

fn complete_operation_json(
    connection: &rusqlite::Connection,
    operation_id: &CreatorImportOperationId,
    result_json: &str,
    timestamp: u64,
) -> Result<(), String> {
    let changed = connection
        .prepare_cached(
            "UPDATE creator_import_operations
             SET bundle_json = NULL, status = 'completed', result_json = ?2, updated_at = ?3
             WHERE id = ?1 AND status = 'prepared'",
        )
        .map_err(|error| error.to_string())?
        .execute(params![operation_id.as_str(), result_json, timestamp])
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("Creator import operation is no longer prepared".into());
    }
    Ok(())
}

fn receipt_result_json(
    connection: &rusqlite::Connection,
    operation_id: &CreatorImportOperationId,
    content_sha256: &str,
) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT json_object(
               'status', CASE WHEN
                 coalesce(json_extract(impact_json, '$.creators_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.creator_scores_filled'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.creator_tags_filled'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.follower_counts_filled'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.platform_accounts_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.claimed_links_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.confirmed_links_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.rejected_links_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.links_promoted_to_confirmed'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.profile_observations_created'), 0) > 0 OR
                 coalesce(json_extract(impact_json, '$.contact_routes_created'), 0) > 0
                 THEN 'imported' ELSE 'unchanged' END,
               'operation_id', ?1,
               'receipt', json_object(
                 'content_sha256', content_sha256,
                 'format_version', format_version,
                 'creator_count', creator_count,
                 'impact', json(impact_json),
                 'imported_at', imported_at
               )
             )
             FROM creator_import_receipts WHERE content_sha256 = ?2",
            params![operation_id.as_str(), content_sha256],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}
