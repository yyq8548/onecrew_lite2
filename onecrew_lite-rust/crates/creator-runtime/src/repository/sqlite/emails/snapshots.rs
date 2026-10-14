use rusqlite::{OptionalExtension, Transaction, params};
use uuid::Uuid;

use crate::{
    CreatorId, EmailDirection, EmailId, EmailMessage, EmailMessageInput, EmailRecipientKind,
    EmailSnapshotInput, EmailThread, EmailThreadFolder, EmailThreadId,
};

use super::{
    super::{json, now},
    queries::{load_messages, load_thread},
};

pub(in super::super) fn persist_snapshot(
    transaction: &Transaction<'_>,
    input: &EmailSnapshotInput,
) -> Result<(EmailThread, Vec<EmailMessage>), String> {
    let provider_thread_key = input.provider_thread_key.trim();
    if provider_thread_key.is_empty() {
        return Err("email thread key is required".into());
    }
    validate_snapshot_owner(transaction, input)?;
    let timestamp = now();
    let existing = transaction
        .query_row(
            "SELECT id, creator_id, contact_route_id, active_folder, created_at
             FROM email_threads
             WHERE mailbox_ref_id = ?1 AND provider_thread_key = ?2",
            params![input.mailbox_ref_id.as_str(), provider_thread_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, u64>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((_, existing_creator_id, existing_contact_route_id, _, _)) = existing.as_ref() {
        validate_existing_thread_owner(
            existing_creator_id.as_deref(),
            existing_contact_route_id.as_deref(),
            input,
        )?;
    }
    let incoming_inbound = input
        .messages
        .iter()
        .any(|message| message.direction == EmailDirection::Inbound);
    let (thread_id, active_folder, created_at) = existing
        .map(|(id, _, _, active_folder, created_at)| {
            (
                EmailThreadId::new(id),
                if incoming_inbound {
                    EmailThreadFolder::Inbox.as_str().to_owned()
                } else {
                    active_folder
                },
                created_at,
            )
        })
        .unwrap_or_else(|| {
            (
                EmailThreadId::new(Uuid::new_v4().to_string()),
                if incoming_inbound {
                    EmailThreadFolder::Inbox.as_str().to_owned()
                } else {
                    EmailThreadFolder::Sent.as_str().to_owned()
                },
                timestamp,
            )
        });
    transaction
        .execute(
            "INSERT INTO email_threads
               (id, mailbox_ref_id, creator_id, contact_route_id, provider_thread_key,
                subject, unread_count, active_folder, archived_at, deleted_at,
                created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, ?9, ?10)
             ON CONFLICT(mailbox_ref_id, provider_thread_key) DO UPDATE SET
               creator_id = excluded.creator_id,
               contact_route_id = excluded.contact_route_id,
               subject = excluded.subject,
               unread_count = excluded.unread_count, active_folder = excluded.active_folder,
               updated_at = excluded.updated_at",
            params![
                thread_id.as_str(),
                input.mailbox_ref_id.as_str(),
                input.creator_id.as_ref().map(CreatorId::as_str),
                input
                    .contact_route_id
                    .as_ref()
                    .map(crate::CreatorContactRouteId::as_str),
                provider_thread_key,
                input.subject.trim(),
                0_u32,
                active_folder,
                created_at,
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    for message in &input.messages {
        persist_message(transaction, &input.mailbox_ref_id, &thread_id, message)?;
    }
    let unread_count = transaction
        .query_row(
            "SELECT COUNT(*) FROM emails
             WHERE thread_id = ?1 AND direction = 'inbound' AND delivery_status = 'unread'",
            [thread_id.as_str()],
            |row| row.get::<_, u32>(0),
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE email_threads SET unread_count = ?2 WHERE id = ?1",
            params![thread_id.as_str(), unread_count],
        )
        .map_err(|error| error.to_string())?;
    Ok((
        load_thread(transaction, &thread_id)?,
        load_messages(transaction, &thread_id)?,
    ))
}

fn validate_snapshot_owner(
    transaction: &Transaction<'_>,
    input: &EmailSnapshotInput,
) -> Result<(), String> {
    match (&input.creator_id, &input.contact_route_id) {
        (None, None) => Ok(()),
        (Some(creator_id), Some(contact_route_id)) => {
            let route_creator_id = transaction
                .query_row(
                    "SELECT creator_id FROM creator_contact_routes WHERE id = ?1",
                    [contact_route_id.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "email snapshot contact route was not found".to_string())?;
            if route_creator_id != creator_id.as_str() {
                return Err("email snapshot contact route belongs to a different Creator".into());
            }
            Ok(())
        }
        _ => {
            Err("email thread ownership requires both Creator and contact route or neither".into())
        }
    }
}

fn validate_existing_thread_owner(
    existing_creator_id: Option<&str>,
    existing_contact_route_id: Option<&str>,
    input: &EmailSnapshotInput,
) -> Result<(), String> {
    let incoming_creator_id = input.creator_id.as_ref().map(CreatorId::as_str);
    let incoming_contact_route_id = input
        .contact_route_id
        .as_ref()
        .map(crate::CreatorContactRouteId::as_str);
    match (
        existing_creator_id,
        existing_contact_route_id,
        incoming_creator_id,
        incoming_contact_route_id,
    ) {
        (None, None, None, None) => Ok(()),
        (None, None, Some(_), Some(_)) => Ok(()),
        (
            Some(existing_creator),
            Some(existing_route),
            Some(incoming_creator),
            Some(incoming_route),
        ) if existing_creator == incoming_creator && existing_route == incoming_route => Ok(()),
        (Some(_), Some(_), None, None) => {
            Err("unowned email snapshot cannot reuse an owned thread".into())
        }
        _ => Err("email thread cannot be rebound to a different Creator or contact route".into()),
    }
}

fn persist_message(
    transaction: &Transaction<'_>,
    mailbox_ref_id: &crate::MailboxRefId,
    thread_id: &EmailThreadId,
    input: &EmailMessageInput,
) -> Result<(), String> {
    if input.provider_message_id.trim().is_empty() {
        return Err("provider message id is required".into());
    }
    let existing = transaction
        .query_row(
            "SELECT id FROM emails WHERE thread_id = ?1 AND provider_message_id = ?2",
            params![thread_id.as_str(), input.provider_message_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let id = existing
        .map(EmailId::new)
        .unwrap_or_else(|| EmailId::new(Uuid::new_v4().to_string()));
    transaction
        .execute(
            "INSERT INTO emails
               (id, thread_id, provider_message_id, internet_message_id, direction,
                subject, body_text, body_html, in_reply_to, references_json,
                delivery_status, headers_json, occurred_at, origin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(thread_id, provider_message_id) DO UPDATE SET
               internet_message_id = excluded.internet_message_id,
               direction = excluded.direction, subject = excluded.subject,
               body_text = excluded.body_text, body_html = excluded.body_html,
               in_reply_to = excluded.in_reply_to, references_json = excluded.references_json,
               -- Read is local attention state the provider does not know about,
               -- so re-syncing the same inbound message must not mark it unread
               -- again. Every other transition still comes from the provider.
               delivery_status = CASE
                 WHEN emails.delivery_status = 'read' AND excluded.delivery_status = 'unread'
                 THEN emails.delivery_status
                 ELSE excluded.delivery_status
               END,
               headers_json = excluded.headers_json,
               occurred_at = excluded.occurred_at,
               -- Provenance is strongest-wins: a locally confirmed send is
               -- never downgraded by a later provider import of the same
               -- message.
               origin = CASE
                 WHEN emails.origin = 'confirmed_send' THEN emails.origin
                 ELSE excluded.origin
               END",
            params![
                id.as_str(),
                thread_id.as_str(),
                input.provider_message_id,
                input.internet_message_id,
                input.direction.as_str(),
                input.subject,
                input.text,
                input.html,
                input.in_reply_to,
                json(&input.references)?,
                input.delivery_status,
                json(&input.headers)?,
                input.occurred_at,
                input.origin.as_str()
            ],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "DELETE FROM email_recipients WHERE email_id = ?1",
            [id.as_str()],
        )
        .map_err(|error| error.to_string())?;
    for (kind, values) in [
        (EmailRecipientKind::From, vec![input.from.clone()]),
        (EmailRecipientKind::To, input.to.clone()),
        (EmailRecipientKind::Cc, input.cc.clone()),
        (EmailRecipientKind::Bcc, input.bcc.clone()),
        (EmailRecipientKind::ReceivedFor, input.received_for.clone()),
        (EmailRecipientKind::ReplyTo, input.reply_to.clone()),
    ] {
        for address in values {
            transaction
                .execute(
                    "INSERT INTO email_recipients (email_id, recipient_kind, address, locator_id)
                     VALUES (?1, ?2, ?3, NULL)",
                    params![id.as_str(), kind.as_str(), address],
                )
                .map_err(|error| error.to_string())?;
        }
    }
    let timestamp = now();
    transaction
        .execute(
            "DELETE FROM email_attachments WHERE email_id = ?1",
            [id.as_str()],
        )
        .map_err(|error| error.to_string())?;
    for attachment in &input.attachments {
        transaction
            .execute(
                "INSERT INTO email_attachments
                   (id, email_id, provider_attachment_id, filename, content_type, content_id,
                    content_disposition, size_bytes, direction, available, created_at,
                    updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![
                    Uuid::new_v4().to_string(),
                    id.as_str(),
                    attachment.provider_attachment_id,
                    attachment.filename,
                    attachment.content_type,
                    attachment.content_id,
                    attachment.content_disposition,
                    attachment.size_bytes,
                    input.direction.as_str(),
                    attachment.available,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
    }
    // The mailbox-scoped identity map is what lets a later sync skip this
    // message without a detail request, even if its thread assignment
    // changes. Detail completeness is monotonic.
    transaction
        .execute(
            "INSERT INTO email_provider_identities
               (mailbox_ref_id, provider_message_id, email_id, direction, detail_complete,
                updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(mailbox_ref_id, provider_message_id) DO UPDATE SET
               email_id = excluded.email_id,
               direction = excluded.direction,
               detail_complete = 1,
               updated_at = excluded.updated_at",
            params![
                mailbox_ref_id.as_str(),
                input.provider_message_id,
                id.as_str(),
                input.direction.as_str(),
                timestamp
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
