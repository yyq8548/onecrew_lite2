use std::sync::Arc;

use chat_xdk_core::keys::juicebox::JuiceboxApi;
use chat_xdk_core::prelude::JuiceboxConfig;
use chat_xdk_core::{
    Chat, DecryptEventsResult, DecryptedMessage, EncryptMessageParams, Event,
    PrepareUnencryptedMessageRequestParams, SendPayload,
};
use serde_json::Value;

use crate::{ChatKeyMaterial, XChatError};

pub struct NativeXChat {
    chat: Chat,
}

impl NativeXChat {
    /// The Juicebox implementation is injected rather than defaulted: the
    /// stock one builds its own un-proxied reqwest client, which would send
    /// this account's key recovery to X's realms from a different IP and
    /// fingerprint than everything else it does (see [`crate::juicebox`]).
    pub fn new(config: JuiceboxConfig, juicebox: Arc<dyn JuiceboxApi>) -> Self {
        Self {
            chat: Chat::with_juicebox(config, juicebox),
        }
    }

    pub fn from_x_api_config(
        config: &Value,
        juicebox: Arc<dyn JuiceboxApi>,
    ) -> Result<Self, XChatError> {
        JuiceboxConfig::from_x_api_json(&config.to_string())
            .map(|config| Self::new(config, juicebox))
            .map_err(XChatError::Crypto)
    }

    /// Build the chat from a profile's Juicebox config. Identity and signing
    /// keys are applied *after* [`Self::unlock`] — see [`Self::apply_identity`].
    pub fn from_key_material(
        material: &ChatKeyMaterial,
        juicebox: Arc<dyn JuiceboxApi>,
    ) -> Result<Self, XChatError> {
        Self::from_x_api_config(&material.juicebox_config, juicebox)
    }

    /// Decode X Chat message requests that intentionally carry plaintext
    /// contents and no signature. This uses the SDK's supported opt-in for
    /// unverified events, but never exposes other unverified event types to the
    /// application.
    pub(crate) fn decrypt_unencrypted_message_requests(
        material: &ChatKeyMaterial,
        events: &[&str],
    ) -> Result<Vec<DecryptedMessage>, XChatError> {
        let config = JuiceboxConfig::from_x_api_json(&material.juicebox_config.to_string())
            .map_err(XChatError::Crypto)?;
        let mut chat = Chat::new(config);
        chat.set_reject_unverified(false);

        Ok(chat
            .decrypt_events(events, &[])
            .messages
            .into_iter()
            .filter(|decrypted| {
                matches!(
                    &decrypted.event,
                    Event::Message(message)
                        if message.key_version.is_none() && !message.verified
                )
            })
            .collect())
    }

    pub async fn unlock(&self, passcode: &mut [u8]) -> Result<(), XChatError> {
        let result = self
            .chat
            .unlock(passcode)
            .await
            .map_err(|error| XChatError::Crypto(error.to_string()));
        passcode.fill(0);
        result
    }

    /// Apply identity + signing keys. MUST run *after* [`Self::unlock`]:
    /// unlocking loads the recovered keypair and resets the stored signing-key
    /// version to `None` ("the caller sets the new one after import"), so the
    /// identity has to be set last — otherwise `encrypt_message` fails with
    /// "signing_key_version is not set". This mirrors the official client's
    /// unlock → setIdentity → setSigningKeys order.
    pub fn apply_identity(&self, user_id: &str, material: &ChatKeyMaterial) {
        self.chat
            .set_identity(user_id, &material.signing_key_version);
        self.chat.set_cache_keys(true);
        self.chat.set_signing_keys(material.signing_keys.clone());
    }

    pub fn decrypt_events(&self, events: &[&str]) -> DecryptEventsResult {
        self.chat.decrypt_events(events, &[])
    }

    pub fn encrypt_message(
        &self,
        conversation_id: &str,
        text: &str,
    ) -> Result<SendPayload, XChatError> {
        self.chat
            .encrypt_message(EncryptMessageParams::new(conversation_id, text))
            .map_err(|error| XChatError::Crypto(error.to_string()))
    }

    pub fn prepare_unencrypted_message_request(
        &self,
        conversation_id: &str,
        text: &str,
    ) -> Result<SendPayload, XChatError> {
        self.chat
            .prepare_unencrypted_message_request(PrepareUnencryptedMessageRequestParams::new(
                conversation_id,
                text,
            ))
            .map_err(|error| XChatError::Crypto(error.to_string()))
    }
}
