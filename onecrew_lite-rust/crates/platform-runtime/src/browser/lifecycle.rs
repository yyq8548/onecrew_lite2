//! The browser login state machine: create -> open -> extract -> finalize,
//! plus reconnect. Owned invariant: nothing persists until finalize succeeds;
//! reconnect replaces the vault record + profile directory with layered
//! rollback or not at all.

use std::path::PathBuf;

use onecrew_chrome::{Chrome, Fingerprint, chrome_version, detect_chrome};
use onecrew_local_domain::{LocalProfile, Platform, ProfileSecret};
use onecrew_proxy::spawn_forwarder;
use uuid::Uuid;

use crate::LocalPlatform;

use super::{BrowserLoginCreate, PendingBrowserLogin, ReconnectBrowserTarget, platform_browser};

impl LocalPlatform {
    /// Step 1 — register a new profile intent (name + platform + verified 家宽).
    /// Generates the isolated profile dir; nothing is launched or stored yet.
    pub(crate) fn browser_create(
        &self,
        platform: Platform,
        name: String,
        proxy: onecrew_proxy::ProxyConfig,
    ) -> Result<BrowserLoginCreate, String> {
        if !matches!(platform, Platform::X | Platform::Instagram) {
            return Err("浏览器登录流程仅用于 X / Instagram".into());
        }
        let id = Uuid::new_v4().to_string();
        let profile_dir = self.data_dir.join("profiles").join(&id);
        let token = id;
        let created = BrowserLoginCreate {
            token: token.clone(),
        };
        self.pending_browser
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?
            .insert(
                token,
                PendingBrowserLogin {
                    platform,
                    name,
                    profile_dir,
                    proxy,
                    x_chat_passcode: None,
                    fingerprint: Fingerprint::default(),
                    forwarder: None,
                    child: None,
                    reconnect_target: None,
                },
            );
        Ok(created)
    }

    /// Attach an X Chat passcode to a pending X profile (optional, before login).
    pub fn browser_set_passcode(
        &self,
        token: &str,
        passcode: Option<String>,
    ) -> Result<(), String> {
        let mut pending = self
            .pending_browser
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?;
        let entry = pending.get_mut(token).ok_or("会话已过期,请重新开始")?;
        entry.x_chat_passcode = passcode.filter(|value| !value.is_empty());
        Ok(())
    }

    /// Re-login into an *existing* profile: reopen Chrome on the profile's own
    /// `user-data-dir` (its stored cookies/session) through the same 家宽, so the
    /// user can refresh an expired login. Confirms the 家宽 is reachable first
    /// (a dead proxy errors here instead of opening a broken window), then opens
    /// Chrome and returns a token to finish through the canonical operator
    /// account workflow.
    pub(crate) async fn browser_relogin(
        &self,
        profile_id: &str,
    ) -> Result<BrowserLoginCreate, String> {
        let id = Uuid::parse_str(profile_id).map_err(|_| "invalid profile id".to_string())?;
        let (profile, secret) = self.vault.get(id).map_err(|error| error.to_string())?;
        if profile.provider_account_id.trim().is_empty()
            || secret.provider_account_id.trim().is_empty()
            || profile.provider_account_id != secret.provider_account_id
        {
            return Err(
                "operator account has no stable provider identity; start a new connection".into(),
            );
        }
        let handler = platform_browser(profile.platform)
            .map_err(|_| "重新登录仅用于 X / Instagram".to_string())?;
        let persistent_profile_dir = PathBuf::from(&secret.browser_profile_dir);
        if secret.browser_profile_dir.is_empty() || !persistent_profile_dir.exists() {
            return Err("该 profile 缺少浏览器目录,请改用「连接」重新创建".into());
        }
        let proxy = secret.proxy.clone().ok_or("该 profile 未配置家宽")?;
        let binary = detect_chrome().ok_or("未检测到 Chrome,请先安装 Google Chrome")?;

        // Confirm the 家宽 routes (surfaces a dead proxy) and take its geo.
        let probe = self.proxy_validate(proxy.clone()).await?;
        let fingerprint = Fingerprint::aligned(probe.timezone, probe.accept_language);

        let token = Uuid::new_v4().to_string();
        let profile_dir = self
            .data_dir
            .join("profiles")
            .join(".reconnect")
            .join(&token);
        copy_directory(&persistent_profile_dir, &profile_dir)?;

        let forwarder = spawn_forwarder(proxy.clone()).await.map_err(|error| {
            remove_directory_if_present(&profile_dir);
            format!("本地代理启动失败:{error}")
        })?;
        let port = forwarder.port();
        let child = Chrome::new(binary)
            .launch_login(
                &profile_dir,
                &fingerprint,
                "127.0.0.1",
                port,
                Some(handler.login_url().to_string()),
            )
            .map_err(|error| {
                remove_directory_if_present(&profile_dir);
                format!("启动 Chrome 失败:{error}")
            })?;
        let created = BrowserLoginCreate {
            token: token.clone(),
        };
        self.pending_browser
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?
            .insert(
                token,
                PendingBrowserLogin {
                    platform: profile.platform,
                    name: secret.profile_name.clone(),
                    profile_dir,
                    proxy,
                    x_chat_passcode: secret.x_chat_passcode.clone(),
                    fingerprint,
                    forwarder: Some(forwarder),
                    child: Some(child),
                    reconnect_target: Some(ReconnectBrowserTarget {
                        profile_id: id,
                        provider_account_id: profile.provider_account_id,
                        persistent_profile_dir,
                    }),
                },
            );
        Ok(created)
    }

    /// Step 2 — open Chrome (headful) with a fresh isolated profile through the
    /// profile's 家宽 so the user can log in. `timezone` / `accept_language` are
    /// the 家宽-aligned soft fingerprint the UI showed (derived by `probe_proxy`
    /// from the egress country); Chrome is launched with them so the login
    /// presents the same fingerprint the user confirmed. The forwarder + browser
    /// process are held until extract.
    pub async fn browser_open_login(
        &self,
        token: &str,
        timezone: Option<String>,
        accept_language: Option<String>,
    ) -> Result<(), String> {
        let binary = detect_chrome().ok_or("未检测到 Chrome,请先安装 Google Chrome")?;
        let (platform, profile_dir, proxy) = {
            let pending = self
                .pending_browser
                .lock()
                .map_err(|_| "browser lock poisoned".to_string())?;
            let entry = pending.get(token).ok_or("会话已过期,请重新开始")?;
            (
                entry.platform,
                entry.profile_dir.clone(),
                entry.proxy.clone(),
            )
        };
        let handler = platform_browser(platform)?;

        // Loopback forwarder → upstream 家宽 (browsers can't carry proxy creds).
        let forwarder = spawn_forwarder(proxy)
            .await
            .map_err(|error| format!("本地代理启动失败:{error}"))?;
        let port = forwarder.port();

        let fingerprint = Fingerprint::aligned(timezone, accept_language);
        let child = Chrome::new(binary)
            .launch_login(
                &profile_dir,
                &fingerprint,
                "127.0.0.1",
                port,
                Some(handler.login_url().to_string()),
            )
            .map_err(|error| format!("启动 Chrome 失败:{error}"))?;

        let mut pending = self
            .pending_browser
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?;
        let entry = pending.get_mut(token).ok_or("会话已过期,请重新开始")?;
        entry.forwarder = Some(forwarder);
        entry.child = Some(child);
        entry.fingerprint = fingerprint;
        Ok(())
    }

    /// The only extraction entry point. Whether this is a new connection or a
    /// reconnect, and the reconnect identity expectation, are read from the
    /// server-owned pending token rather than caller input.
    pub(crate) async fn browser_extract_for_account(
        &self,
        token: &str,
        connection_flow_id: &str,
    ) -> Result<LocalProfile, String> {
        let binary = detect_chrome().ok_or("未检测到 Chrome,请先安装 Google Chrome")?;
        // Read the version before `binary` is moved into Chrome::new below.
        let version = chrome_version(&binary);
        let mut pending = self.take_pending_browser(token)?;
        if self.connection_cancel_requested(connection_flow_id)? {
            discard_pending_browser(pending);
            return Err("operator account connection was canceled before extraction".into());
        }
        let handler = match platform_browser(pending.platform) {
            Ok(handler) => handler,
            Err(error) => {
                discard_pending_browser(pending);
                return Err(error);
            }
        };
        let child = match pending.child.take() {
            Some(child) => child,
            None => {
                discard_pending_browser(pending);
                return Err("请先打开浏览器登录".into());
            }
        };
        let cookies = Chrome::new(binary).close_and_extract(
            child,
            &pending.profile_dir,
            handler.cookie_hosts(),
        );
        // Browser is closed; the forwarder is no longer needed (the API check
        // below hits the upstream 家宽 directly with credentials in the URL).
        pending.forwarder = None;
        let cookies = match cookies {
            Ok(cookies) => cookies,
            Err(error) => {
                remove_directory_if_present(&pending.profile_dir);
                return Err(format!("读取 Cookie 失败:{error}"));
            }
        };
        if cookies.is_empty() {
            remove_directory_if_present(&pending.profile_dir);
            return Err("未提取到登录 Cookie,请确认已在浏览器中登录成功".into());
        }

        let proxy_url = pending.proxy.to_url();
        // Record the real egress (the residential exit) so the list shows it
        // instead of the gateway host:port (best-effort).
        let proxy_egress = crate::proxy::proxy_egress_label(&pending.proxy).await;
        let base = ProfileSecret {
            provider_account_id: String::new(),
            cookies: cookies.clone(),
            x_chat_passcode: None,
            instagram_session_settings: None,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_session: None,
            profile_name: pending.name.clone(),
            browser_profile_dir: pending.profile_dir.to_string_lossy().into_owned(),
            fingerprint_seed: fingerprint_summary(version.as_deref(), &pending.fingerprint),
            proxy: Some(pending.proxy.clone()),
            proxy_egress,
        };

        let (provider_account_id, username, secret) =
            match handler.finalize(&pending, cookies, &proxy_url, base).await {
                Ok(result) => result,
                Err(error) => {
                    remove_directory_if_present(&pending.profile_dir);
                    return Err(error);
                }
            };
        // Final remote identity resolution is complete, but no Vault/profile
        // directory replacement has been committed yet. Honor a concurrent
        // cancellation at this last safe boundary and discard the staging
        // browser state.
        if self.connection_cancel_requested(connection_flow_id)? {
            remove_directory_if_present(&pending.profile_dir);
            return Err("operator account connection was canceled before persistence".into());
        }
        self.persist_finalized_browser_login(pending, provider_account_id, username, secret)
    }

    fn persist_finalized_browser_login(
        &self,
        pending: PendingBrowserLogin,
        provider_account_id: String,
        username: String,
        mut secret: ProfileSecret,
    ) -> Result<LocalProfile, String> {
        if provider_account_id.trim().is_empty()
            || secret.provider_account_id != provider_account_id
        {
            remove_directory_if_present(&pending.profile_dir);
            return Err("connected account returned no stable provider identity".into());
        }
        let reconnect_target = pending.reconnect_target.clone();
        let mut reconnect_original = None;
        if reconnect_target.is_none() {
            let profiles = match self.vault.list(Some(pending.platform)) {
                Ok(profiles) => profiles,
                Err(error) => {
                    remove_directory_if_present(&pending.profile_dir);
                    return Err(error.to_string());
                }
            };
            if profiles
                .iter()
                .any(|profile| profile.provider_account_id == provider_account_id)
            {
                remove_directory_if_present(&pending.profile_dir);
                return Err(
                    "this operator account already exists; use reconnect to replace its credential"
                        .into(),
                );
            }
        }
        if let Some(target) = reconnect_target.as_ref() {
            if target.provider_account_id.trim().is_empty() {
                remove_directory_if_present(&pending.profile_dir);
                return Err(
                    "reconnect target has no stable provider identity; start a new connection"
                        .into(),
                );
            }
            let (expected, expected_secret) = match self.vault.get(target.profile_id) {
                Ok(value) => value,
                Err(_) => {
                    remove_directory_if_present(&pending.profile_dir);
                    return Err(
                        "reconnect target credential is unavailable; start a new connection".into(),
                    );
                }
            };
            if expected.platform != pending.platform
                || expected.id != target.profile_id
                || expected.provider_account_id != target.provider_account_id
                || expected_secret.provider_account_id != target.provider_account_id
                || expected.provider_account_id != provider_account_id
                || expected_secret.provider_account_id != provider_account_id
            {
                remove_directory_if_present(&pending.profile_dir);
                return Err(
                    "reconnect identity mismatch; selected operator account was not replaced"
                        .into(),
                );
            }
            reconnect_original = Some((expected, expected_secret));
        }
        let backup = if let Some(target) = reconnect_target.as_ref() {
            secret.browser_profile_dir =
                target.persistent_profile_dir.to_string_lossy().into_owned();
            match replace_profile_directory(&pending.profile_dir, &target.persistent_profile_dir) {
                Ok(backup) => Some(backup),
                Err(error) => {
                    remove_directory_if_present(&pending.profile_dir);
                    return Err(error);
                }
            }
        } else {
            None
        };
        let vault_write = match reconnect_target.as_ref() {
            Some(target) => {
                self.vault
                    .replace_profile(target.profile_id, pending.platform, username, &secret)
            }
            None => self.vault.upsert(pending.platform, username, &secret),
        };
        let profile = match vault_write {
            Ok(profile) => profile,
            Err(error) => {
                if let (Some(target), Some(backup)) = (reconnect_target.as_ref(), backup.as_ref())
                    && !rollback_profile_directory(&target.persistent_profile_dir, backup)
                {
                    self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
                }
                return Err(error.to_string());
            }
        };
        if let Some(target) = reconnect_target.as_ref()
            && profile.id != target.profile_id
        {
            if let Some((original, original_secret)) = reconnect_original.as_ref()
                && let Err(error) = self.vault.replace_profile(
                    original.id,
                    original.platform,
                    original.username.clone(),
                    original_secret,
                )
            {
                log::warn!("restoring the original browser profile failed: {error}");
                self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
            }
            if let Some(backup) = backup.as_ref()
                && !rollback_profile_directory(&target.persistent_profile_dir, backup)
            {
                self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
            }
            return Err("reconnect replaced the wrong operator account".into());
        }
        if let Err(error) = self.sync_profile_account_ref(&profile) {
            let mut rollback_failed = false;
            if let Some(target) = reconnect_target.as_ref() {
                if let Some((original, original_secret)) = reconnect_original.as_ref()
                    && let Err(error) = self.vault.replace_profile(
                        original.id,
                        original.platform,
                        original.username.clone(),
                        original_secret,
                    )
                {
                    log::warn!("restoring the original browser profile failed: {error}");
                    rollback_failed = true;
                }
                if let Some(backup) = backup.as_ref() {
                    rollback_failed |=
                        !rollback_profile_directory(&target.persistent_profile_dir, backup);
                }
            } else {
                if let Err(error) = self.vault.delete(profile.id) {
                    log::warn!("deleting the unsynchronised browser profile failed: {error}");
                    rollback_failed = true;
                }
                remove_directory_if_present(&pending.profile_dir);
            }
            if rollback_failed {
                self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
            }
            return Err(format!(
                "failed to synchronize the connected account with the Creator store; local rollback {}: {error}",
                if rollback_failed {
                    "also failed"
                } else {
                    "completed"
                }
            ));
        }
        if let Some(backup) = backup {
            remove_directory_if_present(&backup);
        }
        self.publish_changes([crate::PlatformChangeScope::OperatorAccounts]);
        Ok(profile)
    }

    /// Discard a pending profile (user cancelled). Drops the forwarder + browser.
    pub fn browser_cancel(&self, token: &str) -> Result<(), String> {
        if let Ok(pending) = self.take_pending_browser(token) {
            discard_pending_browser(pending);
        }
        Ok(())
    }

    fn take_pending_browser(&self, token: &str) -> Result<PendingBrowserLogin, String> {
        self.pending_browser
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?
            .remove(token)
            .ok_or_else(|| "会话已过期,请重新开始".to_string())
    }
}

fn discard_pending_browser(mut pending: PendingBrowserLogin) {
    if let Some(mut child) = pending.child.take() {
        // `kill` may report a child that already exited; `wait` still reaps it.
        if let Err(error) = child.kill() {
            log::debug!("pending browser kill failed: {error}");
        }
        if let Err(error) = child.wait() {
            log::debug!("pending browser wait failed: {error}");
        }
    }
    pending.forwarder = None;
    remove_directory_if_present(&pending.profile_dir);
}

fn copy_directory(source: &std::path::Path, destination: &std::path::Path) -> Result<(), String> {
    if destination.exists() {
        std::fs::remove_dir_all(destination).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in std::fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn replace_profile_directory(
    staging: &std::path::Path,
    persistent: &std::path::Path,
) -> Result<PathBuf, String> {
    let backup = persistent.with_extension(format!("backup-{}", Uuid::new_v4()));
    std::fs::rename(persistent, &backup)
        .map_err(|error| format!("failed to stage existing browser profile: {error}"))?;
    if let Err(error) = std::fs::rename(staging, persistent) {
        if let Err(restore) = std::fs::rename(&backup, persistent) {
            log::warn!("restoring the staged browser profile failed: {restore}");
        }
        return Err(format!(
            "failed to install validated browser profile: {error}"
        ));
    }
    Ok(backup)
}

fn rollback_profile_directory(persistent: &std::path::Path, backup: &std::path::Path) -> bool {
    remove_directory_if_present(persistent);
    if let Err(error) = std::fs::rename(backup, persistent) {
        log::warn!("browser profile rollback rename failed: {error}");
        return false;
    }
    true
}

fn remove_directory_if_present(path: &std::path::Path) {
    if path.exists()
        && let Err(error) = std::fs::remove_dir_all(path)
    {
        log::warn!("browser profile directory removal failed: {error}");
    }
}

/// A short human-readable fingerprint summary for the account row, e.g.
/// `Chrome 151 · en-US · America/New_York` (version omitted if undetected).
fn fingerprint_summary(version: Option<&str>, fingerprint: &Fingerprint) -> String {
    let mut parts = vec![match version {
        Some(v) => format!("Chrome {v}"),
        None => "Chrome".to_string(),
    }];
    if let Some(lang) = fingerprint
        .accept_language
        .as_deref()
        .and_then(|value| value.split(',').next())
    {
        parts.push(lang.to_string());
    }
    if let Some(tz) = &fingerprint.timezone {
        parts.push(tz.clone());
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use onecrew_chrome::Fingerprint;
    use onecrew_creator_runtime::AccountRefId;
    use onecrew_local_domain::{Platform, ProfileSecret};

    use crate::LocalPlatform;
    use crate::browser::{PendingBrowserLogin, ReconnectBrowserTarget};

    use super::{
        copy_directory, discard_pending_browser, remove_directory_if_present,
        replace_profile_directory, rollback_profile_directory,
    };
    use uuid::Uuid;

    #[test]
    fn reconnect_profile_replacement_can_be_rolled_back_without_cross_account_pollution() {
        let root = std::env::temp_dir().join(format!("onecrew-browser-{}", Uuid::new_v4()));
        let persistent = root.join("persistent");
        let staging = root.join("staging");
        std::fs::create_dir_all(&persistent).unwrap();
        std::fs::write(persistent.join("identity"), "original").unwrap();

        copy_directory(&persistent, &staging).unwrap();
        std::fs::write(staging.join("identity"), "replacement").unwrap();
        let backup = replace_profile_directory(&staging, &persistent).unwrap();
        assert_eq!(
            std::fs::read_to_string(persistent.join("identity")).unwrap(),
            "replacement"
        );
        assert!(!staging.exists());

        rollback_profile_directory(&persistent, &backup);
        assert_eq!(
            std::fs::read_to_string(persistent.join("identity")).unwrap(),
            "original"
        );
        assert!(!backup.exists());
        remove_directory_if_present(&root);
    }

    #[test]
    fn failed_extraction_cleanup_removes_the_staging_profile() {
        let root = std::env::temp_dir().join(format!(
            "onecrew-browser-failed-extraction-{}",
            Uuid::new_v4()
        ));
        let staging = root.join("staging");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("Cookies"), "encrypted-cookie-data").unwrap();
        discard_pending_browser(PendingBrowserLogin {
            platform: Platform::X,
            name: "failed".into(),
            profile_dir: staging.clone(),
            proxy: onecrew_proxy::ProxyConfig {
                kind: onecrew_proxy::ProxyKind::Http,
                host: "proxy.example".into(),
                port: 8080,
                username: None,
                password: None,
            },
            x_chat_passcode: None,
            fingerprint: Fingerprint::default(),
            forwarder: None,
            child: None,
            reconnect_target: None,
        });
        assert!(!staging.exists());
        remove_directory_if_present(&root);
    }

    #[test]
    fn reconnect_token_rejects_unseen_remote_identity_without_any_local_mutation() {
        let root =
            std::env::temp_dir().join(format!("onecrew-browser-identity-test-{}", Uuid::new_v4()));
        let platform = LocalPlatform::open(&root).unwrap();
        let persistent = root.join("profiles").join("persistent");
        let staging = root.join("profiles").join(".reconnect").join("token");
        std::fs::create_dir_all(&persistent).unwrap();
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(persistent.join("identity"), "original-directory").unwrap();
        std::fs::write(staging.join("identity"), "different-directory").unwrap();

        let original_secret = ProfileSecret {
            provider_account_id: "remote-original".into(),
            cookies: Vec::new(),
            x_chat_passcode: None,
            instagram_session_settings: None,
            telegram_api_id: None,
            telegram_api_hash: None,
            telegram_session: None,
            profile_name: "Original label".into(),
            browser_profile_dir: persistent.to_string_lossy().into_owned(),
            fingerprint_seed: String::new(),
            proxy: None,
            proxy_egress: None,
        };
        let original = platform
            .vault
            .upsert(Platform::X, "original_handle", &original_secret)
            .unwrap();
        platform.sync_profile_account_ref(&original).unwrap();

        let replacement_secret = ProfileSecret {
            provider_account_id: "remote-never-seen-before".into(),
            profile_name: "Replacement label".into(),
            browser_profile_dir: staging.to_string_lossy().into_owned(),
            ..original_secret.clone()
        };
        let pending = PendingBrowserLogin {
            platform: Platform::X,
            name: "Original label".into(),
            profile_dir: staging.clone(),
            proxy: onecrew_proxy::ProxyConfig {
                kind: onecrew_proxy::ProxyKind::Socks5,
                host: "127.0.0.1".into(),
                port: 1080,
                username: None,
                password: None,
            },
            x_chat_passcode: None,
            fingerprint: Fingerprint::default(),
            forwarder: None,
            child: None,
            reconnect_target: Some(ReconnectBrowserTarget {
                profile_id: original.id,
                provider_account_id: original.provider_account_id.clone(),
                persistent_profile_dir: persistent.clone(),
            }),
        };

        let error = platform
            .persist_finalized_browser_login(
                pending,
                "remote-never-seen-before".into(),
                "different_handle".into(),
                replacement_secret,
            )
            .unwrap_err();
        assert!(error.contains("identity mismatch"));
        assert!(!staging.exists());
        assert_eq!(
            std::fs::read_to_string(persistent.join("identity")).unwrap(),
            "original-directory"
        );
        assert_eq!(platform.vault.list(Some(Platform::X)).unwrap().len(), 1);
        let (stored, stored_secret) = platform.vault.get(original.id).unwrap();
        assert_eq!(stored.provider_account_id, "remote-original");
        assert_eq!(stored.username, "original_handle");
        assert_eq!(stored_secret.provider_account_id, "remote-original");
        let creator_account = platform
            .creators()
            .unwrap()
            .get_account_ref(&AccountRefId::new(original.id.to_string()))
            .unwrap();
        assert_eq!(creator_account.label, "original_handle");

        drop(platform);
        remove_directory_if_present(&root);
    }
}
