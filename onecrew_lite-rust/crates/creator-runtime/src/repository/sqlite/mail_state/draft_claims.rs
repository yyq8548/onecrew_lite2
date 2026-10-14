use rusqlite::{Connection, OptionalExtension};

use crate::{
    CommitEmailDraftActionInput, EmailActionKind, EmailDraftId, EmailDraftSendClaim,
    EmailDraftSendFingerprint, EmailDraftSendResult, RemoteAcceptedEmailAction,
    RemoteAcceptedEmailContext, RemoteEmailRecovery, RemoteStartedEmailAction,
    ReserveEmailDraftSendInput,
};

use super::{
    DRAFT_EMAIL_ACTION,
    draft_commit::{draft_reservation_key, parse_draft_send_fingerprint},
};

pub(super) fn load_email_draft_send_claim(
    connection: &Connection,
    input: &ReserveEmailDraftSendInput,
) -> Result<Option<EmailDraftSendClaim>, String> {
    let key = input.idempotency_key.trim();
    if key.is_empty() {
        return Err("idempotency_key is required".into());
    }
    let existing = connection
        .query_row(
            "SELECT action, state, request_fingerprint, result_json
             FROM idempotency_records WHERE key = ?1",
            [key],
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
    let Some((action, state, fingerprint_json, result_json)) = existing else {
        return Ok(None);
    };
    if action != DRAFT_EMAIL_ACTION {
        return Err("idempotency key was already used for another action".into());
    }
    let fingerprint = parse_draft_send_fingerprint(fingerprint_json.as_deref())?;
    fingerprint.ensure_matches(&input.draft_id, input.expected_revision)?;
    let claim = match state.as_str() {
        "in_progress" => {
            return Err(
                "legacy email draft send is missing its frozen provider request; provider delivery is blocked until manually reconciled"
                    .into(),
            );
        }
        "remote_started" => {
            let recovery: RemoteStartedEmailAction =
                serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                    "remote-started draft action is missing recovery data".to_string()
                })?)
                .map_err(|error| error.to_string())?;
            ExpectedDraftRecovery::new(key, input, &fingerprint).validate(connection, &recovery)?;
            EmailDraftSendClaim::RemoteStarted(recovery)
        }
        "remote_accepted" => {
            let recovery: RemoteAcceptedEmailAction =
                serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                    "remote-accepted draft action is missing recovery data".to_string()
                })?)
                .map_err(|error| error.to_string())?;
            ExpectedDraftRecovery::new(key, input, &fingerprint).validate(connection, &recovery)?;
            EmailDraftSendClaim::RemoteAccepted(recovery)
        }
        "remote_verified" => {
            let commit: CommitEmailDraftActionInput =
                serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                    "remote-verified draft action is missing recovery data".to_string()
                })?)
                .map_err(|error| error.to_string())?;
            if commit.draft_id != input.draft_id
                || commit.expected_revision != input.expected_revision
                || commit.snapshot.mailbox_ref_id.as_str() != fingerprint.mailbox_ref_id
                || commit.idempotency_key.trim() != key
                || commit.request_fingerprint != fingerprint_json.as_deref().unwrap_or_default()
            {
                return Err("email draft recovery payload fingerprint mismatch".into());
            }
            ensure_draft_reservation(connection, &input.draft_id, key)?;
            EmailDraftSendClaim::RemoteVerified(commit)
        }
        "completed" => {
            let result: EmailDraftSendResult = serde_json::from_str(
                result_json
                    .as_deref()
                    .ok_or_else(|| "completed draft action is missing its result".to_string())?,
            )
            .map_err(|error| error.to_string())?;
            if result.draft_id != input.draft_id
                || result.thread.mailbox_ref_id.as_str() != fingerprint.mailbox_ref_id
            {
                return Err("email draft completed result fingerprint mismatch".into());
            }
            EmailDraftSendClaim::Completed(Box::new(result))
        }
        "failed" => EmailDraftSendClaim::Failed(
            serde_json::from_str(
                result_json
                    .as_deref()
                    .ok_or_else(|| "failed draft action is missing its failure".to_string())?,
            )
            .map_err(|error| error.to_string())?,
        ),
        _ => return Err(format!("unknown email draft action state: {state}")),
    };
    Ok(Some(claim))
}

struct ExpectedDraftRecovery<'a> {
    key: &'a str,
    draft_id: &'a EmailDraftId,
    expected_revision: u64,
    fingerprint: &'a EmailDraftSendFingerprint,
}

impl<'a> ExpectedDraftRecovery<'a> {
    fn new(
        key: &'a str,
        input: &'a ReserveEmailDraftSendInput,
        fingerprint: &'a EmailDraftSendFingerprint,
    ) -> Self {
        Self {
            key,
            draft_id: &input.draft_id,
            expected_revision: input.expected_revision,
            fingerprint,
        }
    }

    fn validate(
        &self,
        connection: &Connection,
        recovery: &RemoteEmailRecovery,
    ) -> Result<(), String> {
        let RemoteAcceptedEmailContext::Draft {
            draft_id: recovery_draft_id,
            expected_revision: recovery_revision,
            ..
        } = &recovery.context
        else {
            return Err("email draft recovery payload has the wrong action kind".into());
        };
        if recovery.action != EmailActionKind::Draft
            || recovery.request_fingerprint
                != serde_json::to_string(self.fingerprint).map_err(|error| error.to_string())?
            || recovery.mailbox.id.as_str() != self.fingerprint.mailbox_ref_id
            || recovery_draft_id != self.draft_id
            || *recovery_revision != self.expected_revision
            || recovery.request.idempotency_key.trim() != self.key
        {
            return Err("email draft recovery payload fingerprint mismatch".into());
        }
        ensure_draft_reservation(connection, self.draft_id, self.key)
    }
}

fn ensure_draft_reservation(
    connection: &Connection,
    draft_id: &EmailDraftId,
    key: &str,
) -> Result<(), String> {
    if draft_reservation_key(connection, draft_id)?.as_deref() == Some(key) {
        Ok(())
    } else {
        Err("email draft send reservation is inconsistent".into())
    }
}
