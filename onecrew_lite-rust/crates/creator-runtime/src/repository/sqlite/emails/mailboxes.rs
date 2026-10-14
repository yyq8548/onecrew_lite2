use std::collections::HashSet;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use uuid::Uuid;

use crate::{
    EmailAuthConnectionDeletionClaim, EmailAuthConnectionId, EmailCapability, EmailProviderKind,
    EmailRecoveryStage, MailboxCredentialDeletionBlockers, MailboxDetail,
    MailboxEmailRecoveryBlocker, MailboxIdentityMapping, MailboxIdentityMappingInput,
    MailboxProjectionKind, MailboxRecord, MailboxRefId, MailboxRefInput,
    MailboxSendingDraftBlocker, email::mailbox_detail, repository::MailboxRepository,
};

use super::super::{SqliteRepository, json, now};

impl MailboxRepository for SqliteRepository {
    fn upsert_mailbox_ref(&self, input: MailboxRefInput) -> Result<MailboxDetail, String> {
        let input = validate_mailbox_input(input)?;
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        upsert_mailbox(&transaction, &input)?;
        let mailbox = mailbox_detail(load_mailbox(&transaction, &input.id)?)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(mailbox)
    }

    fn reconcile_mailbox_refs(
        &self,
        inputs: Vec<MailboxRefInput>,
    ) -> Result<Vec<MailboxDetail>, String> {
        let mut ids = HashSet::with_capacity(inputs.len());
        let inputs = inputs
            .into_iter()
            .map(validate_mailbox_input)
            .collect::<Result<Vec<_>, _>>()?;
        for input in &inputs {
            if input.provider != "resend" {
                return Err("vault mailbox snapshot only accepts Resend projections".into());
            }
            if !ids.insert(input.id.as_str().to_owned()) {
                return Err(format!(
                    "mailbox snapshot contains duplicate id {}",
                    input.id.as_str()
                ));
            }
        }

        let mut connection = self.lock()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for input in &inputs {
            upsert_mailbox(&transaction, input)?;
        }

        let stale_ids = mailbox_ids(&transaction)?
            .into_iter()
            .filter(|id| !ids.contains(id))
            .collect::<Vec<_>>();
        for id in stale_ids {
            mark_mailbox_credential_missing(&transaction, &MailboxRefId::new(id))?;
        }

        let mailboxes = list_mailboxes(&transaction)?
            .into_iter()
            .map(mailbox_detail)
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(mailboxes)
    }

    fn find_mailbox_ref(&self, id: &MailboxRefId) -> Result<Option<MailboxDetail>, String> {
        let connection = self.read_connection()?;
        find_mailbox(&connection, id)?
            .map(mailbox_detail)
            .transpose()
    }

    fn list_mailbox_refs(&self) -> Result<Vec<MailboxDetail>, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let mailboxes = list_mailboxes(&transaction)?
            .into_iter()
            .map(mailbox_detail)
            .collect::<Result<Vec<_>, _>>()?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(mailboxes)
    }

    fn upsert_mailbox_identity_mapping(
        &self,
        input: MailboxIdentityMappingInput,
    ) -> Result<MailboxIdentityMapping, String> {
        let input = validate_identity_mapping_input(input)?;
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        upsert_mailbox_identity_mapping_inner(&transaction, &input)?;
        let mapping = load_mailbox_identity_mapping(&transaction, &input.mailbox_ref_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(mapping)
    }

    fn resolve_mailbox_auth_binding(
        &self,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<crate::MailboxAuthBinding, String> {
        let mut connection = self.read_connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Deferred)
            .map_err(|error| error.to_string())?;
        let detail = find_mailbox(&transaction, mailbox_ref_id)?
            .map(crate::email::mailbox_detail)
            .transpose()?
            .ok_or_else(|| "mailbox reference not found".to_string())?;
        let auth_connection = load_auth_connection(&transaction, mailbox_ref_id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(crate::MailboxAuthBinding {
            mailbox_ref_id: detail.mailbox_ref_id,
            provider: detail.provider,
            label: detail.label,
            email_address: detail.email_address,
            display_name: detail.display_name,
            capabilities: detail.capabilities,
            available: detail.available,
            auth_connection,
        })
    }

    fn begin_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<EmailAuthConnectionDeletionClaim, String> {
        let mut connection = self.lock()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let exists = transaction
            .query_row(
                "SELECT 1 FROM email_auth_connections WHERE id = ?1",
                [auth_connection_id.as_str()],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .is_some();
        if !exists {
            return Err("email auth connection not found".into());
        }
        let mailbox_ids = mapped_mailbox_ids(&transaction, auth_connection_id)?;
        let mut blockers = Vec::new();
        for mailbox_ref_id in &mailbox_ids {
            let mailbox_blockers =
                load_mailbox_credential_deletion_blockers(&transaction, mailbox_ref_id)?;
            if !mailbox_blockers.is_empty() {
                blockers.push(mailbox_blockers);
            }
        }
        if !blockers.is_empty() {
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(EmailAuthConnectionDeletionClaim::Blocked {
                auth_connection_id: auth_connection_id.clone(),
                blockers,
            });
        }
        for mailbox_ref_id in &mailbox_ids {
            transaction
                .execute(
                    "INSERT INTO mailbox_credential_deletion_guards (mailbox_ref_id, created_at)
                     VALUES (?1, ?2)
                     ON CONFLICT(mailbox_ref_id) DO NOTHING",
                    rusqlite::params![mailbox_ref_id.as_str(), now()],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(EmailAuthConnectionDeletionClaim::Acquired {
            mailbox_ref_ids: mailbox_ids,
        })
    }

    fn finish_email_auth_connection_deletion(
        &self,
        auth_connection_id: &EmailAuthConnectionId,
    ) -> Result<(), String> {
        self.lock()?
            .execute(
                "DELETE FROM mailbox_credential_deletion_guards
                 WHERE mailbox_ref_id IN (
                   SELECT mailbox_ref_id FROM mailbox_identity_mappings
                   WHERE auth_connection_id = ?1
                 )",
                [auth_connection_id.as_str()],
            )
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn provider_kind_for_mailbox(provider: &str) -> Result<EmailProviderKind, String> {
    if provider == "gmail" {
        return Ok(EmailProviderKind::Remote);
    }
    EmailProviderKind::parse(provider)
        .ok_or_else(|| format!("mailbox provider kind is unsupported: {provider}"))
}

fn validate_identity_mapping_input(
    mut input: MailboxIdentityMappingInput,
) -> Result<MailboxIdentityMappingInput, String> {
    if input.mailbox_ref_id.as_str().trim().is_empty()
        || input.auth_connection_id.as_str().trim().is_empty()
        || input.secret_record_id.as_str().trim().is_empty()
    {
        return Err("mailbox identity mapping ids are required".into());
    }
    if input.mailbox_ref_id.as_str() == input.auth_connection_id.as_str()
        || input.auth_connection_id.as_str() == input.secret_record_id.as_str()
    {
        return Err(
            "auth connection identity must differ from mailbox and secret identities".into(),
        );
    }
    input.connection_label = input.connection_label.trim().to_owned();
    if input.connection_label.is_empty() {
        return Err("email auth connection label is required".into());
    }
    input.checkpoint_namespace = input.checkpoint_namespace.trim().to_owned();
    if input.checkpoint_namespace.is_empty() || input.checkpoint_namespace.len() > 256 {
        return Err("email checkpoint namespace is invalid".into());
    }
    if input.capability_projection_revision == 0 {
        return Err("email capability projection revision must be positive".into());
    }
    Ok(input)
}

fn upsert_mailbox_identity_mapping_inner(
    connection: &Connection,
    input: &MailboxIdentityMappingInput,
) -> Result<(), String> {
    let (mailbox_provider, mailbox_available): (String, bool) = connection
        .query_row(
            "SELECT provider, available FROM mailbox_refs WHERE id = ?1",
            [input.mailbox_ref_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "mailbox identity mapping references an unknown mailbox".to_string())?;
    let mailbox_kind = provider_kind_for_mailbox(&mailbox_provider)?;
    if mailbox_kind != input.provider_kind {
        return Err("mailbox provider and identity mapping provider kind differ".into());
    }

    let timestamp = now();
    let existing_connection: Option<(String, String)> = connection
        .query_row(
            "SELECT provider_kind, secret_record_id FROM email_auth_connections WHERE id = ?1",
            [input.auth_connection_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((provider_kind, secret_record_id)) = existing_connection {
        if provider_kind != input.provider_kind.as_str()
            || secret_record_id != input.secret_record_id.as_str()
        {
            return Err("email auth connection identity is already bound to another secret".into());
        }
        if (input.mailbox_ref_id.as_str() == input.secret_record_id.as_str()
            || mailbox_provider == "gmail")
            && input.projection_kind == MailboxProjectionKind::Mailbox
        {
            connection
                .execute(
                    "UPDATE email_auth_connections
                     SET label = ?2, available = ?3, updated_at = ?4
                     WHERE id = ?1",
                    rusqlite::params![
                        input.auth_connection_id.as_str(),
                        input.connection_label,
                        mailbox_available,
                        timestamp,
                    ],
                )
                .map_err(|error| error.to_string())?;
        }
    } else {
        connection
            .execute(
                "INSERT INTO email_auth_connections
                   (id, provider_kind, secret_record_id, label, available, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![
                    input.auth_connection_id.as_str(),
                    input.provider_kind.as_str(),
                    input.secret_record_id.as_str(),
                    input.connection_label,
                    mailbox_available,
                    timestamp,
                ],
            )
            .map_err(|error| error.to_string())?;
    }

    let existing_mapping: Option<(String, String)> = connection
        .query_row(
            "SELECT auth_connection_id, provider_kind
             FROM mailbox_identity_mappings WHERE mailbox_ref_id = ?1",
            [input.mailbox_ref_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some((auth_connection_id, provider_kind)) = existing_mapping
        && (auth_connection_id != input.auth_connection_id.as_str()
            || provider_kind != input.provider_kind.as_str())
    {
        return Err("mailbox identity is already mapped to another auth connection".into());
    }

    connection
        .execute(
            "INSERT INTO mailbox_identity_mappings
               (mailbox_ref_id, auth_connection_id, provider_kind, projection_kind,
                capability_projection_revision, checkpoint_namespace, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(mailbox_ref_id) DO UPDATE SET
               projection_kind = excluded.projection_kind,
               capability_projection_revision = excluded.capability_projection_revision,
               checkpoint_namespace = excluded.checkpoint_namespace,
               updated_at = excluded.updated_at",
            rusqlite::params![
                input.mailbox_ref_id.as_str(),
                input.auth_connection_id.as_str(),
                input.provider_kind.as_str(),
                input.projection_kind.as_str(),
                input.capability_projection_revision,
                input.checkpoint_namespace,
                timestamp,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn mapped_mailbox_ids(
    connection: &Connection,
    auth_connection_id: &EmailAuthConnectionId,
) -> Result<Vec<MailboxRefId>, String> {
    let mut statement = connection
        .prepare(
            "SELECT mailbox_ref_id FROM mailbox_identity_mappings
             WHERE auth_connection_id = ?1 ORDER BY mailbox_ref_id",
        )
        .map_err(|error| error.to_string())?;
    statement
        .query_map([auth_connection_id.as_str()], |row| {
            Ok(MailboxRefId::new(row.get::<_, String>(0)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn load_mailbox_identity_mapping(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
) -> Result<MailboxIdentityMapping, String> {
    connection
        .query_row(
            "SELECT auth_connection_id, provider_kind, projection_kind,
                    capability_projection_revision, checkpoint_namespace,
                    created_at, updated_at
             FROM mailbox_identity_mappings WHERE mailbox_ref_id = ?1",
            [mailbox_ref_id.as_str()],
            |row| {
                let provider = row.get::<_, String>(1)?;
                let provider_kind = EmailProviderKind::parse(&provider).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        provider.len(),
                        rusqlite::types::Type::Text,
                        "unsupported email provider kind".into(),
                    )
                })?;
                let projection = row.get::<_, String>(2)?;
                let projection_kind = match projection.as_str() {
                    "mailbox" => MailboxProjectionKind::Mailbox,
                    "send_as" => MailboxProjectionKind::SendAs,
                    _ => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            projection.len(),
                            rusqlite::types::Type::Text,
                            "unsupported mailbox projection kind".into(),
                        ));
                    }
                };
                Ok(MailboxIdentityMapping {
                    mailbox_ref_id: mailbox_ref_id.clone(),
                    auth_connection_id: EmailAuthConnectionId::new(row.get::<_, String>(0)?),
                    provider_kind,
                    projection_kind,
                    capability_projection_revision: row.get(3)?,
                    checkpoint_namespace: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .map_err(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                "mailbox identity mapping not found".to_string()
            } else {
                error.to_string()
            }
        })
}

fn load_mailbox_credential_deletion_blockers(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
) -> Result<MailboxCredentialDeletionBlockers, String> {
    let recoveries = {
        let mut statement = connection
            .prepare(
                "SELECT key, action, state FROM idempotency_records
                 WHERE mailbox_ref_id = ?1
                   AND state IN ('in_progress','remote_started','remote_accepted','remote_verified')
                 ORDER BY created_at, key",
            )
            .map_err(|error| error.to_string())?;
        statement
            .query_map([mailbox_ref_id.as_str()], |row| {
                let stage = match row.get::<_, String>(2)?.as_str() {
                    "in_progress" => EmailRecoveryStage::InProgress,
                    "remote_started" => EmailRecoveryStage::RemoteStarted,
                    "remote_accepted" => EmailRecoveryStage::RemoteAccepted,
                    "remote_verified" => EmailRecoveryStage::RemoteVerified,
                    _ => unreachable!("query restricts active recovery stages"),
                };
                Ok(MailboxEmailRecoveryBlocker {
                    idempotency_key: row.get(0)?,
                    action: row.get(1)?,
                    stage,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    let sending_drafts = {
        let mut statement = connection
            .prepare(
                "SELECT id, revision, sending_idempotency_key FROM email_drafts
                 WHERE mailbox_ref_id = ?1 AND sending_idempotency_key IS NOT NULL
                 ORDER BY created_at, id",
            )
            .map_err(|error| error.to_string())?;
        statement
            .query_map([mailbox_ref_id.as_str()], |row| {
                Ok(MailboxSendingDraftBlocker {
                    draft_id: crate::EmailDraftId::new(row.get::<_, String>(0)?),
                    revision: row.get(1)?,
                    idempotency_key: row.get(2)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    Ok(MailboxCredentialDeletionBlockers {
        mailbox_ref_id: mailbox_ref_id.clone(),
        recoveries,
        sending_drafts,
    })
}

fn validate_mailbox_input(mut input: MailboxRefInput) -> Result<MailboxRefInput, String> {
    if !matches!(input.provider.as_str(), "resend" | "gmail") {
        return Err("unsupported mailbox provider".into());
    }
    Uuid::parse_str(input.id.as_str())
        .map_err(|_| "mailbox_ref_id must be the opaque vault credential UUID".to_string())?;
    input.label = input.label.trim().to_owned();
    if input.label.is_empty() {
        return Err("mailbox label is required".into());
    }
    input.email_address = input.email_address.trim().to_ascii_lowercase();
    let mut address_parts = input.email_address.split('@');
    if address_parts.next().unwrap_or_default().is_empty()
        || address_parts.next().unwrap_or_default().is_empty()
        || address_parts.next().is_some()
        || input.email_address.chars().any(char::is_whitespace)
    {
        return Err("mailbox email address is invalid".into());
    }
    Ok(input)
}

fn upsert_mailbox(connection: &Connection, input: &MailboxRefInput) -> Result<(), String> {
    let timestamp = now();
    let created_at = connection
        .query_row(
            "SELECT created_at FROM mailbox_refs WHERE id = ?1",
            [input.id.as_str()],
            |row| row.get::<_, u64>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(timestamp);
    connection
        .execute(
            "INSERT INTO mailbox_refs
               (id, provider, label, email_address, display_name, capabilities_json,
                available, last_error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET provider = excluded.provider,
               label = excluded.label, email_address = excluded.email_address,
               display_name = excluded.display_name,
               capabilities_json = excluded.capabilities_json,
               available = excluded.available, last_error = excluded.last_error,
               updated_at = excluded.updated_at",
            params![
                input.id.as_str(),
                input.provider,
                input.label,
                input.email_address,
                input.display_name,
                json(&input.capabilities)?,
                input.available,
                input.last_error,
                created_at,
                timestamp
            ],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn mailbox_ids(connection: &Connection) -> Result<Vec<String>, String> {
    let mut statement = connection
        .prepare("SELECT id FROM mailbox_refs WHERE provider = 'resend'")
        .map_err(|error| error.to_string())?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn list_mailboxes(connection: &Connection) -> Result<Vec<MailboxRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM mailbox_refs
             ORDER BY available DESC, label COLLATE NOCASE, id",
        )
        .map_err(|error| error.to_string())?;
    let ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    ids.into_iter()
        .map(|id| load_mailbox(connection, &MailboxRefId::new(id)))
        .collect()
}

fn mark_mailbox_credential_missing(
    transaction: &Transaction<'_>,
    mailbox_id: &MailboxRefId,
) -> Result<(), String> {
    let changed = transaction
        .execute(
            "UPDATE mailbox_refs
             SET available = 0,
                 last_error = 'credential is missing from the current vault snapshot',
                 updated_at = ?2
             WHERE id = ?1",
            params![mailbox_id.as_str(), now()],
        )
        .map_err(|error| error.to_string())?;
    if changed == 1 {
        Ok(())
    } else {
        Err("mailbox disappeared during credential reconciliation".into())
    }
}

pub(super) fn find_mailbox(
    connection: &Connection,
    id: &MailboxRefId,
) -> Result<Option<MailboxRecord>, String> {
    connection
        .query_row(
            "SELECT provider, label, email_address, display_name, capabilities_json,
                    available, last_error, created_at, updated_at
             FROM mailbox_refs WHERE id = ?1",
            [id.as_str()],
            |row| {
                let capabilities: String = row.get(4)?;
                Ok(MailboxRecord {
                    id: id.clone(),
                    provider: row.get(0)?,
                    label: row.get(1)?,
                    email_address: row.get(2)?,
                    display_name: row.get(3)?,
                    capabilities: serde_json::from_str::<Vec<EmailCapability>>(&capabilities)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                capabilities.len(),
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    available: row.get(5)?,
                    last_error: row.get(6)?,
                    created_at: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn load_mailbox(connection: &Connection, id: &MailboxRefId) -> Result<MailboxRecord, String> {
    find_mailbox(connection, id)?.ok_or_else(|| "mailbox not found".into())
}

fn load_auth_connection(
    connection: &Connection,
    mailbox_ref_id: &MailboxRefId,
) -> Result<crate::EmailAuthConnection, String> {
    let (id, provider, secret, label, available, created, updated) = connection
        .query_row(
            "SELECT auth.id, auth.provider_kind, auth.secret_record_id, auth.label,
                    auth.available, auth.created_at, auth.updated_at
             FROM mailbox_identity_mappings mapping
             JOIN email_auth_connections auth
               ON auth.id = mapping.auth_connection_id
              AND auth.provider_kind = mapping.provider_kind
             WHERE mapping.mailbox_ref_id = ?1",
            [mailbox_ref_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, u64>(5)?,
                    row.get::<_, u64>(6)?,
                ))
            },
        )
        .map_err(|error| {
            if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
                "mailbox auth connection mapping not found".to_string()
            } else {
                error.to_string()
            }
        })?;
    let provider_kind = crate::EmailProviderKind::parse(&provider)
        .ok_or_else(|| "mailbox auth connection provider kind is unsupported".to_string())?;
    Ok(crate::EmailAuthConnection {
        auth_connection_id: crate::EmailAuthConnectionId::new(id),
        provider_kind,
        secret_record_id: crate::EmailSecretRecordId::new(secret),
        label,
        available,
        created_at: created,
        updated_at: updated,
    })
}
