//! Consume selected observations with row savepoints and one transaction receipt.
use super::super::{SqliteRepository, now};
use super::fingerprint;
use super::persistence as facts;
use super::provenance::{
    discovery_selection_source, source_metadata, source_ref, validate_identity, validate_source,
};
use crate::creators::save::SelectedSaveRow;
use crate::*;
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use std::collections::BTreeMap;

const RECEIPT_ACTION: &str = "creator_save_batch_v1";

fn load_receipt(
    connection: &Connection,
    key: &str,
    expected: &str,
) -> Result<Option<CreatorSaveBatchResult>, String> {
    let receipt = connection.query_row("SELECT action, request_fingerprint, state, result_json FROM idempotency_records WHERE key = ?1", [key], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?))).optional().map_err(|error| error.to_string())?;
    match receipt {
        None => Ok(None),
        Some((action, fingerprint, state, result))
            if action == RECEIPT_ACTION
                && fingerprint.as_deref() == Some(expected)
                && state == "completed" =>
        {
            serde_json::from_str(
                result
                    .as_deref()
                    .ok_or("Creator save receipt has no result")?,
            )
            .map(Some)
            .map_err(|error| error.to_string())
        }
        Some(_) => {
            Err("Creator save execution key conflicts with another payload or action".into())
        }
    }
}

pub(in super::super) fn execute_prepared(
    repository: &SqliteRepository,
    prepared: SelectedPreparedCreatorSaveBatch,
    execution_key: String,
) -> Result<(CreatorSaveBatchResult, bool), String> {
    if execution_key.trim().is_empty() || execution_key.len() > 256 {
        return Err("Creator save execution key must be bounded and nonempty".into());
    }
    let payload_fingerprint = fingerprint(&(RECEIPT_ACTION, &prepared))?;
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    if let Some(result) = load_receipt(&transaction, &execution_key, &payload_fingerprint)? {
        return Ok((result, false));
    }
    let mut outcomes = Vec::new();
    let mut batch_tags = BTreeMap::<String, Vec<String>>::new();
    let intent = prepared.intent;
    for row in prepared.rows {
        // Each row has a real SQLite savepoint. All row effects, including
        // source/dependency writes and tag updates, roll back on row failure.
        transaction
            .execute_batch("SAVEPOINT creator_save_row")
            .map_err(|error| error.to_string())?;
        let row_id = row.input.row_id.clone();
        let saved_tags = batch_tags.clone();
        match save_row(&transaction, row, intent, &mut batch_tags) {
            Ok(outcome) => {
                transaction
                    .execute_batch("RELEASE SAVEPOINT creator_save_row")
                    .map_err(|error| error.to_string())?;
                outcomes.push(outcome);
            }
            Err(error) => {
                transaction.execute_batch("ROLLBACK TO SAVEPOINT creator_save_row; RELEASE SAVEPOINT creator_save_row").map_err(|rollback| format!("Creator save row rollback failed: {rollback}"))?;
                batch_tags = saved_tags;
                outcomes.push(CreatorSaveRowOutcome::Failed { row_id, error });
            }
        }
    }
    // Readbacks reflect all successful rows, including merged tags when two
    // selected sources refer to the same Creator.
    for outcome in &mut outcomes {
        if let CreatorSaveRowOutcome::Saved { creator, .. } = outcome {
            **creator =
                super::super::creators::load_creator_record(&transaction, &creator.creator.id)?;
        }
    }
    let result = CreatorSaveBatchResult { outcomes };
    transaction.execute("INSERT INTO idempotency_records(key, action, request_fingerprint, state, result_json, created_at, updated_at) VALUES(?1, ?2, ?3, 'completed', ?4, ?5, ?5)", params![execution_key, RECEIPT_ACTION, payload_fingerprint, serde_json::to_string(&result).map_err(|error| error.to_string())?, now()]).map_err(|error| error.to_string())?;
    if let Err(error) = transaction.commit() {
        // A COMMIT error is not permission to replay writes. Reconcile the
        // exact execution receipt first, including on the next explicit retry.
        drop(connection);
        let mut reader = repository.read_connection().map_err(|read_error| format!("Creator save commit outcome is unknown; receipt reconciliation is unavailable: {read_error}; commit: {error}"))?;
        let read_transaction = reader.transaction_with_behavior(TransactionBehavior::Deferred).map_err(|read_error| format!("Creator save commit outcome is unknown; receipt snapshot is unavailable: {read_error}; commit: {error}"))?;
        let receipt = load_receipt(&read_transaction, &execution_key, &payload_fingerprint)?;
        read_transaction.commit().map_err(|read_error| {
            format!("Creator save receipt reconciliation did not finish: {read_error}")
        })?;
        if let Some(receipt) = receipt {
            return Ok((receipt, true));
        }
        return Err(format!(
            "Creator save commit outcome is unknown; retry the same execution key to reconcile its receipt: {error}"
        ));
    }
    let changed = result
        .outcomes
        .iter()
        .any(|row| matches!(row, CreatorSaveRowOutcome::Saved { .. }));
    Ok((result, changed))
}

fn save_row(
    transaction: &Transaction<'_>,
    row: SelectedSaveRow,
    intent: CreatorSaveIntent,
    batch_tags: &mut BTreeMap<String, Vec<String>>,
) -> Result<CreatorSaveRowOutcome, String> {
    let mut input = row.input;
    validate_source(transaction, &input).map_err(|error| error.to_string())?;
    let identity = super::super::identity::resolve_creator_identity(transaction, &input.account)?;
    validate_identity(&input, &identity)?;
    if intent == CreatorSaveIntent::AddNew
        && matches!(identity, IdentityResolution::ExactExistingCreator { .. })
    {
        return Ok(CreatorSaveRowOutcome::Skipped {
            row_id: input.row_id,
            reason: CreatorSaveSkipReason::ExistingCreator,
        });
    }
    let (creator_id, created) = match identity {
        IdentityResolution::New => (
            facts::insert_creator(
                transaction,
                input.account.display_name.trim(),
                input.notes.as_deref(),
                &input.quality,
                input.tags.clone(),
            )?,
            true,
        ),
        IdentityResolution::ExactExistingCreator { creator_id } => {
            facts::ensure_active_creator(transaction, &creator_id)?;
            (creator_id, false)
        }
        _ => return Err("resolve ambiguous Creator identity before saving".into()),
    };
    if let Some(tags) = input.tags.take() {
        let merged = batch_tags
            .entry(creator_id.as_str().to_owned())
            .or_default();
        for tag in tags {
            if !merged.contains(&tag) {
                merged.push(tag);
            }
        }
        let tags = crate::creators::normalize_creator_tags(merged.clone())
            .map_err(|error| error.to_string())?;
        super::super::creators::apply_creator_tags_in(transaction, &creator_id, Some(tags), now())?;
    }
    let account_id = facts::upsert_account(transaction, &input.account)?;
    facts::link_account(transaction, &creator_id, &account_id)?;
    let source = super::super::contacts::upsert_observation_source_in(
        transaction,
        &ObservationSourceInput {
            kind: if input.provenance.administrator().is_some() {
                SourceKind::CommunityAdministrator
            } else {
                SourceKind::InitialPlatformAccount
            },
            platform: Some(input.account.platform),
            source_url: input.account.profile_url.clone(),
            external_ref: Some(source_ref(&input.account, &input.provenance)?),
            account_ref_id: input.provenance.account_ref().cloned(),
            metadata: source_metadata(&input.provenance)?,
        },
    )?;
    facts::append_observation(
        transaction,
        &account_id,
        &source.id,
        &input.account,
        &serde_json::to_value(&input.profile).map_err(|error| error.to_string())?,
        input.profile_observed_at,
    )?;
    // A delayed confirmation still records its held observation, but cannot
    // replace a follower projection obtained from a later actual profile read.
    let newer_profile_exists = transaction.query_row("SELECT EXISTS(SELECT 1 FROM platform_account_observations WHERE platform_account_id = ?1 AND observed_at > ?2)", params![account_id.as_str(), input.profile_observed_at], |row| row.get::<_, bool>(0)).map_err(|error| error.to_string())?;
    if newer_profile_exists {
        input
            .quality
            .follower_counts
            .remove(&input.account.platform);
    }
    super::super::creators::apply_creator_quality_in(
        transaction,
        &creator_id,
        &input.quality,
        now(),
    )?;
    if let CreatorSaveProvenance::Discovery { origin, .. } = &input.provenance {
        facts::append_origin(transaction, &creator_id, &source.id, origin)?;
    }
    let routes = super::super::contact_confirmations::persist_canonical_contact_confirmations_in(
        transaction,
        &creator_id,
        Some(&account_id),
        &input.account,
        &input.provenance,
        &row.confirmations,
    )?;
    if let Some(context) = discovery_selection_source(&input.account, &input.provenance)? {
        persist_selection_context(
            transaction,
            &creator_id,
            &account_id,
            context,
            &row.confirmations,
            &routes,
        )?;
    }
    let outreach_context =
        if let Some((_, source)) = input.provenance.administrator() {
            let initial =
                row.confirmations
                    .iter()
                    .find(|confirmation| {
                        confirmation.preview.observations.iter().any(|observation| {
                            observation.source == SourceKind::InitialPlatformAccount
                        })
                    })
                    .ok_or("administrator initial contact route is missing")?;
            let route_id = routes
                .get(&crate::contact_routes::locator_key(
                    &initial.preview.locator,
                ))
                .ok_or("administrator initial contact route was not persisted")?;
            if let CommunityAdministratorSource::CreatorContactRoute {
                source_contact_route_id,
                ..
            } = source
            {
                super::super::contacts::link_creator_contact_route_dependency_in(
                    transaction,
                    source_contact_route_id
                        .as_ref()
                        .ok_or("administrator source route is unavailable")?,
                    route_id,
                    CreatorContactRouteDependencyKind::CommunityAdministrator,
                )?;
            }
            Some(CommunityAdministratorOutreachContext {
                creator_id: creator_id.clone(),
                contact_route_id: route_id.clone(),
                operator_account_ref_id: input
                    .provenance
                    .account_ref()
                    .ok_or("administrator operator is missing")?
                    .clone(),
            })
        } else {
            None
        };
    Ok(CreatorSaveRowOutcome::Saved {
        row_id: input.row_id,
        creator: Box::new(super::super::creators::load_creator_record(
            transaction,
            &creator_id,
        )?),
        created,
        outreach_context,
    })
}

fn persist_selection_context(
    transaction: &Transaction<'_>,
    creator_id: &CreatorId,
    account_id: &PlatformAccountId,
    context: ObservationSourceInput,
    confirmations: &[ContactRouteConfirmation],
    routes: &BTreeMap<String, CreatorContactRouteId>,
) -> Result<(), String> {
    let (initial_route, initial_observation) = confirmations
        .iter()
        .find_map(|confirmation| {
            confirmation
                .preview
                .observations
                .iter()
                .find(|observation| observation.source == SourceKind::InitialPlatformAccount)
                .map(|observation| (&confirmation.preview.locator, observation))
        })
        .ok_or("discovery selection context requires its confirmed initial contact route")?;
    let route_id = routes
        .get(&crate::contact_routes::locator_key(initial_route))
        .ok_or("discovery selection context initial route was not persisted")?;
    let locator_id = transaction
        .query_row(
            "SELECT locator_id FROM creator_contact_routes WHERE id = ?1 AND creator_id = ?2",
            params![route_id.as_str(), creator_id.as_str()],
            |row| row.get::<_, String>(0).map(ContactLocatorId::new),
        )
        .map_err(|error| error.to_string())?;
    let source = super::super::contacts::upsert_observation_source_in(transaction, &context)?;
    super::super::contacts::append_locator_observation_in(
        transaction,
        &locator_id,
        &source.id,
        Some(creator_id),
        Some(account_id),
        &initial_observation.value,
    )?;
    Ok(())
}
