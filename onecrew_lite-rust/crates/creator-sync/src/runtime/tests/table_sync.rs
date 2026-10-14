use super::*;
use crate::TableSyncRequest;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn server(response: &str) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let response = response.to_owned();
    let job = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut block = [0; 4096];
        loop {
            let n = socket.read(&mut block).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&block[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let length = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        socket.write_all(response.as_bytes()).await.unwrap();
        bytes
    });
    (origin, job)
}
fn at(origin: &str) -> CreatorSyncRuntime {
    let (m, b) = sample_credential();
    let mut rt = runtime(
        Arc::new(AlwaysUnavailable),
        Arc::new(FakeStore::with_credential(m, b)),
    );
    Arc::get_mut(&mut rt.inner).unwrap().endpoints =
        CreatorSyncEndpoints::with_sync_origin_for_test(origin, true).unwrap();
    rt
}
#[tokio::test]
async fn table_sync_part_uses_exact_bytes_headers_and_envelope() {
    let body = r#"{"part":{"part_number":1,"bytes":3,"etag":"ok","md5":"900150983cd24fb0d6963f7d28e17f72"}}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (origin, job) = server(&response).await;
    let rt = at(&origin);
    let version = Uuid::new_v4();
    let attempt = Uuid::new_v4();
    let reply = rt
        .table_sync_request(TableSyncRequest::Part {
            version_id: version,
            attempt_id: attempt,
            number: 1,
            md5_base64: "kAFQmDzST7DWlj99KOF/cg==".into(),
            bytes: b"abc".to_vec(),
        })
        .await
        .unwrap();
    assert_eq!(reply.body["part"]["bytes"], 3);
    let bytes = job.await.unwrap();
    let request = String::from_utf8(bytes).unwrap();
    let lower = request.to_lowercase();
    assert!(request.starts_with(&format!(
        "PUT /dynamic-table-sync/uploads/{version}/parts/1 "
    )));
    assert!(lower.contains("content-md5: kafqmdzst7dwlj99kof/cg==\r\n"));
    assert!(lower.contains(&format!("x-onecrew-upload-attempt: {attempt}\r\n")));
    assert!(lower.contains("authorization: bearer "));
    assert!(request.ends_with("\r\n\r\nabc"));
}
#[tokio::test]
async fn table_sync_consent_uses_etag_and_redirect_is_never_followed() {
    let body = r#"{"consent":null}"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let (origin, job) = server(&response).await;
    let reply = at(&origin)
        .table_sync_request(TableSyncRequest::SetConsent {
            input: json!({"action":"revoke","operation_id":Uuid::new_v4()}),
            etag: Some("\"old\"".into()),
        })
        .await
        .unwrap();
    assert_eq!(reply.etag.as_deref(), Some("\"v1\""));
    assert!(
        String::from_utf8(job.await.unwrap())
            .unwrap()
            .to_lowercase()
            .contains("if-match: \"old\"\r\n")
    );
    let(origin,job)=server("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/secret\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await;
    let reply = at(&origin)
        .table_sync_request(TableSyncRequest::Capabilities)
        .await
        .unwrap();
    assert_eq!(reply.status, 302);
    job.await.unwrap();
}
#[tokio::test]
async fn table_sync_rejects_missing_login_before_network() {
    let rt = runtime(Arc::new(AlwaysUnavailable), Arc::new(FakeStore::default()));
    assert_eq!(
        rt.table_sync_request(TableSyncRequest::Capabilities)
            .await
            .err()
            .as_deref(),
        Some("table_sync_login_required")
    );
}

#[tokio::test]
async fn table_sync_account_switch_cannot_send_old_user_snapshot() {
    let (m, b) = sample_credential();
    let rt = runtime(
        Arc::new(AlwaysUnavailable),
        Arc::new(FakeStore::with_credential(m, b)),
    );
    assert_eq!(
        rt.table_sync_request_for_user(
            TableSyncRequest::Begin { input: json!({}) },
            Some("another-user")
        )
        .await
        .err()
        .as_deref(),
        Some("table_sync_account_changed")
    );
}
