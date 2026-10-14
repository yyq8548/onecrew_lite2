use std::collections::HashSet;

use onecrew_x_client::XClient;
use onecrew_x_transport::JsonTransport;
use serde_json::Value;

use crate::parser::{conversation, inbox_items, messages, raw_events};
use crate::{
    ChatKeyMaterial, NativeXChat, XChatApi, XChatConversation, XChatDmCapability,
    XChatDmCapabilityProbe, XChatDmPermissions, XChatError, XChatParticipant, XChatSendResult,
};

#[derive(Clone)]
pub struct XChatService<T: JsonTransport + Clone> {
    client: XClient<T>,
    api: XChatApi<T>,
    /// Key recovery runs over the account's own egress; see
    /// [`crate::juicebox`] for why it is injected instead of defaulted.
    juicebox: std::sync::Arc<dyn chat_xdk_core::keys::juicebox::JuiceboxApi>,
    /// De-duplicates the conversation page across the calls one user action
    /// fires together. Refreshing a DM panel runs the capability probe and
    /// the thread read in parallel, and each used to fetch the same page —
    /// and pay for its own key recovery to decrypt it.
    pages: std::sync::Arc<ConversationPages>,
}

/// A per-conversation single-flight window over `GetConversationPageQuery`.
/// A plain cache would not help: the probe and the thread read start at the
/// same moment and would both miss, so the second caller has to wait on the
/// first one's request and then read its result.
#[derive(Default)]
struct ConversationPages {
    slots: std::sync::Mutex<
        std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<Option<CachedPage>>>>,
    >,
}

struct CachedPage {
    page: Value,
    /// The page is reusable for a request that asks for no more events than
    /// it was fetched with; a larger request must go to the provider.
    event_limit: u16,
    fetched_at: std::time::Instant,
}

/// Long enough to cover the concurrent pair of one refresh, short enough
/// that it is a de-duplication window rather than a cache of remote state.
const PAGE_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

impl ConversationPages {
    fn slot(
        &self,
        conversation_id: &str,
    ) -> std::sync::Arc<tokio::sync::Mutex<Option<CachedPage>>> {
        self.slots
            .lock()
            .expect("conversation page slots")
            .entry(conversation_id.to_owned())
            .or_default()
            .clone()
    }

    fn invalidate(&self, conversation_id: &str) {
        if let Some(slot) = self
            .slots
            .lock()
            .expect("conversation page slots")
            .get(conversation_id)
            .cloned()
            && let Ok(mut cached) = slot.try_lock()
        {
            *cached = None;
        }
    }
}

impl<T: JsonTransport + Clone> XChatService<T> {
    /// Share one `XClient` with the caller so its resolved identity and
    /// query-id caches are not duplicated per service, and take the Juicebox
    /// implementation the caller built for this profile's egress.
    pub fn with_client(
        client: XClient<T>,
        transport: T,
        juicebox: std::sync::Arc<dyn chat_xdk_core::keys::juicebox::JuiceboxApi>,
    ) -> Self {
        Self {
            client,
            api: XChatApi::new(transport),
            juicebox,
            pages: std::sync::Arc::new(ConversationPages::default()),
        }
    }

    pub async fn inbox(
        &self,
        passcode: &mut [u8],
        limit: u16,
    ) -> Result<Vec<XChatConversation>, XChatError> {
        let self_user = self.client.current_user().await?;
        // Inbox preview: `limit` conversations, a few recent messages each.
        let page = self.api.inbox(limit, 5).await?;
        let selected = inbox_items(&page)?;
        let mut conversations = selected.iter().map(conversation).collect::<Vec<_>>();
        let participant_ids = conversations
            .iter()
            .flat_map(|item| {
                item.participants
                    .iter()
                    .map(|participant| participant.id.clone())
            })
            .collect::<Vec<_>>();
        let events = selected.iter().flat_map(raw_events).collect();
        let decrypted = self
            .decrypt(&self_user.id, &participant_ids, events, passcode)
            .await?;
        let all_messages = messages(decrypted);
        for item in &mut conversations {
            item.messages = all_messages
                .iter()
                .filter(|message| message.conversation_id == item.id)
                .cloned()
                .collect();
        }
        Ok(conversations)
    }

    /// Every conversation-page read goes through here so one user action
    /// pays for one fetch. Holding the slot lock across the request IS the
    /// single flight: a second caller waits, then reads the result.
    async fn conversation_page(
        &self,
        conversation_id: &str,
        event_limit: u16,
    ) -> Result<Value, XChatError> {
        let slot = self.pages.slot(conversation_id);
        let mut cached = slot.lock().await;
        if let Some(entry) = cached.as_ref()
            && entry.event_limit >= event_limit
            && entry.fetched_at.elapsed() < PAGE_WINDOW
        {
            return Ok(entry.page.clone());
        }
        let page = self.api.conversation(conversation_id, event_limit).await?;
        *cached = Some(CachedPage {
            page: page.clone(),
            event_limit,
            fetched_at: std::time::Instant::now(),
        });
        Ok(page)
    }

    pub async fn thread(
        &self,
        conversation_id: Option<&str>,
        username: &str,
        passcode: &mut [u8],
        limit: usize,
    ) -> Result<Option<XChatConversation>, XChatError> {
        let self_user = self.client.current_user().await?;
        // The initial page returns up to `inbox_conversation_event_limit` recent
        // message events per conversation, so ask for `limit` of them directly.
        let event_limit = limit.clamp(1, 200) as u16;
        if let Some(explicit_id) = conversation_id.map(str::trim).filter(|id| !id.is_empty()) {
            let target_user = self.client.user_profile(username).await?;
            validate_direct_conversation_id(explicit_id, &self_user.id, &target_user.id)?;
            let page = self.conversation_page(explicit_id, event_limit).await?;
            let item = page.pointer("/data/get_conversation_page").ok_or_else(|| {
                XChatError::Protocol(
                    "GetConversationPageQuery response is missing data.get_conversation_page"
                        .into(),
                )
            })?;
            if item.is_null() {
                return Err(XChatError::ConversationNotFound(explicit_id.to_owned()));
            }
            if !item.is_object() {
                return Err(XChatError::Protocol(
                    "GetConversationPageQuery data.get_conversation_page is not an object".into(),
                ));
            }
            let conversation = XChatConversation {
                id: explicit_id.to_owned(),
                participants: vec![
                    XChatParticipant {
                        id: self_user.id.clone(),
                        name: self_user.name,
                        username: self_user.username,
                        profile_image_url: self_user.profile_image_url,
                    },
                    XChatParticipant {
                        id: target_user.id,
                        name: target_user.name,
                        username: target_user.username,
                        profile_image_url: target_user.profile_image_url,
                    },
                ],
                messages: Vec::new(),
            };
            return self
                .decrypt_thread(&self_user.id, conversation, item, passcode, limit)
                .await
                .map(Some);
        }
        let Some((conversation, inbox_item)) = self
            .find_conversation(conversation_id, username, event_limit)
            .await?
        else {
            return Ok(None);
        };
        self.decrypt_thread(&self_user.id, conversation, &inbox_item, passcode, limit)
            .await
            .map(Some)
    }

    async fn decrypt_thread(
        &self,
        self_user_id: &str,
        mut conversation: XChatConversation,
        item: &Value,
        passcode: &mut [u8],
        limit: usize,
    ) -> Result<XChatConversation, XChatError> {
        let events = raw_events(item);
        let participant_ids = conversation
            .participants
            .iter()
            .map(|participant| participant.id.clone())
            .collect::<Vec<_>>();
        let mut decrypted = messages(
            self.decrypt(self_user_id, &participant_ids, events, passcode)
                .await?,
        );
        // Dedupe by id (event fields can overlap), then newest-first, keep `limit`.
        let mut seen = HashSet::new();
        decrypted.retain(|message| seen.insert(message.id.clone()));
        decrypted.sort_by_key(|message| std::cmp::Reverse(message.created_at_msec));
        decrypted.truncate(limit);
        conversation.messages = decrypted;
        Ok(conversation)
    }

    pub async fn send(
        &self,
        username: &str,
        expected_target_user_id: &str,
        text: &str,
        passcode: &mut [u8],
    ) -> Result<XChatSendResult, XChatError> {
        if text.trim().is_empty() {
            return Err(XChatError::Input("message text is required".into()));
        }
        let self_user = self.client.current_user().await?;
        let target_user = self.client.user_profile(username).await?;
        validate_send_target(&target_user.id, expected_target_user_id)?;
        let conversation_id = direct_conversation_id(&self_user.id, &target_user.id)?;
        let ids = vec![self_user.id.clone(), target_user.id.clone()];
        let public_keys = self.api.public_keys(&ids, true).await?;
        let keys = ChatKeyMaterial::from_public_keys_response(&public_keys, &self_user.id)?;
        let chat = NativeXChat::from_key_material(&keys, self.juicebox.clone())?;
        chat.unlock(passcode).await?;
        chat.apply_identity(&self_user.id, &keys);
        let conversation_page = self.conversation_page(&conversation_id, 200).await?;
        let events = raw_events(
            conversation_page
                .pointer("/data/get_conversation_page")
                .unwrap_or(&Value::Null),
        );
        let event_refs = events.iter().map(String::as_str).collect::<Vec<_>>();
        let decrypted = chat.decrypt_events(&event_refs);
        let payload = if decrypted.conversation_keys.is_empty() {
            let permissions =
                XChatDmPermissions::from_public_keys_response(&public_keys, &target_user.id)?;
            permissions.ensure_message_request_allowed()?;
            let payload = chat.prepare_unencrypted_message_request(&conversation_id, text)?;
            self.api
                .send_unencrypted_message_request(&conversation_id, &payload)
                .await?;
            payload
        } else {
            let payload = chat.encrypt_message(&conversation_id, text)?;
            self.api.send(&conversation_id, &payload).await?;
            payload
        };
        // The thread just changed: the window must not serve a page that
        // predates this message to the readback that follows.
        self.pages.invalidate(&conversation_id);
        Ok(XChatSendResult {
            message_id: payload.message_id,
            conversation_id,
            username: username.trim_start_matches('@').to_owned(),
        })
    }

    pub async fn direct_message_capability(
        &self,
        username: &str,
        passcode: &mut [u8],
    ) -> Result<XChatDmCapabilityProbe, XChatError> {
        let self_user = self.client.current_user().await?;
        let target_user = self.client.user_profile(username).await?;
        let conversation_id = direct_conversation_id(&self_user.id, &target_user.id)?;
        let ids = vec![self_user.id.clone(), target_user.id.clone()];
        let public_keys = self.api.public_keys(&ids, true).await?;
        let keys = ChatKeyMaterial::from_public_keys_response(&public_keys, &self_user.id)?;
        let chat = NativeXChat::from_key_material(&keys, self.juicebox.clone())?;
        chat.unlock(passcode).await?;
        chat.apply_identity(&self_user.id, &keys);
        let conversation_page = self.conversation_page(&conversation_id, 200).await?;
        let events = raw_events(
            conversation_page
                .pointer("/data/get_conversation_page")
                .unwrap_or(&Value::Null),
        );
        let event_refs = events.iter().map(String::as_str).collect::<Vec<_>>();
        let decrypted = chat.decrypt_events(&event_refs);
        let capability = if decrypted.conversation_keys.is_empty() {
            message_request_capability(
                XChatDmPermissions::from_public_keys_response(&public_keys, &target_user.id)?,
                &decrypted.errors,
            )
        } else {
            // Building (but not sending) an encrypted payload exercises the
            // same key/signing protocol path as `send`, so genuine established
            // conversation crypto failures remain probe errors.
            chat.encrypt_message(&conversation_id, "capability probe")?;
            XChatDmCapability::ExistingConversation
        };
        Ok(XChatDmCapabilityProbe {
            target_user_id: target_user.id,
            capability,
        })
    }

    async fn find_conversation(
        &self,
        conversation_id: Option<&str>,
        username: &str,
        event_limit: u16,
    ) -> Result<Option<(XChatConversation, Value)>, XChatError> {
        let target = username.trim_start_matches('@').to_lowercase();
        let expected_id = conversation_id.map(str::trim).filter(|id| !id.is_empty());
        let page = self.api.inbox(100, event_limit).await?;
        Ok(inbox_items(&page)?.iter().find_map(|item| {
            let conversation = conversation(item);
            let matches = expected_id.map_or_else(
                || {
                    conversation
                        .participants
                        .iter()
                        .any(|participant| participant.username.to_lowercase() == target)
                },
                |id| conversation.id == id,
            );
            matches.then(|| (conversation, item.clone()))
        }))
    }

    async fn key_material(
        &self,
        self_user_id: &str,
        participant_ids: &[String],
    ) -> Result<ChatKeyMaterial, XChatError> {
        let mut ids = Vec::with_capacity(participant_ids.len() + 1);
        ids.push(self_user_id.to_owned());
        for id in participant_ids {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
        let keys = self.api.public_keys(&ids, true).await?;
        ChatKeyMaterial::from_public_keys_response(&keys, self_user_id)
    }

    async fn decrypt(
        &self,
        self_user_id: &str,
        participant_ids: &[String],
        events: Vec<String>,
        passcode: &mut [u8],
    ) -> Result<chat_xdk_core::DecryptEventsResult, XChatError> {
        let keys = self.key_material(self_user_id, participant_ids).await?;
        let chat = NativeXChat::from_key_material(&keys, self.juicebox.clone())?;
        chat.unlock(passcode).await?;
        chat.apply_identity(self_user_id, &keys);
        let event_refs = events.iter().map(String::as_str).collect::<Vec<_>>();
        let mut decrypted = chat.decrypt_events(&event_refs);
        let failed_events = event_refs
            .iter()
            .enumerate()
            .filter(|(index, _)| decrypted.errors.contains_key(index))
            .map(|(index, event)| (index, *event))
            .collect::<Vec<_>>();
        let failed_event_refs = failed_events
            .iter()
            .map(|(_, event)| *event)
            .collect::<Vec<_>>();
        let message_requests =
            NativeXChat::decrypt_unencrypted_message_requests(&keys, &failed_event_refs)?;
        let recovered_events = message_requests
            .iter()
            .filter_map(|message| message.original_b64.as_deref())
            .collect::<HashSet<_>>();
        for (index, event) in failed_events {
            if recovered_events.contains(event) {
                decrypted.errors.remove(&index);
            }
        }
        let mut seen = decrypted
            .messages
            .iter()
            .filter_map(|message| message.original_b64.clone())
            .collect::<HashSet<_>>();
        decrypted
            .messages
            .extend(message_requests.into_iter().filter(|message| {
                message
                    .original_b64
                    .as_ref()
                    .is_none_or(|event| seen.insert(event.clone()))
            }));
        Ok(decrypted)
    }
}

fn validate_send_target(actual: &str, expected: &str) -> Result<(), XChatError> {
    if expected.is_empty() || actual != expected {
        return Err(XChatError::Input(
            "X direct-message target stable ID changed".into(),
        ));
    }
    Ok(())
}

fn message_request_capability(
    permissions: XChatDmPermissions,
    _non_message_event_errors: &std::collections::HashMap<usize, String>,
) -> XChatDmCapability {
    // Reactions, system events, and newer unknown event variants can be
    // intentionally undecodable as messages. Without conversation keys they
    // are not evidence that a new message request is blocked; only X's typed
    // chat_permissions determine that capability.
    permissions.message_request_capability()
}

fn direct_conversation_id(self_user_id: &str, target_user_id: &str) -> Result<String, XChatError> {
    fn decimal(value: &str) -> bool {
        !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
    }
    if !decimal(self_user_id) || !decimal(target_user_id) {
        return Err(XChatError::Input(
            "direct-message user ids must be decimal X rest ids".into(),
        ));
    }
    let self_first = (self_user_id.len(), self_user_id) <= (target_user_id.len(), target_user_id);
    Ok(if self_first {
        format!("{self_user_id}:{target_user_id}")
    } else {
        format!("{target_user_id}:{self_user_id}")
    })
}

fn validate_direct_conversation_id(
    conversation_id: &str,
    self_user_id: &str,
    target_user_id: &str,
) -> Result<(), XChatError> {
    if conversation_id != direct_conversation_id(self_user_id, target_user_id)? {
        return Err(XChatError::Input(
            "explicit conversation id does not match the selected X users".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn direct_conversation_ids_are_numeric_and_canonical() {
        assert_eq!(
            direct_conversation_id("1642944842", "365968174").unwrap(),
            "365968174:1642944842"
        );
        assert!(direct_conversation_id("Riverfjs", "365968174").is_err());
        assert!(
            validate_direct_conversation_id("365968174:1642944842", "1642944842", "365968174")
                .is_ok()
        );
        assert!(
            validate_direct_conversation_id("365968174:9999999999", "1642944842", "365968174")
                .is_err()
        );
    }

    #[test]
    fn mixed_unknown_events_do_not_override_real_dm_permission_signals() {
        let event_errors = HashMap::from([
            (0, "unsupported reaction event".to_owned()),
            (1, "unknown system event variant".to_owned()),
        ]);
        let allowed = message_request_capability(
            XChatDmPermissions {
                can_dm: true,
                dm_blocking: false,
                passes_premium_check: true,
            },
            &event_errors,
        );
        assert_eq!(allowed, XChatDmCapability::NewMessageRequestAllowed);

        let blocked = message_request_capability(
            XChatDmPermissions {
                can_dm: true,
                dm_blocking: true,
                passes_premium_check: true,
            },
            &event_errors,
        );
        assert_eq!(blocked, XChatDmCapability::TargetDmBlocking);
    }
}

#[cfg(test)]
mod page_window_tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use chat_xdk_core::keys::juicebox::{
        DeleteResult, JuiceboxApi, JuiceboxConfig, RecoverResult, RegisterResult,
    };
    use onecrew_x_client::XClient;
    use onecrew_x_transport::{JsonTransport, Method, TransportError};
    use serde_json::{Value, json};
    use url::Url;

    use super::XChatService;

    #[derive(Clone, Default)]
    struct CountingTransport {
        conversation_requests: Arc<AtomicUsize>,
        changed_target: bool,
    }

    #[async_trait]
    impl JsonTransport for CountingTransport {
        async fn request(
            &self,
            _method: Method,
            url: Url,
            _body: Option<Value>,
        ) -> Result<Value, TransportError> {
            if self.changed_target {
                if url.path().ends_with("account/multi/list.json") {
                    return Ok(json!({"users":[{"screen_name":"operator"}]}));
                }
                if url.path().ends_with("UserByScreenName") {
                    let variables = url
                        .query_pairs()
                        .find(|(name, _)| name == "variables")
                        .unwrap()
                        .1;
                    let variables: Value = serde_json::from_str(&variables).unwrap();
                    let username = variables["screen_name"].as_str().unwrap();
                    let id = if username == "operator" { "100" } else { "300" };
                    return Ok(
                        json!({"data":{"user":{"result":{"rest_id":id,"legacy":{"screen_name":username}}}}}),
                    );
                }
                panic!("a changed target must stop before key recovery or sending");
            }
            // The chat GraphQL reads are GETs whose variables ride the query
            // string, so the operation name in the path is what identifies them.
            if url.path().ends_with("GetConversationPageQuery") {
                self.conversation_requests.fetch_add(1, Ordering::SeqCst);
            }
            Ok(json!({ "data": { "get_conversation_page": { "events": [] } } }))
        }

        async fn request_form(
            &self,
            _method: Method,
            _url: Url,
            _form: BTreeMap<String, String>,
        ) -> Result<Value, TransportError> {
            unreachable!("the page window never posts a form")
        }

        async fn fetch_text(&self, _url: Url) -> Result<String, TransportError> {
            unreachable!("the page window never scans bundles")
        }
    }

    /// The window is transport-level; key recovery never runs in these tests.
    struct UnusedJuicebox;

    #[async_trait]
    impl JuiceboxApi for UnusedJuicebox {
        async fn register_private_key(
            &self,
            _pin: &[u8],
            _config: &JuiceboxConfig,
            _secret: &[u8],
        ) -> RegisterResult {
            unreachable!("no key work in the page window tests")
        }
        async fn recover_private_key(
            &self,
            _pin: &[u8],
            _config: &JuiceboxConfig,
        ) -> RecoverResult {
            unreachable!("no key work in the page window tests")
        }
        async fn delete_keys(&self, _config: &JuiceboxConfig) -> DeleteResult {
            unreachable!("no key work in the page window tests")
        }
    }

    fn chat_service(transport: CountingTransport) -> XChatService<CountingTransport> {
        XChatService::with_client(
            XClient::new(transport.clone()),
            transport,
            Arc::new(UnusedJuicebox),
        )
    }

    #[tokio::test]
    async fn a_reassigned_alias_never_reaches_key_recovery_or_send() {
        let transport = CountingTransport {
            changed_target: true,
            ..Default::default()
        };
        let service = chat_service(transport);
        let mut passcode = [1, 2, 3, 4];
        let error = service
            .send("recipient", "200", "message", &mut passcode)
            .await
            .unwrap_err();
        assert!(matches!(error, crate::XChatError::Input(_)));
    }

    #[tokio::test]
    async fn one_user_action_pays_for_one_conversation_page() {
        let transport = CountingTransport::default();
        let requests = Arc::clone(&transport.conversation_requests);
        let service = chat_service(transport);

        // The capability probe and the thread read start together; the second
        // must wait on the first rather than fetch the same page again.
        let (first, second) = tokio::join!(
            service.conversation_page("conversation-1", 200),
            service.conversation_page("conversation-1", 200),
        );
        first.expect("page");
        second.expect("page");
        assert_eq!(requests.load(Ordering::SeqCst), 1);

        // A narrower request reuses a wider page…
        service
            .conversation_page("conversation-1", 100)
            .await
            .expect("page");
        assert_eq!(requests.load(Ordering::SeqCst), 1);

        // …but a wider one must not be served from a narrower page.
        let narrow = CountingTransport::default();
        let narrow_requests = Arc::clone(&narrow.conversation_requests);
        let narrow_service = chat_service(narrow);
        narrow_service
            .conversation_page("conversation-2", 50)
            .await
            .expect("page");
        narrow_service
            .conversation_page("conversation-2", 200)
            .await
            .expect("page");
        assert_eq!(narrow_requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_send_retires_the_page_it_would_otherwise_replay() {
        let transport = CountingTransport::default();
        let requests = Arc::clone(&transport.conversation_requests);
        let service = chat_service(transport);

        service
            .conversation_page("conversation-1", 200)
            .await
            .expect("page");
        assert_eq!(requests.load(Ordering::SeqCst), 1);

        service.pages.invalidate("conversation-1");
        service
            .conversation_page("conversation-1", 200)
            .await
            .expect("page");
        assert_eq!(
            requests.load(Ordering::SeqCst),
            2,
            "the readback after a send must not see the pre-send page"
        );

        // Other conversations are untouched by that invalidation.
        service.pages.invalidate("conversation-unknown");
        service
            .conversation_page("conversation-1", 200)
            .await
            .expect("page");
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }
}
