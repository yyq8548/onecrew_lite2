#![deny(unreachable_pub)]

//! Encrypted local persistence for operator profiles and service credentials.
//!
//! This crate owns the vault database, master-key lifecycle, at-rest
//! encryption, and typed storage failures. Business records and public status
//! projections remain outside this boundary.

mod crypto;
mod error;
mod path_security;
mod store;

pub use error::VaultError;
pub use store::{
    EmailSecretRecord, EmailSecretRecordKind, EmailSecretRecordSecret, LocalVault,
    ServiceCredentialHealthCommit, VaultCreatorSyncCredential, VaultCreatorSyncCredentialState,
    VaultDataInspection, VaultDataStatus, VaultDataUpgradeError, VaultDataUpgradeFailureKind,
    VaultDataUpgradeResult, VaultDataUpgradeStatus, VaultProfileDirectory,
};
