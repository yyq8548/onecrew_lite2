use onecrew_local_domain::{LocalProfile, Platform, ProfileSecret};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::{LocalVault, timestamp};
use crate::VaultError;

impl LocalVault {
    pub fn upsert(
        &self,
        platform: Platform,
        username: impl Into<String>,
        secret: &ProfileSecret,
    ) -> Result<LocalProfile, VaultError> {
        let username = username.into();
        if secret.provider_account_id.trim().is_empty() {
            return Err(VaultError::Serialization(
                "provider_account_id is required".into(),
            ));
        }
        let platform_value = platform_name(platform);
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let existing: Option<(String, String)> = connection
            .query_row(
                "SELECT id, created_at FROM profiles
                 WHERE platform = ?1 AND provider_account_id = ?2",
                params![platform_value, secret.provider_account_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let (id, created_at) =
            existing.unwrap_or_else(|| (Uuid::new_v4().to_string(), now.clone()));
        let plaintext = Zeroizing::new(
            serde_json::to_vec(secret)
                .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        connection
            .execute(
                "INSERT INTO profiles
                   (id, platform, provider_account_id, username, encrypted_secret, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                   provider_account_id = excluded.provider_account_id,
                   username = excluded.username,
                   encrypted_secret = excluded.encrypted_secret,
                   updated_at = excluded.updated_at",
                params![
                    id,
                    platform_value,
                    secret.provider_account_id,
                    username,
                    encrypted,
                    created_at,
                    now
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        profile_from_parts(
            id,
            platform,
            secret.provider_account_id.clone(),
            username,
            created_at,
            now,
        )
    }

    /// Replace the credential payload for one already-known profile while
    /// preserving its stable identity. Reconnect flows must use this rather
    /// than `upsert`: that one resolves the row from the secret's
    /// `provider_account_id`, whereas reconnect already knows the profile id
    /// and has to keep it — along with `created_at` — even while the provider
    /// identity it carries is being re-verified.
    pub fn replace_profile(
        &self,
        id: Uuid,
        platform: Platform,
        username: impl Into<String>,
        secret: &ProfileSecret,
    ) -> Result<LocalProfile, VaultError> {
        let username = username.into();
        if secret.provider_account_id.trim().is_empty() {
            return Err(VaultError::Serialization(
                "provider_account_id is required".into(),
            ));
        }
        let plaintext = Zeroizing::new(
            serde_json::to_vec(secret)
                .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        let now = timestamp();
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let created_at = connection
            .query_row(
                "SELECT created_at FROM profiles WHERE id = ?1 AND platform = ?2",
                params![id.to_string(), platform_name(platform)],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?
            .ok_or(VaultError::ProfileNotFound)?;
        let changed = connection
            .execute(
                "UPDATE profiles
                 SET provider_account_id = ?2, username = ?3,
                     encrypted_secret = ?4, updated_at = ?5
                 WHERE id = ?1 AND platform = ?6",
                params![
                    id.to_string(),
                    secret.provider_account_id,
                    username,
                    encrypted,
                    now,
                    platform_name(platform)
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        if changed != 1 {
            return Err(VaultError::ProfileNotFound);
        }
        profile_from_parts(
            id.to_string(),
            platform,
            secret.provider_account_id.clone(),
            username,
            created_at,
            now,
        )
    }

    /// Restore an exact profile snapshot after a cross-store reconciliation
    /// failure. This preserves the stable id and both timestamps; callers must
    /// keep the accompanying plaintext secret in memory only.
    pub fn restore_profile(
        &self,
        profile: &LocalProfile,
        secret: &ProfileSecret,
    ) -> Result<(), VaultError> {
        if profile.provider_account_id.trim().is_empty()
            || secret.provider_account_id.trim().is_empty()
            || profile.provider_account_id != secret.provider_account_id
        {
            return Err(VaultError::Serialization(
                "profile and secret require the same provider_account_id".into(),
            ));
        }
        let plaintext = Zeroizing::new(
            serde_json::to_vec(secret)
                .map_err(|error| VaultError::Serialization(error.to_string()))?,
        );
        let encrypted = self.cipher.encrypt(&plaintext)?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        connection
            .execute(
                "INSERT INTO profiles
                   (id, platform, provider_account_id, username, encrypted_secret, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                   platform = excluded.platform,
                   provider_account_id = excluded.provider_account_id,
                   username = excluded.username,
                   encrypted_secret = excluded.encrypted_secret,
                   created_at = excluded.created_at,
                   updated_at = excluded.updated_at",
                params![
                    profile.id.to_string(),
                    platform_name(profile.platform),
                    profile.provider_account_id,
                    profile.username,
                    encrypted,
                    profile.created_at,
                    profile.updated_at,
                ],
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        Ok(())
    }

    pub fn list(&self, platform: Option<Platform>) -> Result<Vec<LocalProfile>, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let mut statement = connection
            .prepare(
                "SELECT id, platform, provider_account_id, username, created_at, updated_at
                 FROM profiles
                 WHERE (?1 IS NULL OR platform = ?1)
                 ORDER BY updated_at DESC",
            )
            .map_err(|error| VaultError::Database(error.to_string()))?;
        let filter = platform.map(platform_name);
        let rows = statement
            .query_map([filter], |row| {
                let platform_value: String = row.get(1)?;
                Ok((
                    row.get::<_, String>(0)?,
                    platform_value,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(|error| VaultError::Database(error.to_string()))?;
        rows.map(|row| {
            let (id, platform, provider_account_id, username, created_at, updated_at) =
                row.map_err(|error| VaultError::Database(error.to_string()))?;
            profile_from_parts(
                id,
                parse_platform(&platform)?,
                provider_account_id,
                username,
                created_at,
                updated_at,
            )
        })
        .collect()
    }

    pub fn get(&self, id: Uuid) -> Result<(LocalProfile, ProfileSecret), VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        let row = connection
            .query_row(
                "SELECT platform, provider_account_id, username, encrypted_secret, created_at, updated_at
                 FROM profiles WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Vec<u8>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| VaultError::Database(error.to_string()))?
            .ok_or(VaultError::ProfileNotFound)?;
        let platform = parse_platform(&row.0)?;
        let plaintext = self.cipher.decrypt(&row.3)?;
        let secret = serde_json::from_slice(&plaintext)
            .map_err(|error| VaultError::Serialization(error.to_string()))?;
        Ok((
            LocalProfile {
                id,
                platform,
                provider_account_id: row.1,
                username: row.2,
                created_at: row.4,
                updated_at: row.5,
            },
            secret,
        ))
    }

    pub fn delete(&self, id: Uuid) -> Result<bool, VaultError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| VaultError::Database("vault lock poisoned".into()))?;
        connection
            .execute("DELETE FROM profiles WHERE id = ?1", [id.to_string()])
            .map(|changed| changed > 0)
            .map_err(|error| VaultError::Database(error.to_string()))
    }
}

fn platform_name(platform: Platform) -> &'static str {
    match platform {
        Platform::X => "x",
        Platform::Instagram => "instagram",
        Platform::Telegram => "telegram",
    }
}

fn parse_platform(value: &str) -> Result<Platform, VaultError> {
    match value {
        "x" => Ok(Platform::X),
        "instagram" => Ok(Platform::Instagram),
        "telegram" => Ok(Platform::Telegram),
        other => Err(VaultError::Serialization(format!(
            "unsupported platform {other}"
        ))),
    }
}

fn profile_from_parts(
    id: String,
    platform: Platform,
    provider_account_id: String,
    username: String,
    created_at: String,
    updated_at: String,
) -> Result<LocalProfile, VaultError> {
    Ok(LocalProfile {
        id: Uuid::parse_str(&id).map_err(|error| VaultError::Serialization(error.to_string()))?,
        platform,
        provider_account_id,
        username,
        created_at,
        updated_at,
    })
}
