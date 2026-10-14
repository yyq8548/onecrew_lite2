use super::transport::{Failure, Transport};
use onecrew_email_contracts::PlatformEmailSendError;
use reqwest::Method;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Message {
    pub(super) id: String,
    pub(super) raw: String,
    #[serde(default)]
    pub(super) label_ids: Vec<String>,
    pub(super) internal_date: String,
}

pub(super) struct GmailClient(Transport);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MessageList {
    #[serde(default)]
    pub messages: Vec<MessageId>,
    pub next_page_token: Option<String>,
}
#[derive(Deserialize)]
pub(super) struct MessageId {
    pub id: String,
}

impl GmailClient {
    pub(super) async fn outreach_reply_ids(&self, token: &str, sent_id: &str) -> Result<Vec<String>, String> {
        if !valid_id(sent_id) { return Err("gmail_message_id_invalid".into()); }
        let seed = self.0.request(token, Method::GET,
            &format!("/{sent_id}?format=minimal&fields=id,threadId,labelIds"), None)
            .await.map_err(Failure::read_code)?;
        let thread_id = seed.get("threadId").and_then(|v| v.as_str()).filter(|id| valid_id(id))
            .ok_or("gmail_thread_id_invalid")?;
        if seed.get("id").and_then(|v| v.as_str()) != Some(sent_id)
            || !seed.get("labelIds").and_then(|v| v.as_array()).is_some_and(|labels| labels.iter().any(|v| v == "SENT")) {
            return Err("gmail_sent_message_mismatch".into());
        }
        // Resolve only the conversation of a locally confirmed send. Never list the inbox.
        let thread = self.0.request(token, Method::GET,
            &format!("/../threads/{thread_id}?format=minimal&fields=id,messages(id,labelIds)"), None)
            .await.map_err(Failure::read_code)?;
        if thread.get("id").and_then(|v| v.as_str()) != Some(thread_id) {
            return Err("gmail_thread_id_mismatch".into());
        }
        let messages = thread.get("messages").and_then(|v| v.as_array()).ok_or("gmail_thread_invalid")?;
        if !messages.iter().any(|m| m.get("id").and_then(|v| v.as_str()) == Some(sent_id)) {
            return Err("gmail_thread_seed_missing".into());
        }
        let mut ids = Vec::new();
        for message in messages {
            let id = message.get("id").and_then(|v| v.as_str()).filter(|id| valid_id(id)).ok_or("gmail_message_id_invalid")?;
            let labels = match message.get("labelIds") {
                None => &[][..],
                Some(value) => value.as_array().ok_or("gmail_thread_labels_invalid")?.as_slice(),
            };
            if labels.iter().any(|v| v == "SENT" || v == "DRAFT" || v == "TRASH" || v == "SPAM") { continue; }
            if !ids.iter().any(|existing| existing == id) { ids.push(id.to_owned()); }
        }
        Ok(ids)
    }

    pub(super) async fn message_time(&self, token: &str, id: &str) -> Result<String, String> {
        if !valid_id(id) {
            return Err("gmail_message_id_invalid".into());
        }
        let value = self
            .0
            .request(
                token,
                Method::GET,
                &format!("/{id}?format=minimal&fields=id,internalDate"),
                None,
            )
            .await
            .map_err(Failure::read_code)?;
        if value.get("id").and_then(|v| v.as_str()) != Some(id) {
            return Err("gmail_message_id_mismatch".into());
        }
        let millis = value
            .get("internalDate")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or("gmail_date_invalid")?;
        chrono::DateTime::from_timestamp_millis(millis)
            .map(|t| t.to_rfc3339())
            .ok_or("gmail_date_invalid".into())
    }
    pub(super) async fn inbox(
        &self,
        token: &str,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<MessageList, String> {
        if limit == 0 || limit > 100 || cursor.is_some_and(|s| s.len() > 4096) {
            return Err("gmail_page_invalid".into());
        }
        let query = {
            let mut query = url::form_urlencoded::Serializer::new(String::new());
            query
                .append_pair("labelIds", "INBOX")
                .append_pair("maxResults", &limit.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("pageToken", cursor);
            }
            query.finish()
        };
        let value = self
            .0
            .request(token, Method::GET, &format!("?{query}"), None)
            .await
            .map_err(Failure::read_code)?;
        let page: MessageList =
            serde_json::from_value(value).map_err(|_| "gmail_inbox_list_invalid")?;
        if page.messages.len() > usize::from(limit)
            || page.messages.iter().any(|m| !valid_id(&m.id))
            || page
                .next_page_token
                .as_deref()
                .is_some_and(|next| next.is_empty() || next.len() > 4096 || Some(next) == cursor)
            || page
                .messages
                .iter()
                .map(|m| &m.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != page.messages.len()
        {
            return Err("gmail_inbox_list_invalid".into());
        }
        Ok(page)
    }
    pub(super) fn new() -> Result<Self, String> {
        Ok(Self(Transport::new()?))
    }
    pub(super) async fn send(
        &self,
        token: &str,
        raw: String,
    ) -> Result<String, PlatformEmailSendError> {
        let value = self
            .0
            .request(
                token,
                Method::POST,
                "/send",
                Some(serde_json::json!({"raw":raw})),
            )
            .await
            .map_err(send_error)?;
        value
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|id| valid_id(id))
            .map(str::to_owned)
            .ok_or_else(|| PlatformEmailSendError::Ambiguous {
                message: "gmail_send_response_invalid".into(),
            })
    }
    pub(super) async fn sent(&self, token: &str, id: &str) -> Result<Message, String> {
        if !valid_id(id) {
            return Err("gmail_message_id_invalid".into());
        }
        let value = self
            .0
            .request(token, Method::GET, &format!("/{id}?format=raw"), None)
            .await
            .map_err(Failure::read_code)?;
        serde_json::from_value(value).map_err(|_| "gmail_readback_invalid".into())
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn send_error(error: Failure) -> PlatformEmailSendError {
    match error {
        Failure::Http(status) | Failure::HttpReason(status, _) if (400..500).contains(&status) => PlatformEmailSendError::Rejected {
            status,
            message: "gmail_send_rejected".into(),
        },
        _ => PlatformEmailSendError::Ambiguous {
            message: "gmail_send_result_unknown".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn outreach_replies_use_only_verified_thread_and_exclude_sent_and_drafts() {
        use std::io::{Read, Write};
        for valid in [true, false] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let responses = [
                    ("/messages/abc?format=minimal&fields=id,threadId,labelIds", r#"{"id":"abc","threadId":"def","labelIds":["SENT"]}"#.to_owned()),
                    ("/threads/def?format=minimal&fields=id,messages(id,labelIds)",
                        if valid { r#"{"id":"def","messages":[{"id":"abc","labelIds":["SENT"]},{"id":"111","labelIds":["INBOX"]},{"id":"222","labelIds":["DRAFT"]},{"id":"333"},{"id":"444","labelIds":["SPAM"]}]}"# }
                        else { r#"{"id":"fff","messages":[{"id":"abc","labelIds":["SENT"]}]}"# }.to_owned()),
                ];
                for (path, body) in responses {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();
                    let mut bytes = [0; 8192];
                    let n = stream.read(&mut bytes).unwrap();
                    let request = std::str::from_utf8(&bytes[..n]).unwrap();
                    assert!(request.starts_with(&format!("GET {path} HTTP/1.1")), "unexpected path");
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            let client = GmailClient(Transport::fixture(format!("http://{address}/messages")));
            let result = client.outreach_reply_ids("fixture", "abc").await;
            if valid { assert_eq!(result.unwrap(), vec!["111", "333"]); }
            else { assert_eq!(result.unwrap_err(), "gmail_thread_id_mismatch"); }
            server.join().unwrap();
        }
    }

    #[tokio::test]
    async fn inbox_pagination_is_encoded_and_invalid_pages_are_rejected() {
        use std::io::{Read, Write};
        for (body, ok) in [
            (
                r#"{"messages":[{"id":"abc123"}],"nextPageToken":"next"}"#,
                true,
            ),
            (r#"{}"#, true),
            (r#"{"messages":[{"id":"../escape"}]}"#, false),
            (r#"{"messages":[{"id":"ab"},{"id":"ab"}]}"#, false),
            (r#"{"nextPageToken":"old&labelIds=SENT"}"#, false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 8192];
                let n = stream.read(&mut bytes).unwrap();
                let request = std::str::from_utf8(&bytes[..n]).unwrap();
                assert!(request.starts_with(
                    "GET /?labelIds=INBOX&maxResults=100&pageToken=old%26labelIds%3DSENT HTTP/1.1"
                ));
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            });
            let client = GmailClient(Transport::fixture(format!("http://{address}")));
            assert_eq!(
                client
                    .inbox("fixture", Some("old&labelIds=SENT"), 100)
                    .await
                    .is_ok(),
                ok
            );
            server.join().unwrap();
        }
    }
    #[tokio::test]
    async fn http_send_preserves_acceptance_and_redacts_provider_errors() {
        use std::io::{Read, Write};
        for (status, body, accepted) in [
            (200, r#"{"id":"abc123"}"#, true),
            (200, r#"{"unexpected":"secret"}"#, false),
            (401, r#"{"error":"secret"}"#, false),
            (500, "secret", false),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = [0; 8192];
                let count = stream.read(&mut bytes).unwrap();
                let request = std::str::from_utf8(&bytes[..count]).unwrap();
                assert!(request.starts_with("POST /send HTTP/1.1"));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("authorization: bearer fixture-token")
                );
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let client = GmailClient(Transport::fixture(format!("http://{address}")));
            let result = client.send("fixture-token", "dGVzdA".into()).await;
            assert_eq!(result.is_ok(), accepted);
            if let Err(error) = result {
                assert!(!error.to_string().contains("secret"));
                assert_eq!(error.is_terminal_rejection(), status == 401);
            }
            server.join().unwrap();
        }
    }
    #[test]
    fn network_and_server_failures_never_become_safe_to_retry() {
        for error in [Failure::Unknown, Failure::Http(500), Failure::Http(302)] {
            assert!(!send_error(error).is_terminal_rejection());
        }
        assert!(send_error(Failure::Http(401)).is_terminal_rejection());
        assert!(!valid_id("../other"));
    }
    #[test]
    fn read_diagnostics_preserve_failure_class_without_provider_content() {
        assert_eq!(Failure::Http(403).read_code(), "gmail_read_http_403");
        assert_eq!(Failure::Timeout.read_code(), "gmail_read_timeout");
        assert_eq!(Failure::TooLarge.read_code(), "gmail_read_response_too_large");
        assert_eq!(Failure::InvalidJson.read_code(), "gmail_read_response_invalid_json");
        for error in [Failure::Timeout, Failure::TooLarge, Failure::InvalidJson] {
            assert!(!send_error(error).is_terminal_rejection());
        }
    }
}
