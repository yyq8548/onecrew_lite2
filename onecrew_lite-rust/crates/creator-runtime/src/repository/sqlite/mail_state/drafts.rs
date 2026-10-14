use rusqlite::{OptionalExtension, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    AuditEvent, AuditEventId, CommitEmailDraftActionInput, CreatorId, EmailActionKind,
    EmailDirection, EmailDraft, EmailDraftId, EmailDraftMutationInput, EmailDraftSendClaim,
    EmailDraftSendFingerprint, EmailDraftSendResult, EmailDraftState, EmailId, EmailTemplateId,
    MailboxRefId, RemoteAcceptedEmailAction, RemoteAcceptedEmailContext, RemoteStartedEmailAction,
    ReserveEmailDraftSendInput, SaveEmailDraftInput, Touchpoint, TouchpointId,
    repository::EmailDraftRepository,
};

use super::{
    super::{SqliteRepository, emails::persist_snapshot, json, now},
    DRAFT_EMAIL_ACTION,
    draft_claims::load_email_draft_send_claim,
    draft_commit::{
        draft_reservation_key, draft_send_fingerprint_for_commit, parse_draft_send_fingerprint,
        validate_draft_for_commit,
    },
    draft_queries::load_draft,
    revision::revision_conflict,
};

impl EmailDraftRepository for SqliteRepository {
    fn list_email_drafts(
        &self,
        mailbox_ref_id: Option<&MailboxRefId>,
        state: Option<EmailDraftState>,
        limit: u32,
        cursor: Option<&str>,
    ) -> Result<crate::Page<crate::EmailDraftSummary>, String> {
        let context = format!(
            "email_draft_list|mailbox={}|state={}",
            mailbox_ref_id.map(MailboxRefId::as_str).unwrap_or("<all>"),
            state.map(EmailDraftState::as_str).unwrap_or("")
        );
        let position = cursor
            .map(|cursor| super::super::decode_keyset_single(cursor, &context))
            .transpose()?;
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mut statement = transaction
            .prepare(
                "SELECT id FROM email_drafts
                 WHERE (?1 IS NULL OR mailbox_ref_id = ?1)
                   AND (?2 IS NULL OR state = ?2)
                   AND (?3 IS NULL OR updated_at < ?3
                     OR (updated_at = ?3 AND id < ?4))
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?5",
            )
            .map_err(|error| error.to_string())?;
        let ids = statement
            .query_map(
                params![
                    mailbox_ref_id.map(MailboxRefId::as_str),
                    state.map(EmailDraftState::as_str),
                    position.as_ref().map(|(updated_at, _)| updated_at),
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
            .map(|id| load_draft(&transaction, &EmailDraftId::new(id)).map(|draft| draft.summary()))
            .collect::<Result<Vec<_>, String>>()?;
        let page = super::super::keyset_page(summaries, limit as usize, &context, |summary| {
            (vec![summary.updated_at], summary.id.as_str())
        });
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(page)
    }

    fn get_email_draft(&self, draft_id: &EmailDraftId) -> Result<EmailDraft, String> {
        let connection = self.read_connection()?;
        load_draft(&connection, draft_id)
    }

    fn save_email_draft(&self, input: SaveEmailDraftInput) -> Result<EmailDraft, String> {
        let timestamp = now();
        let connection = self.lock()?;
        let id = match input.id {
            None => {
                if input.expected_revision.is_some() {
                    return Err("new email draft must not include expected_revision".into());
                }
                let id = EmailDraftId::new(Uuid::new_v4().to_string());
                connection
                    .execute(
                        "INSERT INTO email_drafts
                           (id, mailbox_ref_id, to_address, creator_id, contact_route_id,
                            reply_to_email_id, subject, body_text, body_html, template_id,
                            template_variables_json, state, revision, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                                 'draft', 1, ?12, ?12)",
                        params![
                            id.as_str(),
                            input.mailbox_ref_id.as_str(),
                            input.to,
                            input.creator_id.as_ref().map(CreatorId::as_str),
                            input
                                .contact_route_id
                                .as_ref()
                                .map(crate::CreatorContactRouteId::as_str),
                            input.reply_to_email_id.as_ref().map(EmailId::as_str),
                            input.subject,
                            input.text,
                            input.html,
                            input.template_id.as_ref().map(EmailTemplateId::as_str),
                            json(&input.template_variables)?,
                            timestamp
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                id
            }
            Some(id) => {
                let expected_revision = input
                    .expected_revision
                    .ok_or_else(|| "existing email draft requires expected_revision".to_string())?;
                let changed = connection
                    .execute(
                        "UPDATE email_drafts
                         SET mailbox_ref_id = ?3, to_address = ?4, creator_id = ?5,
                             contact_route_id = ?6, reply_to_email_id = ?7, subject = ?8,
                             body_text = ?9, body_html = ?10, template_id = ?11,
                             template_variables_json = ?12, revision = revision + 1,
                             updated_at = ?13
                         WHERE id = ?1 AND revision = ?2 AND state = 'draft'
                           AND sending_idempotency_key IS NULL",
                        params![
                            id.as_str(),
                            expected_revision,
                            input.mailbox_ref_id.as_str(),
                            input.to,
                            input.creator_id.as_ref().map(CreatorId::as_str),
                            input
                                .contact_route_id
                                .as_ref()
                                .map(crate::CreatorContactRouteId::as_str),
                            input.reply_to_email_id.as_ref().map(EmailId::as_str),
                            input.subject,
                            input.text,
                            input.html,
                            input.template_id.as_ref().map(EmailTemplateId::as_str),
                            json(&input.template_variables)?,
                            timestamp
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed == 0 {
                    let draft = load_draft(&connection, &id)?;
                    if draft.sending {
                        return Err("email draft is reserved for sending".into());
                    }
                    if draft.state == EmailDraftState::Trashed {
                        return Err("trashed email draft must be restored before saving".into());
                    }
                    return Err(revision_conflict(
                        "email draft",
                        expected_revision,
                        draft.revision,
                    ));
                }
                id
            }
        };
        load_draft(&connection, &id)
    }

    fn set_email_draft_state(
        &self,
        input: EmailDraftMutationInput,
        state: EmailDraftState,
    ) -> Result<EmailDraft, String> {
        let connection = self.lock()?;
        let changed = connection
            .execute(
                "UPDATE email_drafts
                 SET state = ?3, revision = revision + 1, updated_at = ?4
                 WHERE id = ?1 AND revision = ?2 AND sending_idempotency_key IS NULL",
                params![
                    input.draft_id.as_str(),
                    input.expected_revision,
                    state.as_str(),
                    now()
                ],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            let draft = load_draft(&connection, &input.draft_id)?;
            if draft.sending {
                return Err("email draft is reserved for sending".into());
            }
            return Err(revision_conflict(
                "email draft",
                input.expected_revision,
                draft.revision,
            ));
        }
        load_draft(&connection, &input.draft_id)
    }

    fn find_email_draft_send(
        &self,
        input: ReserveEmailDraftSendInput,
    ) -> Result<Option<EmailDraftSendClaim>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let claim = load_email_draft_send_claim(&transaction, &input)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(claim)
    }

    fn claim_remote_started_email_draft_send(
        &self,
        input: ReserveEmailDraftSendInput,
        recovery: RemoteStartedEmailAction,
    ) -> Result<EmailDraftSendClaim, String> {
        let key = input.idempotency_key.trim();
        if key.is_empty() {
            return Err("idempotency_key is required".into());
        }
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let existing = transaction
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
        if let Some((action, state, fingerprint_json, result_json)) = existing {
            if action != DRAFT_EMAIL_ACTION {
                return Err("idempotency key was already used for another action".into());
            }
            let fingerprint = parse_draft_send_fingerprint(fingerprint_json.as_deref())?;
            fingerprint.ensure_matches(&input.draft_id, input.expected_revision)?;
            let claim = match state.as_str() {
                "in_progress" => return Err(
                    "email draft send is already in progress; automatic resend is unsafe without a durable provider receipt"
                        .into(),
                ),
                "remote_started" => {
                    let recovery: RemoteStartedEmailAction =
                        serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                            "remote-started draft action is missing recovery data".to_string()
                        })?)
                        .map_err(|error| error.to_string())?;
                    let RemoteAcceptedEmailContext::Draft {
                        draft_id,
                        expected_revision,
                        ..
                    } = &recovery.context
                    else {
                        return Err("email draft recovery payload has the wrong action kind".into());
                    };
                    if recovery.action != EmailActionKind::Draft
                        || recovery.request_fingerprint
                            != fingerprint_json.as_deref().unwrap_or_default()
                        || recovery.mailbox.id.as_str() != fingerprint.mailbox_ref_id
                        || *draft_id != input.draft_id
                        || *expected_revision != input.expected_revision
                        || recovery.request.idempotency_key.trim() != key
                    {
                        return Err("email draft recovery payload fingerprint mismatch".into());
                    }
                    let reservation_key = draft_reservation_key(&transaction, &input.draft_id)?;
                    if reservation_key.as_deref() != Some(key) {
                        return Err("email draft send reservation is inconsistent".into());
                    }
                    EmailDraftSendClaim::RemoteStarted(recovery)
                }
                "remote_accepted" => {
                    let recovery: RemoteAcceptedEmailAction =
                        serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                            "remote-accepted draft action is missing recovery data".to_string()
                        })?)
                        .map_err(|error| error.to_string())?;
                    let RemoteAcceptedEmailContext::Draft {
                        draft_id,
                        expected_revision,
                        ..
                    } = &recovery.context
                    else {
                        return Err("email draft recovery payload has the wrong action kind".into());
                    };
                    if recovery.action != EmailActionKind::Draft
                        || recovery.request_fingerprint
                            != fingerprint_json.as_deref().unwrap_or_default()
                        || recovery.mailbox.id.as_str() != fingerprint.mailbox_ref_id
                        || *draft_id != input.draft_id
                        || *expected_revision != input.expected_revision
                        || recovery.request.idempotency_key.trim() != key
                    {
                        return Err("email draft recovery payload fingerprint mismatch".into());
                    }
                    let reservation_key = draft_reservation_key(&transaction, &input.draft_id)?;
                    if reservation_key.as_deref() != Some(key) {
                        return Err("email draft send reservation is inconsistent".into());
                    }
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
                        || commit.request_fingerprint
                            != fingerprint_json.as_deref().unwrap_or_default()
                    {
                        return Err("email draft recovery payload fingerprint mismatch".into());
                    }
                    EmailDraftSendClaim::RemoteVerified(commit)
                }
                "completed" => {
                    let result: EmailDraftSendResult =
                        serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                            "completed draft action is missing its result".to_string()
                        })?)
                        .map_err(|error| error.to_string())?;
                    if result.draft_id != input.draft_id
                        || result.thread.mailbox_ref_id.as_str() != fingerprint.mailbox_ref_id
                    {
                        return Err("email draft completed result fingerprint mismatch".into());
                    }
                    EmailDraftSendClaim::Completed(Box::new(result))
                }
                "failed" => {
                    EmailDraftSendClaim::Failed(
                        serde_json::from_str(result_json.as_deref().ok_or_else(|| {
                            "failed draft action is missing its failure".to_string()
                        })?)
                        .map_err(|error| error.to_string())?,
                    )
                }
                _ => return Err(format!("unknown email draft action state: {state}")),
            };
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(claim);
        }

        let draft = load_draft(&transaction, &input.draft_id)?;
        if draft.revision != input.expected_revision {
            return Err(revision_conflict(
                "email draft",
                input.expected_revision,
                draft.revision,
            ));
        }
        if draft.state != EmailDraftState::Draft {
            return Err("trashed email draft must be restored before sending".into());
        }
        if draft.sending {
            return Err("email draft is reserved for sending".into());
        }
        let fingerprint = EmailDraftSendFingerprint::new(
            &draft.mailbox_ref_id,
            &draft.id,
            input.expected_revision,
        );
        let fingerprint_json =
            serde_json::to_string(&fingerprint).map_err(|error| error.to_string())?;
        let RemoteAcceptedEmailContext::Draft {
            draft_id,
            expected_revision,
            ..
        } = &recovery.context
        else {
            return Err("email draft recovery payload has the wrong action kind".into());
        };
        if recovery.action != EmailActionKind::Draft
            || recovery.request_fingerprint != fingerprint_json
            || recovery.mailbox.id != draft.mailbox_ref_id
            || *draft_id != draft.id
            || *expected_revision != draft.revision
            || recovery.request.idempotency_key.trim() != key
        {
            return Err("email draft recovery payload fingerprint mismatch".into());
        }
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO idempotency_records
                   (key, action, mailbox_ref_id, contact_route_id, request_fingerprint, state,
                    result_json, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'remote_started', ?6, ?7, ?7)",
                params![
                    key,
                    DRAFT_EMAIL_ACTION,
                    draft.mailbox_ref_id.as_str(),
                    draft
                        .contact_route_id
                        .as_ref()
                        .map(crate::CreatorContactRouteId::as_str),
                    fingerprint_json,
                    json(&recovery)?,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        let reserved = transaction
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = ?3, send_reserved_at = ?4, updated_at = ?4
                 WHERE id = ?1 AND revision = ?2 AND state = 'draft'
                   AND sending_idempotency_key IS NULL",
                params![
                    input.draft_id.as_str(),
                    input.expected_revision,
                    key,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        if reserved != 1 {
            return Err("email draft changed before send reservation".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(EmailDraftSendClaim::RemoteStarted(recovery))
    }

    fn stage_verified_email_draft_action(
        &self,
        input: &CommitEmailDraftActionInput,
    ) -> Result<(), String> {
        let key = input.idempotency_key.trim();
        let expected_fingerprint = draft_send_fingerprint_for_commit(input)?;
        if input.request_fingerprint != expected_fingerprint {
            return Err("email draft commit request fingerprint mismatch".into());
        }
        let recovery_json = json(input)?;
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
            .ok_or_else(|| "email draft idempotency claim was not found".to_string())?;
        if existing.0 != DRAFT_EMAIL_ACTION
            || existing.2.as_deref() != Some(expected_fingerprint.as_str())
        {
            return Err("email draft idempotency fingerprint mismatch".into());
        }
        match existing.1.as_str() {
            "remote_accepted" => {
                let changed = connection
                    .execute(
                        "UPDATE idempotency_records
                         SET state = 'remote_verified', result_json = ?3, updated_at = ?4
                         WHERE key = ?1 AND action = ?2 AND state = 'remote_accepted'
                           AND request_fingerprint = ?5",
                        params![
                            key,
                            DRAFT_EMAIL_ACTION,
                            recovery_json,
                            now(),
                            expected_fingerprint
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if changed != 1 {
                    return Err("email draft recovery staging lost its reservation".into());
                }
                Ok(())
            }
            "remote_verified" => {
                let existing_json = existing.3.ok_or_else(|| {
                    "remote-verified draft action is missing recovery data".to_string()
                })?;
                let existing_value: serde_json::Value =
                    serde_json::from_str(&existing_json).map_err(|error| error.to_string())?;
                let incoming_value: serde_json::Value =
                    serde_json::from_str(&recovery_json).map_err(|error| error.to_string())?;
                if existing_value == incoming_value {
                    Ok(())
                } else {
                    Err("email draft remote verification payload changed".into())
                }
            }
            state => Err(format!(
                "email draft action cannot stage remote verification from state {state}"
            )),
        }
    }

    fn release_email_draft_send(
        &self,
        draft_id: &EmailDraftId,
        idempotency_key: &str,
    ) -> Result<(), String> {
        let key = idempotency_key.trim();
        let connection = self.lock()?;
        let existing = connection
            .query_row(
                "SELECT action, request_fingerprint FROM idempotency_records WHERE key = ?1",
                [key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some((action, fingerprint_json)) = existing {
            if action != DRAFT_EMAIL_ACTION {
                return Err("idempotency key was already used for another action".into());
            }
            let fingerprint = parse_draft_send_fingerprint(fingerprint_json.as_deref())?;
            if fingerprint.draft_id != draft_id.as_str() {
                return Err("idempotency key was already used for another email draft".into());
            }
        }
        let changed = connection
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = NULL, send_reserved_at = NULL, updated_at = ?3
                 WHERE id = ?1 AND sending_idempotency_key = ?2",
                params![draft_id.as_str(), key, now()],
            )
            .map_err(|error| error.to_string())?;
        if changed == 1 {
            return Ok(());
        }
        let draft = load_draft(&connection, draft_id)?;
        if !draft.sending {
            Ok(())
        } else {
            Err("email draft is reserved by another send action".into())
        }
    }

    fn fail_and_release_email_draft_send(
        &self,
        draft_id: &EmailDraftId,
        idempotency_key: &str,
        failure: serde_json::Value,
    ) -> Result<Option<String>, String> {
        let key = idempotency_key.trim();
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let (action, fingerprint_json) = transaction
            .query_row(
                "SELECT action, request_fingerprint FROM idempotency_records WHERE key = ?1",
                [key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .map_err(|error| error.to_string())?;
        if action != DRAFT_EMAIL_ACTION {
            return Err("idempotency key was already used for another action".into());
        }
        let fingerprint = parse_draft_send_fingerprint(fingerprint_json.as_deref())?;
        if fingerprint.draft_id != draft_id.as_str() {
            return Err("idempotency key was already used for another email draft".into());
        }

        let failure_persist_error = match transaction.execute(
            "UPDATE idempotency_records
             SET state = 'failed', result_json = ?2, updated_at = ?3
             WHERE key = ?1 AND action = 'email_draft_send'
               AND state = 'remote_started'",
            params![key, failure.to_string(), now()],
        ) {
            Ok(1) => None,
            Ok(_) => Some("idempotency claim was not in remote_started state".to_owned()),
            Err(error) => Some(error.to_string()),
        };
        if failure_persist_error.is_some() {
            let deleted = transaction
                .execute(
                    "DELETE FROM idempotency_records
                     WHERE key = ?1 AND action = 'email_draft_send'
                       AND state = 'remote_started'",
                    [key],
                )
                .map_err(|error| {
                    format!(
                        "failed to persist draft send failure and failed to release its claim: {error}"
                    )
                })?;
            if deleted != 1 {
                return Err(
                    "failed to persist draft send failure and its claim was not releasable".into(),
                );
            }
        }
        let released = transaction
            .execute(
                "UPDATE email_drafts
                 SET sending_idempotency_key = NULL, send_reserved_at = NULL, updated_at = ?3
                 WHERE id = ?1 AND sending_idempotency_key = ?2",
                params![draft_id.as_str(), key, now()],
            )
            .map_err(|error| error.to_string())?;
        if released != 1 {
            return Err("email draft send reservation was not releasable".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(failure_persist_error)
    }

    fn commit_email_draft_action(
        &self,
        input: CommitEmailDraftActionInput,
    ) -> Result<EmailDraftSendResult, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        validate_draft_for_commit(&transaction, &input)?;
        let request_fingerprint = draft_send_fingerprint_for_commit(&input)?;
        let timestamp = now();
        let (thread, emails) = persist_snapshot(&transaction, &input.snapshot)?;
        let email = emails
            .into_iter()
            .find(|message| message.provider_message_id == input.verified_provider_message_id)
            .ok_or_else(|| "verified draft email was missing from snapshot".to_string())?;
        if email.direction != EmailDirection::Outbound {
            return Err("verified draft email was not outbound".into());
        }
        let provider: String = transaction
            .query_row(
                "SELECT provider FROM mailbox_refs WHERE id=?1",
                [input.snapshot.mailbox_ref_id.as_str()],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let touchpoint = input
            .creator_id
            .as_ref()
            .map(|creator_id| {
                let touchpoint = Touchpoint {
                    id: TouchpointId::new(Uuid::new_v4().to_string()),
                    creator_id: creator_id.clone(),
                    kind: "email".into(),
                    message_id: None,
                    email_id: Some(email.id.clone()),
                    account_relationship_id: None,
                    occurred_at: timestamp,
                    metadata: serde_json::json!({ "verified": true, "provider": provider, "draft_id": input.draft_id }),
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
                            input
                                .snapshot
                                .contact_route_id
                                .as_ref()
                                .map(crate::CreatorContactRouteId::as_str),
                            touchpoint.occurred_at,
                            json(&touchpoint.metadata)?
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>(touchpoint)
            })
            .transpose()?;
        let audit_event = AuditEvent {
            id: AuditEventId::new(Uuid::new_v4().to_string()),
            kind: "email_draft_sent".into(),
            creator_id: input.creator_id.clone(),
            account_ref_id: None,
            resolved_target_id: None,
            mailbox_ref_id: Some(input.snapshot.mailbox_ref_id.clone()),
            contact_locator_id: input.contact_locator_id.clone(),
            email_id: Some(email.id.clone()),
            payload: serde_json::json!({
                "draft_id": input.draft_id,
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
                    mailbox_ref_id, contact_locator_id, email_id, contact_route_id,
                    payload_json, created_at)
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
                    input
                        .snapshot
                        .contact_route_id
                        .as_ref()
                        .map(crate::CreatorContactRouteId::as_str),
                    json(&audit_event.payload)?,
                    audit_event.created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        let deleted = transaction
            .execute(
                "DELETE FROM email_drafts
                 WHERE id = ?1 AND revision = ?2 AND state = 'draft'
                   AND sending_idempotency_key = ?3",
                params![
                    input.draft_id.as_str(),
                    input.expected_revision,
                    input.idempotency_key.trim()
                ],
            )
            .map_err(|error| error.to_string())?;
        if deleted != 1 {
            return Err("email draft changed before local send commit".into());
        }
        let result = EmailDraftSendResult {
            draft_id: input.draft_id,
            thread,
            email: email.view(),
            touchpoint,
            audit_event,
            replayed: false,
        };
        let completed = transaction
            .execute(
                "UPDATE idempotency_records
                 SET state = 'completed', result_json = ?3, updated_at = ?4
                 WHERE key = ?1 AND action = ?2
                   AND state = 'remote_verified'
                   AND request_fingerprint = ?5",
                params![
                    input.idempotency_key.trim(),
                    DRAFT_EMAIL_ACTION,
                    json(&result)?,
                    timestamp,
                    request_fingerprint
                ],
            )
            .map_err(|error| error.to_string())?;
        if completed != 1 {
            return Err("email draft idempotency claim was not found".into());
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }
}
