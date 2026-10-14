use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};

use crate::{
    CreatorId, EmailDraft, EmailDraftId, EmailDraftState, EmailId, EmailTemplateId, MailboxRefId,
};

use super::revision::conversion_error;

pub(super) fn load_draft(connection: &Connection, id: &EmailDraftId) -> Result<EmailDraft, String> {
    connection
        .query_row(
            "SELECT mailbox_ref_id, to_address, creator_id, contact_route_id, reply_to_email_id,
                    subject, body_text, body_html, template_id, template_variables_json,
                    state, sending_idempotency_key, revision, created_at, updated_at
             FROM email_drafts WHERE id = ?1",
            [id.as_str()],
            |row| {
                let variables_json = row.get::<_, String>(9)?;
                let state = row.get::<_, String>(10)?;
                let pending_send_idempotency_key = row.get::<_, Option<String>>(11)?;
                Ok(EmailDraft {
                    id: id.clone(),
                    mailbox_ref_id: MailboxRefId::new(row.get::<_, String>(0)?),
                    to: row.get(1)?,
                    creator_id: row.get::<_, Option<String>>(2)?.map(CreatorId::new),
                    contact_route_id: row
                        .get::<_, Option<String>>(3)?
                        .map(crate::CreatorContactRouteId::new),
                    reply_to_email_id: row.get::<_, Option<String>>(4)?.map(EmailId::new),
                    subject: row.get(5)?,
                    text: row.get(6)?,
                    html: row.get(7)?,
                    template_id: row.get::<_, Option<String>>(8)?.map(EmailTemplateId::new),
                    template_variables: serde_json::from_str::<BTreeMap<String, String>>(
                        &variables_json,
                    )
                    .map_err(|error| conversion_error(error.to_string()))?,
                    state: EmailDraftState::parse(&state).map_err(conversion_error)?,
                    sending: pending_send_idempotency_key.is_some(),
                    pending_send_idempotency_key,
                    revision: row.get(12)?,
                    created_at: row.get(13)?,
                    updated_at: row.get(14)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "email draft not found".into())
}
