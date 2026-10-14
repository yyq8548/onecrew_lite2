use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    AccountRefId, AuditEvent, AuditEventId, CommitMessageActionResult, ContactChannel,
    ContactRouteLastContactAccount, Conversation, ConversationId, ConversationLastMessage,
    ConversationMessageInput, ConversationRoute, ConversationRouteContext, ConversationSummary,
    CreatorContactRouteId, CreatorId, InboxAttention, ListConversationsInput, Message, MessageId,
    Page, PersistConversationSnapshotInput, Platform, ResolvedTargetId, Touchpoint, TouchpointId,
    repository::ConversationRepository,
};

use super::{SqliteRepository, json, now};

impl ConversationRepository for SqliteRepository {
    fn contact_route_last_contact_account(
        &self,
        creator_id: &CreatorId,
        contact_route_id: &CreatorContactRouteId,
    ) -> Result<Option<ContactRouteLastContactAccount>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let route_belongs_to_creator = transaction
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM creator_contact_routes
                   WHERE id = ?1 AND creator_id = ?2
                 )",
                params![contact_route_id.as_str(), creator_id.as_str()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| error.to_string())?;
        if !route_belongs_to_creator {
            return Err("contact route does not belong to the selected Creator".into());
        }

        let result = transaction
            .query_row(
                "SELECT account.id, account.platform, account.kind, account.label,
                        account.active, account.availability, account.created_at,
                        account.updated_at, conversation.channel, message.sent_at
                 FROM creator_contact_routes route
                 JOIN contact_locators locator ON locator.id = route.locator_id
                 JOIN locator_resolutions resolution
                   ON resolution.locator_id = route.locator_id
                 JOIN resolved_targets target
                   ON target.id = resolution.resolved_target_id
                  AND target.account_ref_id = resolution.account_ref_id
                 JOIN conversations conversation
                   ON conversation.account_ref_id = target.account_ref_id
                  AND conversation.resolved_target_id = target.id
                  AND conversation.channel = locator.channel
                 JOIN conversation_contact_routes conversation_owner
                   ON conversation_owner.conversation_id = conversation.id
                  AND conversation_owner.contact_route_id = route.id
                 JOIN messages message ON message.conversation_id = conversation.id
                 JOIN account_refs account ON account.id = conversation.account_ref_id
                 WHERE route.id = ?1 AND route.creator_id = ?2
                   AND message.direction = 'outbound' AND message.status = 'sent'
                 ORDER BY message.sent_at DESC, message.id ASC
                 LIMIT 1",
                params![contact_route_id.as_str(), creator_id.as_str()],
                |row| {
                    let platform = Platform::parse(&row.get::<_, String>(1)?).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::other(error)),
                        )
                    })?;
                    let channel =
                        ContactChannel::parse(&row.get::<_, String>(8)?).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                6,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::other(error)),
                            )
                        })?;
                    Ok(ContactRouteLastContactAccount {
                        contact_route_id: contact_route_id.clone(),
                        account_ref: crate::AccountRef {
                            id: AccountRefId::new(row.get::<_, String>(0)?),
                            platform,
                            kind: crate::AccountKind::parse(&row.get::<_, String>(2)?)
                                .map_err(super::creators::to_sql_conversion_error)?,
                            label: row.get(3)?,
                            active: row.get(4)?,
                            availability: crate::AccountAvailability::parse(
                                &row.get::<_, String>(5)?,
                            )
                            .map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    5,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::other(error)),
                                )
                            })?,
                            created_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        },
                        channel,
                        last_contact_at: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn list_conversations(
        &self,
        input: ListConversationsInput,
    ) -> Result<Page<ConversationSummary>, String> {
        let limit = crate::CREATOR_PAGE_LIMITS.effective(input.limit) as usize;
        let query = input
            .query
            .as_deref()
            .map(|query| query.trim().trim_start_matches('@').trim())
            .and_then(|query| (!query.is_empty()).then(|| query.to_string()));
        let status = input.status.map(|status| status.as_str().to_string());
        let channel = input.channel.map(|channel| channel.as_str().to_string());
        let context = format!(
            "conversation_list|query={}|status={}|channel={}",
            query.as_deref().unwrap_or(""),
            status.as_deref().unwrap_or("any"),
            channel.as_deref().unwrap_or("any")
        );
        let position = input
            .cursor
            .as_deref()
            .map(|cursor| super::decode_keyset_single(cursor, &context))
            .transpose()?;
        let (position_updated_at, position_id) = match &position {
            Some((updated_at, id)) => (Some(*updated_at), Some(id.clone())),
            None => (None, None),
        };
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare(
                "SELECT conversation.id, conversation.channel,
                        conversation.provider_thread_id, conversation.account_ref_id,
                        conversation.resolved_target_id, conversation.updated_at,
                        account.label,
                        route.id AS contact_route_id, route.creator_id,
                        creator.display_name,
                        last_message.direction, last_message.body, last_message.sent_at,
                        peer_observation.handle
                 FROM conversations conversation
                 JOIN account_refs account ON account.id = conversation.account_ref_id
                 JOIN creator_contact_routes route ON route.id = (
                     SELECT link.contact_route_id FROM conversation_contact_routes link
                     WHERE link.conversation_id = conversation.id
                     ORDER BY link.created_at ASC, link.contact_route_id ASC LIMIT 1)
                 JOIN creators creator ON creator.id = route.creator_id
                 LEFT JOIN resolved_targets peer ON peer.id = conversation.resolved_target_id
                 LEFT JOIN platform_account_observations peer_observation
                   ON peer_observation.id = (
                     SELECT latest.id FROM platform_account_observations latest
                     WHERE latest.platform_account_id = peer.platform_account_id
                     ORDER BY latest.observed_at DESC, latest.rowid DESC LIMIT 1)
                 LEFT JOIN messages last_message ON last_message.id = (
                     SELECT id FROM messages
                     WHERE conversation_id = conversation.id
                     ORDER BY sent_at DESC, rowid DESC LIMIT 1)
                 WHERE (?1 IS NULL OR instr(lower(creator.display_name), lower(?1)) > 0
                        OR instr(lower(account.label), lower(?1)) > 0
                        OR (peer_observation.handle IS NOT NULL
                            AND instr(lower(peer_observation.handle), lower(?1)) > 0))
                   AND (?2 IS NULL OR conversation.channel = ?2)
                   AND (?3 IS NULL
                        OR (?3 = 'waiting_reply' AND last_message.direction = 'inbound')
                        OR (?3 = 'replied' AND last_message.direction = 'outbound'))
                   AND (?4 IS NULL OR conversation.updated_at < ?4
                        OR (conversation.updated_at = ?4 AND conversation.id < ?5))
                 ORDER BY conversation.updated_at DESC, conversation.id DESC
                 LIMIT ?6",
            )
            .map_err(|error| error.to_string())?;
        let summaries = statement
            .query_map(
                params![
                    query,
                    channel,
                    status,
                    position_updated_at,
                    position_id,
                    (limit + 1) as i64
                ],
                |row| {
                    let channel: String = row.get(1)?;
                    let body: Option<String> = row.get(11)?;
                    let last_message = match body {
                        Some(body) => Some(ConversationLastMessage {
                            direction: row.get(10)?,
                            preview: body.chars().take(120).collect(),
                            sent_at: row.get(12)?,
                        }),
                        None => None,
                    };
                    Ok(ConversationSummary {
                        triage: None,
                        conversation_id: ConversationId::new(row.get::<_, String>(0)?),
                        creator_id: CreatorId::new(row.get::<_, String>(8)?),
                        creator_display_name: row.get(9)?,
                        // 逐行解析成本高(要按平台取键),放到语句释放后统一补。
                        creator_avatar_url: None,
                        peer_handle: row.get::<_, Option<String>>(13)?,
                        channel: ContactChannel::parse(&channel)
                            .map_err(super::creators::to_sql_conversion_error)?,
                        operator_account_ref_id: AccountRefId::new(row.get::<_, String>(3)?),
                        operator_account_label: row.get(6)?,
                        contact_route_id: CreatorContactRouteId::new(row.get::<_, String>(7)?),
                        resolved_target_id: ResolvedTargetId::new(row.get::<_, String>(4)?),
                        provider_thread_id: row.get(2)?,
                        last_message,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .map_err(|error| error.to_string())?
            .map(|row| row.map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let mut summaries = summaries;
        for summary in &mut summaries {
            summary.creator_avatar_url =
                super::creators::creator_avatar_url(&transaction, summary.creator_id.as_str())?;
            // The agent's reading of the latest inbound message rides the
            // row, because the row is where the human decides what to open.
            summary.triage = super::reply_triage::conversation_triage(
                &transaction,
                summary.conversation_id.as_str(),
            )?;
        }
        let page = super::keyset_page(summaries, limit, &context, |summary| {
            (vec![summary.updated_at], summary.conversation_id.as_str())
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn inbox_attention(&self) -> Result<InboxAttention, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        // 「最后一条是对方发的」= 球在我们手里。子查询取最后一条的排序
        // 必须与列表一致,否则两处对「最后一条」的理解会分叉。
        let conversations = transaction
            .query_row(
                "SELECT COUNT(*) FROM conversations c
                 WHERE (SELECT m.direction FROM messages m
                        WHERE m.conversation_id = c.id
                        ORDER BY m.sent_at DESC, m.rowid DESC LIMIT 1) = 'inbound'",
                [],
                |row| row.get::<_, u32>(0),
            )
            .map_err(|error| error.to_string())?;
        // 删掉的线程不该继续要人看。
        let mail = transaction
            .query_row(
                "SELECT COUNT(*) FROM email_threads
                 WHERE unread_count > 0 AND deleted_at IS NULL",
                [],
                |row| row.get::<_, u32>(0),
            )
            .map_err(|error| error.to_string())?;
        let attention = InboxAttention {
            conversations,
            mail,
        };
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(attention)
    }

    fn get_conversation(
        &self,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
        channel: ContactChannel,
    ) -> Result<Option<Conversation>, String> {
        let connection = self.read_connection()?;
        connection
            .query_row(
                "SELECT id, provider_thread_id, cursor, unread_count, created_at, updated_at
                 FROM conversations
                 WHERE account_ref_id = ?1 AND resolved_target_id = ?2 AND channel = ?3
                 ORDER BY updated_at DESC, id ASC LIMIT 1",
                params![
                    account_ref_id.as_str(),
                    resolved_target_id.as_str(),
                    channel.as_str()
                ],
                |row| {
                    Ok(Conversation {
                        id: ConversationId::new(row.get::<_, String>(0)?),
                        account_ref_id: account_ref_id.clone(),
                        resolved_target_id: resolved_target_id.clone(),
                        channel,
                        provider_thread_id: row.get(1)?,
                        cursor: row.get(2)?,
                        unread_count: row.get(3)?,
                        created_at: row.get(4)?,
                        updated_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    fn conversation_route_context(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ConversationRouteContext>, String> {
        let connection = self.read_connection()?;
        connection
            .query_row(
                // The first-linked contact route is what the inbox list and
                // reply triage already treat as the conversation's owner, so
                // the by-ID read repeats their ordering verbatim. The join is
                // LEFT so an unlinked conversation is an answer, not an
                // absence: the caller decides whether it can act on one.
                "SELECT conversation.account_ref_id, conversation.resolved_target_id,
                        conversation.channel, conversation.provider_thread_id,
                        conversation.cursor, conversation.unread_count,
                        conversation.created_at, conversation.updated_at,
                        route.id, route.creator_id
                 FROM conversations conversation
                 LEFT JOIN creator_contact_routes route ON route.id = (
                     SELECT link.contact_route_id FROM conversation_contact_routes link
                     WHERE link.conversation_id = conversation.id
                     ORDER BY link.created_at ASC, link.contact_route_id ASC LIMIT 1)
                 WHERE conversation.id = ?1",
                params![conversation_id.as_str()],
                |row| {
                    let channel = ContactChannel::parse(&row.get::<_, String>(2)?)
                        .map_err(super::creators::to_sql_conversion_error)?;
                    let route = match (
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                    ) {
                        (Some(contact_route_id), Some(creator_id)) => Some(ConversationRoute {
                            contact_route_id: CreatorContactRouteId::new(contact_route_id),
                            creator_id: CreatorId::new(creator_id),
                        }),
                        _ => None,
                    };
                    Ok(ConversationRouteContext {
                        conversation: Conversation {
                            id: conversation_id.clone(),
                            account_ref_id: AccountRefId::new(row.get::<_, String>(0)?),
                            resolved_target_id: ResolvedTargetId::new(row.get::<_, String>(1)?),
                            channel,
                            provider_thread_id: row.get(3)?,
                            cursor: row.get(4)?,
                            unread_count: row.get(5)?,
                            created_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        },
                        route,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    fn list_messages(&self, conversation_id: &ConversationId) -> Result<Vec<Message>, String> {
        let connection = self.read_connection()?;
        load_messages(&connection, conversation_id)
    }

    fn upsert_conversation(
        &self,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
        channel: ContactChannel,
        provider_thread_id: &str,
        cursor: Option<&str>,
        unread_count: u32,
    ) -> Result<Conversation, String> {
        let provider_thread_id = provider_thread_id.trim();
        if provider_thread_id.is_empty() {
            return Err("provider thread id is required".into());
        }
        let connection = self.lock()?;
        let timestamp = now();
        let existing = connection
            .query_row(
                "SELECT id, created_at FROM conversations
                 WHERE account_ref_id = ?1 AND resolved_target_id = ?2
                   AND channel = ?3 AND provider_thread_id = ?4",
                params![
                    account_ref_id.as_str(),
                    resolved_target_id.as_str(),
                    channel.as_str(),
                    provider_thread_id
                ],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let (id, created_at) = existing
            .map(|(id, created_at)| (ConversationId::new(id), created_at))
            .unwrap_or_else(|| (ConversationId::new(Uuid::new_v4().to_string()), timestamp));
        connection
            .execute(
                "INSERT INTO conversations
                   (id, account_ref_id, resolved_target_id, channel, provider_thread_id,
                    cursor, unread_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(account_ref_id, resolved_target_id, channel, provider_thread_id)
                 DO UPDATE SET cursor = excluded.cursor, unread_count = excluded.unread_count,
                   updated_at = excluded.updated_at",
                params![
                    id.as_str(),
                    account_ref_id.as_str(),
                    resolved_target_id.as_str(),
                    channel.as_str(),
                    provider_thread_id,
                    cursor,
                    unread_count,
                    created_at,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(Conversation {
            id,
            account_ref_id: account_ref_id.clone(),
            resolved_target_id: resolved_target_id.clone(),
            channel,
            provider_thread_id: provider_thread_id.to_owned(),
            cursor: cursor.map(str::to_owned),
            unread_count,
            created_at,
            updated_at: timestamp,
        })
    }

    fn append_message(
        &self,
        super::super::AppendMessageInput {
            conversation_id,
            external_message_id,
            sender_target_id,
            direction,
            body,
            status,
            public_data,
            sent_at,
        }: super::super::AppendMessageInput<'_>,
    ) -> Result<Message, String> {
        if direction.trim().is_empty() || status.trim().is_empty() {
            return Err("message direction and status are required".into());
        }
        let connection = self.lock()?;
        if let Some(external_message_id) = external_message_id {
            let existing = connection
                .query_row(
                    "SELECT id, sender_target_id, direction, body, status, public_json, sent_at
                     FROM messages WHERE conversation_id = ?1 AND external_message_id = ?2",
                    params![conversation_id.as_str(), external_message_id],
                    |row| {
                        let public_json: String = row.get(5)?;
                        Ok(Message {
                            id: MessageId::new(row.get::<_, String>(0)?),
                            conversation_id: conversation_id.clone(),
                            external_message_id: Some(external_message_id.to_owned()),
                            sender_target_id: row
                                .get::<_, Option<String>>(1)?
                                .map(ResolvedTargetId::new),
                            direction: row.get(2)?,
                            body: row.get(3)?,
                            status: row.get(4)?,
                            public_data: serde_json::from_str(&public_json).map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    public_json.len(),
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            })?,
                            sent_at: row.get(6)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(existing) = existing {
                return Ok(existing);
            }
        }
        let id = MessageId::new(Uuid::new_v4().to_string());
        connection
            .execute(
                "INSERT INTO messages
                   (id, conversation_id, external_message_id, sender_target_id, direction,
                    body, status, public_json, sent_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id.as_str(),
                    conversation_id.as_str(),
                    external_message_id,
                    sender_target_id.map(ResolvedTargetId::as_str),
                    direction.trim(),
                    body,
                    status.trim(),
                    json(&public_data)?,
                    sent_at
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(Message {
            id,
            conversation_id: conversation_id.clone(),
            external_message_id: external_message_id.map(str::to_owned),
            sender_target_id: sender_target_id.cloned(),
            direction: direction.trim().to_owned(),
            body: body.to_owned(),
            status: status.trim().to_owned(),
            public_data,
            sent_at,
        })
    }

    fn persist_conversation_snapshot(
        &self,
        input: PersistConversationSnapshotInput,
    ) -> Result<(Conversation, Vec<Message>), String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let (conversation, messages) = persist_snapshot(&transaction, &input)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok((conversation, messages))
    }

    fn record_creator_message(
        &self,
        key: &str,
        message: crate::CreatorOutreachMessageRecord,
    ) -> Result<CommitMessageActionResult, crate::CreatorOutreachError> {
        let mut connection = self
            .lock()
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let state = super::outreach_actions::ledger::require_verified(
            &transaction,
            key,
            crate::CreatorOutreachOperation::DirectMessageSend,
        )?;
        if state.phase == crate::CreatorOutreachPhase::Completed {
            return match super::outreach_actions::record::record_in(&transaction, &state)? {
                crate::CreatorOutreachRecord::DirectMessage(record) => Ok(record),
                _ => Err(crate::CreatorOutreachError::ScopeMismatch),
            };
        }
        super::outreach_actions::ledger::validate_ownership_in(
            &transaction,
            &state.identity,
            false,
        )?;
        let input = super::outreach_actions::record::message_input(&transaction, &state, message)?;
        if let Some(record) =
            super::outreach_actions::record::duplicate_message(&transaction, &state)?
        {
            let ids = crate::CreatorOutreachRecordIds::DirectMessage {
                conversation_id: record.conversation.id.clone(),
                message_id: record.message.id.clone(),
                touchpoint_id: record.touchpoint.id.clone(),
                audit_event_id: record.audit_event.id.clone(),
            };
            super::outreach_actions::ledger::complete_in(&transaction, key, state, ids)?;
            transaction
                .commit()
                .map_err(|_| crate::CreatorOutreachError::CommitUnknown)?;
            return Ok(record);
        }
        let result = (|| -> Result<CommitMessageActionResult, String> {
            let timestamp = now();
            let (conversation, messages) = persist_snapshot(&transaction, &input.snapshot)?;
            let message = messages
                .into_iter()
                .find(|message| {
                    message.external_message_id.as_deref()
                        == Some(input.verified_external_message_id.as_str())
                })
                .ok_or_else(|| "verified outbound message was missing from snapshot".to_string())?;
            let touchpoint = Touchpoint {
                id: TouchpointId::new(Uuid::new_v4().to_string()),
                creator_id: input.snapshot.creator_id.clone(),
                kind: "direct_message".into(),
                message_id: Some(message.id.clone()),
                email_id: None,
                account_relationship_id: None,
                occurred_at: timestamp,
                metadata: serde_json::json!({ "verified": true }),
            };
            transaction
                .execute(
                    "INSERT INTO touchpoints
                   (id, creator_id, kind, message_id, email_id, account_relationship_id,
                    contact_route_id, occurred_at, metadata_json)
                 VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?6, ?7)",
                    params![
                        touchpoint.id.as_str(),
                        touchpoint.creator_id.as_str(),
                        touchpoint.kind,
                        touchpoint.message_id.as_ref().map(MessageId::as_str),
                        input.snapshot.contact_route_id.as_str(),
                        touchpoint.occurred_at,
                        json(&touchpoint.metadata)?
                    ],
                )
                .map_err(|error| error.to_string())?;
            let audit_event = AuditEvent {
                id: AuditEventId::new(Uuid::new_v4().to_string()),
                kind: "direct_message_sent".into(),
                creator_id: Some(input.snapshot.creator_id.clone()),
                account_ref_id: Some(input.snapshot.account_ref_id.clone()),
                resolved_target_id: Some(input.snapshot.resolved_target_id.clone()),
                mailbox_ref_id: None,
                contact_locator_id: None,
                email_id: None,
                payload: serde_json::json!({
                    "external_message_id": input.verified_external_message_id,
                    "provider_thread_id": conversation.provider_thread_id,
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
                            .map(ResolvedTargetId::as_str),
                        input.snapshot.contact_route_id.as_str(),
                        json(&audit_event.payload)?,
                        audit_event.created_at
                    ],
                )
                .map_err(|error| error.to_string())?;
            let result = CommitMessageActionResult {
                conversation,
                message,
                touchpoint,
                audit_event,
            };
            Ok(result)
        })()
        .map_err(|_| crate::CreatorOutreachError::Storage)?;
        let ids = crate::CreatorOutreachRecordIds::DirectMessage {
            conversation_id: result.conversation.id.clone(),
            message_id: result.message.id.clone(),
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

fn persist_snapshot(
    transaction: &rusqlite::Transaction<'_>,
    input: &PersistConversationSnapshotInput,
) -> Result<(Conversation, Vec<Message>), String> {
    let provider_thread_id = input.provider_thread_id.trim();
    if provider_thread_id.is_empty() {
        return Err("provider thread id is required".into());
    }
    let timestamp = now();
    let existing = transaction
        .query_row(
            "SELECT id, created_at FROM conversations
             WHERE account_ref_id = ?1 AND resolved_target_id = ?2
               AND channel = ?3 AND provider_thread_id = ?4",
            params![
                input.account_ref_id.as_str(),
                input.resolved_target_id.as_str(),
                input.channel.as_str(),
                provider_thread_id
            ],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let (id, created_at) = existing
        .map(|(id, created_at)| (ConversationId::new(id), created_at))
        .unwrap_or_else(|| (ConversationId::new(Uuid::new_v4().to_string()), timestamp));
    transaction
        .execute(
            "INSERT INTO conversations
               (id, account_ref_id, resolved_target_id, channel, provider_thread_id,
                cursor, unread_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(account_ref_id, resolved_target_id, channel, provider_thread_id)
             DO UPDATE SET cursor = excluded.cursor, unread_count = excluded.unread_count,
               updated_at = excluded.updated_at",
            params![
                id.as_str(),
                input.account_ref_id.as_str(),
                input.resolved_target_id.as_str(),
                input.channel.as_str(),
                provider_thread_id,
                input.cursor,
                input.unread_count,
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO conversation_contact_routes
               (conversation_id, contact_route_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![id.as_str(), input.contact_route_id.as_str(), timestamp],
        )
        .map_err(|error| error.to_string())?;
    for message in &input.messages {
        append_snapshot_message(transaction, &id, message)?;
    }
    let conversation = Conversation {
        id: id.clone(),
        account_ref_id: input.account_ref_id.clone(),
        resolved_target_id: input.resolved_target_id.clone(),
        channel: input.channel,
        provider_thread_id: provider_thread_id.to_owned(),
        cursor: input.cursor.clone(),
        unread_count: input.unread_count,
        created_at,
        updated_at: timestamp,
    };
    let messages = load_messages(transaction, &id)?;
    Ok((conversation, messages))
}

fn append_snapshot_message(
    connection: &rusqlite::Connection,
    conversation_id: &ConversationId,
    message: &ConversationMessageInput,
) -> Result<(), String> {
    let external_message_id = message.external_message_id.trim();
    if external_message_id.is_empty() {
        return Err("external message id is required for synchronized messages".into());
    }
    connection
        .execute(
            "INSERT INTO messages
               (id, conversation_id, external_message_id, sender_target_id, direction,
                body, status, public_json, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(conversation_id, external_message_id)
             DO UPDATE SET sender_target_id = excluded.sender_target_id,
               direction = excluded.direction, body = excluded.body,
               status = excluded.status, public_json = excluded.public_json,
               sent_at = excluded.sent_at",
            params![
                Uuid::new_v4().to_string(),
                conversation_id.as_str(),
                external_message_id,
                message
                    .sender_target_id
                    .as_ref()
                    .map(ResolvedTargetId::as_str),
                message.direction.trim(),
                message.body,
                message.status.trim(),
                json(&message.public_data)?,
                message.sent_at
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn load_messages(
    connection: &rusqlite::Connection,
    conversation_id: &ConversationId,
) -> Result<Vec<Message>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, external_message_id, sender_target_id, direction, body, status,
                    public_json, sent_at
             FROM messages WHERE conversation_id = ?1 ORDER BY sent_at ASC, id ASC",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([conversation_id.as_str()], |row| {
            let public_json: String = row.get(6)?;
            Ok(Message {
                id: MessageId::new(row.get::<_, String>(0)?),
                conversation_id: conversation_id.clone(),
                external_message_id: row.get(1)?,
                sender_target_id: row.get::<_, Option<String>>(2)?.map(ResolvedTargetId::new),
                direction: row.get(3)?,
                body: row.get(4)?,
                status: row.get(5)?,
                public_data: serde_json::from_str(&public_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        public_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                sent_at: row.get(7)?,
            })
        })
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect()
}
