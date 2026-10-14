//! Manage registered profiles: the desktop list, refresh (re-validate the
//! session and bump updated_at), and deletion.

use onecrew_creator_runtime::{
    AccountAvailability, AccountKind, AccountRefId, Platform as CreatorPlatform,
};
use onecrew_local_domain::{LocalProfile, Platform, ProfileSecret};
use onecrew_sessions::SessionMaterialError;
use uuid::Uuid;

use crate::{LocalPlatform, XChatPasscodeStatus, account::project_x_chat_passcode_status};

/// A registered profile for the desktop list.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProfileRow {
    pub id: String,
    pub platform: String,
    /// Stable provider-side account identifier.
    pub provider_account_id: String,
    /// The signed-in account @handle.
    pub username: String,
    /// User-given profile name (Chrome browser profiles); empty for Telegram.
    pub profile_name: String,
    /// Short fingerprint summary for display (Chrome profiles); empty for TG.
    pub fingerprint_seed: String,
    /// The configured proxy gateway `host:port`, or `None` for a direct
    /// connection. (For a local forwarder this is `127.0.0.1:...`.)
    pub proxy: Option<String>,
    /// The proxy protocol: `http` / `https` / `socks5`. `None` when direct.
    pub proxy_kind: Option<String>,
    /// The proxy's real detected egress (`ip · country`) — the actual exit,
    /// shown in preference to the gateway. `None` until first probed.
    pub proxy_egress: Option<String>,
    pub x_chat_passcode_status: XChatPasscodeStatus,
    pub updated_at: String,
}

fn platform_slug(platform: Platform) -> &'static str {
    match platform {
        Platform::X => "x",
        Platform::Instagram => "instagram",
        Platform::Telegram => "telegram",
    }
}

/// Web copy for session material that cannot become a session during profile
/// refresh or browser login. The copy predates the shared session constructors,
/// so their typed errors map back to it here.
pub(crate) fn session_material_message(error: SessionMaterialError) -> String {
    match error {
        SessionMaterialError::MissingXAuthToken => "缺少 X auth_token".into(),
        SessionMaterialError::MissingXCsrfToken => "缺少 X ct0".into(),
        SessionMaterialError::MissingInstagramSession => "无 Instagram 会话".into(),
        SessionMaterialError::TelegramProxy(_) => crate::telegram::TELEGRAM_SOCKS5_REQUIRED.into(),
        SessionMaterialError::XSessionRejected
        | SessionMaterialError::InstagramSessionRejected
        | SessionMaterialError::MissingTelegramApiId
        | SessionMaterialError::MissingTelegramApiHash
        | SessionMaterialError::MissingTelegramSession => error.to_string(),
    }
}

impl LocalPlatform {
    pub fn list_profiles(&self) -> Result<Vec<ProfileRow>, String> {
        let profiles = self.vault.list(None).map_err(|error| error.to_string())?;
        let rows = profiles
            .into_iter()
            .map(|profile| {
                let secret = self.vault.get(profile.id).ok().map(|(_, secret)| secret);
                let x_chat_passcode_status =
                    project_x_chat_passcode_status(profile.platform, secret.as_ref());
                ProfileRow {
                    id: profile.id.to_string(),
                    platform: platform_slug(profile.platform).into(),
                    provider_account_id: profile.provider_account_id,
                    username: profile.username,
                    profile_name: secret
                        .as_ref()
                        .map(|s| s.profile_name.clone())
                        .unwrap_or_default(),
                    fingerprint_seed: secret
                        .as_ref()
                        .map(|s| s.fingerprint_seed.clone())
                        .unwrap_or_default(),
                    proxy: secret
                        .as_ref()
                        .and_then(|s| s.proxy.as_ref().map(|p| p.endpoint_label())),
                    proxy_kind: secret
                        .as_ref()
                        .and_then(|s| s.proxy.as_ref().map(|p| p.kind.scheme().to_string())),
                    proxy_egress: secret.as_ref().and_then(|s| s.proxy_egress.clone()),
                    x_chat_passcode_status,
                    updated_at: profile.updated_at,
                }
            })
            .collect();
        Ok(rows)
    }

    /// Re-validate a profile's session (X/IG via the platform API through its
    /// 家宽, Telegram via reconnect) and re-store it (bumps updated_at).
    pub async fn refresh_profile(&self, id: &str) -> Result<LocalProfile, String> {
        let id = Uuid::parse_str(id).map_err(|_| "invalid profile id".to_string())?;
        let (profile, mut secret) = self.vault.get(id).map_err(|error| error.to_string())?;
        // Refresh the real egress too, so an already-connected profile's list row
        // shows its exit IP (not the gateway) after a 验证身份 (X/IG; TG re-upserts
        // its own secret so this is a no-op there).
        if let Some(proxy) = secret.proxy.clone()
            && let Some(egress) = crate::proxy::proxy_egress_label(&proxy).await
        {
            secret.proxy_egress = Some(egress);
        }
        match profile.platform {
            Platform::X => {
                let proxy = secret.proxy.as_ref().map(|p| p.to_url());
                let account =
                    onecrew_sessions::x_account_with_proxy(&secret.cookies, proxy.as_deref())
                        .map_err(session_material_message)?;
                let me = account.who_am_i().await.map_err(|e| e.to_string())?;
                ensure_same_operator_identity(&profile, &secret, &me.id)?;
                secret.provider_account_id = me.id;
                self.replace_and_sync(profile.id, Platform::X, me.username, secret)
            }
            Platform::Instagram => {
                let proxy = secret.proxy.as_ref().map(|p| p.to_url());
                let account = onecrew_sessions::instagram_account_with_proxy(
                    secret.instagram_session_settings.as_deref(),
                    proxy.as_deref(),
                )
                .map_err(session_material_message)?;
                let me = account.who_am_i().await.map_err(|e| e.to_string())?;
                ensure_same_operator_identity(&profile, &secret, &me.id)?;
                secret.provider_account_id = me.id;
                self.replace_and_sync(profile.id, Platform::Instagram, me.username, secret)
            }
            Platform::Telegram => {
                let prepared = onecrew_sessions::telegram_session_with_proxy(
                    secret.telegram_api_id,
                    secret.telegram_api_hash.as_deref(),
                    secret.telegram_session.as_deref(),
                    secret.proxy.as_ref(),
                )
                .map_err(session_material_message)?;
                let api_id = prepared.api_id();
                let api_hash = prepared.api_hash().to_owned();
                let account = prepared
                    .connect()
                    .await
                    .map_err(|error| error.to_string())?;
                let me = account
                    .who_am_i()
                    .await
                    .map_err(|error| error.to_string())?;
                if let Err(error) = ensure_same_operator_identity(&profile, &secret, &me.id) {
                    account.close().await;
                    return Err(error);
                }
                let session_json = account.session_json().map_err(|error| error.to_string())?;
                account.close().await;
                self.tg_upsert(crate::telegram::TelegramProfileUpsert {
                    api_id,
                    api_hash,
                    session_json,
                    proxy: secret.proxy.clone(),
                    proxy_egress: secret.proxy_egress.clone(),
                    account: me,
                    label: secret.profile_name.clone(),
                })
            }
        }
    }

    fn replace_and_sync(
        &self,
        profile_id: Uuid,
        platform: Platform,
        username: String,
        secret: ProfileSecret,
    ) -> Result<LocalProfile, String> {
        let profile = self
            .vault
            .replace_profile(profile_id, platform, username, &secret)
            .map_err(|error| error.to_string())?;
        self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
        self.sync_profile_account_ref(&profile)?;
        Ok(profile)
    }

    pub(crate) fn delete_profile_credential(&self, id: Uuid) -> Result<bool, String> {
        // Remove the generated Chrome credential directory before deleting the
        // only vault record that knows its path. Never recursively delete an
        // arbitrary legacy path outside this runtime's profiles directory.
        if let Ok((_, secret)) = self.vault.get(id) {
            let expected_root = self.data_dir.join("profiles");
            let profile_dir = std::path::PathBuf::from(&secret.browser_profile_dir);
            if !secret.browser_profile_dir.trim().is_empty()
                && profile_dir.starts_with(&expected_root)
                && profile_dir.exists()
            {
                std::fs::remove_dir_all(&profile_dir).map_err(|error| {
                    format!("failed to remove local browser credential directory: {error}")
                })?;
            }
        }
        self.vault.delete(id).map_err(|error| error.to_string())
    }
}

fn ensure_same_operator_identity(
    profile: &LocalProfile,
    secret: &ProfileSecret,
    remote_provider_account_id: &str,
) -> Result<(), String> {
    if profile.provider_account_id.trim().is_empty() || secret.provider_account_id.trim().is_empty()
    {
        return Err(
            "operator account has no stable provider identity; start a new connection".into(),
        );
    }
    if profile.provider_account_id == remote_provider_account_id
        && secret.provider_account_id == remote_provider_account_id
    {
        Ok(())
    } else {
        Err("reconnect identity mismatch; selected operator account was not replaced".into())
    }
}

impl LocalPlatform {
    /// Reconcile operator profiles without changing service-credential projections.
    pub(super) fn reconcile_vault_profile_refs(&self) -> Result<(), String> {
        for profile in self.vault.list(None).map_err(|error| error.to_string())? {
            let availability = if self.vault.get(profile.id).is_ok() {
                AccountAvailability::Available
            } else {
                AccountAvailability::Unavailable
            };
            self.sync_profile_account_ref_with_availability(&profile, availability)?;
        }
        Ok(())
    }

    pub(crate) fn sync_profile_account_ref(&self, profile: &LocalProfile) -> Result<(), String> {
        self.sync_profile_account_ref_with_availability(profile, AccountAvailability::Available)
    }

    fn sync_profile_account_ref_with_availability(
        &self,
        profile: &LocalProfile,
        availability: AccountAvailability,
    ) -> Result<(), String> {
        let platform = match profile.platform {
            Platform::X => CreatorPlatform::X,
            Platform::Instagram => CreatorPlatform::Instagram,
            Platform::Telegram => CreatorPlatform::Telegram,
        };
        self.creators.sync_account_ref_with_state(
            AccountRefId::new(profile.id.to_string()),
            platform,
            AccountKind::Profile,
            &profile.username,
            availability,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(username: &str) -> LocalProfile {
        LocalProfile {
            id: Uuid::new_v4(),
            platform: Platform::X,
            provider_account_id: "remote-123".into(),
            username: username.into(),
            created_at: "1".into(),
            updated_at: "2".into(),
        }
    }

    #[test]
    fn reconnect_identity_accepts_only_the_selected_operator() {
        let mut secret = ProfileSecret {
            provider_account_id: "remote-123".into(),
            cookies: vec![],
            x_chat_passcode: None,
            instagram_session_settings: None,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_session: None,
            profile_name: String::new(),
            browser_profile_dir: String::new(),
            fingerprint_seed: String::new(),
            proxy: None,
            proxy_egress: None,
        };
        assert!(ensure_same_operator_identity(&profile("Alice"), &secret, "remote-123").is_ok());
        let error =
            ensure_same_operator_identity(&profile("alice"), &secret, "remote-999").unwrap_err();
        assert!(error.contains("identity mismatch"));
        secret.provider_account_id.clear();
        let error =
            ensure_same_operator_identity(&profile("alice"), &secret, "remote-123").unwrap_err();
        assert!(error.contains("start a new connection"));
    }
}
