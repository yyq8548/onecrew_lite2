use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn strict_versions_and_origins_are_not_relaxed_by_shared_use() {
    assert_eq!(parse_cli_version("0.5.22").unwrap(), [0, 5, 22]);
    for version in ["v0.5.22", "0.05.22", "0.5", "0.5.22?secret=x"] {
        assert!(parse_cli_version(version).is_err());
    }
    assert!(CliReleaseClient::new("http://access.onecrewai.io").is_err());
    assert!(CliReleaseClient::new("https://user:secret@access.onecrewai.io").is_err());
}

async fn http_fixture(
    replies: Vec<(u16, String)>,
    delay: Duration,
) -> (
    CliReleaseClient,
    Arc<AtomicUsize>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    let response_origin = origin.clone();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let task = tokio::spawn(async move {
        for (status, body) in replies {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0 && request.len() < 8192);
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("get /api/cli/update "));
            assert!(request.contains("authorization: bearer synthetic-test-token"));
            assert!(request.contains("cache-control: no-store"));
            observed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(delay).await;
            let body = body.replace("ORIGIN", response_origin.as_str().trim_end_matches('/'));
            let response = format!(
                "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\nLocation: /must-not-follow\r\n\r\n{body}",
                body.len()
            );
            // The client under test may already have closed; the test asserts
            // the outcome it observed.
            if let Err(error) = stream.write_all(response.as_bytes()).await {
                eprintln!("fixture response write failed: {error}");
            }
        }
    });
    let client = Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    (CliReleaseClient { client, origin }, requests, task)
}

fn manifest(version: &str) -> String {
    serde_json::json!({
        "schema_version": 1,
        "product": "onecrew_lite-cli",
        "version": version,
        "assets": [{
            "target": "aarch64-apple-darwin",
            "size": 64,
            "sha256": "a".repeat(64),
            "url": format!("ORIGIN/api/cli/update/{version}/files/aarch64-apple-darwin")
        }]
    })
    .to_string()
}

#[tokio::test]
async fn shared_client_is_fresh_and_fetches_metadata_only() {
    let (client, requests, server) = http_fixture(
        vec![(200, manifest("0.5.23")), (200, manifest("0.5.24"))],
        Duration::ZERO,
    )
    .await;
    for expected in ["0.5.23", "0.5.24"] {
        let release = client
            .latest(
                "synthetic-test-token",
                "aarch64-apple-darwin",
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(release.version, expected);
    }
    server.await.unwrap();
    assert_eq!(
        requests.load(Ordering::SeqCst),
        2,
        "explicit updates do not inherit a status cache"
    );
}

#[tokio::test]
async fn errors_and_redirects_do_not_expose_response_details_or_follow_assets() {
    for (status, expected) in [
        (401, CliReleaseError::LoginRequired),
        (403, CliReleaseError::AccessDenied),
        (302, CliReleaseError::HttpRejected),
    ] {
        let (client, requests, server) = http_fixture(
            vec![(status, "private-auth-response-do-not-expose".into())],
            Duration::ZERO,
        )
        .await;
        let result = client
            .latest(
                "synthetic-test-token",
                "aarch64-apple-darwin",
                Duration::from_secs(1),
            )
            .await;
        assert!(matches!(result, Err(error) if error == expected));
        assert!(!expected.code().contains("private-auth"));
        server.await.unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn short_check_timeout_and_invalid_manifest_are_typed_failures() {
    let (client, _, server) =
        http_fixture(vec![(200, manifest("0.5.23"))], Duration::from_secs(1)).await;
    assert!(matches!(
        client
            .latest(
                "synthetic-test-token",
                "aarch64-apple-darwin",
                Duration::from_millis(20)
            )
            .await,
        Err(CliReleaseError::Timeout)
    ));
    server.abort();
    let (client, _, server) = http_fixture(vec![(200, "{}".into())], Duration::ZERO).await;
    assert!(matches!(
        client
            .latest(
                "synthetic-test-token",
                "aarch64-apple-darwin",
                Duration::from_secs(1)
            )
            .await,
        Err(CliReleaseError::ManifestInvalid)
    ));
    server.await.unwrap();
}
