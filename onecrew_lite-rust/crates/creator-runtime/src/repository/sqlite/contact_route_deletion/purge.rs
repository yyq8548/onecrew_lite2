use rusqlite::Connection;

use crate::CreatorContactRouteId;

use super::{plan::DeletePlan, shared::string_rows};

#[derive(Debug, Default, PartialEq, Eq)]
struct HistoricalDetachCounts {
    creator_origins: u64,
}

pub(super) fn purge_plan(connection: &Connection, plan: &DeletePlan) -> Result<(), String> {
    let route_id = plan.route.route.id.as_str();
    let locator_id = plan.route.locator.id.as_str();
    let email_thread_ids = &plan.email.thread_ids;
    let conversation_ids = string_rows(
        connection,
        "SELECT conversation_id FROM conversation_contact_routes
         WHERE contact_route_id = ?1 ORDER BY conversation_id",
        &[&route_id],
    )?;
    let source_ids = string_rows(
        connection,
        "SELECT DISTINCT source_id FROM locator_observations
         WHERE locator_id = ?1 ORDER BY source_id",
        &[&locator_id],
    )?;
    let target_ids = string_rows(
        connection,
        "SELECT DISTINCT resolved_target_id FROM locator_resolutions
         WHERE locator_id = ?1 ORDER BY resolved_target_id",
        &[&locator_id],
    )?;

    connection
        .execute(
            "DELETE FROM idempotency_records WHERE contact_route_id = ?1",
            [route_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM touchpoints WHERE contact_route_id = ?1",
            [route_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM audit_events WHERE contact_route_id = ?1",
            [route_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM email_drafts WHERE contact_route_id = ?1",
            [route_id],
        )
        .map_err(|error| error.to_string())?;
    for thread_id in email_thread_ids {
        connection
            .execute(
                "DELETE FROM email_recipients WHERE email_id IN (
                   SELECT id FROM emails WHERE thread_id = ?1
                 )",
                [thread_id],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute("DELETE FROM emails WHERE thread_id = ?1", [thread_id])
            .map_err(|error| error.to_string())?;
        connection
            .execute("DELETE FROM email_threads WHERE id = ?1", [thread_id])
            .map_err(|error| error.to_string())?;
    }
    for conversation_id in &conversation_ids {
        connection
            .execute(
                "DELETE FROM messages WHERE conversation_id = ?1",
                [conversation_id],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "DELETE FROM conversation_contact_routes WHERE conversation_id = ?1",
                [conversation_id],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute("DELETE FROM conversations WHERE id = ?1", [conversation_id])
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "DELETE FROM identity_conflicts WHERE locator_id = ?1",
            [locator_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM locator_resolutions WHERE locator_id = ?1",
            [locator_id],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "DELETE FROM locator_observations WHERE locator_id = ?1",
            [locator_id],
        )
        .map_err(|error| error.to_string())?;
    let detached = detach_historical_route_sources(connection, &plan.route.route.id)?;
    ensure_affected_count(
        "creator origin detach",
        detached.creator_origins,
        plan.impact.creator_origins_detached,
    )?;
    connection
        .execute(
            "DELETE FROM creator_contact_routes WHERE id = ?1",
            [route_id],
        )
        .map_err(|error| error.to_string())?;

    for source_id in source_ids {
        connection
            .execute(
                "DELETE FROM observation_sources
                 WHERE id = ?1
                   AND NOT EXISTS (SELECT 1 FROM locator_observations WHERE source_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM platform_account_observations WHERE source_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM creator_origins WHERE source_id = ?1)",
                [source_id],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute(
            "DELETE FROM contact_locators
             WHERE id = ?1
               AND NOT EXISTS (SELECT 1 FROM creator_contact_routes WHERE locator_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM locator_observations WHERE locator_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM locator_resolutions WHERE locator_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM identity_conflicts WHERE locator_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM email_recipients WHERE locator_id = ?1)
               AND NOT EXISTS (SELECT 1 FROM audit_events WHERE contact_locator_id = ?1)",
            [locator_id],
        )
        .map_err(|error| error.to_string())?;
    for target_id in target_ids {
        connection
            .execute(
                "DELETE FROM resolved_targets
                 WHERE id = ?1
                   AND NOT EXISTS (SELECT 1 FROM locator_resolutions WHERE resolved_target_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM conversations WHERE resolved_target_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM messages WHERE sender_target_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM identity_conflicts WHERE resolved_target_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM audit_events WHERE resolved_target_id = ?1)
                   AND NOT EXISTS (SELECT 1 FROM creator_origins WHERE source_resolved_target_id = ?1)",
                [target_id],
            )
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn detach_historical_route_sources(
    connection: &Connection,
    route_id: &CreatorContactRouteId,
) -> Result<HistoricalDetachCounts, String> {
    let creator_origins = connection
        .execute(
            "UPDATE creator_origins
             SET source_contact_route_id = NULL
             WHERE source_contact_route_id = ?1",
            [route_id.as_str()],
        )
        .map_err(|error| error.to_string())? as u64;
    Ok(HistoricalDetachCounts { creator_origins })
}

fn ensure_affected_count(label: &str, actual: u64, expected: u64) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "contact route delete {label} count changed: expected {expected}, found {actual}"
        ))
    }
}
