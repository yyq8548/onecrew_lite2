use rusqlite::Connection;

use crate::VaultError;

use super::{CURRENT_SCHEMA_VERSION, database_error};

pub(super) fn create_schema_unchecked(connection: &Connection) -> Result<(), VaultError> {
    connection
        .execute_batch(&format!(
            "BEGIN IMMEDIATE;
             CREATE TABLE profiles (
               id TEXT PRIMARY KEY,
               platform TEXT NOT NULL,
               provider_account_id TEXT NOT NULL CHECK(length(trim(provider_account_id)) > 0),
               username TEXT NOT NULL,
               encrypted_secret BLOB NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE TABLE service_credentials (
               id TEXT PRIMARY KEY,
               provider TEXT NOT NULL,
               label TEXT NOT NULL,
               metadata_json TEXT NOT NULL DEFAULT '{{}}',
               encrypted_secret BLOB NOT NULL,
               disabled INTEGER NOT NULL DEFAULT 0,
               cooldown_until INTEGER,
               last_used_at INTEGER,
               last_error TEXT,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               UNIQUE(provider, label)
             );
             CREATE UNIQUE INDEX profiles_platform_provider_account_id
               ON profiles(platform, provider_account_id);
             PRAGMA user_version = {CURRENT_SCHEMA_VERSION};
             COMMIT;"
        ))
        .map_err(database_error)
}
