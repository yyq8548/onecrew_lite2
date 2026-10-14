//! Reconstruct records from stable IDs and validate facts before atomic commit.
//! Recovery envelopes contain neither message bodies nor full record snapshots.

use super::super::SqliteRepository;
use super::ledger;
use crate::*;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

pub(in super::super) struct RelationshipRecordFacts {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub actor_account_ref_id: AccountRefId,
    pub resolved_target_id: ResolvedTargetId,
    pub target_platform_account_id: PlatformAccountId,
    pub kind: RelationshipKind,
    pub value: RelationshipValue,
    pub can_message_value: RelationshipValue,
    pub checked_at: u64,
    pub action: RelationshipMutation,
}

pub(in super::super) struct MessageRecordFacts {
    pub snapshot: PersistConversationSnapshotInput,
    pub verified_external_message_id: String,
}

pub(in super::super) struct ContentReplyRecordFacts {
    pub creator_id: CreatorId,
    pub operator_account_ref_id: AccountRefId,
    pub target_platform_account_id: PlatformAccountId,
    pub content_id: String,
    pub reply_to_id: Option<String>,
    pub verified_external_reply_id: String,
}

pub(in super::super) fn read_recorded(
    repository: &SqliteRepository,
    key: &str,
) -> Result<CreatorOutreachRecord, CreatorOutreachError> {
    let mut connection = repository.read_connection().map_err(storage)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(storage)?;
    let state =
        ledger::load_in(&transaction, key)?.ok_or(CreatorOutreachError::ActionUnavailable)?;
    let result = record_in(&transaction, &state)?;
    transaction.commit().map_err(storage)?;
    Ok(result)
}

pub(in super::super) fn record_in(
    connection: &Connection,
    state: &CreatorOutreachActionState,
) -> Result<CreatorOutreachRecord, CreatorOutreachError> {
    if state.phase != CreatorOutreachPhase::Completed {
        return Err(CreatorOutreachError::ActionUnavailable);
    }
    let record = state.record.as_ref().ok_or(CreatorOutreachError::Storage)?;
    let result: Result<CreatorOutreachRecord, CreatorOutreachError> = match record {
        CreatorOutreachRecordIds::Relationship {
            relationship_id,
            can_message_id,
            touchpoint_id,
            audit_event_id,
        } => {
            let CreatorOutreachVerification::Relationship {
                following,
                can_message,
                checked_at,
            } = state
                .verification
                .as_ref()
                .ok_or(CreatorOutreachError::Storage)?
            else {
                return Err(CreatorOutreachError::Storage);
            };
            let touchpoint = load_touchpoint(connection, touchpoint_id)?;
            let audit_event = load_audit(connection, audit_event_id)?;
            let relationship = AccountRelationship {
                id: relationship_id.clone(),
                actor_account_ref_id: state.identity.operator_account_ref_id.clone(),
                target_platform_account_id: state.identity.target_platform_account_id.clone(),
                kind: RelationshipKind::Following,
                value: *following,
                checked_at: *checked_at,
                updated_at: touchpoint.occurred_at,
            };
            let can_message = AccountRelationship {
                id: can_message_id.clone(),
                actor_account_ref_id: state.identity.operator_account_ref_id.clone(),
                target_platform_account_id: state.identity.target_platform_account_id.clone(),
                kind: RelationshipKind::CanMessage,
                value: *can_message,
                checked_at: *checked_at,
                updated_at: touchpoint.occurred_at,
            };
            Ok(CreatorOutreachRecord::Relationship(
                CommitRelationshipActionResult {
                    relationship,
                    can_message,
                    touchpoint,
                    audit_event,
                },
            ))
        }
        CreatorOutreachRecordIds::ContentReply {
            external_reply_id,
            touchpoint_id,
            audit_event_id,
        } => Ok(CreatorOutreachRecord::ContentReply(
            CommitContentReplyActionResult {
                external_reply_id: external_reply_id.clone(),
                touchpoint: load_touchpoint(connection, touchpoint_id)?,
                audit_event: load_audit(connection, audit_event_id)?,
            },
        )),
        CreatorOutreachRecordIds::DirectMessage {
            conversation_id,
            message_id,
            touchpoint_id,
            audit_event_id,
        } => Ok(CreatorOutreachRecord::DirectMessage(
            CommitMessageActionResult {
                conversation: load_conversation(connection, conversation_id)?,
                message: load_message(connection, message_id)?,
                touchpoint: load_touchpoint(connection, touchpoint_id)?,
                audit_event: load_audit(connection, audit_event_id)?,
            },
        )),
    };
    let result = result?;
    validate_record(connection, state, &result)?;
    Ok(result)
}

fn validate_record(
    connection: &Connection,
    state: &CreatorOutreachActionState,
    record: &CreatorOutreachRecord,
) -> Result<(), CreatorOutreachError> {
    let (touchpoint, audit, expected_point, expected_audit) = match record {
        CreatorOutreachRecord::Relationship(record) => {
            if state.operation != CreatorOutreachOperation::RelationshipUpdate
                || record.touchpoint.account_relationship_id.as_ref()
                    != Some(&record.relationship.id)
            {
                return Err(CreatorOutreachError::RecordUnavailable);
            }
            // Values may have been refreshed since completion. The row's stable
            // ownership must still match; the action's verified values remain
            // in its immutable receipt.
            for relationship in [&record.relationship, &record.can_message] {
                let owned = connection.query_row("SELECT EXISTS(SELECT 1 FROM account_relationships WHERE id = ?1 AND actor_account_ref_id = ?2 AND target_platform_account_id = ?3 AND relationship_kind = ?4)", params![relationship.id.as_str(), state.identity.operator_account_ref_id.as_str(), state.identity.target_platform_account_id.as_str(), relationship.kind.as_str()], |row| row.get::<_, bool>(0)).map_err(storage)?;
                if !owned {
                    return Err(CreatorOutreachError::RecordUnavailable);
                }
            }
            (
                &record.touchpoint,
                &record.audit_event,
                state
                    .identity
                    .relationship_action
                    .ok_or(CreatorOutreachError::RecordUnavailable)?
                    .as_str(),
                "relationship_updated",
            )
        }
        CreatorOutreachRecord::ContentReply(record) => {
            if !matches!(&state.verification, Some(CreatorOutreachVerification::ContentReply { external_reply_id }) if external_reply_id == &record.external_reply_id)
                || record
                    .touchpoint
                    .metadata
                    .get("external_reply_id")
                    .and_then(serde_json::Value::as_str)
                    != Some(record.external_reply_id.as_str())
                || record
                    .touchpoint
                    .metadata
                    .get("content_id")
                    .and_then(serde_json::Value::as_str)
                    != state.identity.content_id.as_deref()
                || record
                    .touchpoint
                    .metadata
                    .get("reply_to_id")
                    .and_then(serde_json::Value::as_str)
                    != state.identity.reply_to_id.as_deref()
                || record
                    .touchpoint
                    .metadata
                    .get("target_platform_account_id")
                    .and_then(serde_json::Value::as_str)
                    != Some(state.identity.target_platform_account_id.as_str())
            {
                return Err(CreatorOutreachError::RecordUnavailable);
            }
            (
                &record.touchpoint,
                &record.audit_event,
                "content_reply",
                "content_reply_sent",
            )
        }
        CreatorOutreachRecord::DirectMessage(record) => {
            let Some(CreatorOutreachVerification::DirectMessage {
                external_message_id,
                provider_thread_id,
                sent_at,
            }) = &state.verification
            else {
                return Err(CreatorOutreachError::RecordUnavailable);
            };
            if record.conversation.account_ref_id != state.identity.operator_account_ref_id
                || Some(&record.conversation.resolved_target_id)
                    != state.identity.resolved_target_id.as_ref()
                || record.conversation.provider_thread_id != *provider_thread_id
                || record.message.external_message_id.as_deref()
                    != Some(external_message_id.as_str())
                || record.message.conversation_id != record.conversation.id
                || record.message.sent_at != *sent_at
                || record.message.direction != "outbound"
                || record.touchpoint.message_id.as_ref() != Some(&record.message.id)
                || crate::outreach::intents::fingerprint_v1(
                    &state.identity,
                    Some(&record.message.body),
                )? != state.fingerprint
            {
                return Err(CreatorOutreachError::RecordUnavailable);
            }
            (
                &record.touchpoint,
                &record.audit_event,
                "direct_message",
                "direct_message_sent",
            )
        }
    };
    if touchpoint.creator_id != state.identity.creator_id
        || audit.creator_id.as_ref() != Some(&state.identity.creator_id)
        || audit.account_ref_id.as_ref() != Some(&state.identity.operator_account_ref_id)
        || audit.resolved_target_id != state.identity.resolved_target_id
        || touchpoint.kind != expected_point
        || audit.kind != expected_audit
    {
        return Err(CreatorOutreachError::RecordUnavailable);
    }
    Ok(())
}

pub(in super::super) fn relationship_input(
    state: &CreatorOutreachActionState,
) -> Result<RelationshipRecordFacts, CreatorOutreachError> {
    let CreatorOutreachVerification::Relationship {
        following,
        can_message,
        checked_at,
    } = state
        .verification
        .as_ref()
        .ok_or(CreatorOutreachError::Storage)?
    else {
        return Err(CreatorOutreachError::ScopeMismatch);
    };
    Ok(RelationshipRecordFacts {
        creator_id: state.identity.creator_id.clone(),
        contact_route_id: state
            .identity
            .contact_route_id
            .clone()
            .ok_or(CreatorOutreachError::ScopeMismatch)?,
        actor_account_ref_id: state.identity.operator_account_ref_id.clone(),
        resolved_target_id: state
            .identity
            .resolved_target_id
            .clone()
            .ok_or(CreatorOutreachError::ScopeMismatch)?,
        target_platform_account_id: state.identity.target_platform_account_id.clone(),
        kind: RelationshipKind::Following,
        value: *following,
        can_message_value: *can_message,
        checked_at: *checked_at,
        action: state
            .identity
            .relationship_action
            .ok_or(CreatorOutreachError::ScopeMismatch)?,
    })
}

pub(in super::super) fn content_input(
    state: &CreatorOutreachActionState,
) -> Result<ContentReplyRecordFacts, CreatorOutreachError> {
    let CreatorOutreachVerification::ContentReply { external_reply_id } = state
        .verification
        .as_ref()
        .ok_or(CreatorOutreachError::Storage)?
    else {
        return Err(CreatorOutreachError::ScopeMismatch);
    };
    Ok(ContentReplyRecordFacts {
        creator_id: state.identity.creator_id.clone(),
        operator_account_ref_id: state.identity.operator_account_ref_id.clone(),
        target_platform_account_id: state.identity.target_platform_account_id.clone(),
        content_id: state
            .identity
            .content_id
            .clone()
            .ok_or(CreatorOutreachError::ScopeMismatch)?,
        reply_to_id: state.identity.reply_to_id.clone(),
        verified_external_reply_id: external_reply_id.clone(),
    })
}

pub(in super::super) fn message_input(
    connection: &Connection,
    state: &CreatorOutreachActionState,
    message: CreatorOutreachMessageRecord,
) -> Result<MessageRecordFacts, CreatorOutreachError> {
    let CreatorOutreachVerification::DirectMessage {
        external_message_id,
        provider_thread_id,
        sent_at,
    } = state
        .verification
        .as_ref()
        .ok_or(CreatorOutreachError::Storage)?
    else {
        return Err(CreatorOutreachError::ScopeMismatch);
    };
    if message.external_message_id != *external_message_id
        || message.provider_thread_id != *provider_thread_id
        || message.sent_at != *sent_at
        || crate::outreach::intents::fingerprint_v1(&state.identity, Some(&message.body))?
            != state.fingerprint
    {
        return Err(CreatorOutreachError::IntentConflict);
    }
    let target_id = state
        .identity
        .resolved_target_id
        .clone()
        .ok_or(CreatorOutreachError::ScopeMismatch)?;
    let existing = connection.query_row("SELECT cursor, unread_count FROM conversations WHERE account_ref_id = ?1 AND resolved_target_id = ?2 AND channel = ?3 AND provider_thread_id = ?4", params![state.identity.operator_account_ref_id.as_str(),target_id.as_str(),state.identity.platform.as_str(),provider_thread_id], |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, u32>(1)?))).optional().map_err(storage)?;
    let (cursor, unread_count) = existing.unwrap_or((None, 0));
    let channel = match state.identity.platform {
        Platform::X => ContactChannel::X,
        Platform::Instagram => ContactChannel::Instagram,
        Platform::Telegram => ContactChannel::Telegram,
        _ => return Err(CreatorOutreachError::ScopeMismatch),
    };
    Ok(MessageRecordFacts {
        snapshot: PersistConversationSnapshotInput {
            creator_id: state.identity.creator_id.clone(),
            contact_route_id: state
                .identity
                .contact_route_id
                .clone()
                .ok_or(CreatorOutreachError::ScopeMismatch)?,
            account_ref_id: state.identity.operator_account_ref_id.clone(),
            resolved_target_id: target_id,
            channel,
            provider_thread_id: provider_thread_id.clone(),
            cursor,
            unread_count,
            messages: vec![ConversationMessageInput {
                external_message_id: external_message_id.clone(),
                sender_target_id: None,
                direction: "outbound".into(),
                body: message.body,
                status: "sent".into(),
                public_data: serde_json::json!({"verified":true}),
                sent_at: *sent_at,
            }],
        },
        verified_external_message_id: external_message_id.clone(),
    })
}

/// Public reply IDs are stable provider receipts. Look in the normal audit and
/// touchpoint facts, including released legacy records, not just new ledgers.
pub(in super::super) fn duplicate_content(
    connection: &Connection,
    state: &CreatorOutreachActionState,
) -> Result<Option<CommitContentReplyActionResult>, CreatorOutreachError> {
    let CreatorOutreachVerification::ContentReply { external_reply_id } = state
        .verification
        .as_ref()
        .ok_or(CreatorOutreachError::Storage)?
    else {
        return Err(CreatorOutreachError::ScopeMismatch);
    };
    let ids = connection.query_row("SELECT point.id, audit.id, point.creator_id, audit.account_ref_id, json_extract(point.metadata_json,'$.content_id'), json_extract(point.metadata_json,'$.reply_to_id'), json_extract(point.metadata_json,'$.target_platform_account_id') FROM touchpoints point JOIN audit_events audit ON audit.kind = 'content_reply_sent' AND json_extract(audit.payload_json,'$.external_reply_id') = json_extract(point.metadata_json,'$.external_reply_id') WHERE point.kind = 'content_reply' AND json_extract(point.metadata_json,'$.external_reply_id') = ?1 ORDER BY point.rowid, audit.rowid LIMIT 1", [external_reply_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?,row.get::<_, Option<String>>(4)?,row.get::<_, Option<String>>(5)?,row.get::<_, Option<String>>(6)?))).optional().map_err(storage)?;
    let Some((point, audit, creator, operator, content, reply_to, target_account)) = ids else {
        return Ok(None);
    };
    if creator != state.identity.creator_id.as_str()
        || operator.as_deref() != Some(state.identity.operator_account_ref_id.as_str())
        || content != state.identity.content_id
        || reply_to != state.identity.reply_to_id
        || target_account.as_deref() != Some(state.identity.target_platform_account_id.as_str())
    {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    let record = CommitContentReplyActionResult {
        external_reply_id: external_reply_id.clone(),
        touchpoint: load_touchpoint(connection, &TouchpointId::new(point))?,
        audit_event: load_audit(connection, &AuditEventId::new(audit))?,
    };
    validate_record(
        connection,
        state,
        &CreatorOutreachRecord::ContentReply(record.clone()),
    )?;
    Ok(Some(record))
}

pub(in super::super) fn duplicate_message(
    connection: &Connection,
    state: &CreatorOutreachActionState,
) -> Result<Option<CommitMessageActionResult>, CreatorOutreachError> {
    let CreatorOutreachVerification::DirectMessage {
        external_message_id,
        provider_thread_id,
        ..
    } = state
        .verification
        .as_ref()
        .ok_or(CreatorOutreachError::Storage)?
    else {
        return Err(CreatorOutreachError::ScopeMismatch);
    };
    let ids = connection.query_row("SELECT conversation.id, message.id, point.id, audit.id, point.creator_id FROM conversations conversation JOIN messages message ON message.conversation_id = conversation.id JOIN touchpoints point ON point.message_id = message.id AND point.kind = 'direct_message' JOIN audit_events audit ON audit.kind = 'direct_message_sent' AND audit.account_ref_id = conversation.account_ref_id AND audit.resolved_target_id = conversation.resolved_target_id AND json_extract(audit.payload_json,'$.external_message_id') = message.external_message_id AND json_extract(audit.payload_json,'$.provider_thread_id') = conversation.provider_thread_id WHERE conversation.account_ref_id = ?1 AND conversation.resolved_target_id = ?2 AND conversation.provider_thread_id = ?3 AND message.external_message_id = ?4 ORDER BY point.rowid, audit.rowid LIMIT 1", params![state.identity.operator_account_ref_id.as_str(),state.identity.resolved_target_id.as_ref().map(ResolvedTargetId::as_str),provider_thread_id,external_message_id], |row| Ok((row.get::<_, String>(0)?,row.get::<_, String>(1)?,row.get::<_, String>(2)?,row.get::<_, String>(3)?,row.get::<_, String>(4)?))).optional().map_err(storage)?;
    let Some((conversation, message, point, audit, creator)) = ids else {
        return Ok(None);
    };
    if creator != state.identity.creator_id.as_str() {
        return Err(CreatorOutreachError::ScopeMismatch);
    }
    let record = CommitMessageActionResult {
        conversation: load_conversation(connection, &ConversationId::new(conversation))?,
        message: load_message(connection, &MessageId::new(message))?,
        touchpoint: load_touchpoint(connection, &TouchpointId::new(point))?,
        audit_event: load_audit(connection, &AuditEventId::new(audit))?,
    };
    validate_record(
        connection,
        state,
        &CreatorOutreachRecord::DirectMessage(record.clone()),
    )?;
    Ok(Some(record))
}

fn load_touchpoint(
    connection: &Connection,
    id: &TouchpointId,
) -> Result<Touchpoint, CreatorOutreachError> {
    connection.query_row("SELECT creator_id, kind, message_id, email_id, account_relationship_id, occurred_at, metadata_json FROM touchpoints WHERE id = ?1", [id.as_str()], |row| Ok(Touchpoint { id: id.clone(), creator_id: CreatorId::new(row.get::<_, String>(0)?), kind: row.get(1)?, message_id: row.get::<_, Option<String>>(2)?.map(MessageId::new), email_id: row.get::<_, Option<String>>(3)?.map(EmailId::new), account_relationship_id: row.get::<_, Option<String>>(4)?.map(AccountRelationshipId::new), occurred_at: row.get(5)?, metadata: parse_json(row.get(6)?)? })).optional().map_err(storage)?.ok_or(CreatorOutreachError::RecordUnavailable)
}
fn load_audit(
    connection: &Connection,
    id: &AuditEventId,
) -> Result<AuditEvent, CreatorOutreachError> {
    connection.query_row("SELECT kind, creator_id, account_ref_id, resolved_target_id, mailbox_ref_id, contact_locator_id, email_id, payload_json, created_at FROM audit_events WHERE id = ?1", [id.as_str()], |row| Ok(AuditEvent { id: id.clone(), kind: row.get(0)?, creator_id: row.get::<_, Option<String>>(1)?.map(CreatorId::new), account_ref_id: row.get::<_, Option<String>>(2)?.map(AccountRefId::new), resolved_target_id: row.get::<_, Option<String>>(3)?.map(ResolvedTargetId::new), mailbox_ref_id: row.get::<_, Option<String>>(4)?.map(MailboxRefId::new), contact_locator_id: row.get::<_, Option<String>>(5)?.map(ContactLocatorId::new), email_id: row.get::<_, Option<String>>(6)?.map(EmailId::new), payload: parse_json(row.get(7)?)?, created_at: row.get(8)? })).optional().map_err(storage)?.ok_or(CreatorOutreachError::RecordUnavailable)
}
fn load_conversation(
    connection: &Connection,
    id: &ConversationId,
) -> Result<Conversation, CreatorOutreachError> {
    connection.query_row("SELECT account_ref_id, resolved_target_id, channel, provider_thread_id, cursor, unread_count, created_at, updated_at FROM conversations WHERE id = ?1", [id.as_str()], |row| Ok(Conversation { id: id.clone(), account_ref_id: AccountRefId::new(row.get::<_, String>(0)?), resolved_target_id: ResolvedTargetId::new(row.get::<_, String>(1)?), channel: ContactChannel::parse(&row.get::<_, String>(2)?).map_err(super::super::creators::to_sql_conversion_error)?, provider_thread_id: row.get(3)?, cursor: row.get(4)?, unread_count: row.get(5)?, created_at: row.get(6)?, updated_at: row.get(7)? })).optional().map_err(storage)?.ok_or(CreatorOutreachError::RecordUnavailable)
}
fn load_message(connection: &Connection, id: &MessageId) -> Result<Message, CreatorOutreachError> {
    connection.query_row("SELECT conversation_id, external_message_id, sender_target_id, direction, body, status, public_json, sent_at FROM messages WHERE id = ?1", [id.as_str()], |row| Ok(Message { id: id.clone(), conversation_id: ConversationId::new(row.get::<_, String>(0)?), external_message_id: row.get(1)?, sender_target_id: row.get::<_, Option<String>>(2)?.map(ResolvedTargetId::new), direction: row.get(3)?, body: row.get(4)?, status: row.get(5)?, public_data: parse_json(row.get(6)?)?, sent_at: row.get(7)? })).optional().map_err(storage)?.ok_or(CreatorOutreachError::RecordUnavailable)
}
fn parse_json(value: String) -> rusqlite::Result<serde_json::Value> {
    serde_json::from_str(&value)
        .map_err(|error| super::super::creators::to_sql_conversion_error(error.to_string()))
}
fn storage(_: impl std::fmt::Display) -> CreatorOutreachError {
    CreatorOutreachError::Storage
}
