//! Full-access credential validation and header-driven credential health,
//! end to end against a scripted localhost Resend: adapter -> email-platform
//! transport -> vault health rows.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{CredentialServices, ResendCredentialTestError};
use crate::credentials::{EmailMailboxAuthResolver, email_gateway::RuntimeEmailGateway};
use onecrew_creator_runtime::{
    EmailAuthConnection, EmailAuthConnectionId, EmailGateway, EmailProviderKind,
    EmailSecretRecordId, MailboxAuthBinding, MailboxProvider, MailboxRefId,
};
use onecrew_email::ResendAdmission;
use onecrew_email_contracts::{
    EmailBatchItem, EmailBatchItemOutcome, EmailBatchRequest, EmailBatchValidation,
};
use onecrew_email_contracts::{EmailError, ProviderErrorClass};
use onecrew_local_vault::LocalVault;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const DOMAINS_OK: &str = r#"{"object":"list","has_more":false,"data":[{"id":"d1","name":"example-team.com","status":"verified","created_at":"2026-08-01T00:00:00Z","region":"us-east-1","capabilities":{"sending":"enabled","receiving":"enabled"}}]}"#;
const EMPTY_LIST: &str = r#"{"object":"list","has_more":false,"data":[]}"#;
const AUTH_401: &str =
    r#"{"statusCode":401,"message":"API key is invalid","name":"invalid_api_key"}"#;
const RATE_429: &str =
    r#"{"statusCode":429,"message":"Too many requests","name":"rate_limit_exceeded"}"#;
const DAILY_QUOTA_429: &str =
    r#"{"statusCode":429,"message":"Daily quota reached","name":"daily_quota_exceeded"}"#;
const BATCH_ACCEPTED_TWO: &str = r#"{
    "data":[
        {"id":"9a1c2b3d-0001-4a5b-8c6d-000000000001"},
        {"id":"9a1c2b3d-0002-4a5b-8c6d-000000000002"}
    ],
    "errors":[]
}"#;
struct Scripted {
    status: u16,
    headers: Vec<(String, String)>,
    body: &'static str,
}

fn json(status: u16, body: &'static str) -> Scripted {
    Scripted {
        status,
        headers: vec![
            ("ratelimit-limit".into(), "10".into()),
            ("ratelimit-remaining".into(), "9".into()),
            ("ratelimit-reset".into(), "1".into()),
        ],
        body,
    }
}

fn json_with(status: u16, body: &'static str, headers: &[(&str, &str)]) -> Scripted {
    let mut response = json(status, body);
    response.headers.extend(
        headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
    );
    response
}

async fn start_server(responses: Vec<Scripted>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let queue = Arc::clone(&queue);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 8192];
                let mut read_total = 0;
                loop {
                    match socket.read(&mut buffer[read_total..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(read) => {
                            read_total += read;
                            if buffer[..read_total].windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                }
                let response = queue.lock().unwrap().pop_front().unwrap_or(Scripted {
                    status: 500,
                    headers: Vec::new(),
                    body: r#"{"statusCode":500,"message":"script exhausted","name":"internal_server_error"}"#,
                });
                let mut wire = format!("HTTP/1.1 {} TEST\r\n", response.status);
                for (name, value) in &response.headers {
                    wire.push_str(&format!("{name}: {value}\r\n"));
                }
                wire.push_str(&format!(
                    "content-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    response.body.len(),
                    response.body
                ));
                // Fixture writes are best-effort: the client under test decides
                // when it stops reading, and the test asserts that outcome.
                if let Err(error) = socket.write_all(wire.as_bytes()).await {
                    eprintln!("fixture response write failed: {error}");
                    return;
                }
                if let Err(error) = socket.shutdown().await {
                    eprintln!("fixture socket shutdown failed: {error}");
                }
            });
        }
    });
    base_url
}

/// A local scripted provider that keys responses on the Authorization header.
/// This makes rotation tests fail if the registry reuses a sender containing
/// the old key, while keeping both keys entirely inside the test process.
async fn start_rotation_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0_u8; 8192];
                let mut read_total = 0;
                loop {
                    match socket.read(&mut buffer[read_total..]).await {
                        Ok(0) | Err(_) => return,
                        Ok(read) => {
                            read_total += read;
                            if buffer[..read_total].windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                    }
                }
                let request = String::from_utf8_lossy(&buffer[..read_total]);
                let status_and_body = if request.contains("Bearer re_rotation_old") {
                    (401, AUTH_401)
                } else if request.contains("Bearer re_rotation_new") {
                    if request.starts_with("GET /domains") {
                        (200, DOMAINS_OK)
                    } else {
                        (200, EMPTY_LIST)
                    }
                } else {
                    (401, AUTH_401)
                };
                let (status, body) = status_and_body;
                let wire = format!(
                    "HTTP/1.1 {status} TEST\r\ncontent-type: application/json\r\nratelimit-limit: 10\r\nratelimit-remaining: 9\r\nratelimit-reset: 1\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                // Fixture writes are best-effort: the client under test decides
                // when it stops reading, and the test asserts that outcome.
                if let Err(error) = socket.write_all(wire.as_bytes()).await {
                    eprintln!("fixture response write failed: {error}");
                    return;
                }
                if let Err(error) = socket.shutdown().await {
                    eprintln!("fixture socket shutdown failed: {error}");
                }
            });
        }
    });
    base_url
}

fn adapter_for(base_url: &str) -> (tempfile::TempDir, Arc<CredentialServices>) {
    let root = tempfile::tempdir().unwrap();
    let vault = LocalVault::open(root.path().join("vault.db")).unwrap();
    let transport = Arc::new(ResendAdmission::with_base_url(base_url).unwrap());
    let adapter = Arc::new(CredentialServices::new(vault, transport));
    (root, adapter)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

struct StaticMailboxResolver(MailboxAuthBinding);
#[async_trait::async_trait]
impl EmailMailboxAuthResolver for StaticMailboxResolver {
    async fn resolve_mailbox_auth_connection(
        &self,
        _mailbox_ref_id: &MailboxRefId,
    ) -> Result<MailboxAuthBinding, String> {
        Ok(self.0.clone())
    }
}

fn bind_mailbox(adapter: &CredentialServices, saved: &onecrew_local_domain::ServiceCredential) {
    adapter.bind_email_mailbox_resolver(Arc::new(StaticMailboxResolver(MailboxAuthBinding {
        mailbox_ref_id: MailboxRefId::new(saved.id.to_string()),
        provider: MailboxProvider::Resend,
        label: saved.label.clone(),
        email_address: "hello@example-team.com".into(),
        display_name: None,
        capabilities: vec![
            onecrew_creator_runtime::EmailCapability::Send,
            onecrew_creator_runtime::EmailCapability::SentReadback,
            onecrew_creator_runtime::EmailCapability::Inbox,
            onecrew_creator_runtime::EmailCapability::Reply,
        ],
        available: true,
        auth_connection: EmailAuthConnection {
            auth_connection_id: EmailAuthConnectionId::new("auth-test"),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new(saved.id.to_string()),
            label: saved.label.clone(),
            available: true,
            created_at: 0,
            updated_at: 0,
        },
    })));
}

#[tokio::test]
async fn validation_fails_closed_when_the_sent_read_is_unavailable() {
    // Order of probes: domains, sent list, received list.
    let base_url = start_server(vec![json(200, DOMAINS_OK), json(401, AUTH_401)]).await;
    let (_root, adapter) = adapter_for(&base_url);

    let error = adapter
        .validate_resend_credential("hello@example-team.com", None, "re_secret_key_value")
        .await
        .unwrap_err();
    assert!(
        !error.contains("re_secret_key_value"),
        "validation errors must never echo the key: {error}"
    );

    let listed = adapter.list_resend_credentials().unwrap();
    assert!(listed.is_empty(), "a failed validation persists nothing");
}

#[tokio::test]
async fn registry_rebuilds_sender_after_same_uuid_resend_rotation() {
    let base_url = start_rotation_server().await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential(
            "rotation-mailbox",
            "hello@example-team.com",
            None,
            "re_rotation_old",
        )
        .unwrap();
    bind_mailbox(&adapter, &saved);

    adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .expect_err("old key is rejected by the scripted provider");
    // Same label preserves the typed secret UUID but must advance its
    // updated_at cache revision and rebuild the provider sender.
    let rotated = adapter
        .put_resend_credential(
            "rotation-mailbox",
            "hello@example-team.com",
            None,
            "re_rotation_new",
        )
        .unwrap();
    assert_eq!(rotated.id, saved.id);
    adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .expect("rotated key must be used for the next resolve/send");
}

#[tokio::test]
async fn validation_requires_both_read_directions_and_reports_full_access() {
    let base_url = start_server(vec![
        json(200, DOMAINS_OK),
        json(200, EMPTY_LIST),
        json(200, EMPTY_LIST),
    ])
    .await;
    let (_root, adapter) = adapter_for(&base_url);

    let validation = adapter
        .validate_resend_credential("hello@example-team.com", Some("Reacher"), "re_secret_key")
        .await
        .unwrap();
    assert!(validation.message.contains("full read access"));
    assert!(validation.sender_domain_verified);
}

#[tokio::test]
async fn an_auth_failure_disables_the_credential() {
    let base_url = start_server(vec![json(401, AUTH_401)]).await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);

    adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .unwrap_err();

    let credential = &adapter.list_resend_credentials().unwrap()[0];
    assert!(credential.disabled);
    assert_eq!(credential.cooldown_until, None);
}

#[tokio::test]
async fn a_stored_key_test_keeps_provider_and_network_failures_typed() {
    let base_url = start_server(vec![json(401, AUTH_401)]).await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);
    let rejected = adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        rejected,
        ResendCredentialTestError::Provider(EmailError::Provider(ref failure))
            if failure.class == ProviderErrorClass::Auth
    ));

    // Nothing listens on the released port, so no HTTP response arrives.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let closed = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let (_root, adapter) = adapter_for(&closed);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);
    let unreachable = adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .unwrap_err();
    assert!(matches!(
        unreachable,
        ResendCredentialTestError::Provider(EmailError::Transport(_))
    ));
    assert!(!unreachable.to_string().contains("re_secret"));
}

#[tokio::test]
async fn quota_exhaustion_cools_down_without_retry_and_without_disable() {
    // The daily-quota 429 is terminal: exactly one domains call then one
    // sent-list call reach the server; nothing is retried.
    let base_url = start_server(vec![json(200, DOMAINS_OK), json(429, DAILY_QUOTA_429)]).await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);

    let before = unix_now();
    let error = adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Daily quota"),
        "typed quota message: {error}"
    );

    let credential = &adapter.list_resend_credentials().unwrap()[0];
    assert!(!credential.disabled, "quota exhaustion is not a dead key");
    let cooldown = credential.cooldown_until.expect("quota sets a cooldown");
    assert!(cooldown >= before + 86_399 && cooldown <= before + 86_401);
}

#[tokio::test]
async fn a_transient_429_cools_down_per_retry_after_not_fifteen_minutes() {
    // Four attempts (initial + 3 bounded retries), all rate limited with a
    // one-second retry-after.
    let rate = || json_with(429, RATE_429, &[("retry-after", "1")]);
    let base_url = start_server(vec![json(200, DOMAINS_OK), rate(), rate(), rate(), rate()]).await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);

    let before = unix_now();
    adapter
        .test_resend_credential(&saved.id.to_string())
        .await
        .unwrap_err();

    let credential = &adapter.list_resend_credentials().unwrap()[0];
    assert!(!credential.disabled);
    let cooldown = credential
        .cooldown_until
        .expect("rate limit sets a cooldown");
    assert!(
        cooldown <= before + 30,
        "cooldown follows retry-after, not a fixed fifteen minutes (got +{})",
        cooldown.saturating_sub(before)
    );
    assert!(cooldown >= before);
}

#[tokio::test]
async fn adapter_batch_projection_preserves_indexes_without_duplicate_readback() {
    // The Creator runtime is the single readback owner. The adapter must return
    // the provider correlation unchanged so a Creator readback failure remains a
    // durable remote_accepted recovery state rather than an Unknown+completed
    // result produced by a second adapter read.
    let base_url = start_server(vec![json(200, BATCH_ACCEPTED_TWO)]).await;
    let (_root, adapter) = adapter_for(&base_url);
    let saved = adapter
        .put_resend_credential("mailbox", "hello@example-team.com", None, "re_secret")
        .unwrap();
    bind_mailbox(&adapter, &saved);

    let item = |index: usize| EmailBatchItem {
        from_name: Some("Reacher".into()),
        from_email: "hello@example-team.com".into(),
        to: vec![format!("creator-{index}@example.net")],
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: None,
        references: Vec::new(),
        subject: Some(format!("Hello {index}")),
        text: Some("Batch body".into()),
        html: None,
        template: None,
    };
    let adapter = RuntimeEmailGateway::new(adapter.clone());
    let result = adapter
        .send_many_email(
            &MailboxRefId::new(saved.id.to_string()),
            EmailBatchRequest {
                idempotency_key: "adapter-batch-readback".into(),
                validation: EmailBatchValidation::Strict,
                items: vec![item(0), item(1)],
            },
        )
        .await
        .unwrap();

    assert_eq!(result.outcomes.len(), 2);
    assert_eq!(result.outcomes[0].input_index, 0);
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0001-4a5b-8c6d-000000000001"
    ));
    assert_eq!(result.outcomes[1].input_index, 1);
    assert!(matches!(
        result.outcomes[1].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0002-4a5b-8c6d-000000000002"
    ));
    assert_eq!(result.chunks.len(), 1);
    assert_eq!(
        result.chunks[0]
            .outcomes
            .iter()
            .map(|item| item.input_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(result.chunks[0].outcomes, result.outcomes);
    assert!(!result.logical_fingerprint.is_empty());
}
