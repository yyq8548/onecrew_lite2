mod creator_sync;
mod email_secrets;
mod profiles;
mod schema;
mod service_credentials;
mod upgrade;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use onecrew_local_domain::{Platform, ProfileSecret};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{VaultError, crypto::VaultCipher, path_security::AnchoredParentPath};

pub use creator_sync::{VaultCreatorSyncCredential, VaultCreatorSyncCredentialState};
pub use email_secrets::{EmailSecretRecord, EmailSecretRecordKind, EmailSecretRecordSecret};
pub use service_credentials::ServiceCredentialHealthCommit;

pub use upgrade::{
    VaultDataUpgradeError, VaultDataUpgradeFailureKind, VaultDataUpgradeResult,
    VaultDataUpgradeStatus,
};

const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn sqlite_read_only_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW
}

pub(super) fn sqlite_read_write_flags() -> OpenFlags {
    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW
}

fn sqlite_read_write_create_flags() -> OpenFlags {
    sqlite_read_write_flags() | OpenFlags::SQLITE_OPEN_CREATE
}

pub(super) fn open_sqlite_connection(
    path: &Path,
    flags: OpenFlags,
) -> Result<Connection, VaultError> {
    let anchored = AnchoredParentPath::new(path)
        .map_err(|_| VaultError::Database("database parent path is unsafe".into()))?;
    let connection = Connection::open_with_flags(anchored.resolved(), flags)
        .map_err(|error| VaultError::Database(error.to_string()))?;
    anchored
        .verify_parent()
        .map_err(|_| VaultError::Database("database parent identity changed".into()))?;
    Ok(connection)
}

#[derive(Clone)]
pub struct LocalVault {
    connection: Arc<Mutex<Connection>>,
    cipher: Arc<VaultCipher>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultDataStatus {
    Fresh,
    Ready,
    UpgradeRequired {
        from_version: i64,
        to_version: i64,
    },
    Unsupported {
        created_by_newer_version: bool,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultProfileDirectory {
    pub profile_id: Uuid,
    pub platform: Platform,
    pub browser_profile_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultDataInspection {
    pub status: VaultDataStatus,
    pub profile_directories: Vec<VaultProfileDirectory>,
}

#[derive(Deserialize)]
struct StoredCredentialSecret {
    value: Zeroizing<String>,
}

impl LocalVault {
    /// Open only an existing current vault. Never creates keys, tables, or upgrades data.
    pub fn open_existing_read_only(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        let path = path.as_ref();
        let connection = open_sqlite_connection(path, sqlite_read_only_flags())?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|error| VaultError::Database(error.to_string()))?;
        if schema::inspect_existing_schema(&connection)? != schema::ExistingSchemaStatus::Ready {
            return Err(VaultError::Database(
                "vault upgrade requires the service startup owner".into(),
            ));
        }
        let cipher = VaultCipher::load_existing(&path.with_file_name("vault.key"))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher: Arc::new(cipher),
        })
    }

    pub fn inspect(path: impl AsRef<Path>) -> VaultDataInspection {
        match inspect_existing(path.as_ref()) {
            Ok(inspection) => inspection,
            Err(error) => VaultDataInspection {
                status: VaultDataStatus::Unsupported {
                    created_by_newer_version: detected_schema_version(path.as_ref())
                        .is_some_and(|version| version > schema::CURRENT_SCHEMA_VERSION),
                    reason: error.to_string(),
                },
                profile_directories: Vec::new(),
            },
        }
    }

    /// Opens the vault at `path`, creating its parent directory, database, and
    /// master key when they do not yet exist.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] when the directory or SQLite database cannot be
    /// opened, the schema cannot be initialized or validated, or the master
    /// key cannot be loaded or created securely.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VaultError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| VaultError::Database(error.to_string()))?;
        }
        // Master key file lives next to the DB (e.g. .../vault.key).
        let key_path = path.as_ref().with_file_name("vault.key");
        let connection = open_sqlite_connection(path.as_ref(), sqlite_read_write_create_flags())?;
        connection
            .busy_timeout(SQLITE_BUSY_TIMEOUT)
            .map_err(|error| VaultError::Database(error.to_string()))?;
        schema::initialize_schema(&connection)?;
        connection
            .execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher: Arc::new(VaultCipher::load_or_create(&key_path)?),
        })
    }
}

fn detected_schema_version(path: &Path) -> Option<i64> {
    let connection = open_sqlite_connection(path, sqlite_read_only_flags()).ok()?;
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .ok()
}

fn inspect_existing(path: &Path) -> Result<VaultDataInspection, VaultError> {
    let key_path = path.with_file_name("vault.key");
    match (path.exists(), key_path.exists()) {
        (false, false) => {
            return Ok(VaultDataInspection {
                status: VaultDataStatus::Fresh,
                profile_directories: Vec::new(),
            });
        }
        (false, true) => {
            return Err(VaultError::Database(
                "credential key exists without its local account store".into(),
            ));
        }
        (true, false) => {
            return Err(VaultError::Keychain(
                "credential key is missing for the local account store".into(),
            ));
        }
        (true, true) => {}
    }
    let connection = open_sqlite_connection(path, sqlite_read_only_flags())?;
    connection
        .busy_timeout(SQLITE_BUSY_TIMEOUT)
        .map_err(|error| VaultError::Database(error.to_string()))?;
    let schema_status = schema::inspect_existing_schema(&connection)?;
    let integrity = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|error| VaultError::Database(error.to_string()))?;
    if integrity != "ok" {
        return Err(VaultError::Database(format!(
            "local account data integrity check failed: {integrity}"
        )));
    }
    let cipher = VaultCipher::load_existing(&key_path)?;
    let mut profile_directories = Vec::new();
    let mut profiles = connection
        .prepare(
            "SELECT id, platform, provider_account_id, encrypted_secret FROM profiles ORDER BY id",
        )
        .map_err(|error| VaultError::Database(error.to_string()))?;
    let profile_rows = profiles
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|error| VaultError::Database(error.to_string()))?;
    for row in profile_rows {
        let (id, platform, provider_account_id, encrypted) =
            row.map_err(|error| VaultError::Database(error.to_string()))?;
        let profile_id =
            Uuid::parse_str(&id).map_err(|error| VaultError::Serialization(error.to_string()))?;
        let platform = match platform.as_str() {
            "x" => Platform::X,
            "instagram" => Platform::Instagram,
            "telegram" => Platform::Telegram,
            _ => {
                return Err(VaultError::Serialization(
                    "unsupported stored profile platform".into(),
                ));
            }
        };
        let plaintext = cipher.decrypt(&encrypted)?;
        let secret: ProfileSecret = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        if provider_account_id.trim().is_empty()
            || secret.provider_account_id.trim().is_empty()
            || provider_account_id != secret.provider_account_id
        {
            return Err(VaultError::Serialization(
                "stored profile identity does not match its encrypted credential".into(),
            ));
        }
        profile_directories.push(VaultProfileDirectory {
            profile_id,
            platform,
            browser_profile_dir: secret.browser_profile_dir,
        });
    }
    drop(profiles);

    let mut creator_sync_rows = 0_u64;
    let mut credentials = connection
        .prepare(
            "SELECT id, provider, label, metadata_json, encrypted_secret, disabled,
                    cooldown_until, last_error
             FROM service_credentials ORDER BY id",
        )
        .map_err(|error| VaultError::Database(error.to_string()))?;
    let credential_rows = credentials
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
                row.get::<_, bool>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|error| VaultError::Database(error.to_string()))?;
    for row in credential_rows {
        let (id, provider, label, metadata, encrypted, disabled, cooldown_until, last_error) =
            row.map_err(|error| VaultError::Database(error.to_string()))?;
        Uuid::parse_str(&id).map_err(|error| VaultError::Serialization(error.to_string()))?;
        let provider_is_valid = match schema_status {
            schema::ExistingSchemaStatus::Ready => {
                matches!(
                    provider.as_str(),
                    "youtube"
                        | "creator_sync"
                        | "email_resend_api_key"
                        | "email_smtp_credential"
                        | "email_imap_credential"
                )
            }
            schema::ExistingSchemaStatus::UpgradeRequired {
                from_version: 2, ..
            } => matches!(provider.as_str(), "youtube" | "resend"),
            schema::ExistingSchemaStatus::UpgradeRequired {
                from_version: 3, ..
            } => matches!(
                provider.as_str(),
                "youtube"
                    | "resend"
                    | "creator_sync"
                    | "email_resend_api_key"
                    | "email_smtp_credential"
                    | "email_imap_credential"
            ),
            schema::ExistingSchemaStatus::UpgradeRequired { .. } => false,
        };
        if !provider_is_valid || label.trim().is_empty() {
            return Err(VaultError::Serialization(
                "stored service credential metadata is invalid".into(),
            ));
        }
        let metadata_value = serde_json::from_str::<serde_json::Value>(&metadata)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        let legacy_resend_requires_typed_metadata = provider == "resend"
            && matches!(
                schema_status,
                schema::ExistingSchemaStatus::UpgradeRequired {
                    from_version: 3,
                    ..
                }
            );
        if provider.starts_with("email_") || legacy_resend_requires_typed_metadata {
            email_secrets::validate_metadata(metadata_value)?;
        }
        let plaintext = cipher.decrypt(&encrypted)?;
        let secret: StoredCredentialSecret = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        if secret.value.trim().is_empty() {
            return Err(VaultError::Serialization(
                "stored service credential is empty".into(),
            ));
        }
        if provider == "creator_sync" {
            if last_error.is_some() {
                return Err(VaultError::Serialization(
                    "stored Creator Sync credential contains disallowed error text".into(),
                ));
            }
            creator_sync_rows = creator_sync_rows.saturating_add(1);
            creator_sync::validate_creator_sync_row(
                &label,
                &metadata,
                &secret.value,
                disabled,
                cooldown_until,
            )?;
        }
    }
    drop(credentials);
    if creator_sync_rows > 1 {
        return Err(VaultError::Serialization(
            "Creator Sync credential identity is ambiguous".into(),
        ));
    }
    if matches!(
        schema_status,
        schema::ExistingSchemaStatus::UpgradeRequired {
            from_version: 3,
            ..
        }
    ) {
        let ambiguous_resend = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1
                   FROM service_credentials legacy
                   JOIN service_credentials typed
                     ON typed.provider = 'email_resend_api_key'
                    AND typed.label = legacy.label
                   WHERE legacy.provider = 'resend'
                 )",
                [],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        if ambiguous_resend {
            return Err(VaultError::Serialization(
                "Resend credential identity is ambiguous during typed Email upgrade".into(),
            ));
        }
    }
    Ok(VaultDataInspection {
        status: match schema_status {
            schema::ExistingSchemaStatus::Ready => VaultDataStatus::Ready,
            schema::ExistingSchemaStatus::UpgradeRequired {
                from_version,
                to_version,
            } => VaultDataStatus::UpgradeRequired {
                from_version,
                to_version,
            },
        },
        profile_directories,
    })
}

fn timestamp() -> String {
    unix_timestamp().to_string()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests;
