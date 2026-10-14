use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("profile not found")]
    ProfileNotFound,
    #[error("database error: {0}")]
    Database(String),
    #[error("keychain error: {0}")]
    Keychain(String),
    #[error("encryption error")]
    Encryption,
    #[error("invalid vault data: {0}")]
    Serialization(String),
    #[error("local-vault upgrade required: {from_version} -> {to_version}")]
    UpgradeRequired { from_version: i64, to_version: i64 },
}
