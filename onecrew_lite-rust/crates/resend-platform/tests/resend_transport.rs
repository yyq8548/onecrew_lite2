//! Integration tests for the Resend transport, facade, and error handling
//! against a scripted localhost HTTP server. Fixtures are the Phase 1 frozen
//! contract; no network leaves the machine.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use onecrew_email_contracts::{
    EmailAdministrationProvider, EmailAttachmentProvider, EmailBatchItem, EmailBatchItemOutcome,
    EmailBatchRequest, EmailBatchTemplate, EmailBatchValidation, EmailError, EmailHistoryProvider,
    EmailSendProvider, OutboundEmail, ProviderErrorClass,
};
use onecrew_resend_platform::{ResendAdmission, ResendPlatform};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
struct Recorded {
    method: String,
    target: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    at: std::time::Instant,
}

#[derive(Debug, Clone)]
struct Scripted {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Scripted {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![
                ("content-type".into(), "application/json".into()),
                ("ratelimit-limit".into(), "10".into()),
                ("ratelimit-remaining".into(), "9".into()),
                ("ratelimit-reset".into(), "1".into()),
            ],
            body: body.as_bytes().to_vec(),
        }
    }

    fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    fn connection_drop() -> Self {
        Self {
            status: 0,
            headers: Vec::new(),
            body: Vec::new(),
        }
    }
}

struct TestServer {
    base_url: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl TestServer {
    async fn start(responses: Vec<Scripted>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let requests: Arc<Mutex<Vec<Recorded>>> = Arc::default();
        let queue = Arc::new(Mutex::new(VecDeque::from(responses)));
        let recorded = Arc::clone(&requests);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let recorded = Arc::clone(&recorded);
                let queue = Arc::clone(&queue);
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0_u8; 4096];
                    let header_end = loop {
                        let read = match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(read) => read,
                        };
                        buffer.extend_from_slice(&chunk[..read]);
                        if let Some(position) =
                            buffer.windows(4).position(|window| window == b"\r\n\r\n")
                        {
                            break position + 4;
                        }
                    };
                    let head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
                    let mut lines = head.split("\r\n");
                    let request_line = lines.next().unwrap_or_default();
                    let mut parts = request_line.split(' ');
                    let method = parts.next().unwrap_or_default().to_owned();
                    let target = parts.next().unwrap_or_default().to_owned();
                    let mut headers = BTreeMap::new();
                    for line in lines {
                        if let Some((name, value)) = line.split_once(':') {
                            headers
                                .insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
                        }
                    }
                    let content_length: usize = headers
                        .get("content-length")
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(0);
                    let mut body = buffer[header_end..].to_vec();
                    while body.len() < content_length {
                        let read = match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(read) => read,
                        };
                        body.extend_from_slice(&chunk[..read]);
                    }
                    recorded.lock().unwrap().push(Recorded {
                        method,
                        target,
                        headers,
                        body,
                        at: std::time::Instant::now(),
                    });

                    let response = queue.lock().unwrap().pop_front().unwrap_or(Scripted {
                        status: 404,
                        headers: Vec::new(),
                        body: br#"{"statusCode":404,"message":"not found","name":"not_found"}"#
                            .to_vec(),
                    });
                    // The request bytes reached the server, but no response
                    // made it back to the caller: an ambiguous
                    // timeout-after-write/connection-drop seam.
                    if response.status == 0 {
                        return;
                    }
                    let mut wire = format!("HTTP/1.1 {} TEST\r\n", response.status);
                    for (name, value) in &response.headers {
                        wire.push_str(&format!("{name}: {value}\r\n"));
                    }
                    wire.push_str(&format!(
                        "content-length: {}\r\nconnection: close\r\n\r\n",
                        response.body.len()
                    ));
                    // Fixture writes are best-effort: the client under test
                    // decides when it stops reading, and the test asserts the
                    // outcome it observed.
                    if let Err(error) = socket.write_all(wire.as_bytes()).await {
                        eprintln!("fixture response head write failed: {error}");
                        return;
                    }
                    if let Err(error) = socket.write_all(&response.body).await {
                        eprintln!("fixture response body write failed: {error}");
                        return;
                    }
                    if let Err(error) = socket.shutdown().await {
                        eprintln!("fixture socket shutdown failed: {error}");
                    }
                });
            }
        });
        Self { base_url, requests }
    }

    fn sender(&self) -> ResendPlatform {
        let transport = Arc::new(ResendAdmission::with_base_url(&self.base_url).unwrap());
        ResendPlatform::new("re_test_key".into(), transport).unwrap()
    }

    fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }
}

fn outbound_reply() -> OutboundEmail {
    OutboundEmail {
        from_name: Some("Reacher".into()),
        from_email: "hello@example-team.com".into(),
        to: vec!["creator-one@example.net".into()],
        cc: Vec::new(),
        bcc: Vec::new(),
        reply_to: Vec::new(),
        in_reply_to: Some("<reply-1@mailer.example.net>".into()),
        references: vec![
            "<outreach-1@mail.example-team.com>".into(),
            "<reply-1@mailer.example.net>".into(),
        ],
        idempotency_key: "creator-one-followup-1".into(),
        subject: "Re: Collab idea".into(),
        text: Some("Great, next steps inside.".into()),
        html: None,
    }
}

#[tokio::test]
async fn send_carries_idempotency_and_reply_ancestry_headers() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{"id":"9a1c2b3d-0004-4a5b-8c6d-000000000004"}"#,
    )])
    .await;
    let receipt = server.sender().send(&outbound_reply()).await.unwrap();
    assert!(receipt.accepted);
    assert_eq!(
        receipt.message_id.as_deref(),
        Some("9a1c2b3d-0004-4a5b-8c6d-000000000004")
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/emails");
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer re_test_key")
    );
    assert_eq!(
        request.headers.get("idempotency-key").map(String::as_str),
        Some("creator-one-followup-1")
    );
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["from"], "Reacher <hello@example-team.com>");
    assert_eq!(
        body["headers"]["In-Reply-To"],
        "<reply-1@mailer.example.net>"
    );
    assert_eq!(
        body["headers"]["References"],
        "<outreach-1@mail.example-team.com> <reply-1@mailer.example.net>"
    );
    assert!(body.get("cc").is_none(), "empty lists stay off the wire");
}

#[tokio::test]
async fn sent_page_maps_the_frozen_fixture_through_the_facade() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        include_str!("fixtures/resend/sent_list_page.json"),
    )])
    .await;
    let page = server
        .sender()
        .sent_page(Some("9a1c2b3d-0000-4a5b-8c6d-000000000000"), 100)
        .await
        .unwrap();
    assert!(page.has_more);
    assert_eq!(
        page.next_cursor.as_deref(),
        Some("9a1c2b3d-0003-4a5b-8c6d-000000000003")
    );
    assert_eq!(
        page.messages[0].internet_message_id.as_deref(),
        Some("<outreach-1@mail.example-team.com>")
    );

    let request = &server.requests()[0];
    assert_eq!(
        request.target,
        "/emails?limit=100&after=9a1c2b3d-0000-4a5b-8c6d-000000000000"
    );
}

#[tokio::test]
async fn provider_resource_ids_cannot_change_the_resend_path() {
    // The concrete platform is a public provider owner in its own right;
    // callers must not be able to bypass the facade's path validation.
    let server = TestServer::start(Vec::new()).await;
    let sender = server.sender();

    assert!(sender.sent("../receiving").await.is_err());
    assert!(sender.received_message("").await.is_err());
    assert!(sender.received(Some("page?x=1"), 10).await.is_err());
    assert!(
        sender
            .received_attachment("ok-id", "../../emails")
            .await
            .is_err()
    );

    let invalid = sender
        .received_details(&["good/id".into(), "bad/../id".into()])
        .await;
    assert_eq!(invalid.len(), 2);
    assert!(invalid[0].1.is_err());
    assert!(invalid[1].1.is_err());
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn received_list_and_detail_carry_identity_headers_and_attachments() {
    let server = TestServer::start(vec![
        Scripted::json(200, include_str!("fixtures/resend/received_list_page.json")),
        Scripted::json(200, include_str!("fixtures/resend/received_detail.json")),
    ])
    .await;
    let sender = server.sender();

    let page = sender.received(None, 100).await.unwrap();
    assert_eq!(page.messages.len(), 2);
    assert_eq!(
        page.messages[0].internet_message_id,
        "<reply-1@mailer.example.net>"
    );
    assert_eq!(page.messages[0].subject, "Re: Collab idea");
    assert_eq!(page.messages[0].attachments.len(), 1);

    let detail = sender
        .received_message("7b2d3e4f-1001-4c5d-9e6f-000000000001")
        .await
        .unwrap();
    assert_eq!(detail.internet_message_id, "<reply-1@mailer.example.net>");
    assert_eq!(
        detail.headers["in-reply-to"],
        "<outreach-1@mail.example-team.com>"
    );
    assert_eq!(detail.attachments.len(), 1);
    assert_eq!(
        detail.attachments[0].provider_attachment_id,
        "2a0c9ce0-3112-4728-976e-000000000001"
    );

    let targets: Vec<String> = server
        .requests()
        .iter()
        .map(|request| request.target.clone())
        .collect();
    assert_eq!(
        targets,
        vec![
            "/emails/receiving?limit=100".to_owned(),
            "/emails/receiving/7b2d3e4f-1001-4c5d-9e6f-000000000001".to_owned(),
        ]
    );
}

#[tokio::test]
async fn received_details_return_per_id_outcomes_in_input_order() {
    // Ids are fetched concurrently, so script by path is impossible with one
    // FIFO queue; use one detail body and one non-transient failure and
    // assert ordering plus isolation instead.
    let detail = include_str!("fixtures/resend/received_detail.json");
    let not_found = r#"{"statusCode":404,"message":"gone","name":"not_found"}"#;
    // Three requests total; the 404 is non-transient so no retries occur.
    let server = TestServer::start(vec![
        Scripted::json(200, detail),
        Scripted::json(404, not_found),
        Scripted::json(200, detail),
    ])
    .await;
    let ids = vec![
        "7b2d3e4f-1001-4c5d-9e6f-000000000001".to_owned(),
        "7b2d3e4f-1002-4c5d-9e6f-000000000002".to_owned(),
        "7b2d3e4f-1003-4c5d-9e6f-000000000003".to_owned(),
    ];
    let results = server.sender().received_details(&ids).await;
    assert_eq!(results.len(), 3);
    for (position, (id, _)) in results.iter().enumerate() {
        assert_eq!(id, &ids[position], "results keep input order");
    }
    let failures: Vec<&EmailError> = results
        .iter()
        .filter_map(|(_, outcome)| outcome.as_ref().err())
        .collect();
    assert_eq!(failures.len(), 1, "exactly one id failed");
    assert_eq!(
        failures[0].provider_failure().unwrap().class,
        ProviderErrorClass::NotFound
    );
    assert_eq!(server.requests().len(), 3, "a 404 is not retried");
}

#[tokio::test]
async fn quota_429_is_terminal_and_never_retried() {
    let server = TestServer::start(vec![Scripted::json(
        429,
        include_str!("fixtures/resend/error_429_daily_quota.json"),
    )])
    .await;
    let error = server.sender().received(None, 10).await.unwrap_err();
    let failure = error.provider_failure().unwrap();
    assert_eq!(failure.class, ProviderErrorClass::DailyQuotaExceeded);
    assert!(!error.is_transient());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn auth_failures_surface_immediately_with_their_class() {
    let server = TestServer::start(vec![Scripted::json(
        401,
        include_str!("fixtures/resend/error_401.json"),
    )])
    .await;
    let error = server.sender().domains().await.unwrap_err();
    assert_eq!(
        error.provider_failure().unwrap().class,
        ProviderErrorClass::Auth
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn transient_429_retries_after_the_provider_directed_wait() {
    let server = TestServer::start(vec![
        Scripted::json(429, include_str!("fixtures/resend/error_429_rate.json"))
            .with_header("retry-after", "1")
            .with_header("ratelimit-remaining", "0"),
        Scripted::json(
            200,
            include_str!("fixtures/resend/received_list_empty.json"),
        ),
    ])
    .await;
    let page = server.sender().received(None, 10).await.unwrap();
    assert!(page.messages.is_empty());

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    let waited = requests[1].at.duration_since(requests[0].at);
    assert!(
        waited >= std::time::Duration::from_millis(950),
        "retry must honor retry-after, waited {waited:?}"
    );
}

#[tokio::test]
async fn rate_limit_headers_adapt_shared_spacing_without_request_bursts() {
    let window_20 = Scripted::json(
        200,
        include_str!("fixtures/resend/received_list_empty.json"),
    )
    .with_header("ratelimit-limit", "20")
    .with_header("ratelimit-remaining", "19")
    .with_header("ratelimit-reset", "1");
    let window_5 = Scripted::json(
        200,
        include_str!("fixtures/resend/received_list_empty.json"),
    )
    .with_header("ratelimit-limit", "5")
    .with_header("ratelimit-remaining", "4")
    .with_header("ratelimit-reset", "1");
    let server = TestServer::start(vec![
        window_20.clone(),
        window_5.clone(),
        window_5.clone(),
        window_5,
    ])
    .await;
    let sender = server.sender();
    for _ in 0..4 {
        sender.received(None, 10).await.unwrap();
    }

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    let first_spacing = requests[1].at.duration_since(requests[0].at);
    let second_spacing = requests[2].at.duration_since(requests[1].at);
    let third_spacing = requests[3].at.duration_since(requests[2].at);
    assert!(
        first_spacing >= std::time::Duration::from_millis(40),
        "20-request header should permit approximately 50 ms spacing, got {first_spacing:?}"
    );
    assert!(
        second_spacing >= std::time::Duration::from_millis(180)
            && third_spacing >= std::time::Duration::from_millis(180),
        "five-request header must slow every subsequent request without a burst: {:?}, {:?}",
        second_spacing,
        third_spacing
    );
}

#[tokio::test]
async fn sync_read_missing_rate_limit_metadata_fails_before_next_page() {
    let server = TestServer::start(vec![
        Scripted {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: include_str!("fixtures/resend/received_list_empty.json")
                .as_bytes()
                .to_vec(),
        },
        Scripted::json(
            200,
            include_str!("fixtures/resend/received_list_empty.json"),
        )
        .with_header("ratelimit-limit", "10")
        .with_header("ratelimit-remaining", "9")
        .with_header("ratelimit-reset", "1"),
    ])
    .await;
    let error = server.sender().received(None, 10).await.unwrap_err();
    assert!(error.to_string().contains("rate-limit headers"), "{error}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn sync_read_invalid_rate_limit_metadata_fails_closed() {
    let server = TestServer::start(vec![
        Scripted::json(
            200,
            include_str!("fixtures/resend/received_list_empty.json"),
        )
        .with_header("ratelimit-reset", "not-a-duration"),
    ])
    .await;
    let error = server.sender().received(None, 10).await.unwrap_err();
    assert!(error.to_string().contains("metadata invalid"), "{error}");
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn attachment_access_flows_but_debug_never_shows_the_signed_url() {
    let server = TestServer::start(vec![
        Scripted::json(
            200,
            include_str!("fixtures/resend/sent_attachments_list.json"),
        ),
        Scripted::json(200, include_str!("fixtures/resend/attachment_get.json")),
    ])
    .await;
    let sender = server.sender();

    let sent = sender
        .sent_attachments("9a1c2b3d-0001-4a5b-8c6d-000000000001")
        .await
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].download_url.contains("signature="));

    let received = sender
        .received_attachment(
            "7b2d3e4f-1001-4c5d-9e6f-000000000001",
            "2a0c9ce0-3112-4728-976e-000000000001",
        )
        .await
        .unwrap();
    assert_eq!(received.metadata.filename, "media-kit.pdf");
    let debug = format!("{received:?}");
    assert!(debug.contains("<redacted signed url>") && !debug.contains("signature="));

    let targets: Vec<String> = server
        .requests()
        .iter()
        .map(|request| request.target.clone())
        .collect();
    assert_eq!(
        targets[1],
        "/emails/receiving/7b2d3e4f-1001-4c5d-9e6f-000000000001/attachments/2a0c9ce0-3112-4728-976e-000000000001"
    );
}

#[tokio::test]
async fn oversized_responses_are_a_transport_fault_not_a_hang() {
    let huge = vec![b'x'; 33 * 1024 * 1024];
    let server = TestServer::start(vec![Scripted {
        status: 200,
        headers: vec![
            ("content-type".into(), "application/json".into()),
            ("ratelimit-limit".into(), "10".into()),
            ("ratelimit-remaining".into(), "9".into()),
            ("ratelimit-reset".into(), "1".into()),
        ],
        body: huge,
    }])
    .await;
    let error = server.sender().received(None, 10).await.unwrap_err();
    match &error {
        EmailError::OversizedResponse { path, .. } => assert_eq!(path, "/emails/receiving"),
        other => panic!("expected an oversized-response fault, got {other:?}"),
    }
    assert!(
        !error.is_transient(),
        "refetching an oversized body is waste"
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn attachment_download_is_bounded_and_stays_inside_the_trust_boundary() {
    let server = TestServer::start(vec![
        Scripted::json(200, include_str!("fixtures/resend/attachment_get.json")),
        Scripted {
            status: 200,
            headers: vec![("content-type".into(), "application/pdf".into())],
            body: b"%PDF-fixture".to_vec(),
        },
    ])
    .await;
    let sender = server.sender();
    let mut access = sender
        .received_attachment(
            "7b2d3e4f-1001-4c5d-9e6f-000000000001",
            "2a0c9ce0-3112-4728-976e-000000000001",
        )
        .await
        .unwrap();

    // The production URL points at the Resend CDN; the localhost test seam
    // exercises the same code path against the scripted server.
    access.download_url = format!("{}/signed/attachment?signature=sig", server.base_url);
    let bytes = sender.download(&access.download_url, 1024).await.unwrap();
    assert_eq!(bytes, b"%PDF-fixture".to_vec());

    // Outside the provider trust boundary: refused before any request.
    let mut foreign = access.clone();
    foreign.download_url = "https://evil.example.com/steal?signature=sig".into();
    let error = sender
        .download(&foreign.download_url, 1024)
        .await
        .unwrap_err();
    assert!(matches!(error, EmailError::InvalidMessage(_)), "{error:?}");

    // Bounded: a cap smaller than the body is a typed oversize, not a hang.
    let server_two = TestServer::start(vec![Scripted {
        status: 200,
        headers: Vec::new(),
        body: vec![b'x'; 64],
    }])
    .await;
    let mut small_cap = access.clone();
    small_cap.download_url = format!("{}/signed/big?signature=sig", server_two.base_url);
    let sender_two = server_two.sender();
    let error = sender_two
        .download(&small_cap.download_url, 16)
        .await
        .unwrap_err();
    assert!(matches!(error, EmailError::OversizedResponse { .. }));
}

#[tokio::test]
async fn a_409_idempotency_race_retries_the_same_key_for_the_twin_outcome() {
    // concurrent_idempotent_requests means the same key is processing right
    // now; the retry reuses the SAME key so the provider replays the
    // original response instead of sending twice.
    let server = TestServer::start(vec![
        Scripted::json(
            409,
            r#"{"statusCode":409,"message":"concurrent request for this key","name":"concurrent_idempotent_requests"}"#,
        ),
        Scripted::json(200, r#"{"id":"9a1c2b3d-0004-4a5b-8c6d-000000000004"}"#),
    ])
    .await;
    let receipt = server.sender().send(&outbound_reply()).await.unwrap();
    assert!(receipt.accepted);

    let requests = server.requests();
    assert_eq!(requests.len(), 2, "the race must be retried, not surfaced");
    assert_eq!(
        requests[0].headers.get("idempotency-key"),
        requests[1].headers.get("idempotency-key"),
        "the retry must reuse the same idempotency key"
    );
}

#[tokio::test]
async fn a_rate_limit_on_the_final_attempt_still_pauses_the_pool() {
    let fault = include_str!("fixtures/resend/error_500.json");
    let server = TestServer::start(vec![
        Scripted::json(500, fault),
        Scripted::json(500, fault),
        Scripted::json(500, fault),
        Scripted::json(429, include_str!("fixtures/resend/error_429_rate.json"))
            .with_header("retry-after", "2"),
        Scripted::json(
            200,
            include_str!("fixtures/resend/received_list_empty.json"),
        ),
    ])
    .await;
    let sender = server.sender();
    let error = sender.received(None, 10).await.unwrap_err();
    assert_eq!(
        error.provider_failure().unwrap().class,
        ProviderErrorClass::RateLimited
    );

    // The terminal 429's retry-after must have reached the shared pool: the
    // next operation waits out the declared window instead of firing into it.
    let page = sender.received(None, 10).await.unwrap();
    assert!(page.messages.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    let waited = requests[4].at.duration_since(requests[3].at);
    assert!(
        waited >= std::time::Duration::from_millis(1900),
        "pool must pause for the terminal 429's retry-after, waited {waited:?}"
    );
}

fn batch_item(index: usize) -> EmailBatchItem {
    EmailBatchItem {
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
    }
}

#[tokio::test]
async fn native_batch_uses_the_released_sdk_endpoint_and_preserves_input_order() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{"data":[{"id":"9a1c2b3d-0005-4a5b-8c6d-000000000005"}],"errors":[]}"#,
    )])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-1".into(),
            validation: EmailBatchValidation::Permissive,
            items: vec![batch_item(0)],
        })
        .await
        .unwrap();
    assert_eq!(result.outcomes.len(), 1);
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0005-4a5b-8c6d-000000000005"
    ));
    let request = &server.requests()[0];
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/emails/batch");
    assert_eq!(
        request
            .headers
            .get("x-batch-validation")
            .map(String::as_str),
        Some("permissive")
    );
    assert!(
        request
            .headers
            .get("idempotency-key")
            .is_some_and(|key| key.starts_with("batch-intent-1:chunk-0-"))
    );
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body[0]["from"], "Reacher <hello@example-team.com>");
    assert_eq!(body[0]["text"], "Batch body");
    assert!(body[0].get("idempotency_key").is_none());
}

#[tokio::test]
async fn native_batch_chunks_at_one_hundred_and_derives_distinct_keys() {
    let server = TestServer::start(vec![
        Scripted::json(200, r#"{"data":[{"id":"first-chunk"}],"errors":[]}"#),
        Scripted::json(200, r#"{"data":[{"id":"second-chunk"}],"errors":[]}"#),
    ])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-2".into(),
            validation: EmailBatchValidation::Strict,
            items: (0..101).map(batch_item).collect(),
        })
        .await
        .unwrap();
    assert_eq!(result.chunks.len(), 2);
    assert_eq!(result.outcomes.len(), 101);
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "first-chunk"
    ));
    assert!(matches!(
        result.outcomes[100].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "second-chunk"
    ));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_ne!(
        requests[0].headers.get("idempotency-key"),
        requests[1].headers.get("idempotency-key")
    );
    let first_body: Vec<serde_json::Value> = serde_json::from_slice(&requests[0].body).unwrap();
    let second_body: Vec<serde_json::Value> = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(first_body.len(), 100);
    assert_eq!(second_body.len(), 1);
}

#[tokio::test]
async fn native_batch_exhausted_transient_fault_is_unknown_and_reuses_chunk_key() {
    let fault = include_str!("fixtures/resend/error_500.json");
    let server = TestServer::start(vec![
        Scripted::json(500, fault),
        Scripted::json(500, fault),
        Scripted::json(500, fault),
        Scripted::json(500, fault),
    ])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-unknown".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![batch_item(0)],
        })
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Unknown { .. }
    ));
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .map(|request| request.headers.get("idempotency-key"))
            .all(|key| key == requests[0].headers.get("idempotency-key"))
    );
}

#[tokio::test]
async fn native_batch_connection_drop_after_write_is_unknown_and_reuses_chunk_key() {
    let server = TestServer::start(vec![
        Scripted::connection_drop(),
        Scripted::connection_drop(),
        Scripted::connection_drop(),
        Scripted::connection_drop(),
    ])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-timeout-after-write".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![batch_item(0)],
        })
        .await
        .unwrap();

    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Unknown { .. }
    ));
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(
        requests
            .iter()
            .map(|request| request.headers.get("idempotency-key"))
            .all(|key| key == requests[0].headers.get("idempotency-key"))
    );
}

#[tokio::test]
async fn native_batch_terminal_provider_errors_remain_request_failures() {
    let server = TestServer::start(vec![Scripted::json(
        401,
        include_str!("fixtures/resend/error_401.json"),
    )])
    .await;
    let error = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-auth-failure".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![batch_item(0)],
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.provider_failure().map(|failure| failure.class),
        Some(ProviderErrorClass::Auth)
    );
    assert_eq!(server.requests().len(), 1, "auth failures are not retried");
}

#[tokio::test]
async fn native_batch_correlates_provider_rejections_and_acceptances_by_input_index() {
    // The provider returns accepted ids only for non-rejected items. The
    // facade must retain the original indexes when it merges those two
    // response collections, rather than treating the error as a whole-batch
    // failure or shifting the second accepted id onto the rejected input.
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{
            "data":[
                {"id":"9a1c2b3d-0010-4a5b-8c6d-000000000010"},
                {"id":"9a1c2b3d-0012-4a5b-8c6d-000000000012"}
            ],
            "errors":[{"index":1,"message":"invalid recipient"}]
        }"#,
    )])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-correlate".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![batch_item(0), batch_item(1), batch_item(2)],
        })
        .await
        .unwrap();

    assert_eq!(result.outcomes.len(), 3);
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0010-4a5b-8c6d-000000000010"
    ));
    assert!(matches!(
        result.outcomes[1].outcome,
        EmailBatchItemOutcome::Rejected { ref message }
            if message == "invalid recipient"
    ));
    assert!(matches!(
        result.outcomes[2].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0012-4a5b-8c6d-000000000012"
    ));
    assert_eq!(
        result
            .chunks
            .first()
            .unwrap()
            .outcomes
            .iter()
            .map(|item| item.input_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[tokio::test]
async fn native_batch_omitted_provider_ids_become_unknown_without_reindexing() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{"data":[{"id":"9a1c2b3d-0013-4a5b-8c6d-000000000013"}],"errors":[]}"#,
    )])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-omitted".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![batch_item(0), batch_item(1)],
        })
        .await
        .unwrap();

    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "9a1c2b3d-0013-4a5b-8c6d-000000000013"
    ));
    assert!(matches!(
        result.outcomes[1].outcome,
        EmailBatchItemOutcome::Unknown { ref message }
            if message == "Resend batch response omitted this item"
    ));
    assert_eq!(result.outcomes[0].input_index, 0);
    assert_eq!(result.outcomes[1].input_index, 1);
}

#[tokio::test]
async fn native_batch_malformed_provider_indexes_fail_closed_without_guessing_ids() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{
            "data":[{"id":"provider-0"},{"id":"provider-1"}],
            "errors":[{"index":99,"message":"invalid recipient"}]
        }"#,
    )])
    .await;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-malformed-index".into(),
            validation: EmailBatchValidation::Permissive,
            items: vec![batch_item(0), batch_item(1)],
        })
        .await
        .unwrap();

    assert!(result.outcomes.iter().all(|item| matches!(
        item.outcome,
        EmailBatchItemOutcome::Unknown { ref message }
            if message == "Resend batch response could not be correlated"
    )));
}

#[tokio::test]
async fn permissive_batch_with_no_valid_items_never_sends_an_empty_provider_batch() {
    let server = TestServer::start(Vec::new()).await;
    let mut first = batch_item(0);
    first.subject = None;
    first.text = None;
    let mut second = batch_item(1);
    second.subject = None;
    second.text = None;

    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-all-rejected".into(),
            validation: EmailBatchValidation::Permissive,
            items: vec![first, second],
        })
        .await
        .unwrap();

    assert!(result.chunks.is_empty());
    assert_eq!(
        result
            .outcomes
            .iter()
            .map(|item| item.input_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(
        result
            .outcomes
            .iter()
            .all(|item| matches!(item.outcome, EmailBatchItemOutcome::Rejected { .. }))
    );
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn published_template_rejects_inline_body_before_any_request() {
    let server = TestServer::start(Vec::new()).await;
    let mut item = batch_item(0);
    item.template = Some(EmailBatchTemplate::Published {
        template_id_or_alias: "welcome".into(),
        variables: std::collections::BTreeMap::new(),
        from_override: None,
        subject_override: Some("Welcome".into()),
        reply_to_override: Vec::new(),
    });
    let error = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-3".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![item],
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("inline text or html"));
    assert!(server.requests().is_empty());
}

#[tokio::test]
async fn published_template_payload_uses_id_variables_and_explicit_overrides() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{"data":[{"id":"published-item"}],"errors":[]}"#,
    )])
    .await;
    let mut item = batch_item(0);
    item.subject = None;
    item.text = None;
    item.template = Some(EmailBatchTemplate::Published {
        template_id_or_alias: "welcome".into(),
        variables: [
            ("name".into(), serde_json::Value::String("Ada".into())),
            ("count".into(), serde_json::json!(3)),
        ]
        .into_iter()
        .collect(),
        from_override: Some("Override <hello@example-team.com>".into()),
        subject_override: Some("Welcome".into()),
        reply_to_override: vec!["reply@example-team.com".into()],
    });
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-5".into(),
            validation: EmailBatchValidation::Strict,
            items: vec![item],
        })
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "published-item"
    ));
    let body: Vec<serde_json::Value> = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(body[0]["from"], "Override <hello@example-team.com>");
    assert_eq!(body[0]["subject"], "Welcome");
    assert_eq!(body[0]["template"]["id"], "welcome");
    assert_eq!(body[0]["template"]["variables"]["count"], 3);
    assert_eq!(body[0]["reply_to"][0], "reply@example-team.com");
    assert!(body[0].get("text").is_none());
    assert!(body[0].get("html").is_none());
}

#[tokio::test]
async fn permissive_batch_rejects_invalid_items_locally_and_sends_valid_items() {
    let server = TestServer::start(vec![Scripted::json(
        200,
        r#"{"data":[{"id":"valid-item"}],"errors":[]}"#,
    )])
    .await;
    let mut invalid = batch_item(0);
    invalid.subject = None;
    invalid.text = None;
    let result = server
        .sender()
        .send_many(&EmailBatchRequest {
            idempotency_key: "batch-intent-4".into(),
            validation: EmailBatchValidation::Permissive,
            items: vec![invalid, batch_item(1)],
        })
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes[0].outcome,
        EmailBatchItemOutcome::Rejected { .. }
    ));
    assert!(matches!(
        result.outcomes[1].outcome,
        EmailBatchItemOutcome::Accepted { ref provider_message_id }
            if provider_message_id == "valid-item"
    ));
    let body: Vec<serde_json::Value> = serde_json::from_slice(&server.requests()[0].body).unwrap();
    assert_eq!(body.len(), 1);
    assert_eq!(body[0]["subject"], "Hello 1");
}

#[tokio::test]
async fn signed_download_errors_never_leak_the_url() {
    // Reserve a port, then drop the listener: the connection is refused and
    // reqwest's error would normally embed the full URL — signature included.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let server = TestServer::start(vec![Scripted::json(
        200,
        include_str!("fixtures/resend/attachment_get.json"),
    )])
    .await;
    let sender = server.sender();
    let mut access = sender
        .received_attachment(
            "7b2d3e4f-1001-4c5d-9e6f-000000000001",
            "2a0c9ce0-3112-4728-976e-000000000001",
        )
        .await
        .unwrap();
    access.download_url = format!("http://127.0.0.1:{port}/signed/gone?signature=TOPSECRET");

    let error = sender
        .download(&access.download_url, 1024)
        .await
        .unwrap_err();
    let message = format!("{error}");
    assert!(
        !message.contains("TOPSECRET")
            && !message.contains("signature")
            && !message.contains("/signed/gone"),
        "download error must not carry the signed url: {message}"
    );
}
