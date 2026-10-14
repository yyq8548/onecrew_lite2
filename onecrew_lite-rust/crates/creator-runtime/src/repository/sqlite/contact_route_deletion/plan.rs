use std::collections::BTreeMap;

use rusqlite::{Connection, ToSql, params};
use serde::Serialize;

use crate::{
    ContactChannel, CreatorContactRouteDeleteBlockingArtifact,
    CreatorContactRouteDeleteBlockingArtifactKind, CreatorContactRouteDeleteChildSummary,
    CreatorContactRouteDeleteImpact, CreatorContactRouteDependencyKind, CreatorContactRouteId,
    CreatorContactRouteRecord, CreatorId, EmailAddress,
};

use super::shared::{string_rows, to_sql_error};

#[derive(Debug, Serialize)]
struct DeletePlanFingerprint<'a> {
    creator_id: &'a str,
    contact_route_id: &'a str,
    locator_id: &'a str,
    route_updated_at: u64,
    locator_updated_at: u64,
    impact: &'a CreatorContactRouteDeleteImpact,
    observations: Vec<String>,
    dependencies: Vec<String>,
    email_threads: Vec<String>,
    emails: Vec<String>,
    email_recipients: Vec<String>,
    email_drafts: Vec<String>,
    conversations: Vec<String>,
    messages: Vec<String>,
    touchpoints: Vec<String>,
    audit_events: Vec<String>,
    idempotency_records: Vec<String>,
    resolutions: Vec<String>,
    identity_conflicts: Vec<String>,
    observation_sources: Vec<String>,
    resolved_targets: Vec<String>,
    creator_origins: Vec<String>,
}

pub(super) struct DeletePlan {
    pub(super) route: CreatorContactRouteRecord,
    pub(super) impact: CreatorContactRouteDeleteImpact,
    pub(super) fingerprint: String,
    pub(super) email: EmailDeletionPlan,
    pub(super) children: Vec<CreatorContactRouteDeleteChildSummary>,
    pub(super) blocking_artifacts: Vec<CreatorContactRouteDeleteBlockingArtifact>,
}

#[derive(Debug, Default)]
pub(super) struct EmailDeletionPlan {
    pub(super) thread_ids: Vec<String>,
    pub(super) email_ids: Vec<String>,
    pub(super) thread_fingerprints: Vec<String>,
    pub(super) email_fingerprints: Vec<String>,
    pub(super) recipient_fingerprints: Vec<String>,
    pub(super) shared_threads: u64,
}

#[derive(Debug)]
struct EmailThreadCandidate {
    id: String,
    creator_id: Option<String>,
    contact_route_id: Option<String>,
    mailbox_address: String,
    updated_at: u64,
}

#[derive(Debug)]
struct EmailParticipant {
    address: String,
    locator_id: Option<String>,
}

pub(super) fn build_plan(
    connection: &Connection,
    route: CreatorContactRouteRecord,
) -> Result<DeletePlan, String> {
    let email = load_email_deletion_plan(connection, &route)?;
    let children = load_children(connection, &route.route.id)?;
    let blocking_artifacts = load_blocking_artifacts(connection, &route, &email)?;
    let impact = load_impact(connection, &route, &email)?;
    let fingerprint = build_fingerprint(connection, &route, &impact, &email)?;
    Ok(DeletePlan {
        route,
        impact,
        fingerprint,
        email,
        children,
        blocking_artifacts,
    })
}

fn load_children(
    connection: &Connection,
    route_id: &CreatorContactRouteId,
) -> Result<Vec<CreatorContactRouteDeleteChildSummary>, String> {
    let mut statement = connection
        .prepare(
            "SELECT child.creator_id, dependency.child_contact_route_id, dependency.kind,
                    locator.channel, locator.display_value
             FROM creator_contact_route_dependencies dependency
             JOIN creator_contact_routes child ON child.id = dependency.child_contact_route_id
             JOIN contact_locators locator ON locator.id = child.locator_id
             WHERE dependency.parent_contact_route_id = ?1
             ORDER BY dependency.child_contact_route_id",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([route_id.as_str()], |row| {
            let kind: String = row.get(2)?;
            let channel: String = row.get(3)?;
            Ok(CreatorContactRouteDeleteChildSummary {
                creator_id: CreatorId::new(row.get::<_, String>(0)?),
                contact_route_id: CreatorContactRouteId::new(row.get::<_, String>(1)?),
                dependency_kind: parse_dependency_kind(&kind)?,
                channel: ContactChannel::parse(&channel).map_err(to_sql_error)?,
                display_value: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

fn load_email_deletion_plan(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
) -> Result<EmailDeletionPlan, String> {
    let mut thread_statement = connection
        .prepare(
            "SELECT thread.id, thread.creator_id, thread.contact_route_id,
                    mailbox.email_address, thread.updated_at
             FROM email_threads thread
             JOIN mailbox_refs mailbox ON mailbox.id = thread.mailbox_ref_id
             ORDER BY thread.id",
        )
        .map_err(|error| error.to_string())?;
    let threads = thread_statement
        .query_map([], |row| {
            Ok(EmailThreadCandidate {
                id: row.get(0)?,
                creator_id: row.get(1)?,
                contact_route_id: row.get(2)?,
                mailbox_address: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(thread_statement);

    let mut participants = BTreeMap::<String, Vec<EmailParticipant>>::new();
    let mut participant_statement = connection
        .prepare(
            "SELECT email.thread_id, recipient.address, recipient.locator_id
             FROM emails email
             JOIN email_recipients recipient ON recipient.email_id = email.id
             ORDER BY email.thread_id, recipient.email_id,
                      recipient.recipient_kind, recipient.address",
        )
        .map_err(|error| error.to_string())?;
    let participant_rows = participant_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                EmailParticipant {
                    address: row.get(1)?,
                    locator_id: row.get(2)?,
                },
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    drop(participant_statement);
    for (thread_id, participant) in participant_rows {
        participants.entry(thread_id).or_default().push(participant);
    }

    let target_address = match route.locator.channel {
        ContactChannel::Email => Some(
            EmailAddress::parse(&route.locator.normalized_value)
                .map_err(|error| format!("stored email contact route is invalid: {error}"))?,
        ),
        _ => None,
    };
    let route_id = route.route.id.as_str();
    let creator_id = route.route.creator_id.as_str();
    let locator_id = route.locator.id.as_str();
    let mut plan = EmailDeletionPlan::default();

    for thread in threads {
        let thread_participants = participants.remove(&thread.id).unwrap_or_default();
        let references_target = thread.contact_route_id.as_deref() == Some(route_id)
            || thread_participants
                .iter()
                .any(|participant| participant.locator_id.as_deref() == Some(locator_id))
            || target_address.as_ref().is_some_and(|target| {
                thread_participants.iter().any(|participant| {
                    EmailAddress::parse(&participant.address)
                        .is_ok_and(|address| address == *target)
                })
            });
        if !references_target {
            continue;
        }

        let mailbox_address = EmailAddress::parse(&thread.mailbox_address).ok();
        let mut saw_target = false;
        let participants_are_exclusive = target_address.as_ref().is_some_and(|target| {
            mailbox_address.as_ref().is_some_and(|mailbox| {
                thread_participants.iter().all(|participant| {
                    let Ok(address) = EmailAddress::parse(&participant.address) else {
                        return false;
                    };
                    if let Some(participant_locator_id) = participant.locator_id.as_deref()
                        && (participant_locator_id != locator_id || address != *target)
                    {
                        return false;
                    }
                    if address == *mailbox {
                        return true;
                    }
                    if address == *target {
                        saw_target = true;
                        return true;
                    }
                    false
                })
            })
        }) && saw_target;
        let owner_is_exclusive = thread.creator_id.as_deref() == Some(creator_id)
            && match thread.contact_route_id.as_deref() {
                None => true,
                Some(id) => id == route_id,
            };

        if !owner_is_exclusive || !participants_are_exclusive {
            plan.shared_threads += 1;
            continue;
        }

        plan.thread_fingerprints.push(format!(
            "{}|{}|{}|{}",
            thread.id,
            thread.creator_id.as_deref().unwrap_or_default(),
            thread.contact_route_id.as_deref().unwrap_or_default(),
            thread.updated_at
        ));
        let email_rows = fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', id, delivery_status, occurred_at)
             FROM emails WHERE thread_id = ?1 ORDER BY id",
            &[&thread.id],
        )?;
        plan.email_ids.extend(string_rows(
            connection,
            "SELECT id FROM emails WHERE thread_id = ?1 ORDER BY id",
            &[&thread.id],
        )?);
        plan.email_fingerprints.extend(email_rows);
        plan.recipient_fingerprints.extend(fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s|%s', recipient.email_id, recipient.recipient_kind,
                    recipient.address, COALESCE(recipient.locator_id,''))
             FROM email_recipients recipient
             JOIN emails email ON email.id = recipient.email_id
             WHERE email.thread_id = ?1
             ORDER BY recipient.email_id, recipient.recipient_kind, recipient.address",
            &[&thread.id],
        )?);
        plan.thread_ids.push(thread.id);
    }
    Ok(plan)
}

fn count_reply_drafts_outside_route(
    connection: &Connection,
    email: &EmailDeletionPlan,
    route_id: &str,
) -> Result<u64, String> {
    email.email_ids.iter().try_fold(0_u64, |total, email_id| {
        count(
            connection,
            "SELECT COUNT(*) FROM email_drafts
             WHERE reply_to_email_id = ?1 AND contact_route_id IS NOT ?2",
            &[email_id, &route_id],
        )
        .map(|count| total + count)
    })
}

fn count_unowned_drafts_for_route(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
) -> Result<u64, String> {
    if route.locator.channel != ContactChannel::Email {
        return Ok(0);
    }
    let target = EmailAddress::parse(&route.locator.normalized_value)
        .map_err(|error| format!("stored email contact route is invalid: {error}"))?;
    let mut statement = connection
        .prepare(
            "SELECT to_address FROM email_drafts
             WHERE contact_route_id IS NULL ORDER BY id",
        )
        .map_err(|error| error.to_string())?;
    let addresses = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(addresses
        .iter()
        .filter(|address| EmailAddress::parse(address).is_ok_and(|address| address == target))
        .count() as u64)
}

fn load_blocking_artifacts(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
    email: &EmailDeletionPlan,
) -> Result<Vec<CreatorContactRouteDeleteBlockingArtifact>, String> {
    let mut artifacts = Vec::new();
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::InFlightDirectMessage,
        super::super::outreach_actions::guards::route_pending_count(connection, &route.route.id)?,
    );
    let route_id = route.route.id.as_str();
    let locator_id = route.locator.id.as_str();
    let creator_id = route.route.creator_id.as_str();

    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedContactLocator,
        count(
            connection,
            "SELECT COUNT(*) FROM locator_observations observation
             WHERE observation.locator_id = ?1
               AND ((observation.creator_id IS NOT NULL AND observation.creator_id != ?2)
                 OR (observation.platform_account_id IS NOT NULL AND (
                       NOT EXISTS (
                         SELECT 1 FROM creator_account_links owner
                         WHERE owner.platform_account_id = observation.platform_account_id
                           AND owner.creator_id = ?2
                       )
                       OR EXISTS (
                         SELECT 1 FROM creator_account_links other_owner
                         WHERE other_owner.platform_account_id = observation.platform_account_id
                           AND other_owner.creator_id != ?2
                       )
                 )))",
            &[&locator_id, &creator_id],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedContactLocator,
        count(
            connection,
            "SELECT COUNT(*) FROM identity_conflicts
             WHERE locator_id = ?1
               AND ((creator_id IS NULL OR creator_id != ?2)
                 OR (platform_account_id IS NOT NULL AND (
                       NOT EXISTS (
                         SELECT 1 FROM creator_account_links owner
                         WHERE owner.platform_account_id = identity_conflicts.platform_account_id
                           AND owner.creator_id = ?2
                       )
                       OR EXISTS (
                         SELECT 1 FROM creator_account_links other_owner
                         WHERE other_owner.platform_account_id = identity_conflicts.platform_account_id
                           AND other_owner.creator_id != ?2
                       )
                 )))",
            &[&locator_id, &creator_id],
        )?,
    );

    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::InFlightEmail,
        count(
            connection,
            "SELECT COUNT(*) FROM email_drafts
             WHERE contact_route_id = ?1
               AND (sending_idempotency_key IS NOT NULL OR send_reserved_at IS NOT NULL)",
            &[&route_id],
        )?,
    );
    let mut statement = connection
        .prepare(
            "SELECT action, COUNT(*) FROM idempotency_records
             WHERE contact_route_id = ?1
               AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
             GROUP BY action ORDER BY action",
        )
        .map_err(|error| error.to_string())?;
    let inflight = statement
        .query_map([route_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    for (action, count) in inflight {
        let kind = if action.contains("email") {
            CreatorContactRouteDeleteBlockingArtifactKind::InFlightEmail
        } else if action.contains("relationship") {
            CreatorContactRouteDeleteBlockingArtifactKind::InFlightRelationshipMutation
        } else {
            CreatorContactRouteDeleteBlockingArtifactKind::InFlightDirectMessage
        };
        push_artifact(&mut artifacts, kind, count);
    }

    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory,
        email.shared_threads,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory,
        count_reply_drafts_outside_route(connection, email, route_id)?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory,
        count(
            connection,
            "SELECT COUNT(*) FROM email_drafts draft
             WHERE draft.contact_route_id = ?1
               AND draft.creator_id IS NOT ?2",
            &[&route_id, &creator_id],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedEmailHistory,
        count_unowned_drafts_for_route(connection, route)?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousDirectMessageHistory,
        count(
            connection,
            "SELECT COUNT(DISTINCT conversation.id)
             FROM conversations conversation
             JOIN locator_resolutions resolution
               ON resolution.resolved_target_id = conversation.resolved_target_id
              AND resolution.account_ref_id = conversation.account_ref_id
             WHERE resolution.locator_id = ?1
               AND NOT EXISTS (
                 SELECT 1 FROM conversation_contact_routes owner
                 WHERE owner.conversation_id = conversation.id
               )",
            &[&locator_id],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedDirectMessageHistory,
        count(
            connection,
            "SELECT COUNT(*) FROM conversation_contact_routes owner
             WHERE owner.contact_route_id = ?1
               AND (SELECT COUNT(*) FROM conversation_contact_routes sibling
                    WHERE sibling.conversation_id = owner.conversation_id) > 1",
            &[&route_id],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::SharedContactLocator,
        count(
            connection,
            "SELECT CASE WHEN COUNT(*) > 1 THEN COUNT(*) ELSE 0 END
             FROM creator_contact_routes WHERE locator_id = ?1",
            &[&locator_id],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory,
        count(
            connection,
            "SELECT COUNT(*) FROM audit_events audit
             WHERE (audit.contact_route_id = ?1
                    AND ((audit.creator_id IS NULL OR audit.creator_id != ?3)
                      OR (audit.contact_locator_id IS NOT NULL
                          AND audit.contact_locator_id != ?2)
                      OR (audit.email_id IS NOT NULL AND NOT EXISTS (
                            SELECT 1 FROM emails audit_email
                            JOIN email_threads audit_thread
                              ON audit_thread.id = audit_email.thread_id
                            WHERE audit_email.id = audit.email_id
                              AND (audit_thread.contact_route_id = ?1
                                OR (audit_thread.contact_route_id IS NULL
                                    AND audit_thread.creator_id = ?3
                                    AND EXISTS (
                                      SELECT 1 FROM email_recipients audit_recipient
                                      WHERE audit_recipient.email_id = audit_email.id
                                        AND (audit_recipient.locator_id = ?2
                                          OR (audit_recipient.locator_id IS NULL
                                              AND lower(trim(audit_recipient.address)) = lower(trim(?4))))
                                    )))
                      ))))
                OR (audit.contact_locator_id = ?2
                    AND (audit.contact_route_id IS NULL OR audit.contact_route_id != ?1))
                OR (audit.email_id IN (
                      SELECT email.id FROM emails email
                      JOIN email_threads thread ON thread.id = email.thread_id
                      WHERE thread.contact_route_id = ?1
                         OR (thread.contact_route_id IS NULL AND thread.creator_id = ?3
                             AND EXISTS (
                               SELECT 1 FROM emails candidate_email
                               JOIN email_recipients recipient
                                 ON recipient.email_id = candidate_email.id
                               WHERE candidate_email.thread_id = thread.id
                                 AND (recipient.locator_id = ?2
                                   OR (recipient.locator_id IS NULL
                                       AND lower(trim(recipient.address)) = lower(trim(?4))))
                             ))
                    )
                    AND (audit.contact_route_id IS NULL OR audit.contact_route_id != ?1))",
            &[
                &route_id,
                &locator_id,
                &creator_id,
                &route.locator.normalized_value,
            ],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory,
        count(
            connection,
            "SELECT COUNT(*) FROM touchpoints
             WHERE contact_route_id = ?1
               AND (creator_id != ?2
                 OR (message_id IS NOT NULL AND NOT EXISTS (
                       SELECT 1 FROM messages touchpoint_message
                       JOIN conversation_contact_routes message_owner
                         ON message_owner.conversation_id = touchpoint_message.conversation_id
                       WHERE touchpoint_message.id = touchpoints.message_id
                         AND message_owner.contact_route_id = ?1
                 ))
                 OR (email_id IS NOT NULL AND NOT EXISTS (
                       SELECT 1 FROM emails touchpoint_email
                       JOIN email_threads touchpoint_thread
                         ON touchpoint_thread.id = touchpoint_email.thread_id
                       WHERE touchpoint_email.id = touchpoints.email_id
                         AND (touchpoint_thread.contact_route_id = ?1
                           OR (touchpoint_thread.contact_route_id IS NULL
                               AND touchpoint_thread.creator_id = ?2
                               AND EXISTS (
                                 SELECT 1 FROM email_recipients touchpoint_recipient
                                 WHERE touchpoint_recipient.email_id = touchpoint_email.id
                                   AND (touchpoint_recipient.locator_id = ?3
                                     OR (touchpoint_recipient.locator_id IS NULL
                                         AND lower(trim(touchpoint_recipient.address)) = lower(trim(?4))))
                               )))
                 ))
                 OR (account_relationship_id IS NOT NULL AND NOT EXISTS (
                       SELECT 1 FROM account_relationships touchpoint_relationship
                       JOIN resolved_targets relationship_target
                         ON relationship_target.platform_account_id =
                              touchpoint_relationship.target_platform_account_id
                        AND relationship_target.account_ref_id =
                              touchpoint_relationship.actor_account_ref_id
                       JOIN locator_resolutions relationship_resolution
                         ON relationship_resolution.resolved_target_id = relationship_target.id
                        AND relationship_resolution.account_ref_id =
                              touchpoint_relationship.actor_account_ref_id
                       WHERE touchpoint_relationship.id = touchpoints.account_relationship_id
                         AND relationship_resolution.locator_id = ?3
                 )))",
            &[
                &route_id,
                &creator_id,
                &locator_id,
                &route.locator.normalized_value,
            ],
        )?,
    );
    push_artifact(
        &mut artifacts,
        CreatorContactRouteDeleteBlockingArtifactKind::AmbiguousLocalAuditHistory,
        count(
            connection,
            "SELECT COUNT(DISTINCT touchpoint.id)
             FROM touchpoints touchpoint
             LEFT JOIN messages message ON message.id = touchpoint.message_id
             LEFT JOIN conversations conversation ON conversation.id = message.conversation_id
             LEFT JOIN locator_resolutions message_resolution
               ON message_resolution.resolved_target_id = conversation.resolved_target_id
              AND message_resolution.account_ref_id = conversation.account_ref_id
             LEFT JOIN emails email ON email.id = touchpoint.email_id
             LEFT JOIN email_recipients recipient ON recipient.email_id = email.id
             LEFT JOIN account_relationships relationship
               ON relationship.id = touchpoint.account_relationship_id
             LEFT JOIN resolved_targets relationship_target
               ON relationship_target.platform_account_id = relationship.target_platform_account_id
              AND relationship_target.account_ref_id = relationship.actor_account_ref_id
             LEFT JOIN locator_resolutions relationship_resolution
               ON relationship_resolution.resolved_target_id = relationship_target.id
              AND relationship_resolution.account_ref_id = relationship.actor_account_ref_id
             WHERE (touchpoint.contact_route_id IS NULL OR touchpoint.contact_route_id != ?2)
               AND (message_resolution.locator_id = ?1
                    OR recipient.locator_id = ?1
                    OR relationship_resolution.locator_id = ?1)",
            &[&locator_id, &route_id],
        )?,
    );
    Ok(artifacts)
}

fn load_impact(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
    email: &EmailDeletionPlan,
) -> Result<CreatorContactRouteDeleteImpact, String> {
    let route_id = route.route.id.as_str();
    let locator_id = route.locator.id.as_str();
    let mut impact = CreatorContactRouteDeleteImpact {
        contact_routes: 1,
        dependencies: count(
            connection,
            "SELECT COUNT(*) FROM creator_contact_route_dependencies
             WHERE child_contact_route_id = ?1",
            &[&route_id],
        )?,
        email_threads: email.thread_ids.len() as u64,
        email_messages: email.email_ids.len() as u64,
        email_recipients: email.recipient_fingerprints.len() as u64,
        email_drafts: count(
            connection,
            "SELECT COUNT(*) FROM email_drafts WHERE contact_route_id = ?1",
            &[&route_id],
        )?,
        direct_message_conversations: count(
            connection,
            "SELECT COUNT(*) FROM conversation_contact_routes WHERE contact_route_id = ?1",
            &[&route_id],
        )?,
        direct_messages: count(
            connection,
            "SELECT COUNT(*) FROM messages WHERE conversation_id IN (
               SELECT conversation_id FROM conversation_contact_routes WHERE contact_route_id = ?1
             )",
            &[&route_id],
        )?,
        outreach_touchpoints: count(
            connection,
            "SELECT COUNT(*) FROM touchpoints WHERE contact_route_id = ?1",
            &[&route_id],
        )?,
        audit_events: count(
            connection,
            "SELECT COUNT(*) FROM audit_events WHERE contact_route_id = ?1",
            &[&route_id],
        )?,
        idempotency_records: count(
            connection,
            "SELECT COUNT(*) FROM idempotency_records WHERE contact_route_id = ?1",
            &[&route_id],
        )?,
        target_resolutions: count(
            connection,
            "SELECT COUNT(*) FROM locator_resolutions WHERE locator_id = ?1",
            &[&locator_id],
        )?,
        identity_conflicts: count(
            connection,
            "SELECT COUNT(*) FROM identity_conflicts WHERE locator_id = ?1",
            &[&locator_id],
        )?,
        observations: count(
            connection,
            "SELECT COUNT(*) FROM locator_observations WHERE locator_id = ?1",
            &[&locator_id],
        )?,
        contact_locators: orphan_locator_count(connection, route, email)?,
        observation_sources: orphan_source_count(connection, locator_id)?,
        resolved_targets: orphan_target_count(connection, route)?,
        creator_origins_detached: count(
            connection,
            "SELECT COUNT(*) FROM creator_origins WHERE source_contact_route_id = ?1",
            &[&route_id],
        )?,
        local_records_total: 0,
    };
    impact.local_records_total = impact.contact_routes
        + impact.dependencies
        + impact.email_threads
        + impact.email_messages
        + impact.email_recipients
        + impact.email_drafts
        + impact.direct_message_conversations
        + impact.direct_messages
        + impact.outreach_touchpoints
        + impact.audit_events
        + impact.idempotency_records
        + impact.target_resolutions
        + impact.identity_conflicts
        + impact.observations
        + impact.contact_locators
        + impact.observation_sources
        + impact.resolved_targets
        + impact.creator_origins_detached;
    Ok(impact)
}

fn build_fingerprint(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
    impact: &CreatorContactRouteDeleteImpact,
    email: &EmailDeletionPlan,
) -> Result<String, String> {
    let route_id = route.route.id.as_str();
    let locator_id = route.locator.id.as_str();
    let route_param: &[&dyn ToSql] = &[&route_id];
    let locator_param: &[&dyn ToSql] = &[&locator_id];
    serde_json::to_string(&DeletePlanFingerprint {
        creator_id: route.route.creator_id.as_str(),
        contact_route_id: route_id,
        locator_id,
        route_updated_at: route.route.updated_at,
        locator_updated_at: route.locator.updated_at,
        impact,
        observations: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s|%s', observation.id, observation.source_id,
                    observation.observed_at, source.observed_at)
             FROM locator_observations observation
             JOIN observation_sources source ON source.id = observation.source_id
             WHERE observation.locator_id = ?1 ORDER BY observation.id",
            locator_param,
        )?,
        dependencies: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', parent_contact_route_id, child_contact_route_id, kind)
             FROM creator_contact_route_dependencies
             WHERE parent_contact_route_id = ?1 OR child_contact_route_id = ?1
             ORDER BY parent_contact_route_id, child_contact_route_id",
            route_param,
        )?,
        email_threads: email.thread_fingerprints.clone(),
        emails: email.email_fingerprints.clone(),
        email_recipients: email.recipient_fingerprints.clone(),
        email_drafts: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s|%s', id, revision, updated_at,
                    COALESCE(sending_idempotency_key,''))
             FROM email_drafts WHERE contact_route_id = ?1 ORDER BY id",
            route_param,
        )?,
        conversations: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s', conversation.id, conversation.updated_at)
             FROM conversations conversation
             JOIN conversation_contact_routes owner ON owner.conversation_id = conversation.id
             WHERE owner.contact_route_id = ?1 ORDER BY conversation.id",
            route_param,
        )?,
        messages: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', message.id, message.status, message.sent_at)
             FROM messages message JOIN conversation_contact_routes owner
               ON owner.conversation_id = message.conversation_id
             WHERE owner.contact_route_id = ?1 ORDER BY message.id",
            route_param,
        )?,
        touchpoints: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s', id, occurred_at) FROM touchpoints
             WHERE contact_route_id = ?1 ORDER BY id",
            route_param,
        )?,
        audit_events: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s', id, created_at) FROM audit_events
             WHERE contact_route_id = ?1 ORDER BY id",
            route_param,
        )?,
        idempotency_records: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', key, state, updated_at) FROM idempotency_records
             WHERE contact_route_id = ?1 ORDER BY key",
            route_param,
        )?,
        resolutions: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s|%s', id, state, resolved_at,
                    COALESCE(invalidated_at,''))
             FROM locator_resolutions WHERE locator_id = ?1 ORDER BY id",
            locator_param,
        )?,
        identity_conflicts: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', id, state, created_at) FROM identity_conflicts
             WHERE locator_id = ?1 ORDER BY id",
            locator_param,
        )?,
        observation_sources: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s|%s', source.id, source.kind, source.observed_at)
             FROM observation_sources source
             WHERE EXISTS (
               SELECT 1 FROM locator_observations observation
               WHERE observation.source_id = source.id AND observation.locator_id = ?1
             )
             ORDER BY source.id",
            locator_param,
        )?,
        resolved_targets: fingerprint_rows(
            connection,
            "SELECT DISTINCT printf('%s|%s', target.id, target.updated_at)
             FROM resolved_targets target
             JOIN locator_resolutions resolution ON resolution.resolved_target_id = target.id
             WHERE resolution.locator_id = ?1 ORDER BY target.id",
            locator_param,
        )?,
        creator_origins: fingerprint_rows(
            connection,
            "SELECT printf('%s|%s', id, discovered_at) FROM creator_origins
             WHERE source_contact_route_id = ?1 ORDER BY id",
            route_param,
        )?,
    })
    .map_err(|error| error.to_string())
}

fn orphan_locator_count(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
    email: &EmailDeletionPlan,
) -> Result<u64, String> {
    let sibling_routes = count(
        connection,
        "SELECT COUNT(*) FROM creator_contact_routes
         WHERE locator_id = ?1 AND id != ?2",
        &[&route.locator.id.as_str(), &route.route.id.as_str()],
    )?;
    if sibling_routes != 0 {
        return Ok(0);
    }
    let recipient_threads = string_rows(
        connection,
        "SELECT DISTINCT email.thread_id
         FROM email_recipients recipient
         JOIN emails email ON email.id = recipient.email_id
         WHERE recipient.locator_id = ?1 ORDER BY email.thread_id",
        &[&route.locator.id.as_str()],
    )?;
    if recipient_threads
        .iter()
        .any(|thread_id| !email.thread_ids.contains(thread_id))
    {
        return Ok(0);
    }
    let shared_audits = count(
        connection,
        "SELECT COUNT(*) FROM audit_events
         WHERE contact_locator_id = ?1 AND contact_route_id IS NOT ?2",
        &[&route.locator.id.as_str(), &route.route.id.as_str()],
    )?;
    Ok(u64::from(shared_audits == 0))
}

fn orphan_source_count(connection: &Connection, locator_id: &str) -> Result<u64, String> {
    count(
        connection,
        "SELECT COUNT(*) FROM observation_sources source
         WHERE EXISTS (
           SELECT 1 FROM locator_observations owned
           WHERE owned.source_id = source.id AND owned.locator_id = ?1
         )
           AND NOT EXISTS (
             SELECT 1 FROM locator_observations sibling
             WHERE sibling.source_id = source.id AND sibling.locator_id != ?1
           )
           AND NOT EXISTS (
             SELECT 1 FROM platform_account_observations platform_observation
             WHERE platform_observation.source_id = source.id
           )
           AND NOT EXISTS (
             SELECT 1 FROM creator_origins origin WHERE origin.source_id = source.id
           )",
        &[&locator_id],
    )
}

fn orphan_target_count(
    connection: &Connection,
    route: &CreatorContactRouteRecord,
) -> Result<u64, String> {
    let targets = string_rows(
        connection,
        "SELECT DISTINCT resolved_target_id FROM locator_resolutions
         WHERE locator_id = ?1 ORDER BY resolved_target_id",
        &[&route.locator.id.as_str()],
    )?;
    let mut orphaned = 0;
    for target_id in targets {
        let will_be_orphaned: bool = connection
            .query_row(
                "SELECT
                   NOT EXISTS (
                     SELECT 1 FROM locator_resolutions sibling
                     WHERE sibling.resolved_target_id = ?1 AND sibling.locator_id != ?2
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM conversations conversation
                     WHERE conversation.resolved_target_id = ?1
                       AND NOT EXISTS (
                         SELECT 1 FROM conversation_contact_routes owner
                         WHERE owner.conversation_id = conversation.id
                           AND owner.contact_route_id = ?3
                       )
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM messages message
                     WHERE message.sender_target_id = ?1
                       AND NOT EXISTS (
                         SELECT 1 FROM conversation_contact_routes owner
                         WHERE owner.conversation_id = message.conversation_id
                           AND owner.contact_route_id = ?3
                       )
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM identity_conflicts conflict
                     WHERE conflict.resolved_target_id = ?1
                       AND (conflict.locator_id IS NULL OR conflict.locator_id != ?2)
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM audit_events audit
                     WHERE audit.resolved_target_id = ?1
                       AND (audit.contact_route_id IS NULL OR audit.contact_route_id != ?3)
                   )
                   AND NOT EXISTS (
                     SELECT 1 FROM creator_origins origin
                     WHERE origin.source_resolved_target_id = ?1
                   )
",
                params![
                    target_id,
                    route.locator.id.as_str(),
                    route.route.id.as_str(),
                ],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        orphaned += u64::from(will_be_orphaned);
    }
    Ok(orphaned)
}

fn count(connection: &Connection, sql: &str, params: &[&dyn ToSql]) -> Result<u64, String> {
    connection
        .query_row(sql, params, |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn fingerprint_rows(
    connection: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<Vec<String>, String> {
    string_rows(connection, sql, params)
}

fn push_artifact(
    artifacts: &mut Vec<CreatorContactRouteDeleteBlockingArtifact>,
    kind: CreatorContactRouteDeleteBlockingArtifactKind,
    count: u64,
) {
    if count == 0 {
        return;
    }
    if let Some(existing) = artifacts.iter_mut().find(|artifact| artifact.kind == kind) {
        existing.count += count;
    } else {
        artifacts.push(CreatorContactRouteDeleteBlockingArtifact { kind, count });
    }
}

fn parse_dependency_kind(value: &str) -> rusqlite::Result<CreatorContactRouteDependencyKind> {
    match value {
        "link_hub" => Ok(CreatorContactRouteDependencyKind::LinkHub),
        "community_administrator" => Ok(CreatorContactRouteDependencyKind::CommunityAdministrator),
        other => Err(to_sql_error(format!(
            "invalid contact route dependency kind: {other}"
        ))),
    }
}
