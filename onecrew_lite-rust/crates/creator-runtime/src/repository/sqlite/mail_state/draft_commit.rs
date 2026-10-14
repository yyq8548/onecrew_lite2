use rusqlite::{Connection, OptionalExtension, Transaction};

use crate::{
    CommitEmailDraftActionInput, CreatorId, EmailDraftId, EmailDraftSendFingerprint,
    EmailDraftState,
};

use super::{DRAFT_EMAIL_ACTION, revision::revision_conflict};

pub(super) fn validate_draft_for_commit(
    transaction: &Transaction<'_>,
    input: &CommitEmailDraftActionInput,
) -> Result<(), String> {
    let stored = transaction
        .query_row(
            "SELECT mailbox_ref_id, creator_id, contact_route_id, revision, state,
                    sending_idempotency_key
             FROM email_drafts WHERE id = ?1",
            [input.draft_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, u64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "email draft not found".to_string())?;
    if stored.0 != input.snapshot.mailbox_ref_id.as_str()
        || stored.1.as_deref() != input.creator_id.as_ref().map(CreatorId::as_str)
        || stored.2.as_deref()
            != input
                .snapshot
                .contact_route_id
                .as_ref()
                .map(crate::CreatorContactRouteId::as_str)
    {
        return Err("email draft ownership changed before local send commit".into());
    }
    if stored.3 != input.expected_revision {
        return Err(revision_conflict(
            "email draft",
            input.expected_revision,
            stored.3,
        ));
    }
    if stored.4 != EmailDraftState::Draft.as_str() {
        return Err("trashed email draft must be restored before sending".into());
    }
    if stored.5.as_deref() != Some(input.idempotency_key.trim()) {
        return Err("email draft send reservation was not held by this action".into());
    }
    let expected_fingerprint = draft_send_fingerprint_for_commit(input)?;
    let action = transaction
        .query_row(
            "SELECT action, state, request_fingerprint
             FROM idempotency_records WHERE key = ?1",
            [input.idempotency_key.trim()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "email draft idempotency claim was not found".to_string())?;
    if action.0 != DRAFT_EMAIL_ACTION
        || action.1 != "remote_verified"
        || action.2.as_deref() != Some(expected_fingerprint.as_str())
        || input.request_fingerprint != expected_fingerprint
    {
        return Err("email draft idempotency reservation is not recoverable".into());
    }
    Ok(())
}

pub(super) fn parse_draft_send_fingerprint(
    value: Option<&str>,
) -> Result<EmailDraftSendFingerprint, String> {
    serde_json::from_str(
        value.ok_or_else(|| "email draft idempotency fingerprint is missing".to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(super) fn draft_send_fingerprint_for_commit(
    input: &CommitEmailDraftActionInput,
) -> Result<String, String> {
    serde_json::to_string(&EmailDraftSendFingerprint::new(
        &input.snapshot.mailbox_ref_id,
        &input.draft_id,
        input.expected_revision,
    ))
    .map_err(|error| error.to_string())
}

pub(super) fn draft_reservation_key(
    connection: &Connection,
    draft_id: &EmailDraftId,
) -> Result<Option<String>, String> {
    let value = connection
        .query_row(
            "SELECT sending_idempotency_key FROM email_drafts WHERE id = ?1",
            [draft_id.as_str()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    Ok(value.flatten())
}
