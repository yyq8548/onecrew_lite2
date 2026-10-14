use onecrew_local_domain::{
    CreatorSyncCredentialMetadata, CreatorSyncCredentialStatus, ServiceCredentialSecret,
    ServiceProvider,
};
use rusqlite::{Connection, TransactionBehavior, params};
use uuid::Uuid;

use super::{LocalVault, unix_timestamp};
use crate::VaultError;

const CREATOR_SYNC_CREDENTIAL_LABEL: &str = "creator-sync";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCreatorSyncCredential {
    pub vault_id: Uuid,
    pub metadata: CreatorSyncCredentialMetadata,
}

pub enum VaultCreatorSyncCredentialState {
    Active {
        credential: VaultCreatorSyncCredential,
        bearer: ServiceCredentialSecret,
    },
    NonActive {
        credential: VaultCreatorSyncCredential,
    },
}

impl VaultCreatorSyncCredentialState {
    pub fn credential(&self) -> &VaultCreatorSyncCredential {
        match self {
            Self::Active { credential, .. } | Self::NonActive { credential } => credential,
        }
    }
}

impl LocalVault {
    pub fn save_creator_sync_credential(
        &self,
        metadata: &CreatorSyncCredentialMetadata,
        bearer: &ServiceCredentialSecret,
    ) -> Result<VaultCreatorSyncCredential, VaultError> {
        validate_creator_sync_metadata(metadata)?;
        if metadata.status != CreatorSyncCredentialStatus::Active {
            return Err(invalid_creator_sync_credential());
        }
        validate_creator_sync_bearer(metadata, bearer.expose_secret())?;
        ensure_singleton_shape(self)?;

        self.upsert_service_credential_inner(
            ServiceProvider::CreatorSync,
            CREATOR_SYNC_CREDENTIAL_LABEL.to_owned(),
            serde_json::to_value(metadata)
                .map_err(|error| VaultError::Serialization(error.to_string()))?,
            bearer,
        )?;

        match self
            .get_creator_sync_credential()?
            .ok_or_else(invalid_creator_sync_credential)?
        {
            VaultCreatorSyncCredentialState::Active {
                credential,
                bearer: saved_bearer,
            } if credential.metadata == *metadata
                && saved_bearer.expose_secret() == bearer.expose_secret() =>
            {
                Ok(credential)
            }
            _ => Err(VaultError::Serialization(
                "Creator Sync credential readback did not match the submitted value".into(),
            )),
        }
    }

    pub fn get_creator_sync_credential(
        &self,
    ) -> Result<Option<VaultCreatorSyncCredentialState>, VaultError> {
        let credentials = self.list_service_credentials(ServiceProvider::CreatorSync, false)?;
        let credential = match credentials.as_slice() {
            [] => return Ok(None),
            [credential] if credential.label == CREATOR_SYNC_CREDENTIAL_LABEL => credential,
            _ => {
                return Err(VaultError::Serialization(
                    "Creator Sync credential identity is ambiguous".into(),
                ));
            }
        };
        let metadata = decode_creator_sync_metadata(&credential.metadata)?;
        validate_creator_sync_state(&metadata, credential.disabled)?;
        if credential.cooldown_until.is_some() || credential.last_error.is_some() {
            return Err(invalid_creator_sync_credential());
        }
        let record = VaultCreatorSyncCredential {
            vault_id: credential.id,
            metadata,
        };
        if record.metadata.status != CreatorSyncCredentialStatus::Active {
            return Ok(Some(VaultCreatorSyncCredentialState::NonActive {
                credential: record,
            }));
        }

        let (stored, bearer) = self.get_service_credential_inner(credential.id, true)?;
        if stored.provider != ServiceProvider::CreatorSync
            || stored.label != CREATOR_SYNC_CREDENTIAL_LABEL
            || stored.metadata != credential.metadata
            || stored.disabled
        {
            return Err(invalid_creator_sync_credential());
        }
        validate_creator_sync_bearer(&record.metadata, bearer.expose_secret())?;
        Ok(Some(VaultCreatorSyncCredentialState::Active {
            credential: record,
            bearer,
        }))
    }

    pub fn invalidate_creator_sync_credential(
        &self,
    ) -> Result<Option<VaultCreatorSyncCredential>, VaultError> {
        self.invalidate_creator_sync_credential_with_readback_hook(|| Ok(()))
    }

    pub fn delete_creator_sync_credential(&self) -> Result<bool, VaultError> {
        self.delete_creator_sync_credential_with_readback_hook(|| Ok(()))
    }

    pub(super) fn invalidate_creator_sync_credential_with_readback_hook(
        &self,
        before_readback: impl FnOnce() -> Result<(), VaultError>,
    ) -> Result<Option<VaultCreatorSyncCredential>, VaultError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let rows = load_creator_sync_rows(&transaction)?;
        let row = match rows.as_slice() {
            [] => {
                transaction.commit().map_err(database_error)?;
                return Ok(None);
            }
            [row] => row,
            _ => return Err(ambiguous_creator_sync_credential()),
        };
        let bearer = self.decrypt_service_credential_secret(&row.encrypted_secret)?;
        validate_creator_sync_row(
            &row.label,
            &row.metadata_json,
            bearer.expose_secret(),
            row.disabled,
            row.cooldown_until,
        )?;
        let vault_id = Uuid::parse_str(&row.id).map_err(|_| invalid_creator_sync_credential())?;
        let original_encrypted = row.encrypted_secret.clone();
        let mut metadata =
            serde_json::from_str::<CreatorSyncCredentialMetadata>(&row.metadata_json)
                .map_err(|_| invalid_creator_sync_credential())?;
        drop(bearer);

        if metadata.status != CreatorSyncCredentialStatus::Invalidated {
            metadata.status = CreatorSyncCredentialStatus::Invalidated;
            metadata.updated_at = metadata.updated_at.saturating_add(1).max(unix_timestamp());
            let metadata_json = serde_json::to_string(&metadata)
                .map_err(|error| VaultError::Serialization(error.to_string()))?;
            let changed = transaction
                .execute(
                    "UPDATE service_credentials
                     SET metadata_json = ?2, disabled = 1, cooldown_until = NULL,
                         last_error = NULL, updated_at = ?3
                     WHERE id = ?1 AND provider = 'creator_sync'
                       AND label = 'creator-sync'",
                    params![
                        vault_id.to_string(),
                        metadata_json,
                        metadata.updated_at.to_string()
                    ],
                )
                .map_err(database_error)?;
            if changed != 1 {
                return Err(invalid_creator_sync_credential());
            }
        }

        before_readback()?;
        let readback = load_creator_sync_rows(&transaction)?;
        let [readback] = readback.as_slice() else {
            return Err(invalid_creator_sync_credential());
        };
        let readback_metadata =
            serde_json::from_str::<CreatorSyncCredentialMetadata>(&readback.metadata_json)
                .map_err(|_| invalid_creator_sync_credential())?;
        if readback.id != vault_id.to_string()
            || readback.label != CREATOR_SYNC_CREDENTIAL_LABEL
            || readback_metadata != metadata
            || !readback.disabled
            || readback.last_error.is_some()
            || readback.cooldown_until.is_some()
            || readback.updated_at != metadata.updated_at.to_string()
            || readback.encrypted_secret != original_encrypted
        {
            return Err(invalid_creator_sync_credential());
        }
        validate_creator_sync_state(&readback_metadata, readback.disabled)?;
        transaction.commit().map_err(database_error)?;
        Ok(Some(VaultCreatorSyncCredential {
            vault_id,
            metadata: readback_metadata,
        }))
    }

    pub(super) fn delete_creator_sync_credential_with_readback_hook(
        &self,
        before_readback: impl FnOnce() -> Result<(), VaultError>,
    ) -> Result<bool, VaultError> {
        let Some(tombstone) = self.invalidate_creator_sync_credential()? else {
            return Ok(false);
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(database_error)?;
        let changed = transaction
            .execute(
                "DELETE FROM service_credentials
                 WHERE id = ?1 AND provider = 'creator_sync'
                   AND label = 'creator-sync' AND disabled = 1",
                [tombstone.vault_id.to_string()],
            )
            .map_err(database_error)?;
        if changed != 1 {
            return Err(invalid_creator_sync_credential());
        }
        before_readback()?;
        let remaining = transaction
            .query_row(
                "SELECT COUNT(*) FROM service_credentials WHERE provider = 'creator_sync'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(database_error)?;
        if remaining != 0 {
            return Err(invalid_creator_sync_credential());
        }
        transaction.commit().map_err(database_error)?;
        Ok(true)
    }
}

fn ensure_singleton_shape(vault: &LocalVault) -> Result<(), VaultError> {
    vault.get_creator_sync_credential().map(|_| ())
}

pub(super) fn decode_creator_sync_metadata(
    value: &serde_json::Value,
) -> Result<CreatorSyncCredentialMetadata, VaultError> {
    let metadata = serde_json::from_value::<CreatorSyncCredentialMetadata>(value.clone())
        .map_err(|_| invalid_creator_sync_credential())?;
    validate_creator_sync_metadata(&metadata)?;
    Ok(metadata)
}

pub(super) fn validate_creator_sync_row(
    label: &str,
    metadata_json: &str,
    bearer: &str,
    disabled: bool,
    cooldown_until: Option<i64>,
) -> Result<(), VaultError> {
    if label != CREATOR_SYNC_CREDENTIAL_LABEL {
        return Err(invalid_creator_sync_credential());
    }
    let metadata = serde_json::from_str::<CreatorSyncCredentialMetadata>(metadata_json)
        .map_err(|_| invalid_creator_sync_credential())?;
    validate_creator_sync_metadata(&metadata)?;
    validate_creator_sync_state(&metadata, disabled)?;
    if cooldown_until.is_some() {
        return Err(invalid_creator_sync_credential());
    }
    validate_creator_sync_bearer(&metadata, bearer)
}

fn validate_creator_sync_metadata(
    metadata: &CreatorSyncCredentialMetadata,
) -> Result<(), VaultError> {
    if metadata.installation_id == metadata.credential_id
        || !bounded_access_identifier(&metadata.access_user_id)
        || !bounded_access_identifier(&metadata.access_device_id)
        || !bounded_access_identifier(&metadata.access_pairing_id)
        || !valid_app_version(&metadata.app_version)
        || metadata.revision == 0
        || metadata.protocol_version == 0
        || metadata.accepted_bundle.format_version == 0
        || metadata.accepted_bundle.max_snapshot_bytes == 0
        || metadata.accepted_bundle.max_chunk_bytes == 0
        || metadata.accepted_bundle.max_chunks == 0
        || metadata.accepted_bundle.max_namespace_staged_bytes == 0
        || metadata.accepted_bundle.max_access_user_staged_bytes == 0
        || metadata.updated_at < metadata.created_at
        || matches!(
            (metadata.status, metadata.revoked_at),
            (CreatorSyncCredentialStatus::Active, Some(_))
                | (CreatorSyncCredentialStatus::Revoked, None)
        )
    {
        return Err(invalid_creator_sync_credential());
    }
    Ok(())
}

fn validate_creator_sync_state(
    metadata: &CreatorSyncCredentialMetadata,
    disabled: bool,
) -> Result<(), VaultError> {
    let valid = match metadata.status {
        CreatorSyncCredentialStatus::Active => !disabled,
        CreatorSyncCredentialStatus::Revoked | CreatorSyncCredentialStatus::Invalidated => disabled,
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_creator_sync_credential())
    }
}

fn validate_creator_sync_bearer(
    metadata: &CreatorSyncCredentialMetadata,
    bearer: &str,
) -> Result<(), VaultError> {
    let expected_prefix = format!("{}_", metadata.credential_id);
    let Some(secret) = bearer.strip_prefix(&expected_prefix) else {
        return Err(invalid_creator_sync_credential());
    };
    if !(80..=165).contains(&bearer.len())
        || !(43..=128).contains(&secret.len())
        || !secret
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(invalid_creator_sync_credential());
    }
    Ok(())
}

fn bounded_access_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_app_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'+' | b'-'))
}

fn load_creator_sync_rows(connection: &Connection) -> Result<Vec<CreatorSyncRow>, VaultError> {
    let mut statement = connection
        .prepare(
            "SELECT id, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_error, updated_at
             FROM service_credentials WHERE provider = 'creator_sync' ORDER BY id",
        )
        .map_err(database_error)?;
    statement
        .query_map([], |row| {
            Ok(CreatorSyncRow {
                id: row.get(0)?,
                label: row.get(1)?,
                metadata_json: row.get(2)?,
                encrypted_secret: row.get(3)?,
                disabled: row.get(4)?,
                cooldown_until: row.get(5)?,
                last_error: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(database_error)?
        .map(|row| row.map_err(database_error))
        .collect()
}

struct CreatorSyncRow {
    id: String,
    label: String,
    metadata_json: String,
    encrypted_secret: Vec<u8>,
    disabled: bool,
    cooldown_until: Option<i64>,
    last_error: Option<String>,
    updated_at: String,
}

fn ambiguous_creator_sync_credential() -> VaultError {
    VaultError::Serialization("Creator Sync credential identity is ambiguous".into())
}

fn invalid_creator_sync_credential() -> VaultError {
    VaultError::Serialization("stored Creator Sync credential is invalid".into())
}

fn database_error(error: rusqlite::Error) -> VaultError {
    VaultError::Database(error.to_string())
}
