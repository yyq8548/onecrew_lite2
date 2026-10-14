use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    CreatorContactRouteDeleteBlockReason, CreatorContactRouteDeleteBlockingArtifact,
    CreatorContactRouteDeleteBlockingArtifactKind, CreatorContactRouteDeleteImpact,
    CreatorContactRouteDeleteOperationId, CreatorContactRouteDeleteStaleReason,
    CreatorContactRouteId, CreatorId, PrepareCreatorContactRouteDeleteInput,
    PrepareCreatorContactRouteDeleteResult,
};

use super::{
    super::{SqliteRepository, now},
    plan::{DeletePlan, build_plan},
    purge::purge_plan,
    shared::to_sql_error,
};

const DELETE_CONFIRMATION_TTL_SECONDS: u64 = 10 * 60;
// A terminal stale or expired result is stored in `result_json` with this
// value inside it. Rows written before the 2026-09-11 rename replay the name
// they were written with; ADR 0004 records why they are not rewritten.
const CONFIRM_TOOL: &str = "onecrew.creator.contact_route.delete.confirm";
const PREPARE_TOOL: &str = "onecrew.creator.contact_route.delete.prepare";

struct StoredOperation {
    creator_id: CreatorId,
    contact_route_id: CreatorContactRouteId,
    plan_fingerprint: String,
    impact: CreatorContactRouteDeleteImpact,
    status: String,
    result_json: Option<String>,
    expires_at: u64,
}

pub(in super::super) fn prepare(
    repository: &SqliteRepository,
    input: PrepareCreatorContactRouteDeleteInput,
) -> Result<PrepareCreatorContactRouteDeleteResult, String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let creator_active =
        super::super::contacts::creator_is_active_in(&transaction, &input.creator_id)?
            .ok_or_else(|| "creator not found".to_string())?;
    let route = super::super::contacts::load_creator_contact_route_by_id(
        &transaction,
        &input.creator_id,
        &input.contact_route_id,
    )?;
    let plan = build_plan(&transaction, route)?;

    if !creator_active {
        return Ok(blocked(
            &plan,
            CreatorContactRouteDeleteBlockReason::ArchivedCreator,
        ));
    }
    if super::super::contacts::has_root_initial_platform_observation(&plan.route) {
        return Ok(blocked(
            &plan,
            CreatorContactRouteDeleteBlockReason::RootInitialPlatformAccount,
        ));
    }
    if !plan.children.is_empty() {
        return Ok(blocked(
            &plan,
            CreatorContactRouteDeleteBlockReason::HasDerivedContactRoutes,
        ));
    }
    if let Some(reason) = blocking_reason(&plan.blocking_artifacts) {
        return Ok(blocked(&plan, reason));
    }

    let operation_id = CreatorContactRouteDeleteOperationId::new(Uuid::new_v4().to_string());
    let created_at = now();
    let expires_at = created_at.saturating_add(DELETE_CONFIRMATION_TTL_SECONDS);
    transaction
        .execute(
            "INSERT INTO creator_contact_route_delete_operations
               (id, creator_id, contact_route_id, plan_fingerprint, impact_json, status,
                result_json, expires_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', NULL, ?6, ?7, ?7)",
            params![
                operation_id.as_str(),
                input.creator_id.as_str(),
                input.contact_route_id.as_str(),
                plan.fingerprint,
                serde_json::to_string(&plan.impact).map_err(|error| error.to_string())?,
                expires_at,
                created_at,
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(
        PrepareCreatorContactRouteDeleteResult::ConfirmationRequired {
            operation_id,
            creator_id: input.creator_id,
            contact_route_id: input.contact_route_id,
            impact: plan.impact,
            remote_not_deleted: true,
            expires_at,
            next_tool: CONFIRM_TOOL.into(),
        },
    )
}

pub(in super::super) fn confirm(
    repository: &SqliteRepository,
    input: ConfirmCreatorContactRouteDeleteInput,
) -> Result<(ConfirmCreatorContactRouteDeleteResult, bool), String> {
    let mut connection = repository.lock()?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let stored = load_operation(&transaction, &input.operation_id)?
        .ok_or_else(|| "contact route delete operation not found".to_string())?;

    if stored.status == "completed" {
        let result =
            serde_json::from_str(stored.result_json.as_deref().ok_or_else(|| {
                "completed contact route delete operation is corrupt".to_string()
            })?)
            .map_err(|error| error.to_string())?;
        return Ok((result, false));
    }
    if now() > stored.expires_at {
        let result = ConfirmCreatorContactRouteDeleteResult::Expired {
            operation_id: input.operation_id.clone(),
            next_tool: PREPARE_TOOL.into(),
        };
        persist_terminal_result(&transaction, &input.operation_id, &result)?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok((result, false));
    }

    let route = match super::super::contacts::load_creator_contact_route_by_id(
        &transaction,
        &stored.creator_id,
        &stored.contact_route_id,
    ) {
        Ok(route) => route,
        Err(_) => {
            let result = stale(input.operation_id.clone());
            persist_terminal_result(&transaction, &input.operation_id, &result)?;
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok((result, false));
        }
    };
    let plan = build_plan(&transaction, route)?;
    let creator_active =
        super::super::contacts::creator_is_active_in(&transaction, &stored.creator_id)?
            .unwrap_or(false);
    let root_initial = super::super::contacts::has_root_initial_platform_observation(&plan.route);
    if !creator_active
        || root_initial
        || !plan.children.is_empty()
        || !plan.blocking_artifacts.is_empty()
        || plan.fingerprint != stored.plan_fingerprint
        || plan.impact != stored.impact
    {
        let result = stale(input.operation_id.clone());
        persist_terminal_result(&transaction, &input.operation_id, &result)?;
        transaction.commit().map_err(|error| error.to_string())?;
        return Ok((result, false));
    }

    purge_plan(&transaction, &plan)?;
    let result = ConfirmCreatorContactRouteDeleteResult::Deleted {
        operation_id: input.operation_id.clone(),
        creator_id: stored.creator_id,
        contact_route_id: stored.contact_route_id,
        deleted: plan.impact,
        remote_not_deleted: true,
    };
    persist_terminal_result(&transaction, &input.operation_id, &result)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok((result, true))
}

fn persist_terminal_result(
    connection: &Connection,
    operation_id: &CreatorContactRouteDeleteOperationId,
    result: &ConfirmCreatorContactRouteDeleteResult,
) -> Result<(), String> {
    let result_json = serde_json::to_string(result).map_err(|error| error.to_string())?;
    let affected = connection
        .execute(
            "UPDATE creator_contact_route_delete_operations
             SET status = 'completed', result_json = ?1, updated_at = ?2
             WHERE id = ?3 AND status = 'prepared'",
            params![result_json, now(), operation_id.as_str()],
        )
        .map_err(|error| error.to_string())?;
    if affected == 1 {
        Ok(())
    } else {
        Err("contact route delete operation changed while being confirmed".into())
    }
}

fn blocked(
    plan: &DeletePlan,
    reason: CreatorContactRouteDeleteBlockReason,
) -> PrepareCreatorContactRouteDeleteResult {
    PrepareCreatorContactRouteDeleteResult::Blocked {
        creator_id: plan.route.route.creator_id.clone(),
        contact_route_id: plan.route.route.id.clone(),
        reason,
        children: plan.children.clone(),
        blocking_artifacts: plan.blocking_artifacts.clone(),
    }
}

fn stale(
    operation_id: CreatorContactRouteDeleteOperationId,
) -> ConfirmCreatorContactRouteDeleteResult {
    ConfirmCreatorContactRouteDeleteResult::Stale {
        operation_id,
        reason: CreatorContactRouteDeleteStaleReason::PlanChanged,
        next_tool: PREPARE_TOOL.into(),
    }
}

fn blocking_reason(
    artifacts: &[CreatorContactRouteDeleteBlockingArtifact],
) -> Option<CreatorContactRouteDeleteBlockReason> {
    if artifacts.iter().any(|artifact| {
        matches!(
            artifact.kind,
            CreatorContactRouteDeleteBlockingArtifactKind::InFlightEmail
                | CreatorContactRouteDeleteBlockingArtifactKind::InFlightDirectMessage
                | CreatorContactRouteDeleteBlockingArtifactKind::InFlightRelationshipMutation
        )
    }) {
        return Some(CreatorContactRouteDeleteBlockReason::InFlightExternalAction);
    }
    if artifacts.iter().any(|artifact| {
        matches!(
            artifact.kind,
            CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousDirectMessageHistory
                | CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory
        )
    }) {
        return Some(CreatorContactRouteDeleteBlockReason::AmbiguousLegacyHistory);
    }
    (!artifacts.is_empty()).then_some(CreatorContactRouteDeleteBlockReason::SharedLocalHistory)
}

fn load_operation(
    connection: &Connection,
    operation_id: &CreatorContactRouteDeleteOperationId,
) -> Result<Option<StoredOperation>, String> {
    connection
        .query_row(
            "SELECT creator_id, contact_route_id, plan_fingerprint, impact_json, status,
                    result_json, expires_at
             FROM creator_contact_route_delete_operations WHERE id = ?1",
            [operation_id.as_str()],
            |row| {
                let impact_json: String = row.get(3)?;
                Ok(StoredOperation {
                    creator_id: CreatorId::new(row.get::<_, String>(0)?),
                    contact_route_id: CreatorContactRouteId::new(row.get::<_, String>(1)?),
                    plan_fingerprint: row.get(2)?,
                    impact: serde_json::from_str(&impact_json).map_err(to_sql_error)?,
                    status: row.get(4)?,
                    result_json: row.get(5)?,
                    expires_at: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}
