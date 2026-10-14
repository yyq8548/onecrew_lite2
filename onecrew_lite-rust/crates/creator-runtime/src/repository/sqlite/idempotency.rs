use rusqlite::{OptionalExtension, params};

use crate::{
    CreatorContactRouteId, IdempotencyClaim, MailboxRefId, RecoverableActionClaim,
    repository::IdempotencyRepository,
};

use super::{SqliteRepository, now};

impl IdempotencyRepository for SqliteRepository {
    fn read_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<Option<crate::CreatorOutreachActionState>, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::read(self, key, intent)
    }
    fn creator_outreach_status(
        &self,
        key: &str,
    ) -> Result<Option<crate::CreatorOutreachStatus>, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::status(self, key)
    }
    fn validate_creator_outreach(
        &self,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<(), crate::CreatorOutreachError> {
        super::outreach_actions::ledger::validate_new(self, intent)
    }
    fn claim_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<crate::CreatorOutreachClaim, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::claim(self, key, intent)
    }
    fn accept_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
        receipt: crate::CreatorOutreachReceipt,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::accept(self, key, intent, receipt)
    }
    fn verify_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
        verification: crate::CreatorOutreachVerification,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::verify(self, key, intent, verification)
    }
    fn refuse_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError> {
        super::outreach_actions::ledger::refuse(self, key, intent)
    }
    fn recorded_creator_outreach(
        &self,
        key: &str,
    ) -> Result<crate::CreatorOutreachRecord, crate::CreatorOutreachError> {
        super::outreach_actions::record::read_recorded(self, key)
    }
    fn find_action(&self, key: &str, action: &str) -> Result<Option<IdempotencyClaim>, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("idempotency_key is required".into());
        }
        let connection = self.read_connection()?;
        load_existing_claim(&connection, key, action).transpose()
    }

    fn begin_action(&self, key: &str, action: &str) -> Result<IdempotencyClaim, String> {
        self.begin_action_for(key, action, None, None)
    }

    fn begin_action_with_fingerprint(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
    ) -> Result<IdempotencyClaim, String> {
        let key = key.trim();
        let request_fingerprint = request_fingerprint.trim();
        if key.is_empty() || request_fingerprint.is_empty() {
            return Err("idempotency key and request fingerprint are required".into());
        }
        let connection = self.lock()?;
        if let Some(stored) = connection
            .query_row(
                "SELECT request_fingerprint FROM idempotency_records WHERE key = ?1 AND action = ?2",
                params![key, action],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            if stored.as_deref() != Some(request_fingerprint) {
                return Err("idempotency key was already used for a different request".into());
            }
            return load_existing_claim(&connection, key, action)
                .transpose()?
                .ok_or_else(|| "idempotency claim disappeared during fingerprint validation".into());
        }
        let timestamp = now();
        let changed = connection
            .execute(
                "INSERT OR IGNORE INTO idempotency_records
                   (key, action, mailbox_ref_id, contact_route_id, request_fingerprint,
                    state, result_json, created_at, updated_at)
                 VALUES (?1, ?2, NULL, NULL, ?3, 'in_progress', NULL, ?4, ?4)",
                params![key, action, request_fingerprint, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            return Ok(IdempotencyClaim::Acquired);
        }
        let stored: Option<String> = connection
            .query_row(
                "SELECT request_fingerprint FROM idempotency_records WHERE key = ?1 AND action = ?2",
                params![key, action],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if stored.as_deref() != Some(request_fingerprint) {
            return Err("idempotency key was already used for a different request".into());
        }
        load_existing_claim(&connection, key, action)
            .transpose()?
            .ok_or_else(|| "idempotency claim disappeared during fingerprint validation".into())
    }

    fn begin_mailbox_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<IdempotencyClaim, String> {
        self.begin_action_for(key, action, Some(mailbox_ref_id), None)
    }

    fn find_recoverable_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
        request_fingerprint: &str,
    ) -> Result<Option<RecoverableActionClaim>, String> {
        let key = validate_recoverable_email_identity(key, request_fingerprint)?;
        let connection = self.read_connection()?;
        load_recoverable_email_claim(
            &connection,
            key,
            action,
            mailbox_ref_id,
            contact_route_id,
            request_fingerprint.trim(),
        )
    }

    fn stage_remote_accepted_action(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<(), String> {
        self.stage_recoverable_email_action(
            key,
            action,
            request_fingerprint,
            &["remote_started"],
            "remote_accepted",
            recovery,
        )
    }

    fn claim_remote_started_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<RecoverableActionClaim, String> {
        self.claim_remote_started_action_for(
            key,
            action,
            mailbox_ref_id,
            contact_route_id,
            request_fingerprint,
            recovery,
        )
    }

    fn stage_remote_verified_action(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<(), String> {
        self.stage_recoverable_email_action(
            key,
            action,
            request_fingerprint,
            &["remote_started", "remote_accepted"],
            "remote_verified",
            recovery,
        )
    }

    fn complete_action(
        &self,
        key: &str,
        action: &str,
        result: serde_json::Value,
    ) -> Result<(), String> {
        let changed = self
            .lock()?
            .execute(
                "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2 AND state = 'in_progress'",
                params![key.trim(), action, result.to_string(), now()],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            Ok(())
        } else {
            Err("idempotency claim was not found".into())
        }
    }

    fn fail_action(
        &self,
        key: &str,
        action: &str,
        failure: serde_json::Value,
    ) -> Result<(), String> {
        let changed = self
            .lock()?
            .execute(
                "UPDATE idempotency_records
                 SET state = 'failed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2
                   AND state IN ('in_progress', 'remote_started')",
                params![key.trim(), action, failure.to_string(), now()],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            Ok(())
        } else {
            Err("idempotency claim was not found".into())
        }
    }

    fn cancel_action(&self, key: &str, action: &str) -> Result<(), String> {
        let changed = self
            .lock()?
            .execute(
                "DELETE FROM idempotency_records
                 WHERE key = ?1 AND action = ?2
                   AND state IN ('in_progress', 'remote_started')",
                params![key.trim(), action],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            Ok(())
        } else {
            Err("idempotency claim was not found".into())
        }
    }
}

impl SqliteRepository {
    fn claim_remote_started_action_for(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<RecoverableActionClaim, String> {
        let key = validate_recoverable_email_identity(key, request_fingerprint)?;
        let request_fingerprint = request_fingerprint.trim();
        let connection = self.lock()?;
        if let Some(claim) = load_recoverable_email_claim(
            &connection,
            key,
            action,
            mailbox_ref_id,
            contact_route_id,
            request_fingerprint,
        )? {
            return Ok(claim);
        }
        let timestamp = now();
        let changed = connection
            .execute(
                "INSERT OR IGNORE INTO idempotency_records
                   (key, action, mailbox_ref_id, contact_route_id, request_fingerprint, state,
                    result_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'remote_started', ?6, ?7, ?7)",
                params![
                    key,
                    action,
                    mailbox_ref_id.map(MailboxRefId::as_str),
                    contact_route_id.map(CreatorContactRouteId::as_str),
                    request_fingerprint,
                    recovery.to_string(),
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            return Ok(RecoverableActionClaim::RemoteStarted(recovery));
        }
        load_recoverable_email_claim(
            &connection,
            key,
            action,
            mailbox_ref_id,
            contact_route_id,
            request_fingerprint,
        )?
        .ok_or_else(|| "email idempotency claim disappeared during creation".into())
    }

    fn stage_recoverable_email_action(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
        from_states: &[&str],
        to_state: &str,
        recovery: serde_json::Value,
    ) -> Result<(), String> {
        let key = key.trim();
        let request_fingerprint = request_fingerprint.trim();
        let recovery_json = recovery.to_string();
        let connection = self.lock()?;
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
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "email idempotency claim was not found".to_string())?;
        ensure_action_name(&existing.0, action)?;
        if existing.2.as_deref() != Some(request_fingerprint) {
            return Err("email idempotency request fingerprint mismatch".into());
        }
        if existing.1 == to_state {
            let stored: serde_json::Value = serde_json::from_str(
                existing
                    .3
                    .as_deref()
                    .ok_or_else(|| format!("{to_state} email action is missing recovery data"))?,
            )
            .map_err(|error| error.to_string())?;
            return if stored == recovery {
                Ok(())
            } else {
                Err(format!("{to_state} email recovery payload changed"))
            };
        }
        if !from_states.contains(&existing.1.as_str()) {
            return Err(format!(
                "staged action cannot transition from {} to {to_state}",
                existing.1
            ));
        }
        let changed = connection
            .execute(
                "UPDATE idempotency_records
                 SET state = ?4, result_json = ?5, updated_at = ?6
                 WHERE key = ?1 AND action = ?2 AND request_fingerprint = ?3
                   AND state = ?7",
                params![
                    key,
                    action,
                    request_fingerprint,
                    to_state,
                    recovery_json,
                    now(),
                    existing.1
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            Ok(())
        } else {
            Err("email recovery transition lost its idempotency claim".into())
        }
    }

    fn begin_action_for(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
    ) -> Result<IdempotencyClaim, String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("idempotency_key is required".into());
        }
        let connection = self.lock()?;
        let existing = load_existing_action(&connection, key)?;
        if let Some(existing) = existing {
            ensure_action_name(&existing.action, action)?;
            if existing.mailbox_ref_id.as_deref() != mailbox_ref_id.map(MailboxRefId::as_str) {
                return Err("idempotency key was already used for another mailbox scope".into());
            }
            if existing.contact_route_id.as_deref()
                != contact_route_id.map(CreatorContactRouteId::as_str)
            {
                return Err("idempotency key was already used for another contact route".into());
            }
            return decode_claim(&existing.state, existing.result_json);
        }
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO idempotency_records
                   (key, action, mailbox_ref_id, contact_route_id, state, result_json,
                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 'in_progress', NULL, ?5, ?5)",
                params![
                    key,
                    action,
                    mailbox_ref_id.map(MailboxRefId::as_str),
                    contact_route_id.map(CreatorContactRouteId::as_str),
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(IdempotencyClaim::Acquired)
    }
}

fn load_existing_claim(
    connection: &rusqlite::Connection,
    key: &str,
    action: &str,
) -> Option<Result<IdempotencyClaim, String>> {
    match load_existing_action(connection, key) {
        Err(error) => Some(Err(error)),
        Ok(None) => None,
        Ok(Some(existing)) => Some(
            ensure_action_name(&existing.action, action)
                .and_then(|_| decode_claim(&existing.state, existing.result_json)),
        ),
    }
}

struct StoredIdempotencyAction {
    action: String,
    state: String,
    result_json: Option<String>,
    mailbox_ref_id: Option<String>,
    contact_route_id: Option<String>,
}

fn load_existing_action(
    connection: &rusqlite::Connection,
    key: &str,
) -> Result<Option<StoredIdempotencyAction>, String> {
    connection
        .query_row(
            "SELECT action, state, result_json, mailbox_ref_id, contact_route_id
             FROM idempotency_records WHERE key = ?1",
            [key],
            |row| {
                Ok(StoredIdempotencyAction {
                    action: row.get(0)?,
                    state: row.get(1)?,
                    result_json: row.get(2)?,
                    mailbox_ref_id: row.get(3)?,
                    contact_route_id: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn ensure_action_name(stored: &str, expected: &str) -> Result<(), String> {
    if stored == expected {
        Ok(())
    } else {
        Err("idempotency key was already used for another action".into())
    }
}

fn decode_claim(state: &str, result_json: Option<String>) -> Result<IdempotencyClaim, String> {
    if state == "in_progress" {
        return Err("idempotent action is already in progress".into());
    }
    let result_json = result_json
        .ok_or_else(|| "completed idempotency record is missing its result".to_string())?;
    let result = serde_json::from_str(&result_json).map_err(|error| error.to_string())?;
    match state {
        "completed" => Ok(IdempotencyClaim::Completed(result)),
        "failed" => Ok(IdempotencyClaim::Failed(result)),
        _ => Err(format!("unknown idempotency state: {state}")),
    }
}

fn validate_recoverable_email_identity<'a>(
    key: &'a str,
    request_fingerprint: &str,
) -> Result<&'a str, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("idempotency_key is required".into());
    }
    if request_fingerprint.trim().is_empty() {
        return Err("email request fingerprint is required".into());
    }
    Ok(key)
}

fn load_recoverable_email_claim(
    connection: &rusqlite::Connection,
    key: &str,
    action: &str,
    mailbox_ref_id: Option<&MailboxRefId>,
    contact_route_id: Option<&CreatorContactRouteId>,
    request_fingerprint: &str,
) -> Result<Option<RecoverableActionClaim>, String> {
    let existing = connection
        .query_row(
            "SELECT action, state, request_fingerprint, result_json, mailbox_ref_id,
                    contact_route_id
             FROM idempotency_records WHERE key = ?1",
            [key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((stored_action, state, stored_fingerprint, result_json, stored_mailbox, stored_route)) =
        existing
    else {
        return Ok(None);
    };
    ensure_action_name(&stored_action, action)?;
    if stored_mailbox.as_deref() != mailbox_ref_id.map(MailboxRefId::as_str) {
        return Err("idempotency key was already used for another mailbox scope".into());
    }
    if stored_route.as_deref() != contact_route_id.map(CreatorContactRouteId::as_str) {
        return Err("idempotency key was already used for another contact route".into());
    }
    // Rows written before fingerprinting have no stored fingerprint; the
    // action and route checks above still bind the key to its claim.
    if let Some(stored_fingerprint) = stored_fingerprint.as_deref()
        && stored_fingerprint != request_fingerprint
    {
        return Err("idempotency key was already used for a different request".into());
    }
    decode_recoverable_email_claim(&state, result_json).map(Some)
}

fn decode_recoverable_email_claim(
    state: &str,
    result_json: Option<String>,
) -> Result<RecoverableActionClaim, String> {
    if state == "in_progress" {
        return Err(
            "email action is already in progress; automatic resend is unsafe without a durable provider receipt"
                .into(),
        );
    }
    let result_json = result_json
        .ok_or_else(|| format!("{state} email idempotency record is missing its payload"))?;
    let result = serde_json::from_str(&result_json).map_err(|error| error.to_string())?;
    match state {
        "remote_started" => Ok(RecoverableActionClaim::RemoteStarted(result)),
        "remote_accepted" => Ok(RecoverableActionClaim::RemoteAccepted(result)),
        "remote_verified" => Ok(RecoverableActionClaim::RemoteVerified(result)),
        "completed" => Ok(RecoverableActionClaim::Completed(result)),
        "failed" => Ok(RecoverableActionClaim::Failed(result)),
        _ => Err(format!(
            "unknown recoverable email idempotency state: {state}"
        )),
    }
}
