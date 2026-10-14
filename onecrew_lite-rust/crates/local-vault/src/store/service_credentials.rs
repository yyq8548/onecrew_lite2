use onecrew_local_domain::{ServiceCredential, ServiceCredentialSecret, ServiceProvider};
use rusqlite::{OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{ZeroizeOnDrop, Zeroizing};

use super::{LocalVault, timestamp, unix_timestamp};
use crate::VaultError;

/// The result of a health observation guarded by the credential revision it
/// read before provider work started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceCredentialHealthCommit {
    Applied { revision: String },
    Superseded,
}

impl LocalVault {
    pub fn upsert_service_credential(
        &self,
        provider: ServiceProvider,
        label: impl Into<String>,
        metadata: serde_json::Value,
        secret: &ServiceCredentialSecret,
    ) -> Result<ServiceCredential, VaultError> {
        if provider == ServiceProvider::CreatorSync {
            return Err(VaultError::Serialization(
                "Creator Sync credentials require the dedicated vault API".into(),
            ));
        }
        if provider == ServiceProvider::Resend {
            return Err(VaultError::Serialization(
                "Resend credentials require the dedicated Email secret API".into(),
            ));
        }
        self.upsert_service_credential_inner(provider, label.into(), metadata, secret)
    }

    pub(super) fn upsert_service_credential_inner(
        &self,
        provider: ServiceProvider,
        label: String,
        metadata: serde_json::Value,
        secret: &ServiceCredentialSecret,
    ) -> Result<ServiceCredential, VaultError> {
        if label.trim().is_empty() || secret.expose_secret().trim().is_empty() {
            return Err(VaultError::Serialization(
                "credential label and secret are required".into(),
            ));
        }
        let provider_value = provider.as_str();
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let existing: Option<(String, String, String)> = connection
            .query_row(
                "SELECT id, created_at, updated_at FROM service_credentials
                 WHERE provider = ?1 AND label = ?2",
                params![provider_value, label],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let (id, created_at, now) = match existing {
            Some((id, created_at, updated_at)) => {
                (id, created_at, next_revision(&updated_at, &now)?)
            }
            None => (Uuid::new_v4().to_string(), now.clone(), now.clone()),
        };
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&StoredServiceCredentialSecretRef {
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
                   label = excluded.label,
                   metadata_json = excluded.metadata_json,
                   encrypted_secret = excluded.encrypted_secret,
                   disabled = 0,
                   cooldown_until = NULL,
                   last_error = NULL,
                   updated_at = excluded.updated_at",
                params![
                    id,
                    provider_value,
                    label,
                    metadata_json,
                    encrypted,
                    created_at,
                    now
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        StoredServiceCredential {
            id,
            provider,
            label,
            metadata,
            disabled: false,
            cooldown_until: None,
            last_used_at: None,
            last_error: None,
            created_at,
            updated_at: now,
        }
        .into_credential()
    }

    /// Restore an in-memory credential snapshot after a cross-store
    /// reconciliation failure. This preserves the original encrypted value,
    /// metadata and health fields exactly; callers must never serialize or log
    /// the accompanying secret.
    pub fn restore_service_credential(
        &self,
        credential: &ServiceCredential,
        secret: &ServiceCredentialSecret,
    ) -> Result<(), VaultError> {
        if credential.provider == ServiceProvider::CreatorSync {
            return Err(VaultError::Serialization(
                "Creator Sync credentials require the dedicated vault API".into(),
            ));
        }
        if credential.provider == ServiceProvider::Resend {
            return Err(VaultError::Serialization(
                "Resend credentials require the dedicated Email secret API".into(),
            ));
        }
        if credential.label.trim().is_empty() || secret.expose_secret().trim().is_empty() {
            return Err(VaultError::Serialization(
                "credential label and secret are required".into(),
            ));
        }
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&StoredServiceCredentialSecretRef {
                value: secret.expose_secret(),
            })
            .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        let metadata_json = serde_json::to_string(&credential.metadata)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        connection
            .execute(
                "INSERT INTO service_credentials
                   (id, provider, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_used_at, last_error, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(id) DO UPDATE SET
                   provider = excluded.provider,
                   label = excluded.label,
                   metadata_json = excluded.metadata_json,
                   encrypted_secret = excluded.encrypted_secret,
                   disabled = excluded.disabled,
                   cooldown_until = excluded.cooldown_until,
                   last_used_at = excluded.last_used_at,
                   last_error = excluded.last_error,
                   created_at = excluded.created_at,
                   updated_at = excluded.updated_at",
                params![
                    credential.id.to_string(),
                    credential.provider.as_str(),
                    credential.label,
                    metadata_json,
                    encrypted,
                    credential.disabled,
                    credential.cooldown_until.map(|value| value as i64),
                    credential.last_used_at.map(|value| value as i64),
                    credential.last_error,
                    credential.created_at,
                    credential.updated_at,
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(())
    }

    pub fn list_service_credentials(
        &self,
        provider: ServiceProvider,
        available_only: bool,
    ) -> Result<Vec<ServiceCredential>, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let now = unix_timestamp() as i64;
        let mut statement = connection
            .prepare(
                "SELECT id, label, metadata_json, disabled, cooldown_until,
                        last_used_at, last_error, created_at, updated_at
                 FROM service_credentials
                 WHERE provider = ?1
                   AND (?2 = 0 OR (disabled = 0 AND
                        (cooldown_until IS NULL OR cooldown_until <= ?3)))
                 ORDER BY last_used_at IS NOT NULL, last_used_at ASC, created_at ASC",
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let rows = statement
            .query_map(
                params![provider.as_str(), i64::from(available_only), now],
                |row| {
                    StoredServiceCredentialRow::decode(
                        row,
                        ServiceCredentialRowProjection::List { provider },
                    )
                },
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        rows.map(|row| {
            let row = row.map_err(|error| VaultError::Database(error.to_string()))?;
            let (stored, encrypted_secret) = row.into_stored()?;
            debug_assert!(encrypted_secret.is_none());
            stored.into_credential()
        })
        .collect()
    }

    pub fn get_service_credential(
        &self,
        id: Uuid,
    ) -> Result<(ServiceCredential, ServiceCredentialSecret), VaultError> {
        self.get_service_credential_inner(id, false)
    }

    pub(super) fn get_service_credential_inner(
        &self,
        id: Uuid,
        allow_creator_sync: bool,
    ) -> Result<(ServiceCredential, ServiceCredentialSecret), VaultError> {
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
                    StoredServiceCredentialRow::decode(
                        row,
                        ServiceCredentialRowProjection::Get { id },
                    )
                },
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?
            .ok_or(VaultError::ProfileNotFound)?;
        let (stored, encrypted_secret) = row.into_stored()?;
        if stored.provider == ServiceProvider::CreatorSync && !allow_creator_sync {
            return Err(VaultError::Serialization(
                "Creator Sync credentials require the dedicated vault API".into(),
            ));
        }
        let secret = self.decrypt_service_credential_secret(
            encrypted_secret
                .as_deref()
                .expect("get projection always includes the encrypted secret"),
        )?;
        Ok((stored.into_credential()?, secret))
    }

    pub(super) fn decrypt_service_credential_secret(
        &self,
        encrypted: &[u8],
    ) -> Result<ServiceCredentialSecret, VaultError> {
        let plaintext = self.cipher.decrypt(encrypted)?;
        serde_json::from_slice::<StoredServiceCredentialSecret>(&plaintext)
            .map(|stored| ServiceCredentialSecret::from_zeroizing(stored.value))
            .map_err(|error| VaultError::Serialization(error.to_string()))
    }

    pub fn delete_service_credential(&self, id: Uuid) -> Result<bool, VaultError> {
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
        if provider.as_deref() == Some(ServiceProvider::CreatorSync.as_str()) {
            return Err(VaultError::Serialization(
                "Creator Sync credentials require the dedicated vault API".into(),
            ));
        }
        if provider.as_deref().is_some_and(is_typed_email_provider) {
            return Err(VaultError::Serialization(
                "typed Email secret records require the dedicated vault API".into(),
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

    /// Rename one credential in place without changing its encrypted secret,
    /// metadata, health state, or stable id. The single UPDATE is atomic at
    /// the vault database boundary; the provider/runtime owner is responsible
    /// for synchronizing its public reference and rolling this row back if
    /// that cross-store step fails.
    pub fn rename_service_credential(
        &self,
        id: Uuid,
        label: impl Into<String>,
    ) -> Result<ServiceCredential, VaultError> {
        let label = label.into().trim().to_owned();
        if label.is_empty() {
            return Err(VaultError::Serialization(
                "credential label is required".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let (provider, current_revision) = connection
            .query_row(
                "SELECT provider, updated_at FROM service_credentials WHERE id = ?1",
                [id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?
            .ok_or(VaultError::ProfileNotFound)?;
        if provider == ServiceProvider::CreatorSync.as_str() {
            return Err(VaultError::Serialization(
                "Creator Sync credentials require the dedicated vault API".into(),
            ));
        }
        if is_typed_email_provider(&provider) {
            return Err(VaultError::Serialization(
                "typed Email secret records require the dedicated vault API".into(),
            ));
        }
        let changed = connection
            .execute(
                "UPDATE service_credentials SET label = ?2, updated_at = ?3 WHERE id = ?1",
                params![
                    id.to_string(),
                    label,
                    next_revision(&current_revision, &timestamp())?
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        drop(connection);
        if changed == 0 {
            return Err(VaultError::ProfileNotFound);
        }
        self.get_service_credential(id)
            .map(|(credential, _)| credential)
    }

    /// Records a successful use without erasing newer negative state: a
    /// success that raced with a fresh cooldown or a disable must not
    /// re-enable the credential or shorten an active cooldown. It clears
    /// only an already-expired cooldown and, when the credential is healthy,
    /// the stale last error.
    pub fn mark_service_credential_success(&self, id: Uuid) -> Result<(), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        reject_creator_sync_generic_mutation(&connection, id)?;
        let now = unix_timestamp() as i64;
        let revision = next_revision_for_row(&connection, id, &timestamp())?;
        connection
            .execute(
                "UPDATE service_credentials SET
                   cooldown_until = CASE
                     WHEN cooldown_until IS NOT NULL AND cooldown_until > ?2 THEN cooldown_until
                     ELSE NULL END,
                   last_error = CASE
                     WHEN disabled OR (cooldown_until IS NOT NULL AND cooldown_until > ?2)
                     THEN last_error
                     ELSE NULL END,
                   last_used_at = ?2, updated_at = ?3 WHERE id = ?1",
                params![id.to_string(), now, revision],
            )
            .map(|_| ())
            .map_err(|error| VaultError::Database(error.to_string()))
    }

    /// Records a successful use only when `updated_at` still matches the
    /// revision observed before provider work began. A newer rotation, health
    /// observation, or deletion wins without treating the old result as an
    /// error in the vault itself.
    pub fn mark_service_credential_success_if_revision(
        &self,
        id: Uuid,
        expected_updated_at: &str,
    ) -> Result<ServiceCredentialHealthCommit, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        reject_creator_sync_generic_mutation(&connection, id)?;
        let now = unix_timestamp() as i64;
        let next = next_revision_for_row(&connection, id, &timestamp())?;
        let changed = connection
            .execute(
                "UPDATE service_credentials SET
                   cooldown_until = CASE
                     WHEN cooldown_until IS NOT NULL AND cooldown_until > ?2 THEN cooldown_until
                     ELSE NULL END,
                   last_error = CASE
                     WHEN disabled OR (cooldown_until IS NOT NULL AND cooldown_until > ?2)
                     THEN last_error
                     ELSE NULL END,
                   last_used_at = ?2, updated_at = ?3
                 WHERE id = ?1 AND updated_at = ?4",
                params![id.to_string(), now, next, expected_updated_at],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(if changed == 0 {
            ServiceCredentialHealthCommit::Superseded
        } else {
            ServiceCredentialHealthCommit::Applied { revision: next }
        })
    }

    pub fn mark_service_credential_failure(
        &self,
        id: Uuid,
        error: &str,
        disable: bool,
        cooldown_until: Option<u64>,
    ) -> Result<(), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        reject_creator_sync_generic_mutation(&connection, id)?;
        let revision = next_revision_for_row(&connection, id, &timestamp())?;
        connection
            .execute(
                "UPDATE service_credentials SET disabled = ?2, cooldown_until = ?3,
                   last_error = CASE WHEN provider = 'creator_sync' THEN NULL ELSE ?4 END,
                   updated_at = ?5 WHERE id = ?1",
                params![
                    id.to_string(),
                    disable,
                    cooldown_until.map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
                    error,
                    revision
                ],
            )
            .map(|_| ())
            .map_err(|error| VaultError::Database(error.to_string()))
    }

    /// Records a failed use only when `updated_at` still matches the revision
    /// observed before provider work began. The failure text is expected to be
    /// the already-classified, secret-free health message from the owner.
    pub fn mark_service_credential_failure_if_revision(
        &self,
        id: Uuid,
        expected_updated_at: &str,
        error: &str,
        disable: bool,
        cooldown_until: Option<u64>,
    ) -> Result<ServiceCredentialHealthCommit, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        reject_creator_sync_generic_mutation(&connection, id)?;
        let next = next_revision_for_row(&connection, id, &timestamp())?;
        let changed = connection
            .execute(
                "UPDATE service_credentials SET disabled = ?2, cooldown_until = ?3,
                   last_error = CASE WHEN provider = 'creator_sync' THEN NULL ELSE ?4 END,
                   updated_at = ?5
                 WHERE id = ?1 AND updated_at = ?6",
                params![
                    id.to_string(),
                    disable,
                    cooldown_until.map(|value| i64::try_from(value).unwrap_or(i64::MAX)),
                    error,
                    next,
                    expected_updated_at,
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(if changed == 0 {
            ServiceCredentialHealthCommit::Superseded
        } else {
            ServiceCredentialHealthCommit::Applied { revision: next }
        })
    }
}

fn next_revision_for_row(
    connection: &rusqlite::Connection,
    id: Uuid,
    wall_clock: &str,
) -> Result<String, VaultError> {
    let current: Option<String> = connection
        .query_row(
            "SELECT updated_at FROM service_credentials WHERE id = ?1",
            [id.to_string()],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| VaultError::Database(error.to_string()))?;
    match current.as_deref() {
        Some(value) => next_revision(value, wall_clock),
        None => Ok(wall_clock.to_owned()),
    }
}

fn next_revision(current: &str, wall_clock: &str) -> Result<String, VaultError> {
    let current = current
        .parse::<u64>()
        .map_err(|_| VaultError::Serialization("credential revision is invalid".into()))?;
    let wall_clock = wall_clock
        .parse::<u64>()
        .map_err(|_| VaultError::Serialization("credential revision clock is invalid".into()))?;
    let next = current
        .checked_add(1)
        .ok_or_else(|| VaultError::Serialization("credential revision is exhausted".into()))?;
    Ok(next.max(wall_clock).to_string())
}

/// Private persistence shape. Keeping serde here lets the Vault encrypt a
/// stable JSON payload without making the public secret type serializable.
#[derive(Serialize)]
struct StoredServiceCredentialSecretRef<'a> {
    value: &'a str,
}

#[derive(Deserialize)]
struct StoredServiceCredentialSecret {
    value: Zeroizing<String>,
}

impl ZeroizeOnDrop for StoredServiceCredentialSecret {}

enum ServiceCredentialRowProjection {
    List { provider: ServiceProvider },
    Get { id: Uuid },
}

enum StoredServiceCredentialProvider {
    Known(ServiceProvider),
    Encoded(String),
}

struct StoredServiceCredentialRow {
    id: String,
    provider: StoredServiceCredentialProvider,
    label: String,
    metadata_json: String,
    encrypted_secret: Option<Vec<u8>>,
    disabled: bool,
    cooldown_until: Option<i64>,
    last_used_at: Option<i64>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl StoredServiceCredentialRow {
    fn decode(row: &Row<'_>, projection: ServiceCredentialRowProjection) -> rusqlite::Result<Self> {
        match projection {
            ServiceCredentialRowProjection::List { provider } => Ok(Self {
                id: row.get(0)?,
                provider: StoredServiceCredentialProvider::Known(provider),
                label: row.get(1)?,
                metadata_json: row.get(2)?,
                encrypted_secret: None,
                disabled: row.get(3)?,
                cooldown_until: row.get(4)?,
                last_used_at: row.get(5)?,
                last_error: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            }),
            ServiceCredentialRowProjection::Get { id } => Ok(Self {
                id: id.to_string(),
                provider: StoredServiceCredentialProvider::Encoded(row.get(0)?),
                label: row.get(1)?,
                metadata_json: row.get(2)?,
                encrypted_secret: Some(row.get(3)?),
                disabled: row.get(4)?,
                cooldown_until: row.get(5)?,
                last_used_at: row.get(6)?,
                last_error: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            }),
        }
    }

    fn into_stored(self) -> Result<(StoredServiceCredential, Option<Vec<u8>>), VaultError> {
        let provider = match self.provider {
            StoredServiceCredentialProvider::Known(provider) => provider,
            StoredServiceCredentialProvider::Encoded(provider) => {
                parse_service_provider(&provider)?
            }
        };
        let metadata = serde_json::from_str(&self.metadata_json)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        Ok((
            StoredServiceCredential {
                id: self.id,
                provider,
                label: self.label,
                metadata,
                disabled: self.disabled,
                cooldown_until: self.cooldown_until.map(|value| value as u64),
                last_used_at: self.last_used_at.map(|value| value as u64),
                last_error: self.last_error,
                created_at: self.created_at,
                updated_at: self.updated_at,
            },
            self.encrypted_secret,
        ))
    }
}

fn parse_service_provider(value: &str) -> Result<ServiceProvider, VaultError> {
    match value {
        "youtube" => Ok(ServiceProvider::Youtube),
        "resend" => Ok(ServiceProvider::Resend),
        "creator_sync" => Ok(ServiceProvider::CreatorSync),
        other => Err(VaultError::Serialization(format!(
            "unsupported service provider {other}"
        ))),
    }
}

fn reject_creator_sync_generic_mutation(
    connection: &rusqlite::Connection,
    id: Uuid,
) -> Result<(), VaultError> {
    let provider = connection
        .query_row(
            "SELECT provider FROM service_credentials WHERE id = ?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| VaultError::Database(error.to_string()))?;
    if provider.as_deref() == Some(ServiceProvider::CreatorSync.as_str()) {
        Err(VaultError::Serialization(
            "Creator Sync credentials require the dedicated vault API".into(),
        ))
    } else if provider.as_deref().is_some_and(is_typed_email_provider) {
        Err(VaultError::Serialization(
            "typed Email secret records require the dedicated vault API".into(),
        ))
    } else {
        Ok(())
    }
}

fn is_typed_email_provider(value: &str) -> bool {
    matches!(
        value,
        "email_resend_api_key" | "email_smtp_credential" | "email_imap_credential"
    )
}

struct StoredServiceCredential {
    id: String,
    provider: ServiceProvider,
    label: String,
    metadata: serde_json::Value,
    disabled: bool,
    cooldown_until: Option<u64>,
    last_used_at: Option<u64>,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

impl StoredServiceCredential {
    fn into_credential(self) -> Result<ServiceCredential, VaultError> {
        Ok(ServiceCredential {
            id: Uuid::parse_str(&self.id)
                .map_err(|error| VaultError::Serialization(error.to_string()))?,
            provider: self.provider,
            label: self.label,
            metadata: self.metadata,
            disabled: self.disabled,
            cooldown_until: self.cooldown_until,
            last_used_at: self.last_used_at,
            last_error: self.last_error,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
