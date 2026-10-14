use rusqlite::{Connection, params};

use crate::{
    CreatorId, EmailDirection, EmailId, EmailMessage, EmailRecipientKind, EmailThread,
    EmailThreadFolder, EmailThreadId, MailboxRefId,
};

use super::super::{SqliteRepository, now};

pub(super) fn load_thread(
    connection: &Connection,
    id: &EmailThreadId,
) -> Result<EmailThread, String> {
    connection
        .query_row(
            "SELECT mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                    subject, unread_count, active_folder, archived_at, deleted_at,
                    EXISTS(SELECT 1 FROM emails
                           WHERE emails.thread_id = email_threads.id
                             AND emails.direction = 'inbound'),
                    created_at, updated_at
             FROM email_threads WHERE id = ?1",
            [id.as_str()],
            |row| {
                let active_folder = row.get::<_, String>(6)?;
                let archived_at = row.get::<_, Option<u64>>(7)?;
                let deleted_at = row.get::<_, Option<u64>>(8)?;
                let has_inbound = row.get::<_, bool>(9)?;
                let folder = if deleted_at.is_some() {
                    EmailThreadFolder::Deleted
                } else if archived_at.is_some() && has_inbound {
                    EmailThreadFolder::Archive
                } else {
                    EmailThreadFolder::parse(&active_folder).map_err(to_sql_conversion_error)?
                };
                Ok(EmailThread {
                    id: id.clone(),
                    mailbox_ref_id: MailboxRefId::new(row.get::<_, String>(0)?),
                    creator_id: row.get::<_, Option<String>>(1)?.map(CreatorId::new),
                    contact_route_id: row
                        .get::<_, Option<String>>(2)?
                        .map(crate::CreatorContactRouteId::new),
                    provider_thread_key: row.get(3)?,
                    subject: row.get(4)?,
                    unread_count: row.get(5)?,
                    folder,
                    created_at: row.get(10)?,
                    updated_at: row.get(11)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

pub(super) fn update_thread_timestamp(
    repository: &SqliteRepository,
    thread_id: &EmailThreadId,
    field: &str,
    present: bool,
) -> Result<EmailThread, String> {
    let connection = repository.lock()?;
    let timestamp = now();
    load_thread(&connection, thread_id)?;
    if field == "archived_at" && !thread_has_inbound(&connection, thread_id)? {
        return Err("outbound-only email thread cannot be archived".into());
    }
    let sql = match (field, present) {
        ("archived_at", true) => {
            "UPDATE email_threads SET archived_at = ?2, updated_at = ?2 WHERE id = ?1"
        }
        ("archived_at", false) => {
            "UPDATE email_threads
             SET archived_at = NULL, active_folder = 'inbox', updated_at = ?2
             WHERE id = ?1"
        }
        ("deleted_at", true) => {
            "UPDATE email_threads SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1"
        }
        ("deleted_at", false) => {
            "UPDATE email_threads SET deleted_at = NULL, updated_at = ?2 WHERE id = ?1"
        }
        _ => return Err("unsupported email thread disposition transition".into()),
    };
    let changed = connection
        .execute(sql, params![thread_id.as_str(), timestamp])
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("email thread not found".into());
    }
    load_thread(&connection, thread_id)
}

pub(super) fn thread_has_inbound(
    connection: &Connection,
    thread_id: &EmailThreadId,
) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM emails
             WHERE thread_id = ?1 AND direction = 'inbound')",
            [thread_id.as_str()],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}

fn to_sql_conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

pub(super) fn load_messages(
    connection: &Connection,
    thread_id: &EmailThreadId,
) -> Result<Vec<EmailMessage>, String> {
    let mut statement = connection
        .prepare("SELECT id FROM emails WHERE thread_id = ?1 ORDER BY occurred_at, id")
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map([thread_id.as_str()], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    ids.into_iter()
        .map(|id| load_message(connection, &EmailId::new(id)))
        .collect()
}

pub(super) fn load_message(connection: &Connection, id: &EmailId) -> Result<EmailMessage, String> {
    let row = connection
        .query_row(
            "SELECT thread_id, provider_message_id, internet_message_id, direction,
                    subject, body_text, body_html, in_reply_to, references_json,
                    delivery_status, headers_json, occurred_at
             FROM emails WHERE id = ?1",
            [id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, u64>(11)?,
                ))
            },
        )
        .map_err(|error| error.to_string())?;
    let recipient_rows = load_recipients(connection, id)?;
    Ok(EmailMessage {
        id: id.clone(),
        thread_id: EmailThreadId::new(row.0),
        provider_message_id: row.1,
        internet_message_id: row.2,
        direction: EmailDirection::parse(&row.3)?,
        from: recipients(&recipient_rows, EmailRecipientKind::From)
            .into_iter()
            .next()
            .unwrap_or_default(),
        to: recipients(&recipient_rows, EmailRecipientKind::To),
        cc: recipients(&recipient_rows, EmailRecipientKind::Cc),
        bcc: recipients(&recipient_rows, EmailRecipientKind::Bcc),
        received_for: recipients(&recipient_rows, EmailRecipientKind::ReceivedFor),
        reply_to: recipients(&recipient_rows, EmailRecipientKind::ReplyTo),
        in_reply_to: row.7,
        references: serde_json::from_str(&row.8).map_err(|error| error.to_string())?,
        subject: row.4,
        text: row.5,
        html: row.6,
        delivery_status: row.9,
        headers: serde_json::from_str(&row.10).map_err(|error| error.to_string())?,
        occurred_at: row.11,
    })
}

fn load_recipients(
    connection: &Connection,
    email_id: &EmailId,
) -> Result<Vec<(EmailRecipientKind, String)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT recipient_kind, address FROM email_recipients
             WHERE email_id = ?1 ORDER BY recipient_kind, address",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([email_id.as_str()], |row| {
            let kind = row.get::<_, String>(0)?;
            Ok((kind, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .map(|row| {
            let (kind, address) = row.map_err(|error| error.to_string())?;
            Ok((EmailRecipientKind::parse(&kind)?, address))
        })
        .collect()
}

fn recipients(
    recipients: &[(EmailRecipientKind, String)],
    kind: EmailRecipientKind,
) -> Vec<String> {
    recipients
        .iter()
        .filter(|(candidate, _)| *candidate == kind)
        .map(|(_, address)| address.clone())
        .collect()
}
