//! The reply-triage ledger: one agent verdict about one inbound message.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::models::{
    AcknowledgeReplyTriageInput, ConversationId, CreatorId, EmailId, EmailThreadId,
    ListReplyTriageInput, MessageId, Page, REPLY_TRIAGE_PAGE_LIMITS, RecordReplyTriageInput,
    ReplyChannel, ReplyIntent, ReplyTriage, ReplyTriageBrief, ReplyTriageId, ReplyTriageView,
};
use crate::repository::ReplyTriageRepository;

use super::{SqliteRepository, keyset_page, now};

const TRIAGE_COLUMNS: &str = "id, channel, email_id, message_id, intent, summary, \
     suggested_reply, created_at, acknowledged_at";

/// A verdict's excerpt is bounded: the list shows why it was judged that
/// way, not the whole message.
const EXCERPT_CHARS: usize = 160;

impl ReplyTriageRepository for SqliteRepository {
    fn record_reply_triage(&self, input: RecordReplyTriageInput) -> Result<ReplyTriage, String> {
        let summary = input.summary.trim().to_string();
        if summary.is_empty() {
            return Err("reply triage summary is required".into());
        }
        let suggested_reply = input
            .suggested_reply
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let channel = match (&input.email_id, &input.message_id) {
            (Some(_), None) => ReplyChannel::Email,
            (None, Some(_)) => ReplyChannel::Dm,
            _ => {
                return Err(
                    "a reply triage names exactly one message: set email_id or message_id".into(),
                );
            }
        };

        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;

        // The message must exist and must be inbound: a verdict about our
        // own outbound message is meaningless, and one about a message that
        // is not there would be unattributable.
        match channel {
            ReplyChannel::Email => {
                let direction = transaction
                    .query_row(
                        "SELECT direction FROM emails WHERE id = ?1",
                        [input.email_id.as_ref().expect("email id").as_str()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "email not found".to_string())?;
                if direction != "inbound" {
                    return Err("only an inbound email can be triaged".into());
                }
            }
            ReplyChannel::Dm => {
                let direction = transaction
                    .query_row(
                        "SELECT direction FROM messages WHERE id = ?1",
                        [input.message_id.as_ref().expect("message id").as_str()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "message not found".to_string())?;
                if direction != "inbound" {
                    return Err("only an inbound message can be triaged".into());
                }
            }
        }

        // Re-judging a message replaces the reading rather than stacking a
        // second one: a message has one current meaning. The replacement
        // also clears any acknowledgement, because what the human dismissed
        // is not what is being said now.
        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM reply_triage
                 WHERE (email_id IS NOT NULL AND email_id = ?1)
                    OR (message_id IS NOT NULL AND message_id = ?2)",
                params![
                    input.email_id.as_ref().map(|id| id.as_str()),
                    input.message_id.as_ref().map(|id| id.as_str()),
                ],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        let timestamp = now();
        let id = existing.unwrap_or_else(|| Uuid::new_v4().to_string());
        transaction
            .execute(
                "INSERT INTO reply_triage
                   (id, channel, email_id, message_id, intent, summary, suggested_reply,
                    created_at, acknowledged_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)
                 ON CONFLICT(id) DO UPDATE SET
                   intent = excluded.intent,
                   summary = excluded.summary,
                   suggested_reply = excluded.suggested_reply,
                   created_at = excluded.created_at,
                   acknowledged_at = NULL",
                params![
                    id,
                    channel.as_str(),
                    input.email_id.as_ref().map(|value| value.as_str()),
                    input.message_id.as_ref().map(|value| value.as_str()),
                    input.intent.as_str(),
                    summary,
                    suggested_reply,
                    timestamp,
                ],
            )
            .map_err(|error| error.to_string())?;
        let stored = load_triage(&transaction, &id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(stored)
    }

    fn acknowledge_reply_triage(
        &self,
        input: AcknowledgeReplyTriageInput,
    ) -> Result<ReplyTriage, String> {
        let connection = self.lock()?;
        let existing = load_triage(&connection, input.triage_id.as_str())?;
        if existing.acknowledged_at.is_some() {
            return Ok(existing);
        }
        connection
            .execute(
                "UPDATE reply_triage SET acknowledged_at = ?1 WHERE id = ?2",
                params![now(), input.triage_id.as_str()],
            )
            .map_err(|error| error.to_string())?;
        load_triage(&connection, input.triage_id.as_str())
    }

    fn list_reply_triage(
        &self,
        input: ListReplyTriageInput,
    ) -> Result<Page<ReplyTriageView>, String> {
        let limit = REPLY_TRIAGE_PAGE_LIMITS.effective(input.limit) as usize;
        let context = format!("reply_triage|acknowledged={}", input.include_acknowledged);
        let position = input
            .cursor
            .as_deref()
            .map(|cursor| super::decode_keyset_single(cursor, &context))
            .transpose()?;
        let (after_created_at, after_id) = match &position {
            Some((created_at, id)) => (Some(*created_at), Some(id.clone())),
            None => (None, None),
        };

        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare(&format!(
                "SELECT {TRIAGE_COLUMNS} FROM reply_triage
                 WHERE (?1 OR acknowledged_at IS NULL)
                   AND (?2 IS NULL OR created_at < ?2 OR (created_at = ?2 AND id < ?3))
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?4"
            ))
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![
                    input.include_acknowledged,
                    after_created_at,
                    after_id,
                    (limit + 1) as i64
                ],
                triage_from_row,
            )
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);

        let mut views = Vec::with_capacity(rows.len());
        for triage in rows {
            views.push(view_for(&transaction, triage)?);
        }
        let page = keyset_page(views, limit, &context, |view| {
            (vec![view.triage.created_at], view.triage.id.as_str())
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn latest_triage_for_conversation(
        &self,
        conversation_id: &ConversationId,
    ) -> Result<Option<ReplyTriageBrief>, String> {
        let connection = self.read_connection()?;
        conversation_triage(&connection, conversation_id.as_str())
    }

    fn latest_triage_for_email_thread(
        &self,
        thread_id: &EmailThreadId,
    ) -> Result<Option<ReplyTriageBrief>, String> {
        let connection = self.read_connection()?;
        email_thread_triage(&connection, thread_id.as_str())
    }

    fn pending_email_triage(
        &self,
        mailbox_ref_id: &crate::MailboxRefId,
        limit: u32,
    ) -> Result<crate::PendingReplyTriageDebt, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let debt = pending_email_triage_from_connection(&transaction, mailbox_ref_id, limit)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(debt)
    }

    fn pending_conversation_triage(
        &self,
        conversation_id: &ConversationId,
        limit: u32,
    ) -> Result<crate::PendingReplyTriageDebt, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        const UNJUDGED: &str = "FROM messages message
             LEFT JOIN reply_triage triage ON triage.message_id = message.id
             WHERE message.conversation_id = ?1
               AND message.direction = 'inbound'
               AND triage.id IS NULL";
        let total = transaction
            .query_row(
                &format!("SELECT COUNT(*) {UNJUDGED}"),
                params![conversation_id.as_str()],
                |row| row.get::<_, u32>(0),
            )
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare(&format!(
                "SELECT message.id, message.body, message.sent_at
                 {UNJUDGED}
                 ORDER BY message.sent_at DESC, message.rowid DESC
                 LIMIT ?2"
            ))
            .map_err(|error| error.to_string())?;
        let items = statement
            .query_map(params![conversation_id.as_str(), limit], |row| {
                let message_id: String = row.get(0)?;
                let body: Option<String> = row.get(1)?;
                let sent_at: u64 = row.get(2)?;
                Ok(crate::PendingReplyTriage {
                    channel: crate::ReplyChannel::Dm,
                    email_id: None,
                    message_id: Some(crate::MessageId::new(message_id)),
                    // 会话视图自己就带着 creator_id,这里不重复解析一遍。
                    creator_id: None,
                    from: String::new(),
                    subject: None,
                    excerpt: excerpt(body.as_deref().unwrap_or_default()),
                    occurred_at: sent_at,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<RowResult<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        let debt = crate::PendingReplyTriageDebt { items, total };
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(debt)
    }
}

/// Reads the pending inbound-email ledger through the caller's connection or
/// transaction. Composite readbacks use this helper so their sync states and
/// triage debt are guaranteed to come from one committed SQLite snapshot.
pub(super) fn pending_email_triage_from_connection(
    connection: &Connection,
    mailbox_ref_id: &crate::MailboxRefId,
    limit: u32,
) -> Result<crate::PendingReplyTriageDebt, String> {
    // 反连接走 reply_triage_one_per_email 那个唯一索引:没有判断行的
    // 入站邮件就是欠着的。
    const UNJUDGED: &str = "FROM emails email
         JOIN email_threads thread ON thread.id = email.thread_id
         LEFT JOIN reply_triage triage ON triage.email_id = email.id
         WHERE thread.mailbox_ref_id = ?1
           AND email.direction = 'inbound'
           AND triage.id IS NULL";
    let total = connection
        .query_row(
            &format!("SELECT COUNT(*) {UNJUDGED}"),
            params![mailbox_ref_id.as_str()],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|error| error.to_string())?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT email.id, thread.creator_id, email.subject, email.body_text,
                    email.occurred_at,
                    (SELECT address FROM email_recipients
                      WHERE email_id = email.id AND recipient_kind = 'from'
                      LIMIT 1)
             {UNJUDGED}
             ORDER BY email.occurred_at DESC, email.rowid DESC
             LIMIT ?2"
        ))
        .map_err(|error| error.to_string())?;
    let items = statement
        .query_map(params![mailbox_ref_id.as_str(), limit], |row| {
            let email_id: String = row.get(0)?;
            let creator_id: Option<String> = row.get(1)?;
            let subject: String = row.get(2)?;
            let body: Option<String> = row.get(3)?;
            let occurred_at: u64 = row.get(4)?;
            let from: Option<String> = row.get(5)?;
            Ok(crate::PendingReplyTriage {
                channel: crate::ReplyChannel::Email,
                email_id: Some(crate::EmailId::new(email_id)),
                message_id: None,
                creator_id: creator_id.map(crate::CreatorId::new),
                from: from.unwrap_or_default(),
                subject: (!subject.trim().is_empty()).then_some(subject),
                excerpt: excerpt(body.as_deref().unwrap_or_default()),
                occurred_at,
            })
        })
        .map_err(|error| error.to_string())?
        .collect::<RowResult<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(crate::PendingReplyTriageDebt { items, total })
}

/// The newest verdict on any inbound message of this conversation.
pub(super) fn conversation_triage(
    connection: &Connection,
    conversation_id: &str,
) -> Result<Option<ReplyTriageBrief>, String> {
    connection
        .query_row(
            "SELECT t.intent, t.summary, t.suggested_reply, t.created_at, t.acknowledged_at
             FROM reply_triage t
             JOIN messages m ON m.id = t.message_id
             WHERE m.conversation_id = ?1
             ORDER BY t.created_at DESC, t.id DESC
             LIMIT 1",
            [conversation_id],
            brief_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .transpose()
}

/// The newest verdict on any inbound email of this thread.
pub(super) fn email_thread_triage(
    connection: &Connection,
    thread_id: &str,
) -> Result<Option<ReplyTriageBrief>, String> {
    connection
        .query_row(
            "SELECT t.intent, t.summary, t.suggested_reply, t.created_at, t.acknowledged_at
             FROM reply_triage t
             JOIN emails e ON e.id = t.email_id
             WHERE e.thread_id = ?1
             ORDER BY t.created_at DESC, t.id DESC
             LIMIT 1",
            [thread_id],
            brief_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .transpose()
}

type RowResult<T> = Result<T, rusqlite::Error>;

fn brief_from_row(row: &rusqlite::Row<'_>) -> RowResult<Result<ReplyTriageBrief, String>> {
    let intent = row.get::<_, String>(0)?;
    let summary = row.get::<_, String>(1)?;
    let suggested_reply = row.get::<_, Option<String>>(2)?;
    let created_at = row.get::<_, u64>(3)?;
    let acknowledged_at = row.get::<_, Option<u64>>(4)?;
    Ok(ReplyIntent::parse(&intent).map(|intent| ReplyTriageBrief {
        intent,
        summary,
        suggested_reply,
        created_at,
        acknowledged: acknowledged_at.is_some(),
    }))
}

fn triage_from_row(row: &rusqlite::Row<'_>) -> RowResult<Result<ReplyTriage, String>> {
    let channel = row.get::<_, String>(1)?;
    let intent = row.get::<_, String>(4)?;
    Ok((|| {
        Ok(ReplyTriage {
            id: ReplyTriageId::new(row.get::<_, String>(0).map_err(|e| e.to_string())?),
            channel: ReplyChannel::parse(&channel)?,
            email_id: row
                .get::<_, Option<String>>(2)
                .map_err(|e| e.to_string())?
                .map(EmailId::new),
            message_id: row
                .get::<_, Option<String>>(3)
                .map_err(|e| e.to_string())?
                .map(MessageId::new),
            intent: ReplyIntent::parse(&intent)?,
            summary: row.get(5).map_err(|e| e.to_string())?,
            suggested_reply: row.get(6).map_err(|e| e.to_string())?,
            created_at: row.get(7).map_err(|e| e.to_string())?,
            acknowledged_at: row.get(8).map_err(|e| e.to_string())?,
        })
    })())
}

fn load_triage(connection: &Connection, id: &str) -> Result<ReplyTriage, String> {
    connection
        .query_row(
            &format!("SELECT {TRIAGE_COLUMNS} FROM reply_triage WHERE id = ?1"),
            [id],
            triage_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "reply triage not found".to_string())?
}

/// Resolve the context a verdict is rendered with. Nothing here is stored
/// on the row: a thread that gets re-linked to a different creator must
/// not leave stale names behind in judgments about it.
fn view_for(connection: &Connection, triage: ReplyTriage) -> Result<ReplyTriageView, String> {
    let mut view = ReplyTriageView {
        creator_id: None,
        creator_name: None,
        email_thread_id: None,
        conversation_id: None,
        excerpt: String::new(),
        triage,
    };
    match view.triage.channel {
        ReplyChannel::Email => {
            let email_id = view
                .triage
                .email_id
                .as_ref()
                .ok_or_else(|| "stored email triage has no email".to_string())?;
            let row = connection
                .query_row(
                    "SELECT e.thread_id, e.body_text, t.creator_id, c.display_name
                     FROM emails e
                     JOIN email_threads t ON t.id = e.thread_id
                     LEFT JOIN creators c ON c.id = t.creator_id
                     WHERE e.id = ?1",
                    [email_id.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some((thread_id, body, creator_id, creator_name)) = row {
                view.email_thread_id = Some(EmailThreadId::new(thread_id));
                view.excerpt = excerpt(body.as_deref().unwrap_or(""));
                view.creator_id = creator_id.map(CreatorId::new);
                view.creator_name = creator_name;
            }
        }
        ReplyChannel::Dm => {
            let message_id = view
                .triage
                .message_id
                .as_ref()
                .ok_or_else(|| "stored dm triage has no message".to_string())?;
            let row = connection
                .query_row(
                    // A conversation resolves to a creator through its
                    // contact route, exactly as the inbox list does; the
                    // resolved target only names the peer account.
                    "SELECT m.conversation_id, m.body, route.creator_id, c.display_name
                     FROM messages m
                     LEFT JOIN creator_contact_routes route ON route.id = (
                         SELECT link.contact_route_id FROM conversation_contact_routes link
                         WHERE link.conversation_id = m.conversation_id
                         ORDER BY link.created_at ASC, link.contact_route_id ASC LIMIT 1)
                     LEFT JOIN creators c ON c.id = route.creator_id
                     WHERE m.id = ?1",
                    [message_id.as_str()],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(|error| error.to_string())?;
            if let Some((conversation_id, body, creator_id, creator_name)) = row {
                view.conversation_id = Some(ConversationId::new(conversation_id));
                view.excerpt = excerpt(&body);
                view.creator_id = creator_id.map(CreatorId::new);
                view.creator_name = creator_name;
            }
        }
    }
    Ok(view)
}

fn excerpt(body: &str) -> String {
    let flattened = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= EXCERPT_CHARS {
        return flattened;
    }
    let mut cut: String = flattened.chars().take(EXCERPT_CHARS).collect();
    cut.push('…');
    cut
}
