use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use super::{LocalVault, timestamp};
use crate::VaultError;

/// Typed local secret kinds accepted by the Email provider registry. Remote
/// OAuth grants intentionally have no variant and remain outside local-vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailSecretRecordKind {
    ResendApiKey,
    SmtpCredential,
    ImapCredential,
}

impl EmailSecretRecordKind {
    fn as_provider(self) -> &'static str {
        match self {
            Self::ResendApiKey => "email_resend_api_key",
            Self::SmtpCredential => "email_smtp_credential",
            Self::ImapCredential => "email_imap_credential",
        }
    }

    fn from_provider(value: &str) -> Option<Self> {
        match value {
            "email_resend_api_key" => Some(Self::ResendApiKey),
            "email_smtp_credential" => Some(Self::SmtpCredential),
            "email_imap_credential" => Some(Self::ImapCredential),
            _ => None,
        }
    }
}

/// Secret-bearing input owned by local-vault. It deliberately has no Debug,
/// Serialize, or public field surface and zeroizes on drop.
pub struct EmailSecretRecordSecret {
    value: Zeroizing<String>,
}

impl EmailSecretRecordSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: Zeroizing::new(value.into()),
        }
    }

    pub fn expose_secret(&self) -> &str {
        &self.value
    }
}

impl Zeroize for EmailSecretRecordSecret {
    fn zeroize(&mut self) {
        self.value.zeroize();
    }
}

impl ZeroizeOnDrop for EmailSecretRecordSecret {}

/// Secret-free metadata projection. The encrypted value is returned only as
/// a separate `EmailSecretRecordSecret` from an explicit get operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmailSecretRecord {
    pub id: Uuid,
    pub kind: EmailSecretRecordKind,
    pub label: String,
    pub metadata: serde_json::Value,
    pub disabled: bool,
    pub cooldown_until: Option<u64>,
    pub last_used_at: Option<u64>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
struct StoredEmailSecretRef<'a> {
    value: &'a str,
}

#[derive(Deserialize)]
struct StoredEmailSecret {
    value: Zeroizing<String>,
}

impl ZeroizeOnDrop for StoredEmailSecret {}

impl LocalVault {
    pub fn restore_email_secret_record(
        &self,
        record: &EmailSecretRecord,
        secret: &EmailSecretRecordSecret,
    ) -> Result<(), VaultError> {
        if record.kind != EmailSecretRecordKind::ResendApiKey
            || record.label.trim().is_empty()
            || secret.expose_secret().trim().is_empty()
        {
            return Err(VaultError::Serialization(
                "typed Email secret record is invalid".into(),
            ));
        }
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&StoredEmailSecretRef {
                value: secret.expose_secret(),
            })
            .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        let metadata_json = serde_json::to_string(&record.metadata)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        connection.execute("INSERT INTO service_credentials (id, provider, label, metadata_json, encrypted_secret, disabled, cooldown_until, last_used_at, last_error, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(id) DO UPDATE SET provider=excluded.provider, label=excluded.label, metadata_json=excluded.metadata_json, encrypted_secret=excluded.encrypted_secret, disabled=excluded.disabled, cooldown_until=excluded.cooldown_until, last_used_at=excluded.last_used_at, last_error=excluded.last_error, created_at=excluded.created_at, updated_at=excluded.updated_at", params![record.id.to_string(), record.kind.as_provider(), record.label, metadata_json, encrypted, record.disabled, record.cooldown_until.map(|value| i64::try_from(value).unwrap_or(i64::MAX)), record.last_used_at.map(|value| i64::try_from(value).unwrap_or(i64::MAX)), record.last_error, record.created_at, record.updated_at]).map(|_| ()).map_err(|error| VaultError::Database(error.to_string()))
    }

    /// Store one explicitly selected local Email credential in the existing
    /// encrypted credential table with a typed provider tag. The generic
    /// service-credential APIs never enumerate these rows.
    pub fn upsert_email_secret_record(
        &self,
        kind: EmailSecretRecordKind,
        label: impl Into<String>,
        metadata: serde_json::Value,
        secret: &EmailSecretRecordSecret,
    ) -> Result<EmailSecretRecord, VaultError> {
        let label = required_label(label.into())?;
        let metadata = validate_metadata(metadata)?;
        if secret.expose_secret().trim().is_empty() {
            return Err(VaultError::Serialization(
                "email secret record value is required".into(),
            ));
        }
        let provider = kind.as_provider();
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let existing: Option<(String, String, String)> = connection
            .query_row(
                "SELECT id, created_at, updated_at FROM service_credentials
                 WHERE provider = ?1 AND label = ?2",
                params![provider, label],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let (id, created_at, now) = existing
            .and_then(|(id, created_at, updated_at)| {
                Uuid::parse_str(&id).ok().map(|id| {
                    // `updated_at` is the typed cache revision. Wall-clock
                    // timestamps have second precision, so force a strictly
                    // increasing value when a rotation happens in one second.
                    let next = updated_at
                        .parse::<u64>()
                        .ok()
                        .map_or_else(|| now.clone(), |value| value.saturating_add(1).to_string());
                    let now = if now > next { now.clone() } else { next };
                    (id, created_at, now)
                })
            })
            .unwrap_or_else(|| (Uuid::new_v4(), now.clone(), now));
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&StoredEmailSecretRef {
                value: secret.expose_secret(),
            })
            .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        let metadata_json = serde_json::to_string(&metadata)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        connection
            .execute(
                "INSERT INTO service_credentials
                   (id, provider, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, NULL, NULL, NULL, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                   provider = excluded.provider,
                   label = excluded.label,
                   metadata_json = excluded.metadata_json,
                   encrypted_secret = excluded.encrypted_secret,
                   disabled = 0,
                   cooldown_until = NULL,
                   last_used_at = NULL,
                   last_error = NULL,
                   updated_at = excluded.updated_at",
                params![
                    id.to_string(),
                    provider,
                    label,
                    metadata_json,
                    encrypted,
                    created_at,
                    now,
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(EmailSecretRecord {
            id,
            kind,
            label,
            metadata,
            disabled: false,
            cooldown_until: None,
            last_used_at: None,
            last_error: None,
            created_at,
            updated_at: now,
        })
    }

    pub fn get_email_secret_record(
        &self,
        id: Uuid,
    ) -> Result<(EmailSecretRecord, EmailSecretRecordSecret), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let row = connection
            .query_row(
                "SELECT provider, label, metadata_json, encrypted_secret, disabled,
                        cooldown_until, last_used_at, last_error, created_at, updated_at
                 FROM service_credentials WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, bool>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?
            .ok_or(VaultError::ProfileNotFound)?;
        let kind = EmailSecretRecordKind::from_provider(&row.0).ok_or_else(|| {
            VaultError::Serialization("credential is not a typed Email secret record".into())
        })?;
        let metadata: serde_json::Value = serde_json::from_str(&row.2)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        validate_metadata(metadata.clone())?;
        let plaintext = self.cipher.decrypt(&row.3)?;
        let secret: StoredEmailSecret = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        Ok((
            EmailSecretRecord {
                id,
                kind,
                label: row.1,
                metadata,
                disabled: row.4,
                cooldown_until: row.5.map(|value| value as u64),
                last_used_at: row.6.map(|value| value as u64),
                last_error: row.7,
                created_at: row.8,
                updated_at: row.9,
            },
            EmailSecretRecordSecret {
                value: secret.value,
            },
        ))
    }

    /// Resolve a Creator connection's canonical bare secret-record UUID. The
    /// expected typed provider kind is mandatory; old generic service rows,
    /// malformed values, unknown records, and provider mismatches fail closed.
    pub fn get_email_secret_record_for_connection(
        &self,
        secret_record_id: &str,
        expected_kind: EmailSecretRecordKind,
    ) -> Result<(EmailSecretRecord, EmailSecretRecordSecret), VaultError> {
        let id = Uuid::parse_str(secret_record_id).map_err(|_| {
            VaultError::Serialization("email secret record reference is invalid".into())
        })?;
        let (record, secret) = self.get_email_secret_record(id)?;
        if record.kind != expected_kind {
            return Err(VaultError::Serialization(
                "email secret record provider kind does not match the typed connection".into(),
            ));
        }
        Ok((record, secret))
    }

    pub fn list_email_secret_records(
        &self,
        kind: EmailSecretRecordKind,
        available_only: bool,
    ) -> Result<Vec<EmailSecretRecord>, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let now = crate::store::unix_timestamp() as i64;
        let mut statement = connection
            .prepare(
                "SELECT id, label, metadata_json, disabled, cooldown_until,
                        last_used_at, last_error, created_at, updated_at
                 FROM service_credentials
                 WHERE provider = ?1
                   AND (?2 = 0 OR (disabled = 0 AND
                        (cooldown_until IS NULL OR cooldown_until <= ?3)))
                 ORDER BY created_at, id",
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let rows = statement
            .query_map(
                params![kind.as_provider(), i64::from(available_only), now],
                |row| {
                    let metadata_json: String = row.get(2)?;
                    let metadata = serde_json::from_str(&metadata_json).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            metadata_json.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?;
                    Ok(EmailSecretRecord {
                        id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                        kind,
                        label: row.get(1)?,
                        metadata,
                        disabled: row.get(3)?,
                        cooldown_until: row.get::<_, Option<i64>>(4)?.map(|value| value as u64),
                        last_used_at: row.get::<_, Option<i64>>(5)?.map(|value| value as u64),
                        last_error: row.get(6)?,
                        created_at: row.get(7)?,
                        updated_at: row.get(8)?,
                    })
                },
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let records = rows
            .map(|row| row.map_err(|error| VaultError::Database(error.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        records
            .into_iter()
            .map(|record| {
                validate_metadata(record.metadata.clone())?;
                Ok(record)
            })
            .collect()
    }

    pub fn delete_email_secret_record(
        &self,
        id: Uuid,
        expected_kind: EmailSecretRecordKind,
    ) -> Result<bool, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let provider = connection
            .query_row(
                "SELECT provider FROM service_credentials WHERE id = ?1",
                [id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?;
        if provider.as_deref() != Some(expected_kind.as_provider()) {
            return Err(VaultError::Serialization(
                "credential is not the expected typed Email secret record".into(),
            ));
        }
        connection
            .execute(
                "DELETE FROM service_credentials WHERE id = ?1",
                [id.to_string()],
            )
            .map(|changed| changed > 0)
            .map_err(|error| VaultError::Database(error.to_string()))
    }

    pub fn mark_email_secret_record_success(
        &self,
        id: Uuid,
        expected_kind: EmailSecretRecordKind,
    ) -> Result<(), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let now = crate::store::unix_timestamp() as i64;
        let changed = connection.execute("UPDATE service_credentials SET cooldown_until = CASE WHEN cooldown_until IS NOT NULL AND cooldown_until > ?2 THEN cooldown_until ELSE NULL END, last_error = CASE WHEN disabled OR (cooldown_until IS NOT NULL AND cooldown_until > ?2) THEN last_error ELSE NULL END, last_used_at = ?2, updated_at = ?3 WHERE id = ?1 AND provider = ?4", params![id.to_string(), now, timestamp(), expected_kind.as_provider()]).map_err(|error| VaultError::Database(error.to_string()))?;
        if changed == 0 {
            return Err(VaultError::Serialization(
                "credential is not the expected typed Email secret record".into(),
            ));
        }
        Ok(())
    }

    pub fn mark_email_secret_record_failure(
        &self,
        id: Uuid,
        expected_kind: EmailSecretRecordKind,
        error: &str,
        disable: bool,
        cooldown_until: Option<u64>,
    ) -> Result<(), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let changed = connection.execute("UPDATE service_credentials SET disabled = ?2, cooldown_until = ?3, last_error = ?4, updated_at = ?5 WHERE id = ?1 AND provider = ?6", params![id.to_string(), disable, cooldown_until.map(|value| i64::try_from(value).unwrap_or(i64::MAX)), error, timestamp(), expected_kind.as_provider()]).map_err(|error| VaultError::Database(error.to_string()))?;
        if changed == 0 {
            return Err(VaultError::Serialization(
                "credential is not the expected typed Email secret record".into(),
            ));
        }
        Ok(())
    }
}

fn required_label(label: String) -> Result<String, VaultError> {
    let label = label.trim().to_owned();
    if label.is_empty() {
        Err(VaultError::Serialization(
            "email secret record label is required".into(),
        ))
    } else {
        Ok(label)
    }
}

pub(super) fn validate_metadata(value: serde_json::Value) -> Result<serde_json::Value, VaultError> {
    let serde_json::Value::Object(object) = &value else {
        return Err(VaultError::Serialization(
            "email secret record metadata must be an object".into(),
        ));
    };
    let mut forbidden = Vec::new();
    collect_forbidden_keys(object, &mut forbidden);
    if let Some(key) = forbidden.into_iter().next() {
        return Err(VaultError::Serialization(format!(
            "email secret record metadata contains a secret-bearing field: {key}"
        )));
    }
    Ok(value)
}

fn collect_forbidden_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    output: &mut Vec<String>,
) {
    const FORBIDDEN: &[&str] = &[
        "secret",
        "token",
        "password",
        "api_key",
        "private_key",
        "client_secret",
        "authorization",
    ];
    for (key, value) in object {
        let lower = key.to_ascii_lowercase();
        if FORBIDDEN.iter().any(|part| lower.contains(part)) {
            output.push(key.clone());
        }
        if let serde_json::Value::Object(nested) = value {
            collect_forbidden_keys(nested, output);
        }
    }
}
