use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    AuditEvent, AuditEventId, CommitEmailActionInput, CommitManualEmailActionInput, CreatorId,
    EmailActionResult, EmailDirection, EmailFolder, EmailFolderSummary, EmailId, EmailMessage,
    EmailSnapshotInput, EmailThread, EmailThreadId, EmailThreadSummary, MailboxRefId,
    ManualEmailActionResult, Touchpoint, TouchpointId, encode_persisted_email_action_result,
    encode_persisted_manual_email_action_result, repository::EmailRepository,
};

use super::{
    super::{SqliteRepository, decode_keyset_single, json, keyset_page, now},
    queries::{load_message, load_thread, update_thread_timestamp},
    snapshots::persist_snapshot,
};

impl EmailRepository for SqliteRepository {
    fn outreach_reply_seeds(&self, mailbox_ref_id: &MailboxRefId) -> Result<Vec<(EmailThreadId, String)>, String> {
        let connection = self.read_connection()?;
        let mut statement = connection.prepare(
            "SELECT e.thread_id, MIN(e.provider_message_id) FROM emails e
             JOIN email_threads t ON t.id = e.thread_id
             WHERE t.mailbox_ref_id = ?1 AND e.direction = 'outbound'
               AND e.origin = 'confirmed_send' AND e.provider_message_id != ''
             GROUP BY e.thread_id ORDER BY e.thread_id"
        ).map_err(|e| e.to_string())?;
        statement.query_map([mailbox_ref_id.as_str()], |row| {
            Ok((EmailThreadId::new(row.get::<_, String>(0)?), row.get(1)?))
        }).map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    }

    fn persist_email_snapshot(
        &self,
        input: EmailSnapshotInput,
    ) -> Result<(EmailThread, Vec<EmailMessage>), String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let result = persist_snapshot(&transaction, &input)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn commit_email_sync_page(
        &self,
        input: crate::EmailSyncPageCommit,
    ) -> Result<crate::EmailSyncPageOutcome, String> {
        SqliteRepository::commit_email_sync_page(self, input)
    }

    fn email_sync_checkpoint(
        &self,
        mailbox_ref_id: &MailboxRefId,
        direction: EmailDirection,
    ) -> Result<crate::EmailSyncCheckpoint, String> {
        SqliteRepository::email_sync_checkpoint(self, mailbox_ref_id, direction)
    }

    fn mailbox_sync_readback(
        &self,
        mailbox_ref_id: &MailboxRefId,
        triage_limit: u32,
    ) -> Result<
        (
            Option<crate::EmailSyncState>,
            Option<crate::EmailSyncState>,
            crate::PendingReplyTriageDebt,
        ),
        String,
    > {
        SqliteRepository::mailbox_sync_readback(self, mailbox_ref_id, triage_limit)
    }

    fn complete_email_provider_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        provider_message_ids: &[String],
    ) -> Result<Vec<String>, String> {
        SqliteRepository::complete_email_provider_ids(self, mailbox_ref_id, provider_message_ids)
    }

    fn reconcile_sent_statuses(
        &self,
        mailbox_ref_id: &MailboxRefId,
        updates: &[(String, String)],
    ) -> Result<Vec<EmailThreadId>, String> {
        SqliteRepository::reconcile_sent_statuses(self, mailbox_ref_id, updates)
    }

    fn find_email_threads_referencing(
        &self,
        mailbox_ref_id: &MailboxRefId,
        internet_message_id: &str,
    ) -> Result<Vec<EmailThreadId>, String> {
        SqliteRepository::find_email_threads_referencing(self, mailbox_ref_id, internet_message_id)
    }

    fn merge_email_threads(
        &self,
        target: &EmailThreadId,
        sources: &[EmailThreadId],
    ) -> Result<(), String> {
        SqliteRepository::merge_email_threads(self, target, sources)
    }

    fn email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        let connection = self.read_connection()?;
        load_thread(&connection, thread_id)
    }

    fn email_attachments(
        &self,
        email_id: &EmailId,
    ) -> Result<Vec<crate::EmailAttachmentView>, String> {
        SqliteRepository::email_attachments(self, email_id)
    }

    fn email_provider_coordinates(
        &self,
        email_id: &EmailId,
    ) -> Result<(MailboxRefId, String, EmailDirection), String> {
        SqliteRepository::email_provider_coordinates(self, email_id)
    }

    fn record_email_attachments(
        &self,
        email_id: &EmailId,
        direction: EmailDirection,
        metadata: &[onecrew_email_contracts::PlatformEmailAttachment],
    ) -> Result<(), String> {
        SqliteRepository::record_email_attachments(self, email_id, direction, metadata)
    }

    fn mark_email_attachment_unavailable(
        &self,
        email_id: &EmailId,
        provider_attachment_id: &str,
    ) -> Result<(), String> {
        SqliteRepository::mark_email_attachment_unavailable(self, email_id, provider_attachment_id)
    }

    fn list_email_threads(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
        folder: EmailFolder,
        sort: crate::EmailThreadSort,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<EmailThreadSummary>, String> {
        let context = format!(
            "email_thread_list|mailbox={}|folder={}|sort={}",
            mailbox_ref_id.map(MailboxRefId::as_str).unwrap_or("<all>"),
            folder.as_str(),
            sort.as_str(),
        );
        let position = cursor
            .map(|cursor| decode_keyset_single(cursor, &context))
            .transpose()?;
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let (comparison, order) = match sort {
            crate::EmailThreadSort::Newest => ("<", "DESC"),
            crate::EmailThreadSort::Oldest => (">", "ASC"),
        };
        let query = format!(
            "WITH thread_activity AS (
               SELECT t.id, COALESCE(MAX(e.occurred_at), t.created_at) AS latest_message_at
               FROM email_threads t
               LEFT JOIN emails e ON e.thread_id = t.id
               WHERE (?1 IS NULL OR t.mailbox_ref_id = ?1)
                 AND ((?2 = 'all' AND t.deleted_at IS NULL)
                   OR (?2 = 'inbox' AND t.archived_at IS NULL AND t.deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = t.id AND e.direction = 'inbound'))
                   OR (?2 = 'sent' AND t.deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = t.id AND e.direction = 'outbound'))
                   OR (?2 = 'archive' AND t.archived_at IS NOT NULL AND t.deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = t.id AND e.direction = 'inbound'))
                   OR (?2 = 'deleted' AND t.deleted_at IS NOT NULL))
               GROUP BY t.id
             )
             SELECT id FROM thread_activity
             WHERE (?3 IS NULL OR latest_message_at {comparison} ?3
               OR (latest_message_at = ?3 AND id {comparison} ?4))
             ORDER BY latest_message_at {order}, id {order}
             LIMIT ?5"
        );
        let mut statement = transaction
            .prepare(&query)
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map(
                params![
                    mailbox_ref_id.map(MailboxRefId::as_str),
                    folder.as_str(),
                    position.as_ref().map(|(occurred_at, _)| occurred_at),
                    position.as_ref().map(|(_, id)| id.as_str()),
                    u64::from(limit) + 1
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        let summaries = ids
            .into_iter()
            .map(|id| {
                let thread = load_thread(&transaction, &EmailThreadId::new(id))?;
                let (latest_message_id, message_count, has_outbound) =
                    email_thread_activity(&transaction, &thread.id)?;
                let latest_message = latest_message_id
                    .map(|id| load_message(&transaction, &id))
                    .transpose()?
                    .map(|message| message.preview());
                let triage = crate::repository::sqlite::reply_triage::email_thread_triage(
                    &transaction,
                    thread.id.as_str(),
                )?;
                Ok(EmailThreadSummary {
                    triage,
                    thread,
                    latest_message,
                    message_count,
                    has_outbound,
                    creator_display_name: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let page = keyset_page(summaries, limit as usize, &context, |summary| {
            (
                vec![
                    summary
                        .latest_message
                        .as_ref()
                        .map(|message| message.occurred_at)
                        .unwrap_or(summary.thread.created_at),
                ],
                summary.thread.id.as_str(),
            )
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn list_email_folder_counts(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
    ) -> Result<Vec<EmailFolderSummary>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mailbox_ref_id = mailbox_ref_id.map(MailboxRefId::as_str);
        let thread_count = |predicate: &str| -> Result<u64, String> {
            transaction
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM email_threads
                         WHERE (?1 IS NULL OR mailbox_ref_id = ?1) AND ({predicate})"
                    ),
                    [mailbox_ref_id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
        };
        let draft_count = |state: &str| -> Result<u64, String> {
            transaction
                .query_row(
                    "SELECT COUNT(*) FROM email_drafts
                     WHERE (?1 IS NULL OR mailbox_ref_id = ?1) AND state = ?2",
                    params![mailbox_ref_id, state],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())
        };
        let deleted = thread_count("deleted_at IS NOT NULL")? + draft_count("trashed")?;
        let counts = vec![
            EmailFolderSummary {
                folder: EmailFolder::All,
                count: thread_count("deleted_at IS NULL")?,
            },
            EmailFolderSummary {
                folder: EmailFolder::Inbox,
                count: thread_count(
                    "archived_at IS NULL AND deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = email_threads.id
                                   AND e.direction = 'inbound')",
                )?,
            },
            EmailFolderSummary {
                folder: EmailFolder::Sent,
                count: thread_count(
                    "deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = email_threads.id
                                   AND e.direction = 'outbound')",
                )?,
            },
            EmailFolderSummary {
                folder: EmailFolder::Drafts,
                count: draft_count("draft")?,
            },
            EmailFolderSummary {
                folder: EmailFolder::Archive,
                count: thread_count(
                    "archived_at IS NOT NULL AND deleted_at IS NULL
                     AND EXISTS (SELECT 1 FROM emails e
                                 WHERE e.thread_id = email_threads.id
                                   AND e.direction = 'inbound')",
                )?,
            },
            EmailFolderSummary {
                folder: EmailFolder::Deleted,
                count: deleted,
            },
        ];
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(counts)
    }

    fn get_email_thread_record(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        let connection = self.read_connection()?;
        load_thread(&connection, thread_id)
    }

    fn get_email_thread_messages(
        &self,
        thread_id: &EmailThreadId,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailMessageView>, String> {
        let context = format!("email_thread_messages|thread={}", thread_id.as_str());
        let position = cursor
            .map(|cursor| decode_keyset_single(cursor, &context))
            .transpose()?;
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        load_thread(&transaction, thread_id)?;
        let mut statement = transaction
            .prepare(
                "SELECT id FROM emails
                 WHERE thread_id = ?1
                   AND (?2 IS NULL OR occurred_at < ?2
                     OR (occurred_at = ?2 AND id < ?3))
                 ORDER BY occurred_at DESC, id DESC
                 LIMIT ?4",
            )
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map(
                params![
                    thread_id.as_str(),
                    position.as_ref().map(|(occurred_at, _)| occurred_at),
                    position.as_ref().map(|(_, id)| id.as_str()),
                    u64::from(limit) + 1
                ],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        let messages = ids
            .into_iter()
            .map(|id| load_message(&transaction, &EmailId::new(id)).map(EmailMessage::view))
            .collect::<Result<Vec<_>, String>>()?;
        let page = keyset_page(messages, limit as usize, &context, |message| {
            (vec![message.occurred_at], message.id.as_str())
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn get_email_message(&self, email_id: &EmailId) -> Result<EmailMessage, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let message = load_message(&transaction, email_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(message)
    }

    fn archive_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        update_thread_timestamp(self, thread_id, "archived_at", true)
    }

    fn unarchive_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        update_thread_timestamp(self, thread_id, "archived_at", false)
    }

    fn trash_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        update_thread_timestamp(self, thread_id, "deleted_at", true)
    }

    fn restore_email_thread(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        update_thread_timestamp(self, thread_id, "deleted_at", false)
    }

    fn mark_email_thread_read(&self, thread_id: &EmailThreadId) -> Result<EmailThread, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let thread_exists = transaction
            .query_row(
                "SELECT 1 FROM email_threads WHERE id = ?1",
                [thread_id.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !thread_exists {
            return Err("email thread not found".into());
        }
        transaction
            .execute(
                "UPDATE emails SET delivery_status = 'read'
                 WHERE thread_id = ?1 AND direction = 'inbound' AND delivery_status = 'unread'",
                [thread_id.as_str()],
            )
            .map_err(|error| error.to_string())?;
        // Deliberately no updated_at: the thread list is ordered and paged by
        // it, so opening a message must not reshuffle the list under the reader.
        transaction
            .execute(
                "UPDATE email_threads SET unread_count = 0 WHERE id = ?1",
                [thread_id.as_str()],
            )
            .map_err(|error| error.to_string())?;
        let thread = load_thread(&transaction, thread_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(thread)
    }

    fn find_email_thread_by_internet_message_ids(
        &self,
        mailbox_ref_id: &MailboxRefId,
        internet_message_ids: &[String],
    ) -> Result<Option<EmailThread>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        for internet_message_id in internet_message_ids {
            let internet_message_id = internet_message_id.trim();
            if internet_message_id.is_empty() {
                continue;
            }
            let id = transaction
                .query_row(
                    "SELECT thread.id
                     FROM email_threads thread
                     JOIN emails email ON email.thread_id = thread.id
                     WHERE thread.mailbox_ref_id = ?1 AND email.internet_message_id = ?2
                     ORDER BY email.occurred_at DESC LIMIT 1",
                    params![mailbox_ref_id.as_str(), internet_message_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(id) = id {
                let thread = load_thread(&transaction, &EmailThreadId::new(id))?;
                transaction.commit().map_err(|error| error.to_string())?;
                return Ok(Some(thread));
            }
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(None)
    }

    fn commit_email_action(
        &self,
        input: CommitEmailActionInput,
    ) -> Result<EmailActionResult, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        if input.preparation.creator.creator.id != input.creator_id
            || input.preparation.route.locator.id != input.contact_locator_id
            || input.preparation.mailbox.mailbox_ref_id != input.snapshot.mailbox_ref_id
        {
            return Err("verified email recovery context does not match its local commit".into());
        }
        let timestamp = now();
        let (thread, emails) = persist_snapshot(&transaction, &input.snapshot)?;
        let email = emails
            .into_iter()
            .find(|message| message.provider_message_id == input.verified_provider_message_id)
            .ok_or_else(|| "verified outbound email was missing from snapshot".to_string())?;
        if email.direction != EmailDirection::Outbound {
            return Err("verified email was not outbound".into());
        }
        let touchpoint = Touchpoint {
            id: TouchpointId::new(Uuid::new_v4().to_string()),
            creator_id: input.creator_id.clone(),
            kind: "email".into(),
            message_id: None,
            email_id: Some(email.id.clone()),
            account_relationship_id: None,
            occurred_at: timestamp,
            metadata: serde_json::json!({ "verified": true, "provider": input.preparation.mailbox.provider.as_str() }),
        };
        transaction
            .execute(
                "INSERT INTO touchpoints
                   (id, creator_id, kind, message_id, email_id, account_relationship_id,
                    contact_route_id, occurred_at, metadata_json)
                 VALUES (?1, ?2, ?3, NULL, ?4, NULL, ?5, ?6, ?7)",
                params![
                    touchpoint.id.as_str(),
                    touchpoint.creator_id.as_str(),
                    touchpoint.kind,
                    email.id.as_str(),
                    input.preparation.route.route.id.as_str(),
                    touchpoint.occurred_at,
                    json(&touchpoint.metadata)?
                ],
            )
            .map_err(|error| error.to_string())?;
        let audit_event = AuditEvent {
            id: AuditEventId::new(Uuid::new_v4().to_string()),
            kind: "email_sent".into(),
            creator_id: Some(input.creator_id.clone()),
            account_ref_id: None,
            resolved_target_id: None,
            mailbox_ref_id: Some(input.snapshot.mailbox_ref_id.clone()),
            contact_locator_id: Some(input.contact_locator_id),
            email_id: Some(email.id.clone()),
            payload: serde_json::json!({
                "provider_message_id": input.verified_provider_message_id,
                "provider_thread_key": thread.provider_thread_key,
                "verified": true,
            }),
            created_at: timestamp,
        };
        transaction
            .execute(
                "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id,
                    mailbox_ref_id, contact_locator_id, email_id,
                    contact_route_id, payload_json, created_at)
                 VALUES (?1, ?2, ?3, NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    audit_event.id.as_str(),
                    audit_event.kind,
                    audit_event.creator_id.as_ref().map(CreatorId::as_str),
                    audit_event
                        .mailbox_ref_id
                        .as_ref()
                        .map(MailboxRefId::as_str),
                    audit_event
                        .contact_locator_id
                        .as_ref()
                        .map(crate::ContactLocatorId::as_str),
                    audit_event.email_id.as_ref().map(EmailId::as_str),
                    input.preparation.route.route.id.as_str(),
                    json(&audit_event.payload)?,
                    audit_event.created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        let result = EmailActionResult {
            preparation: input.preparation,
            thread,
            email: email.view(),
            touchpoint,
            audit_event,
            replayed: false,
        };
        let changed = transaction
            .execute(
                "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2 AND state = 'remote_verified'
                   AND request_fingerprint = ?5",
                params![
                    input.idempotency_key.trim(),
                    "email_send",
                    json(&encode_persisted_email_action_result(&result)?)?,
                    timestamp,
                    input.request_fingerprint
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("email idempotency claim was not found".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn commit_manual_email_action(
        &self,
        input: CommitManualEmailActionInput,
    ) -> Result<ManualEmailActionResult, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        if input.snapshot.creator_id.is_some() || input.snapshot.contact_route_id.is_some() {
            return Err("manual email snapshot must not be bound to a Creator".into());
        }
        if input.preparation.mailbox.mailbox_ref_id != input.snapshot.mailbox_ref_id
            || input.preparation.to != input.recipient
        {
            return Err(
                "verified manual email recovery context does not match its local commit".into(),
            );
        }
        let (thread, emails) = persist_snapshot(&transaction, &input.snapshot)?;
        let email = emails
            .into_iter()
            .find(|message| message.provider_message_id == input.verified_provider_message_id)
            .ok_or_else(|| "verified manual email was missing from snapshot".to_string())?;
        if email.direction != EmailDirection::Outbound {
            return Err("verified manual email was not outbound".into());
        }
        let audit_event = AuditEvent {
            id: AuditEventId::new(Uuid::new_v4().to_string()),
            kind: "manual_email_sent".into(),
            creator_id: None,
            account_ref_id: None,
            resolved_target_id: None,
            mailbox_ref_id: Some(input.snapshot.mailbox_ref_id.clone()),
            contact_locator_id: None,
            email_id: Some(email.id.clone()),
            payload: serde_json::json!({
                "provider_message_id": input.verified_provider_message_id,
                "provider_thread_key": thread.provider_thread_key,
                "recipient": input.recipient,
                "verified": true,
            }),
            created_at: timestamp,
        };
        transaction
            .execute(
                "INSERT INTO audit_events
                   (id, kind, creator_id, account_ref_id, resolved_target_id,
                    mailbox_ref_id, contact_locator_id, email_id,
                    payload_json, created_at)
                 VALUES (?1, ?2, NULL, NULL, NULL, ?3, NULL, ?4, ?5, ?6)",
                params![
                    audit_event.id.as_str(),
                    audit_event.kind,
                    audit_event
                        .mailbox_ref_id
                        .as_ref()
                        .map(MailboxRefId::as_str),
                    audit_event.email_id.as_ref().map(EmailId::as_str),
                    json(&audit_event.payload)?,
                    audit_event.created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        let result = ManualEmailActionResult {
            preparation: input.preparation,
            thread,
            email: email.view(),
            audit_event,
            replayed: false,
        };
        let changed = transaction
            .execute(
                "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2 AND state = 'remote_verified'
                   AND request_fingerprint = ?5",
                params![
                    input.idempotency_key.trim(),
                    "manual_email_send",
                    json(&encode_persisted_manual_email_action_result(&result)?)?,
                    timestamp,
                    input.request_fingerprint
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("manual email idempotency claim was not found".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn email_batch_preflight_snapshot(
        &self,
        mailbox_ref_id: &MailboxRefId,
        creator_ids: &[crate::CreatorId],
        template_ids: &[crate::EmailTemplateId],
    ) -> Result<crate::EmailBatchPreflightSnapshot, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mailbox = super::mailboxes::find_mailbox(&transaction, mailbox_ref_id)?
            .map(crate::email::mailbox_detail)
            .transpose()?
            .ok_or_else(|| "mailbox reference not found".to_string())?;
        let mut creators = std::collections::HashMap::new();
        let creator_values = creator_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect::<Vec<_>>();
        if !creator_values.is_empty() {
            for creator_chunk in creator_values.chunks(crate::PREFLIGHT_CREATOR_CHUNK_SIZE) {
                let placeholders = (1..=creator_chunk.len())
                    .map(|index| format!("?{index}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let query = format!(
                    "SELECT creator.id, creator.display_name, creator.archived_at,
                        relationship.stage
                 FROM creators creator
                 JOIN creator_relationships relationship
                   ON relationship.creator_id = creator.id
                 WHERE creator.id IN ({placeholders})"
                );
                let mut statement = transaction
                    .prepare(&query)
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(rusqlite::params_from_iter(creator_chunk.iter()), |row| {
                        let stage = crate::CreatorStage::parse(&row.get::<_, String>(3)?).map_err(
                            |error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    3,
                                    rusqlite::types::Type::Text,
                                    Box::new(std::io::Error::other(error)),
                                )
                            },
                        )?;
                        Ok((
                            crate::CreatorId::new(row.get::<_, String>(0)?),
                            crate::EmailBatchCreatorSnapshot {
                                display_name: row.get(1)?,
                                archived_at: row.get(2)?,
                                stage,
                                confirmed_handles: std::collections::HashSet::new(),
                                email_routes: std::collections::HashMap::new(),
                            },
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                for row in rows {
                    let (id, snapshot) = row.map_err(|error| error.to_string())?;
                    creators.insert(id, snapshot);
                }

                let handle_query = format!(
                    "SELECT link.creator_id, account.id, observation.handle
                 FROM creator_account_links link
                 JOIN platform_accounts account ON account.id = link.platform_account_id
                 JOIN platform_account_observations observation
                   ON observation.platform_account_id = account.id
                 WHERE link.state = 'confirmed'
                   AND link.creator_id IN ({placeholders})
                   AND observation.handle IS NOT NULL
                   AND trim(observation.handle) <> ''
                 ORDER BY observation.observed_at DESC, observation.rowid DESC"
                );
                let mut statement = transaction
                    .prepare(&handle_query)
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(rusqlite::params_from_iter(creator_chunk.iter()), |row| {
                        Ok((
                            crate::CreatorId::new(row.get::<_, String>(0)?),
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                let mut seen_accounts = std::collections::HashSet::new();
                for row in rows {
                    let (creator_id, account_id, handle) =
                        row.map_err(|error| error.to_string())?;
                    if seen_accounts.insert((creator_id.clone(), account_id))
                        && let Some(snapshot) = creators.get_mut(&creator_id)
                    {
                        snapshot.confirmed_handles.insert(handle.trim().to_owned());
                    }
                }

                let route_query = format!(
                    "SELECT route.creator_id, route.id, locator.normalized_value
                 FROM creator_contact_routes route
                 JOIN contact_locators locator ON locator.id = route.locator_id
                 WHERE route.creator_id IN ({placeholders})
                   AND locator.channel = 'email'
                   AND locator.kind = 'email_address'"
                );
                let mut statement = transaction
                    .prepare(&route_query)
                    .map_err(|error| error.to_string())?;
                let rows = statement
                    .query_map(rusqlite::params_from_iter(creator_chunk.iter()), |row| {
                        Ok((
                            crate::CreatorId::new(row.get::<_, String>(0)?),
                            crate::CreatorContactRouteId::new(row.get::<_, String>(1)?),
                            row.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|error| error.to_string())?;
                for row in rows {
                    let (creator_id, route_id, address) = row.map_err(|error| error.to_string())?;
                    if let Some(snapshot) = creators.get_mut(&creator_id) {
                        snapshot.email_routes.insert(route_id, address);
                    }
                }
            }
        }
        let mut templates = std::collections::HashMap::new();
        for template_id in template_ids {
            let template = transaction
                .query_row(
                    "SELECT name, subject_template, text_template, html_template, variables_json,
                            revision, created_at, updated_at
                     FROM email_templates WHERE id = ?1",
                    [template_id.as_str()],
                    |row| {
                        let variables_json = row.get::<_, String>(4)?;
                        Ok(crate::EmailTemplate {
                            id: template_id.clone(),
                            name: row.get(0)?,
                            subject: row.get(1)?,
                            text: row.get(2)?,
                            html: row.get(3)?,
                            variables: serde_json::from_str(&variables_json).map_err(|error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    variables_json.len(),
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            })?,
                            revision: row.get(5)?,
                            created_at: row.get(6)?,
                            updated_at: row.get(7)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some(template) = template {
                templates.insert(template_id.clone(), template);
            }
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(crate::EmailBatchPreflightSnapshot {
            mailbox,
            creators,
            templates,
        })
    }

    fn commit_email_batch_action(
        &self,
        input: crate::EmailBatchCommitInput,
    ) -> Result<crate::EmailBatchActionResult, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let timestamp = super::super::now();
        for verified in &input.verified {
            let item = input
                .request
                .items
                .get(verified.input_index)
                .ok_or_else(|| "verified batch item index is out of range".to_string())?;
            let message = crate::email::provider_message_input(
                verified.message.clone(),
                EmailDirection::Outbound,
                crate::EmailOrigin::ConfirmedSend,
                verified.in_reply_to.clone(),
                verified.references.clone(),
                "sent",
            )?;
            if message.provider_message_id != verified.provider_message_id {
                return Err("verified batch provider message id did not match readback".into());
            }
            let snapshot = EmailSnapshotInput {
                mailbox_ref_id: input.mailbox.mailbox_ref_id.clone(),
                creator_id: verified.creator_id.clone(),
                contact_route_id: verified.contact_route_id.clone(),
                provider_thread_key: format!(
                    "batch:{}:{}",
                    input.idempotency_key.trim(),
                    verified.input_index
                ),
                subject: item
                    .subject
                    .clone()
                    .unwrap_or_else(|| verified.message.subject.clone()),
                messages: vec![message],
            };
            let (thread, emails) = persist_snapshot(&transaction, &snapshot)?;
            let email = emails
                .into_iter()
                .find(|email| email.provider_message_id == verified.provider_message_id)
                .ok_or_else(|| "verified batch email was missing from snapshot".to_string())?;
            if let (Some(creator_id), Some(route_id)) = (
                verified.creator_id.clone(),
                verified.contact_route_id.clone(),
            ) {
                // The route must belong to the Creator; the locator itself is
                // not recorded on the touchpoint, so only the row's existence
                // is consumed here.
                transaction
                    .query_row(
                        "SELECT locator_id FROM creator_contact_routes WHERE id = ?1 AND creator_id = ?2",
                        rusqlite::params![route_id.as_str(), creator_id.as_str()],
                        |row| row.get::<_, String>(0),
                    )
                    .map_err(|error| error.to_string())?;
                let touchpoint = Touchpoint {
                    id: TouchpointId::new(Uuid::new_v4().to_string()),
                    creator_id,
                    kind: "email".into(),
                    message_id: None,
                    email_id: Some(email.id.clone()),
                    account_relationship_id: None,
                    occurred_at: timestamp,
                    metadata: serde_json::json!({
                        "verified": true,
                        "provider": input.mailbox.provider.as_str(),
                        "batch_input_index": verified.input_index,
                    }),
                };
                transaction
                    .execute(
                        "INSERT INTO touchpoints
                           (id, creator_id, kind, message_id, email_id, account_relationship_id,
                            contact_route_id, occurred_at, metadata_json)
                         VALUES (?1, ?2, ?3, NULL, ?4, NULL, ?5, ?6, ?7)",
                        rusqlite::params![
                            touchpoint.id.as_str(),
                            touchpoint.creator_id.as_str(),
                            touchpoint.kind,
                            email.id.as_str(),
                            route_id.as_str(),
                            touchpoint.occurred_at,
                            json(&touchpoint.metadata)?,
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
            let audit_event = AuditEvent {
                id: AuditEventId::new(Uuid::new_v4().to_string()),
                kind: "email_batch_item_sent".into(),
                creator_id: verified.creator_id.clone(),
                account_ref_id: None,
                resolved_target_id: None,
                mailbox_ref_id: Some(input.mailbox.mailbox_ref_id.clone()),
                contact_locator_id: None,
                email_id: Some(email.id.clone()),
                payload: serde_json::json!({
                    "provider_message_id": verified.provider_message_id,
                    "batch_input_index": verified.input_index,
                    "provider_thread_key": thread.provider_thread_key,
                    "verified": true,
                }),
                created_at: timestamp,
            };
            transaction
                .execute(
                    "INSERT INTO audit_events
                       (id, kind, creator_id, account_ref_id, resolved_target_id,
                        mailbox_ref_id, contact_locator_id, email_id,
                        payload_json, created_at)
                     VALUES (?1, ?2, ?3, NULL, NULL, ?4, NULL, ?5, ?6, ?7)",
                    rusqlite::params![
                        audit_event.id.as_str(),
                        audit_event.kind,
                        audit_event.creator_id.as_ref().map(CreatorId::as_str),
                        input.mailbox.mailbox_ref_id.as_str(),
                        email.id.as_str(),
                        json(&audit_event.payload)?,
                        audit_event.created_at,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
        let mut result = input.result;
        result.replayed = false;
        let changed = transaction
            .execute(
                "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = 'email_batch_send'
                   AND state = 'remote_accepted'
                   AND request_fingerprint = ?2",
                rusqlite::params![
                    input.idempotency_key.trim(),
                    input.logical_fingerprint,
                    json(&result)?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("email batch idempotency claim was not found".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }
}

fn email_thread_activity(
    connection: &rusqlite::Connection,
    thread_id: &EmailThreadId,
) -> Result<(Option<EmailId>, u64, bool), String> {
    connection
        .query_row(
            "SELECT
               (SELECT id FROM emails WHERE thread_id = ?1
                ORDER BY occurred_at DESC, id DESC LIMIT 1),
               COUNT(*),
               COALESCE(MAX(CASE WHEN direction = 'outbound' THEN 1 ELSE 0 END), 0)
             FROM emails WHERE thread_id = ?1",
            [thread_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?.map(EmailId::new),
                    row.get(1)?,
                    row.get::<_, i64>(2)? != 0,
                ))
            },
        )
        .map_err(|error| error.to_string())
}
