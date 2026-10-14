//! CRM v36 -> v37 (app 0.6.1): retire transient discovery storage and old outreach claims.
//! The upgrade owner performs backup and owns the single chain transaction.

use rusqlite::{Connection, params};
use serde::Serialize;

use super::MigrationPreflightError;
use crate::{CreatorDatabaseUpgradeBlocker, CreatorDatabaseUpgradeImpact};

mod impact;
#[cfg(test)]
mod tests;

#[derive(Debug)]
struct OperationRewrite {
    id: String,
    impact: String,
    result: Option<String>,
    count: u64,
}

pub(super) fn preflight(
    connection: &Connection,
    from_version: i64,
    impact: &mut CreatorDatabaseUpgradeImpact,
) -> Result<(), MigrationPreflightError> {
    impact.removed_candidate_snapshots = count(
        connection,
        "SELECT COUNT(*) FROM creator_candidate_snapshots",
    )?;
    impact.removed_tiktok_one_account_bindings = if from_version >= 33 {
        count(
            connection,
            "SELECT COUNT(*) FROM tiktok_one_account_bindings",
        )?
    } else {
        0
    };
    impact.removed_creator_save_confirmations = count(
        connection,
        "SELECT COUNT(*) FROM idempotency_records WHERE action='creator_candidate_save_confirmation'",
    )?;
    impact.removed_contact_confirmations = count(
        connection,
        "SELECT COUNT(*) FROM idempotency_records WHERE action='creator_contact_confirmation'",
    )?;
    impact.unresolved_retired_outreach_actions = count(
        connection,
        "SELECT COUNT(*) FROM idempotency_records WHERE action IN ('relationship_update','content_reply_send','direct_message_send') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')",
    )?;
    impact.rewritten_contact_route_delete_impacts = operations(connection)?
        .iter()
        .map(|operation| operation.count)
        .sum();
    Ok(())
}

fn operations(connection: &Connection) -> Result<Vec<OperationRewrite>, MigrationPreflightError> {
    let mut statement = connection.prepare("SELECT id, creator_id, contact_route_id, impact_json, result_json FROM creator_contact_route_delete_operations ORDER BY id").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| {
        let (id, creator, route, source, result) = row.map_err(|error| error.to_string())?;
        let old: impact::ImpactV36 = serde_json::from_str(&source).map_err(|_| ambiguous())?;
        let rewritten = old.rewrite().map_err(|_| ambiguous())?;
        let mut rewritten_count = 1;
        let result = result
            .map(|bytes| -> Result<String, MigrationPreflightError> {
                let frozen: impact::ResultV36 =
                    serde_json::from_str(&bytes).map_err(|_| ambiguous())?;
                let impact::ResultV36::Deleted {
                    operation_id,
                    creator_id,
                    contact_route_id,
                    deleted,
                    ..
                } = &frozen
                else {
                    if frozen.operation_id() != id {
                        return Err(ambiguous());
                    }
                    return Ok(bytes);
                };
                if operation_id != &id
                    || creator_id != &creator
                    || contact_route_id != &route
                    || deleted != &old
                {
                    return Err(ambiguous());
                }
                let replacement = deleted.rewrite().map_err(|_| ambiguous())?;
                let mut result = serde_json::to_value(frozen).map_err(|error| error.to_string())?;
                result["deleted"] = replacement;
                rewritten_count += 1;
                serde_json::to_string(&result).map_err(|error| error.to_string().into())
            })
            .transpose()?;
        Ok(OperationRewrite {
            id,
            impact: serde_json::to_string(&rewritten).map_err(|error| error.to_string())?,
            result,
            count: rewritten_count,
        })
    })
    .collect()
}

#[derive(Serialize)]
struct RetiredActionMarker<'a> {
    kind: &'static str,
    action: &'a str,
    state_before: &'a str,
    // Preserve the original bytes, including whitespace or a non-JSON legacy
    // payload. This is private recovery evidence, never a wire projection.
    recovery: Option<&'a str>,
}

pub(super) fn migrate(connection: &Connection) -> Result<(), String> {
    let rewrites = operations(connection).map_err(|error| error.to_string())?;
    for operation in rewrites {
        connection.execute("UPDATE creator_contact_route_delete_operations SET impact_json=?2, result_json=?3 WHERE id=?1", params![operation.id,operation.impact,operation.result]).map_err(|error| error.to_string())?;
    }
    let mut statement = connection.prepare("SELECT key, action, state, result_json FROM idempotency_records WHERE action IN ('relationship_update','content_reply_send','direct_message_send') AND state IN ('in_progress','remote_started','remote_accepted','remote_verified') ORDER BY key").map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(statement);
    for (key, action, state, recovery) in rows {
        let marker = serde_json::to_string(&RetiredActionMarker {
            kind: "retired_action_unresolved",
            action: &action,
            state_before: &state,
            recovery: recovery.as_deref(),
        })
        .map_err(|error| error.to_string())?;
        connection.execute("UPDATE idempotency_records SET state='failed', result_json=?2, updated_at=?3 WHERE key=?1", params![key,marker,crate::repository::sqlite::now()]).map_err(|error| error.to_string())?;
    }
    connection.execute_batch("DELETE FROM idempotency_records WHERE action IN ('creator_candidate_save_confirmation','creator_contact_confirmation'); DROP TABLE creator_candidate_snapshots; DROP TABLE tiktok_one_account_bindings; PRAGMA user_version=37;").map_err(|error| error.to_string())
}

fn count(connection: &Connection, query: &str) -> Result<u64, MigrationPreflightError> {
    connection
        .query_row(query, [], |row| row.get(0))
        .map_err(|error| error.to_string().into())
}
fn ambiguous() -> MigrationPreflightError {
    MigrationPreflightError::Blocked(
        CreatorDatabaseUpgradeBlocker::AmbiguousContactRouteDeleteImpact,
    )
}
