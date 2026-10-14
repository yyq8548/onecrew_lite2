//! Shared finish/persist for both Telegram login flows and profile refresh:
//! close the live MTProto connection, then upsert the vault record with the
//! vault-upsert -> Creator-sync -> exact-rollback invariant.

use onecrew_local_domain::{LocalProfile, Platform, ProfileSecret};
use onecrew_tg::TgAccount;

use crate::LocalPlatform;

use super::TelegramProfileUpsert;

impl LocalPlatform {
    pub(super) async fn tg_finish(
        &self,
        api_id: i32,
        api_hash: String,
        proxy: Option<onecrew_proxy::ProxyConfig>,
        account: TgAccount,
        label: String,
    ) -> Result<LocalProfile, String> {
        let me = account
            .who_am_i()
            .await
            .map_err(|error| error.to_string())?;
        let session_json = account.session_json().map_err(|error| error.to_string())?;
        account.close().await;
        // The login egressed through this 家宽, so it works — record its egress.
        let proxy_egress = match &proxy {
            Some(config) => crate::proxy::proxy_egress_label(config).await,
            None => None,
        };
        self.tg_upsert(TelegramProfileUpsert {
            api_id,
            api_hash,
            session_json,
            proxy,
            proxy_egress,
            label,
            account: me,
        })
    }

    pub(crate) fn tg_upsert(&self, request: TelegramProfileUpsert) -> Result<LocalProfile, String> {
        let TelegramProfileUpsert {
            api_id,
            api_hash,
            session_json,
            proxy,
            proxy_egress,
            account,
            label,
        } = request;
        let username = account
            .username
            .clone()
            .unwrap_or_else(|| account.id.clone());
        let original = self
            .vault
            .list(Some(Platform::Telegram))
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|profile| profile.provider_account_id == account.id)
            .map(|profile| {
                self.vault
                    .get(profile.id)
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        // Prefer the proxy this login used; otherwise preserve the exact proxy
        // attached to the stable provider identity even if its username changed.
        let proxy = proxy.or_else(|| {
            original
                .as_ref()
                .and_then(|(_, secret)| secret.proxy.clone())
        });
        let replacement_secret = ProfileSecret {
            provider_account_id: account.id,
            cookies: Vec::new(),
            x_chat_passcode: None,
            instagram_session_settings: None,
            telegram_api_id: Some(api_id),
            telegram_api_hash: Some(api_hash),
            telegram_session: Some(session_json),
            // Telegram is phone login, not a browser — no Chrome profile.
            profile_name: label,
            browser_profile_dir: String::new(),
            fingerprint_seed: String::new(),
            proxy,
            proxy_egress,
        };
        let profile = self
            .vault
            .upsert(Platform::Telegram, username, &replacement_secret)
            .map_err(|error| error.to_string())?;
        if let Err(sync_error) = self.sync_profile_account_ref(&profile) {
            let rollback = match original.as_ref() {
                Some((original_profile, original_secret)) => self
                    .vault
                    .restore_profile(original_profile, original_secret)
                    .map_err(|error| error.to_string()),
                None => self
                    .vault
                    .delete(profile.id)
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
            };
            if rollback.is_err() {
                self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
            }
            return Err(match rollback {
                Ok(()) => format!(
                    "failed to synchronize the Telegram account with the Creator store; encrypted profile change was rolled back: {sync_error}"
                ),
                Err(rollback_error) => format!(
                    "failed to synchronize the Telegram account with the Creator store: {sync_error}; encrypted profile rollback also failed: {rollback_error}"
                ),
            });
        }
        self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use onecrew_local_domain::Platform;
    use onecrew_tg::TgUser;
    use uuid::Uuid;

    use crate::LocalPlatform;
    use crate::telegram::TelegramProfileUpsert;

    fn test_user(id: &str, username: &str) -> TgUser {
        TgUser {
            id: id.into(),
            username: Some(username.into()),
            first_name: Some("Test".into()),
            last_name: None,
            phone: None,
            is_bot: false,
            verified: false,
        }
    }

    fn socks_proxy() -> onecrew_proxy::ProxyConfig {
        onecrew_proxy::ProxyConfig {
            kind: onecrew_proxy::ProxyKind::Socks5,
            host: "socks.example".into(),
            port: 1080,
            username: Some("user".into()),
            password: Some("password".into()),
        }
    }

    fn force_account_ref_sync_failure(directory: &std::path::Path) {
        let connection = rusqlite::Connection::open(directory.join("crm.db")).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_account_ref_insert
                   BEFORE INSERT ON account_refs
                   BEGIN SELECT RAISE(FAIL, 'forced account ref sync failure'); END;
                 CREATE TRIGGER fail_account_ref_update
                   BEFORE UPDATE ON account_refs
                   BEGIN SELECT RAISE(FAIL, 'forced account ref sync failure'); END;",
            )
            .unwrap();
    }

    #[test]
    fn telegram_reconcile_failure_restores_the_exact_previous_profile_and_secret() {
        let directory = std::env::temp_dir().join(format!("tg-rollback-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        let original = platform
            .tg_upsert(TelegramProfileUpsert {
                api_id: 100,
                api_hash: "original-hash".into(),
                session_json: "original-session".into(),
                proxy: Some(socks_proxy()),
                proxy_egress: Some("203.0.113.10".into()),
                account: test_user("stable-tg-id", "original_name"),
                label: "Original label".into(),
            })
            .unwrap();
        let (original_profile, original_secret) = platform.vault.get(original.id).unwrap();
        force_account_ref_sync_failure(&directory);

        let error = platform
            .tg_upsert(TelegramProfileUpsert {
                api_id: 200,
                api_hash: "replacement-hash".into(),
                session_json: "replacement-session".into(),
                proxy: Some(socks_proxy()),
                proxy_egress: Some("203.0.113.20".into()),
                account: test_user("stable-tg-id", "renamed_user"),
                label: "Replacement label".into(),
            })
            .unwrap_err();
        assert!(error.contains("encrypted profile change was rolled back"));
        for secret in [
            "original-hash",
            "original-session",
            "replacement-hash",
            "replacement-session",
            "password",
        ] {
            assert!(!error.contains(secret));
        }
        let (restored_profile, restored_secret) = platform.vault.get(original.id).unwrap();
        assert_eq!(restored_profile.id, original_profile.id);
        assert_eq!(restored_profile.platform, original_profile.platform);
        assert_eq!(
            restored_profile.provider_account_id,
            original_profile.provider_account_id
        );
        assert_eq!(restored_profile.username, original_profile.username);
        assert_eq!(restored_profile.created_at, original_profile.created_at);
        assert_eq!(restored_profile.updated_at, original_profile.updated_at);
        assert_eq!(
            serde_json::to_value(restored_secret).unwrap(),
            serde_json::to_value(original_secret).unwrap()
        );
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn telegram_reconcile_failure_deletes_a_new_profile() {
        let directory = std::env::temp_dir().join(format!("tg-new-rollback-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&directory).unwrap();
        force_account_ref_sync_failure(&directory);

        let error = platform
            .tg_upsert(TelegramProfileUpsert {
                api_id: 100,
                api_hash: "private-hash".into(),
                session_json: "private-session".into(),
                proxy: Some(socks_proxy()),
                proxy_egress: None,
                account: test_user("new-tg-id", "new_user"),
                label: "New label".into(),
            })
            .unwrap_err();
        assert!(error.contains("encrypted profile change was rolled back"));
        assert!(
            platform
                .vault
                .list(Some(Platform::Telegram))
                .unwrap()
                .is_empty()
        );
        assert!(!error.contains("private-hash"));
        assert!(!error.contains("private-session"));
        drop(platform);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
