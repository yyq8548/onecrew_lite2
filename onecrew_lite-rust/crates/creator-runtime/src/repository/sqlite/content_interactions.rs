use rusqlite::params;
use uuid::Uuid;

use crate::{
    AccountRefId, AuditEvent, AuditEventId, CommitContentReplyActionResult, CreatorId, Touchpoint,
    TouchpointId, repository::ContentRepository,
};

use super::{SqliteRepository, json, now};

impl ContentRepository for SqliteRepository {
    fn record_creator_content_reply(
        &self,
        key: &str,
    ) -> Result<CommitContentReplyActionResult, crate::CreatorOutreachError> {
        let mut connection = self
            .lock()
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let state = super::outreach_actions::ledger::require_verified(
            &transaction,
            key,
            crate::CreatorOutreachOperation::ContentReplySend,
        )?;
        if state.phase == crate::CreatorOutreachPhase::Completed {
            return match super::outreach_actions::record::record_in(&transaction, &state)? {
                crate::CreatorOutreachRecord::ContentReply(record) => Ok(record),
                _ => Err(crate::CreatorOutreachError::ScopeMismatch),
            };
        }
        super::outreach_actions::ledger::validate_ownership_in(
            &transaction,
            &state.identity,
            false,
        )?;
        let input = super::outreach_actions::record::content_input(&state)?;
        if let Some(record) =
            super::outreach_actions::record::duplicate_content(&transaction, &state)?
        {
            let ids = crate::CreatorOutreachRecordIds::ContentReply {
                external_reply_id: record.external_reply_id.clone(),
                touchpoint_id: record.touchpoint.id.clone(),
                audit_event_id: record.audit_event.id.clone(),
            };
            super::outreach_actions::ledger::complete_in(&transaction, key, state, ids)?;
            transaction
                .commit()
                .map_err(|_| crate::CreatorOutreachError::CommitUnknown)?;
            return Ok(record);
        }
        let result = (|| -> Result<CommitContentReplyActionResult, String> {
            let timestamp = now();
            let metadata = serde_json::json!({
                "content_id": input.content_id,
                "reply_to_id": input.reply_to_id,
                "external_reply_id": input.verified_external_reply_id,
                "target_platform_account_id": input.target_platform_account_id,
                "verified": true,
                    "intent_hash": state.fingerprint,
            });
            let touchpoint = Touchpoint {
                id: TouchpointId::new(Uuid::new_v4().to_string()),
                creator_id: input.creator_id.clone(),
                kind: "content_reply".into(),
                message_id: None,
                email_id: None,
                account_relationship_id: None,
                occurred_at: timestamp,
                metadata: metadata.clone(),
            };
            transaction
                .execute(
                    "INSERT INTO touchpoints
                   (id, creator_id, kind, message_id, email_id,
                    account_relationship_id, occurred_at, metadata_json)
                 VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, ?5)",
                    params![
                        touchpoint.id.as_str(),
                        touchpoint.creator_id.as_str(),
                        touchpoint.kind,
                        touchpoint.occurred_at,
                        json(&touchpoint.metadata)?,
                    ],
                )
                .map_err(|error| error.to_string())?;
            let audit_event = AuditEvent {
                id: AuditEventId::new(Uuid::new_v4().to_string()),
                kind: "content_reply_sent".into(),
                creator_id: Some(input.creator_id),
                account_ref_id: Some(input.operator_account_ref_id),
                resolved_target_id: None,
                mailbox_ref_id: None,
                contact_locator_id: None,
                email_id: None,
                payload: metadata,
                created_at: timestamp,
            };
            transaction
                .execute(
                    "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id,
                    mailbox_ref_id, contact_locator_id, email_id,
                    payload_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, NULL, NULL, ?5, ?6)",
                    params![
                        audit_event.id.as_str(),
                        audit_event.kind,
                        audit_event.creator_id.as_ref().map(CreatorId::as_str),
                        audit_event
                            .account_ref_id
                            .as_ref()
                            .map(AccountRefId::as_str),
                        json(&audit_event.payload)?,
                        audit_event.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
            let result = CommitContentReplyActionResult {
                external_reply_id: input.verified_external_reply_id,
                touchpoint,
                audit_event,
            };
            Ok(result)
        })()
        .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let ids = crate::CreatorOutreachRecordIds::ContentReply {
            external_reply_id: result.external_reply_id.clone(),
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
