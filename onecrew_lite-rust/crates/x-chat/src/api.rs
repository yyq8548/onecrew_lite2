use std::sync::Arc;

use chat_xdk_core::SendPayload;
use onecrew_x_transport::JsonTransport;
use onecrew_x_transport::Method;
use serde_json::{Value, json};
use url::Url;

use crate::XChatError;

const CHAT_GRAPHQL: &str = "https://api.x.com/graphql";

#[derive(Clone)]
pub struct XChatApi<T: JsonTransport> {
    transport: Arc<T>,
}

impl<T: JsonTransport> XChatApi<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Arc::new(transport),
        }
    }

    async fn get(
        &self,
        query_id: &str,
        operation: &str,
        variables: Value,
    ) -> Result<Value, XChatError> {
        let mut url = Url::parse(&format!("{CHAT_GRAPHQL}/{query_id}/{operation}"))
            .map_err(|error| XChatError::Url(error.to_string()))?;
        url.query_pairs_mut()
            .append_pair("variables", &variables.to_string());
        Ok(self.transport.request(Method::GET, url, None).await?)
    }

    pub async fn public_keys(
        &self,
        user_ids: &[String],
        include_juicebox_tokens: bool,
    ) -> Result<Value, XChatError> {
        self.get(
            "nyLCqvDlxI4YoEBf-2ARmQ",
            "GetPublicKeysQuery",
            json!({
                "ids": user_ids,
                "include_juicebox_tokens": include_juicebox_tokens,
            }),
        )
        .await
    }

    /// Initial X Chat page. `conversation_limit` bounds how many conversations
    /// are returned; `event_limit` (`inbox_conversation_event_limit`) bounds how
    /// many recent message events come back per conversation — a full DM thread
    /// is loaded by raising this rather than by a separate pagination query.
    pub async fn inbox(
        &self,
        conversation_limit: u16,
        event_limit: u16,
    ) -> Result<Value, XChatError> {
        self.get(
            "h-zxrvrkbzmYTcTTusI8-w",
            "GetInitialXChatPageQuery",
            json!({
                "max_local_sequence_id": null,
                "query_settings": query_settings(conversation_limit, event_limit),
                "message_pull_version": null
            }),
        )
        .await
    }

    /// Load a direct conversation by its canonical `min:max` id without
    /// consulting Inbox. X includes any missing conversation-key change events
    /// required to decrypt the returned messages.
    pub async fn conversation(
        &self,
        conversation_id: &str,
        event_limit: u16,
    ) -> Result<Value, XChatError> {
        self.get(
            "GX9ZijkxG8AqRMQVD7hMnQ",
            "GetConversationPageQuery",
            json!({
                "conversation_id": conversation_id,
                "min_local_sequence_id": "9223372036854775807",
                "min_conversation_key_version": "9223372036854775807",
                "query_settings": query_settings(20, event_limit),
            }),
        )
        .await
    }

    pub async fn send(
        &self,
        conversation_id: &str,
        payload: &SendPayload,
    ) -> Result<Value, XChatError> {
        let operation = "SendMessageCreateMutation";
        let url = Url::parse(&format!(
            "{CHAT_GRAPHQL}/TWRPP7gnKwV_R8-tE-Dd3Q/{operation}"
        ))
        .map_err(|error| XChatError::Url(error.to_string()))?;
        let variables = json!({
            "conversation_id": conversation_id,
            "message_id": payload.message_id,
            "encoded_message_create_event": payload.encrypted_content,
            "encoded_message_event_signature": payload.encoded_event_signature,
        });
        Ok(self
            .transport
            .request(
                Method::POST,
                url,
                Some(json!({"variables": variables.to_string()})),
            )
            .await?)
    }

    /// Send the signed plaintext message-request form. The explicit null token
    /// is part of X's message-request mutation shape captured from the web
    /// client; established encrypted sends keep using [`Self::send`].
    pub async fn send_unencrypted_message_request(
        &self,
        conversation_id: &str,
        payload: &SendPayload,
    ) -> Result<Value, XChatError> {
        let operation = "SendMessageCreateMutation";
        let url = Url::parse(&format!(
            "{CHAT_GRAPHQL}/TWRPP7gnKwV_R8-tE-Dd3Q/{operation}"
        ))
        .map_err(|error| XChatError::Url(error.to_string()))?;
        let variables = json!({
            "conversation_id": conversation_id,
            "message_id": payload.message_id,
            "conversation_token": null,
            "encoded_message_create_event": payload.encrypted_content,
            "encoded_message_event_signature": payload.encoded_event_signature,
        });
        Ok(self
            .transport
            .request(
                Method::POST,
                url,
                Some(json!({"variables": variables.to_string()})),
            )
            .await?)
    }
}

fn query_settings(conversation_limit: u16, event_limit: u16) -> Value {
    json!({
        "conversation_event_limit": 200,
        "inbox_conversation_event_limit": event_limit.clamp(1, 200),
        "inbox_conversation_limit": conversation_limit.clamp(1, 100),
        "user_event_limit": 500
    })
}
