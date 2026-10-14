//! X browser-login: build the X session from the profile's captured
//! `auth_token` and `ct0` cookies through the same 家宽 the login used, confirm
//! the account via the wreq API, and record the optional X Chat passcode.

use async_trait::async_trait;
use onecrew_local_domain::{BrowserCookie, ProfileSecret};

use super::{PendingBrowserLogin, PlatformBrowser};
use crate::profiles::session_material_message;

pub(crate) struct XBrowser;

#[async_trait]
impl PlatformBrowser for XBrowser {
    fn login_url(&self) -> &'static str {
        "https://x.com/login"
    }

    fn cookie_hosts(&self) -> &'static [&'static str] {
        &["x.com", "twitter.com"]
    }

    async fn finalize(
        &self,
        pending: &PendingBrowserLogin,
        cookies: Vec<BrowserCookie>,
        proxy_url: &str,
        base: ProfileSecret,
    ) -> Result<(String, String, ProfileSecret), String> {
        let account = onecrew_sessions::x_account_with_proxy(&cookies, Some(proxy_url))
            .map_err(session_material_message)?;
        let me = account.who_am_i().await.map_err(|e| e.to_string())?;
        let secret = ProfileSecret {
            provider_account_id: me.id.clone(),
            x_chat_passcode: pending.x_chat_passcode.clone(),
            ..base
        };
        Ok((me.id, me.username, secret))
    }
}
