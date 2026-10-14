use chat_xdk_core::{DecryptEventsResult, Event};
use serde_json::Value;

use crate::{XChatConversation, XChatError, XChatMessage, XChatParticipant};

pub(crate) fn inbox_items(page: &Value) -> Result<&[Value], XChatError> {
    page.pointer("/data/get_initial_chat_page/items")
        .ok_or_else(|| {
            XChatError::Protocol(
                "GetInitialXChatPageQuery response is missing data.get_initial_chat_page.items"
                    .into(),
            )
        })?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| {
            XChatError::Protocol(
                "GetInitialXChatPageQuery data.get_initial_chat_page.items is not an array".into(),
            )
        })
}

pub(crate) fn conversation(item: &Value) -> XChatConversation {
    let detail = item.get("conversation_detail").unwrap_or(&Value::Null);
    XChatConversation {
        id: text(detail.get("conversation_id")),
        participants: participants(detail),
        messages: Vec::new(),
    }
}

fn participants(detail: &Value) -> Vec<XChatParticipant> {
    detail
        .get("participants_results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|row| {
            let user = row.get("result").unwrap_or(&Value::Null);
            XChatParticipant {
                id: text(row.get("rest_id")).or_else_text(|| text(user.get("rest_id"))),
                name: text(user.pointer("/core/name")),
                username: text(user.pointer("/core/screen_name")),
                profile_image_url: text(user.pointer("/avatar/image_url")),
            }
        })
        .collect()
}

pub(crate) fn raw_events(item: &Value) -> Vec<String> {
    const EVENT_FIELDS: [&str; 4] = [
        "latest_conversation_key_change_events",
        "missing_conversation_key_change_events",
        "latest_message_events",
        "encoded_message_events",
    ];
    EVENT_FIELDS
        .iter()
        .flat_map(|name| {
            item.get(*name)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(Value::as_str)
        .filter(|event| !event.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn messages(result: DecryptEventsResult) -> Vec<XChatMessage> {
    result
        .messages
        .into_iter()
        .filter_map(|decrypted| match decrypted.event {
            Event::Message(message) => {
                let text = message.text().unwrap_or_default().to_owned();
                Some(XChatMessage {
                    id: message.meta.id.unwrap_or_default(),
                    conversation_id: message.meta.conversation_id.unwrap_or_default(),
                    sender_id: message.meta.sender_id.unwrap_or_default(),
                    text,
                    created_at_msec: message.meta.created_at_msec,
                    verified: message.verified,
                })
            }
            _ => None,
        })
        .collect()
}

fn text(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).unwrap_or_default().to_owned()
}

trait StringFallback {
    fn or_else_text(self, fallback: impl FnOnce() -> String) -> String;
}

impl StringFallback for String {
    fn or_else_text(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::inbox_items;

    #[test]
    fn empty_inbox_is_distinct_from_a_malformed_response() {
        let empty = json!({
            "data": { "get_initial_chat_page": { "items": [] } }
        });
        assert!(inbox_items(&empty).unwrap().is_empty());

        let missing = json!({ "data": { "get_initial_chat_page": {} } });
        assert!(inbox_items(&missing).is_err());

        let wrong_type = json!({
            "data": { "get_initial_chat_page": { "items": {} } }
        });
        assert!(inbox_items(&wrong_type).is_err());
    }
}
