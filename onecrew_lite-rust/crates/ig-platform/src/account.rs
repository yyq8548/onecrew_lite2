use std::collections::BTreeMap;

use crate::IgWriteFailure;
use onecrew_ig_client::{
    IgClient, IgCommentPage, IgMediaPage, IgRelationshipStatus, IgThread, IgUser, IgUserPage,
};
use onecrew_ig_transport::{IgError, IgGeo, IgSession, IgTransport};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgRelationshipAction {
    Follow,
    Unfollow,
}

/// Application-facing Instagram facade. The desktop app and MCP layer call this
/// and never see device headers, endpoints, or session-JSON details.
#[derive(Clone)]
pub struct IgAccount {
    client: IgClient,
}

impl IgAccount {
    /// Restore an account from a session JSON previously produced by
    /// [`IgAccount::import`] (stored in the vault).
    pub fn from_session_json(session_json: &str, proxy: Option<&str>) -> Result<Self, IgError> {
        let session = IgSession::from_json(session_json)?;
        Ok(Self {
            client: IgClient::new(IgTransport::new(session, proxy)?),
        })
    }

    /// First-time import from the browser login's **full** Instagram cookie set
    /// (sessionid + the real rur/shbid/shbts/mid/csrftoken routing cookies): build
    /// a fresh device session aligned to the profile's 家宽 `geo` (locale / country
    /// / timezone — so the device geo matches the login IP), validate it live, and
    /// return the signed-in user plus the full session JSON to persist and reuse
    /// (never re-import per request).
    pub async fn import(
        cookies: &BTreeMap<String, String>,
        proxy: Option<&str>,
        geo: Option<IgGeo>,
    ) -> Result<(IgUser, String), IgError> {
        let mut session = IgSession::from_browser_cookies(cookies)?;
        if let Some(geo) = geo {
            session.apply_geo(&geo);
        }
        let client = IgClient::new(IgTransport::new(session, proxy)?);
        let user = client.account_info().await?;
        let session_json = client.session_snapshot().to_json()?;
        Ok((user, session_json))
    }

    /// Current session JSON (refreshed with any server-issued `mid`/www-claim);
    /// persist after operations so the reused session stays current.
    pub fn session_json(&self) -> Result<String, IgError> {
        self.client.session_snapshot().to_json()
    }

    pub async fn who_am_i(&self) -> Result<IgUser, IgError> {
        self.client.account_info().await
    }

    pub async fn user(&self, username: &str) -> Result<IgUser, IgError> {
        self.client.user_by_username(username).await
    }

    pub async fn search_users(&self, query: &str, count: u16) -> Result<Vec<IgUser>, IgError> {
        self.client.search_users(query, count).await
    }

    pub async fn inbox(&self, amount: u32) -> Result<Vec<IgThread>, IgError> {
        self.client.inbox(amount).await
    }

    pub async fn thread(&self, thread_id: &str, amount: u32) -> Result<IgThread, IgError> {
        self.client.thread(thread_id, amount).await
    }

    /// A page of a user's posts, resolved by @username.
    pub async fn user_posts(
        &self,
        username: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<IgMediaPage, IgError> {
        let user = self.client.user_by_username(username).await?;
        self.client.user_medias(&user.id, amount, max_id).await
    }

    /// A page of a user's followers, resolved by @username.
    pub async fn followers(
        &self,
        username: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<IgUserPage, IgError> {
        let user = self.client.user_by_username(username).await?;
        self.client.followers(&user.id, amount, max_id).await
    }

    /// A page of the accounts a user follows, resolved by @username.
    pub async fn following(
        &self,
        username: &str,
        amount: u16,
        max_id: Option<&str>,
    ) -> Result<IgUserPage, IgError> {
        let user = self.client.user_by_username(username).await?;
        self.client.following(&user.id, amount, max_id).await
    }

    /// Resolve a username to its stable scoped id and read relationship state.
    pub async fn relationship_status(
        &self,
        username: &str,
    ) -> Result<(String, IgRelationshipStatus), IgError> {
        let user = self.client.user_by_username(username).await?;
        let status = self.client.relationship_status(&user.id).await?;
        Ok((user.id, status))
    }

    /// A page of comments on a media id (from `user_posts`).
    pub async fn media_comments(
        &self,
        media_id: &str,
        max_id: Option<&str>,
    ) -> Result<IgCommentPage, IgError> {
        self.client.media_comments(media_id, max_id).await
    }

    /// Relationship operations bind the immutable provider ID all the way to
    /// the request path and form. No mutable alias is resolved after approval.
    pub async fn update_relationship(
        &self,
        action: IgRelationshipAction,
        stable_id: &str,
    ) -> Result<(), IgWriteFailure> {
        if !accepts_stable_user_id(stable_id) {
            return Err(IgWriteFailure::Refused);
        }
        let result = match action {
            IgRelationshipAction::Follow => self.client.follow(stable_id).await,
            IgRelationshipAction::Unfollow => self.client.unfollow(stable_id).await,
        }
        .map_err(|error| IgWriteFailure::from(&error))?;
        if !result {
            return Err(IgWriteFailure::Unknown);
        }
        Ok(())
    }

    pub async fn relationship_status_by_id(
        &self,
        stable_id: &str,
    ) -> Result<IgRelationshipStatus, IgError> {
        if !accepts_stable_user_id(stable_id) {
            return Err(IgError::Api("Instagram stable user ID is required".into()));
        }
        self.client.relationship_status(stable_id).await
    }

    /// Send a DM to the immutable numeric ID verified by the caller.
    pub async fn send_dm(&self, stable_id: &str, text: &str) -> Result<String, IgError> {
        if !accepts_stable_user_id(stable_id) || text.trim().is_empty() {
            return Err(IgError::Api(
                "Instagram stable recipient ID and body are required".into(),
            ));
        }
        self.client.send_dm(stable_id, text).await
    }

    /// Like (or unlike) a media by id.
    pub async fn like(&self, media_id: &str, revert: bool) -> Result<bool, IgError> {
        self.client.like(media_id, revert).await
    }

    /// Comment on a media id (optionally replying to a comment id).
    pub async fn comment(
        &self,
        media_id: &str,
        text: &str,
        replied_to: Option<&str>,
    ) -> Result<String, IgError> {
        self.client.comment(media_id, text, replied_to).await
    }

    /// Delete your own comment(s) on a media id (`comment_ids` comma-separated).
    pub async fn delete_comments(
        &self,
        media_id: &str,
        comment_ids: &str,
    ) -> Result<bool, IgError> {
        self.client.delete_comments(media_id, comment_ids).await
    }
}

/// Whether `value` is an Instagram stable user ID: a non-empty run of ASCII
/// decimal digits. Stable-ID reads and writes check it before any request.
pub fn accepts_stable_user_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::accepts_stable_user_id;

    #[test]
    fn a_stable_user_id_is_a_non_empty_run_of_ascii_digits() {
        for (value, accepted) in [
            ("", false),
            ("0", true),
            ("9007199254740993", true),
            (" 42", false),
            ("42 ", false),
            ("-42", false),
            ("4a2", false),
            ("@creator", false),
            ("\u{0664}\u{0662}", false),
        ] {
            assert_eq!(accepts_stable_user_id(value), accepted, "{value:?}");
        }
    }
}
