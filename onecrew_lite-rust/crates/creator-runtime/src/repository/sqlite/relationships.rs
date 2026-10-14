use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

use crate::{
    AccountRefId, AccountRelationship, AccountRelationshipId, AuditEvent, AuditEventId,
    CommitRelationshipActionResult, PlatformAccountId, RelationshipKind, RelationshipValue,
    Touchpoint, TouchpointId, repository::RelationshipRepository,
};

use super::{SqliteRepository, json, now};

impl RelationshipRepository for SqliteRepository {
    fn get_account_relationship(
        &self,
        actor_account_ref_id: &AccountRefId,
        target_platform_account_id: &PlatformAccountId,
        kind: RelationshipKind,
    ) -> Result<Option<AccountRelationship>, String> {
        let connection = self.read_connection()?;
        load_relationship(
            &connection,
            actor_account_ref_id,
            target_platform_account_id,
            kind,
        )
    }

    fn upsert_account_relationship(
        &self,
        actor_account_ref_id: &AccountRefId,
        target_platform_account_id: &PlatformAccountId,
        kind: RelationshipKind,
        value: RelationshipValue,
        checked_at: u64,
    ) -> Result<AccountRelationship, String> {
        let connection = self.lock()?;
        let timestamp = now();
        let existing_id = connection
            .query_row(
                "SELECT id FROM account_relationships
                 WHERE actor_account_ref_id = ?1 AND target_platform_account_id = ?2
                   AND relationship_kind = ?3",
                params![
                    actor_account_ref_id.as_str(),
                    target_platform_account_id.as_str(),
                    kind.as_str()
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let id =
            AccountRelationshipId::new(existing_id.unwrap_or_else(|| Uuid::new_v4().to_string()));
        connection
            .execute(
                "INSERT INTO account_relationships
                   (id, actor_account_ref_id, target_platform_account_id,
                    relationship_kind, relationship_value, checked_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(actor_account_ref_id, target_platform_account_id, relationship_kind)
                 DO UPDATE SET relationship_value = excluded.relationship_value,
                   checked_at = excluded.checked_at, updated_at = excluded.updated_at",
                params![
                    id.as_str(),
                    actor_account_ref_id.as_str(),
                    target_platform_account_id.as_str(),
                    kind.as_str(),
                    value.as_str(),
                    checked_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(AccountRelationship {
            id,
            actor_account_ref_id: actor_account_ref_id.clone(),
            target_platform_account_id: target_platform_account_id.clone(),
            kind,
            value,
            checked_at,
            updated_at: timestamp,
        })
    }

    fn record_creator_relationship(
        &self,
        key: &str,
    ) -> Result<CommitRelationshipActionResult, crate::CreatorOutreachError> {
        let mut connection = self
            .lock()
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let state = super::outreach_actions::ledger::require_verified(
            &transaction,
            key,
            crate::CreatorOutreachOperation::RelationshipUpdate,
        )?;
        if state.phase == crate::CreatorOutreachPhase::Completed {
            return match super::outreach_actions::record::record_in(&transaction, &state)? {
                crate::CreatorOutreachRecord::Relationship(record) => Ok(record),
                _ => Err(crate::CreatorOutreachError::ScopeMismatch),
            };
        }
        super::outreach_actions::ledger::validate_ownership_in(
            &transaction,
            &state.identity,
            false,
        )?;
        let input = super::outreach_actions::record::relationship_input(&state)?;
        let result = (|| -> Result<CommitRelationshipActionResult, String> {
            let timestamp = now();
            let existing_id = transaction
                .query_row(
                    "SELECT id FROM account_relationships
                 WHERE actor_account_ref_id = ?1 AND target_platform_account_id = ?2
                   AND relationship_kind = ?3",
                    params![
                        input.actor_account_ref_id.as_str(),
                        input.target_platform_account_id.as_str(),
                        input.kind.as_str()
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let relationship_id = AccountRelationshipId::new(
                existing_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            );
            transaction
                .execute(
                    "INSERT INTO account_relationships
                   (id, actor_account_ref_id, target_platform_account_id,
                    relationship_kind, relationship_value, checked_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(actor_account_ref_id, target_platform_account_id, relationship_kind)
                 DO UPDATE SET relationship_value = excluded.relationship_value,
                   checked_at = excluded.checked_at, updated_at = excluded.updated_at",
                    params![
                        relationship_id.as_str(),
                        input.actor_account_ref_id.as_str(),
                        input.target_platform_account_id.as_str(),
                        input.kind.as_str(),
                        input.value.as_str(),
                        input.checked_at,
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
            let relationship = AccountRelationship {
                id: relationship_id.clone(),
                actor_account_ref_id: input.actor_account_ref_id.clone(),
                target_platform_account_id: input.target_platform_account_id.clone(),
                kind: input.kind,
                value: input.value,
                checked_at: input.checked_at,
                updated_at: timestamp,
            };
            let can_message_existing_id = transaction
                .query_row(
                    "SELECT id FROM account_relationships
                 WHERE actor_account_ref_id = ?1 AND target_platform_account_id = ?2
                   AND relationship_kind = 'can_message'",
                    params![
                        input.actor_account_ref_id.as_str(),
                        input.target_platform_account_id.as_str()
                    ],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let can_message_id = AccountRelationshipId::new(
                can_message_existing_id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            );
            transaction
                .execute(
                    "INSERT INTO account_relationships
                   (id, actor_account_ref_id, target_platform_account_id,
                    relationship_kind, relationship_value, checked_at, updated_at)
                 VALUES (?1, ?2, ?3, 'can_message', ?4, ?5, ?6)
                 ON CONFLICT(actor_account_ref_id, target_platform_account_id, relationship_kind)
                 DO UPDATE SET relationship_value = excluded.relationship_value,
                   checked_at = excluded.checked_at, updated_at = excluded.updated_at",
                    params![
                        can_message_id.as_str(),
                        input.actor_account_ref_id.as_str(),
                        input.target_platform_account_id.as_str(),
                        input.can_message_value.as_str(),
                        input.checked_at,
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
            let can_message = AccountRelationship {
                id: can_message_id,
                actor_account_ref_id: input.actor_account_ref_id.clone(),
                target_platform_account_id: input.target_platform_account_id.clone(),
                kind: RelationshipKind::CanMessage,
                value: input.can_message_value,
                checked_at: input.checked_at,
                updated_at: timestamp,
            };
            let touchpoint = Touchpoint {
                id: TouchpointId::new(Uuid::new_v4().to_string()),
                creator_id: input.creator_id.clone(),
                kind: input.action.as_str().to_owned(),
                message_id: None,
                email_id: None,
                account_relationship_id: Some(relationship_id),
                occurred_at: timestamp,
                metadata: serde_json::json!({ "verified": true }),
            };
            transaction
                .execute(
                    "INSERT INTO touchpoints
                   (id, creator_id, kind, message_id, email_id, account_relationship_id,
                    contact_route_id, occurred_at, metadata_json)
                 VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, ?6, ?7)",
                    params![
                        touchpoint.id.as_str(),
                        touchpoint.creator_id.as_str(),
                        touchpoint.kind,
                        touchpoint
                            .account_relationship_id
                            .as_ref()
                            .map(AccountRelationshipId::as_str),
                        input.contact_route_id.as_str(),
                        touchpoint.occurred_at,
                        json(&touchpoint.metadata)?
                    ],
                )
                .map_err(|error| error.to_string())?;
            let audit_event = AuditEvent {
                id: AuditEventId::new(Uuid::new_v4().to_string()),
                kind: "relationship_updated".into(),
                creator_id: Some(input.creator_id.clone()),
                account_ref_id: Some(input.actor_account_ref_id.clone()),
                resolved_target_id: Some(input.resolved_target_id.clone()),
                mailbox_ref_id: None,
                contact_locator_id: None,
                email_id: None,
                payload: serde_json::json!({
                    "action": input.action,
                    "relationship_kind": input.kind,
                    "relationship_value": input.value,
                    "verified": true,
                    "intent_hash": state.fingerprint,
                }),
                created_at: timestamp,
            };
            transaction
                .execute(
                    "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id,
                    contact_route_id, payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        audit_event.id.as_str(),
                        audit_event.kind,
                        audit_event
                            .creator_id
                            .as_ref()
                            .map(crate::CreatorId::as_str),
                        audit_event
                            .account_ref_id
                            .as_ref()
                            .map(AccountRefId::as_str),
                        audit_event
                            .resolved_target_id
                            .as_ref()
                            .map(crate::ResolvedTargetId::as_str),
                        input.contact_route_id.as_str(),
                        json(&audit_event.payload)?,
                        audit_event.created_at
                    ],
                )
                .map_err(|error| error.to_string())?;
            let result = CommitRelationshipActionResult {
                relationship,
                can_message,
                touchpoint,
                audit_event,
            };
            Ok(result)
        })()
        .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let ids = crate::CreatorOutreachRecordIds::Relationship {
            relationship_id: result.relationship.id.clone(),
            can_message_id: result.can_message.id.clone(),
            touchpoint_id: result.touchpoint.id.clone(),
            audit_event_id: result.audit_event.id.clone(),
        };
        super::outreach_actions::ledger::complete_in(&transaction, key, state, ids)?;
        transaction
            .commit()
            .map_err(|_| crate::CreatorOutreachError::CommitUnknown)?;
        Ok(result)
    }
}

fn load_relationship(
    connection: &rusqlite::Connection,
    actor_account_ref_id: &AccountRefId,
    target_platform_account_id: &PlatformAccountId,
    kind: RelationshipKind,
) -> Result<Option<AccountRelationship>, String> {
    connection
        .query_row(
            "SELECT id, relationship_value, checked_at, updated_at
             FROM account_relationships
             WHERE actor_account_ref_id = ?1 AND target_platform_account_id = ?2
               AND relationship_kind = ?3",
            params![
                actor_account_ref_id.as_str(),
                target_platform_account_id.as_str(),
                kind.as_str()
            ],
            |row| {
                let value: String = row.get(1)?;
                Ok(AccountRelationship {
                    id: AccountRelationshipId::new(row.get::<_, String>(0)?),
                    actor_account_ref_id: actor_account_ref_id.clone(),
                    target_platform_account_id: target_platform_account_id.clone(),
                    kind,
                    value: RelationshipValue::parse(&value)
                        .map_err(super::creators::to_sql_conversion_error)?,
                    checked_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}
