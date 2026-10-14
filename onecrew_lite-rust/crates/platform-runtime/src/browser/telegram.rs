//! Telegram doesn't use the create-profile/extract flow — it only borrows a
//! throwaway 家宽 Chrome pointed at my.telegram.org/apps so the user can copy
//! their api_id/api_hash. Nothing is extracted or persisted; the window is held
//! only to keep its 家宽 forwarder alive, then closed + discarded.

use std::path::PathBuf;
use std::process::Child;

use onecrew_chrome::{Chrome, Fingerprint, detect_chrome};
use onecrew_proxy::{Forwarder, spawn_forwarder};
use uuid::Uuid;

use crate::LocalPlatform;

/// A throwaway Chrome window opened at my.telegram.org (through a 家宽). Held
/// only to keep the forwarder alive until [`LocalPlatform::tg_close_credentials`].
pub(crate) struct TgCredsBrowser {
    forwarder: Forwarder,
    child: Child,
    profile_dir: PathBuf,
}

impl LocalPlatform {
    /// Open a throwaway Chrome (through `proxy`) at my.telegram.org/apps so the
    /// user can read + copy their api_id/api_hash. Nothing is extracted; the
    /// browser is held only to keep its 家宽 forwarder alive, and is closed +
    /// discarded by [`Self::tg_close_credentials`] (or when a new one opens).
    pub(crate) async fn tg_open_credentials(
        &self,
        proxy: onecrew_proxy::ProxyConfig,
    ) -> Result<(), String> {
        let binary = detect_chrome().ok_or("未检测到 Chrome,请先安装 Google Chrome")?;
        let profile_dir = self
            .data_dir
            .join("tmp")
            .join(format!("tg-creds-{}", Uuid::new_v4()));
        // Authenticated 家宽 → loopback forwarder (Chrome can't carry proxy creds).
        let forwarder = spawn_forwarder(proxy)
            .await
            .map_err(|error| format!("本地代理启动失败:{error}"))?;
        let port = forwarder.port();
        let child = Chrome::new(binary)
            .launch_login(
                &profile_dir,
                &Fingerprint::default(),
                "127.0.0.1",
                port,
                Some("https://my.telegram.org/apps".to_string()),
            )
            .map_err(|error| format!("启动 Chrome 失败:{error}"))?;
        // Replace (and thereby close) any previous one — only one at a time.
        let previous = self
            .tg_creds
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?
            .replace(TgCredsBrowser {
                forwarder,
                child,
                profile_dir,
            });
        discard_creds_browser(previous);
        Ok(())
    }

    /// Close + discard the credential browser (kill Chrome, drop the 家宽
    /// forwarder, remove the throwaway profile). No-op if none is open.
    pub fn tg_close_credentials(&self) -> Result<(), String> {
        let session = self
            .tg_creds
            .lock()
            .map_err(|_| "browser lock poisoned".to_string())?
            .take();
        discard_creds_browser(session);
        Ok(())
    }
}

/// Kill the Chrome window, drop its 家宽 forwarder, and remove the throwaway
/// profile dir. Best-effort.
fn discard_creds_browser(session: Option<TgCredsBrowser>) {
    if let Some(mut session) = session {
        if let Err(error) = session.child.kill() {
            log::debug!("telegram credentials browser kill failed: {error}");
        }
        if let Err(error) = std::fs::remove_dir_all(&session.profile_dir) {
            log::warn!("telegram credentials browser profile removal failed: {error}");
        }
        drop(session.forwarder);
    }
}
