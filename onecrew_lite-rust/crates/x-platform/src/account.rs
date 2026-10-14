use onecrew_x_chat::{
    WreqJuiceboxApi, XChatConversation, XChatDmCapabilityProbe, XChatSendResult, XChatService,
};
use onecrew_x_client::{
    AdvancedSearchCriteria, AdvancedSearchPage, AdvancedSearchResultMode, BookmarkFolderPage,
    RelationshipAction, TimelineKind, TimelinePage, Tweet, TweetAction, User, UserListKind,
    UserPage, XClient,
};
use onecrew_x_transport::{WreqXTransport, XSession};

use crate::{XError, XWriteFailure};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XRelationshipStatus {
    pub external_id: String,
    pub following: Option<bool>,
    pub can_message: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XReplyReceipt {
    pub external_id: String,
}

#[derive(Clone)]
pub struct XAccount {
    client: XClient<WreqXTransport>,
    chat: XChatService<WreqXTransport>,
}

impl XAccount {
    /// Reads the alias and verifies its immutable identity before returning
    /// viewer relationship flags. Missing provider flags remain unknown.
    pub async fn relationship_status(
        &self,
        stable_id: &str,
        handle: &str,
    ) -> Result<XRelationshipStatus, XError> {
        let user = self.profile(handle).await?;
        if stable_id.is_empty() || user.id != stable_id {
            return Err(onecrew_x_client::XClientError::Input(
                "X relationship target stable ID changed".into(),
            )
            .into());
        }
        Ok(XRelationshipStatus {
            external_id: user.id,
            following: user.viewer_follows,
            can_message: user.can_dm,
        })
    }

    /// Sends to the immutable numeric ID. A successful request still requires
    /// a separate relationship readback by the application workflow.
    pub async fn update_relationship(
        &self,
        action: RelationshipAction,
        stable_id: &str,
    ) -> Result<(), XWriteFailure> {
        if !decimal_id(stable_id) {
            return Err(XWriteFailure::Refused);
        }
        self.relationship(action, stable_id)
            .await
            .map_err(|error| error.write_failure())
    }

    pub async fn reply_with_receipt(
        &self,
        reply_to_id: &str,
        body: &str,
    ) -> Result<XReplyReceipt, XWriteFailure> {
        if !decimal_id(reply_to_id) || body.trim().is_empty() {
            return Err(XWriteFailure::Refused);
        }
        let external_id = self
            .reply(reply_to_id, body, &[])
            .await
            .map_err(|error| error.write_failure())?;
        if !decimal_id(&external_id) {
            return Err(XWriteFailure::Unknown);
        }
        Ok(XReplyReceipt { external_id })
    }

    /// Reads the exact receipt's detail page. A missing item is unverified;
    /// it never proves the earlier write did not happen.
    pub async fn reply_readback(&self, receipt_id: &str) -> Result<Option<Tweet>, XError> {
        Ok(self
            .tweet_thread(receipt_id, 20, None)
            .await?
            .items
            .into_iter()
            .find(|tweet| tweet.id == receipt_id))
    }

    /// Builds one account facade whose Web and Chat clients share the same
    /// authenticated transport policy.
    ///
    /// # Errors
    ///
    /// Returns [`XError`] when the X transport or the X-operated Juicebox
    /// client cannot be constructed, including an invalid proxy URL.
    pub fn new(session: XSession, proxy: Option<&str>) -> Result<Self, XError> {
        let transport = WreqXTransport::new(session, proxy)?;
        // One client for both surfaces: the identity and query-id caches
        // live behind its Arcs, so sharing it halves the lookups a DM sync
        // performs.
        let client = XClient::new(transport.clone());
        // Key recovery leaves by the same door as everything else this
        // account sends: same Chrome emulation, same residential proxy.
        let juicebox = std::sync::Arc::new(WreqJuiceboxApi::new(proxy)?);
        Ok(Self {
            chat: XChatService::with_client(client.clone(), transport, juicebox),
            client,
        })
    }

    pub async fn who_am_i(&self) -> Result<User, XError> {
        Ok(self.client.current_user().await?)
    }

    pub async fn profile(&self, username: &str) -> Result<User, XError> {
        Ok(self.client.user_profile(username).await?)
    }

    pub async fn search_advanced(
        &self,
        criteria: &AdvancedSearchCriteria,
        result_mode: AdvancedSearchResultMode,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<AdvancedSearchPage, XError> {
        Ok(self
            .client
            .search_advanced_page(criteria, result_mode, count, cursor)
            .await?)
    }

    pub async fn timeline(
        &self,
        kind: TimelineKind,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self.client.timeline(kind, count, cursor).await?)
    }

    pub async fn user_posts(
        &self,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self.client.user_tweets(user_id, count, cursor).await?)
    }

    pub async fn user_likes(
        &self,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self.client.user_likes(user_id, count, cursor).await?)
    }

    pub async fn list_timeline(
        &self,
        list_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self.client.list_timeline(list_id, count, cursor).await?)
    }

    pub async fn tweet_thread(
        &self,
        tweet_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self.client.tweet_detail(tweet_id, count, cursor).await?)
    }

    pub async fn article(&self, tweet_id: &str) -> Result<Tweet, XError> {
        Ok(self.client.article(tweet_id).await?)
    }

    pub async fn bookmark_folders(
        &self,
        cursor: Option<&str>,
    ) -> Result<BookmarkFolderPage, XError> {
        Ok(self.client.bookmark_folders(cursor).await?)
    }

    pub async fn bookmark_folder(
        &self,
        folder_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<TimelinePage, XError> {
        Ok(self
            .client
            .bookmark_folder_timeline(folder_id, count, cursor)
            .await?)
    }

    pub async fn users(
        &self,
        kind: UserListKind,
        user_id: &str,
        count: u16,
        cursor: Option<&str>,
    ) -> Result<UserPage, XError> {
        Ok(self.client.users(kind, user_id, count, cursor).await?)
    }

    pub async fn post(&self, text: &str, media_ids: &[String]) -> Result<String, XError> {
        Ok(self.client.create_post(text, None, None, media_ids).await?)
    }

    pub async fn upload_media(&self, bytes: &[u8], media_type: &str) -> Result<String, XError> {
        Ok(self.client.upload_media(bytes, media_type).await?)
    }

    pub async fn reply(
        &self,
        tweet_id: &str,
        text: &str,
        media_ids: &[String],
    ) -> Result<String, XError> {
        Ok(self
            .client
            .create_post(text, Some(tweet_id), None, media_ids)
            .await?)
    }

    pub async fn quote(
        &self,
        tweet_id: &str,
        text: &str,
        media_ids: &[String],
    ) -> Result<String, XError> {
        Ok(self
            .client
            .create_post(text, None, Some(tweet_id), media_ids)
            .await?)
    }

    pub async fn mutate_tweet(&self, action: TweetAction, tweet_id: &str) -> Result<(), XError> {
        Ok(self.client.mutate_tweet(action, tweet_id).await?)
    }

    pub async fn relationship(
        &self,
        action: RelationshipAction,
        user_id: &str,
    ) -> Result<(), XError> {
        Ok(self.client.relationship(action, user_id).await?)
    }

    pub async fn inbox(
        &self,
        passcode: &mut [u8],
        limit: u16,
    ) -> Result<Vec<XChatConversation>, XError> {
        Ok(self.chat.inbox(passcode, limit).await?)
    }

    pub async fn thread(
        &self,
        conversation_id: Option<&str>,
        username: &str,
        passcode: &mut [u8],
        limit: usize,
    ) -> Result<Option<XChatConversation>, XError> {
        Ok(self
            .chat
            .thread(conversation_id, username, passcode, limit)
            .await?)
    }

    pub async fn send_message(
        &self,
        username: &str,
        expected_target_user_id: &str,
        text: &str,
        passcode: &mut [u8],
    ) -> Result<XChatSendResult, XError> {
        Ok(self
            .chat
            .send(username, expected_target_user_id, text, passcode)
            .await?)
    }

    pub async fn direct_message_capability(
        &self,
        username: &str,
        passcode: &mut [u8],
    ) -> Result<XChatDmCapabilityProbe, XError> {
        Ok(self
            .chat
            .direct_message_capability(username, passcode)
            .await?)
    }
}

fn decimal_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}
