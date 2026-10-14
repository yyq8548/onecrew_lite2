//! Bounded HTTP retrieval for allowlisted public link-hub hosts.
//!
//! Every redirect is admitted before a request; no account or application
//! workflow participates in public page retrieval.

use std::{fmt, time::Duration};

use reqwest::{Client, StatusCode, header::LOCATION, redirect::Policy};
use url::Url;

const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;
const MAX_REDIRECTS: usize = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = "Mozilla/5.0 (compatible; ReacherContactDiscovery/1.0)";

/// Failure of one extraction. No variant carries the requested URL: the
/// input is not kept, transport errors are discarded, and an
/// unsupported host is named by host only. `Debug` and
/// [`LinkHubExtractError::safe_message`] never format transport error details.
pub enum LinkHubExtractError {
    /// Not an absolute `http(s)` URL, or one with userinfo.
    InvalidUrl,
    /// The host is outside the link-hub allowlist.
    UnsupportedHost(String),
    /// The request failed; transport details stay private.
    Request,
    Timeout,
    InvalidRedirect,
    TooManyRedirects,
    BodyTooLarge,
    InvalidUtf8,
}

impl LinkHubExtractError {
    /// Secret-free text; carries no URL beyond an unsupported host name.
    pub fn safe_message(&self) -> String {
        self.to_string()
    }

    fn request(_error: reqwest::Error) -> Self {
        Self::Request
    }
}

impl fmt::Debug for LinkHubExtractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => formatter.write_str("InvalidUrl"),
            Self::UnsupportedHost(host) => write!(formatter, "UnsupportedHost({host:?})"),
            // Do not format the transport error or its source chain: even
            // URL-free errors can expose peer addresses.
            Self::Request => write!(formatter, "Request({:?})", self.safe_message()),
            Self::Timeout => formatter.write_str("Timeout"),
            Self::InvalidRedirect => formatter.write_str("InvalidRedirect"),
            Self::TooManyRedirects => formatter.write_str("TooManyRedirects"),
            Self::BodyTooLarge => formatter.write_str("BodyTooLarge"),
            Self::InvalidUtf8 => formatter.write_str("InvalidUtf8"),
        }
    }
}

impl fmt::Display for LinkHubExtractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl => formatter.write_str("invalid link hub URL"),
            Self::UnsupportedHost(host) => write!(formatter, "unsupported link hub host: {host}"),
            Self::Request => formatter.write_str("link hub request failed"),
            Self::Timeout => write!(
                formatter,
                "link hub request timed out after {} seconds",
                REQUEST_TIMEOUT.as_secs()
            ),
            Self::InvalidRedirect => {
                formatter.write_str("link hub redirect did not contain a valid location")
            }
            Self::TooManyRedirects => write!(
                formatter,
                "link hub response exceeded {MAX_REDIRECTS} redirects"
            ),
            Self::BodyTooLarge => write!(
                formatter,
                "link hub response exceeded {MAX_BODY_BYTES} bytes"
            ),
            Self::InvalidUtf8 => formatter.write_str("link hub response was not valid UTF-8"),
        }
    }
}

impl std::error::Error for LinkHubExtractError {}

/// The requested source and the final response location have distinct roles:
/// provenance retains the first URL; relative links use the final URL.
pub(super) struct FetchedPage {
    pub(super) initial_url: Url,
    pub(super) final_url: Url,
    pub(super) body: Vec<u8>,
}

pub(super) async fn fetch(url: &str) -> Result<FetchedPage, LinkHubExtractError> {
    let source = validate_link_hub_url(url)?;
    let client = Client::builder()
        .redirect(Policy::none())
        .user_agent(USER_AGENT)
        .build()
        .map_err(LinkHubExtractError::request)?;
    fetch_with_client(&client, source).await
}

async fn fetch_with_client(
    client: &Client,
    source: Url,
) -> Result<FetchedPage, LinkHubExtractError> {
    tokio::time::timeout(REQUEST_TIMEOUT, fetch_body(client, source))
        .await
        .map_err(|_| LinkHubExtractError::Timeout)?
}

async fn fetch_body(client: &Client, initial_url: Url) -> Result<FetchedPage, LinkHubExtractError> {
    let mut current = initial_url.clone();
    let mut redirects = 0usize;
    loop {
        validate_link_hub_url(current.as_str())?;
        let mut response = client
            .get(current.clone())
            .send()
            .await
            .map_err(LinkHubExtractError::request)?;
        if is_redirect(response.status()) {
            if redirects == MAX_REDIRECTS {
                return Err(LinkHubExtractError::TooManyRedirects);
            }
            let location = response
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or(LinkHubExtractError::InvalidRedirect)?;
            current = next_hop(&current, location)?;
            redirects += 1;
            continue;
        }

        response = response
            .error_for_status()
            .map_err(LinkHubExtractError::request)?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_BODY_BYTES as u64)
        {
            return Err(LinkHubExtractError::BodyTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(LinkHubExtractError::request)?
        {
            if body.len().saturating_add(chunk.len()) > MAX_BODY_BYTES {
                return Err(LinkHubExtractError::BodyTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        return Ok(FetchedPage {
            initial_url,
            final_url: current,
            body,
        });
    }
}

/// The next hop of a manual redirect, validated like the first request: a
/// hop to a host outside the allowlist, or one carrying userinfo, is refused
/// before anything is requested.
fn next_hop(current: &Url, location: &str) -> Result<Url, LinkHubExtractError> {
    let next = current
        .join(location)
        .map_err(|_| LinkHubExtractError::InvalidRedirect)?;
    validate_link_hub_url(next.as_str())
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

/// Strip prose punctuation a human pastes along with the URL - Markdown
/// brackets and CJK fullwidth marks included: `https://linktr.ee/a,` and
/// `.../b],` must reach the real page instead of a percent-encoded 404.
fn trim_pasted_punctuation(value: &str) -> &str {
    value.trim().trim_end_matches(|character: char| {
        matches!(
            character,
            '.' | ','
                | ';'
                | ':'
                | '!'
                | '?'
                | ')'
                | ']'
                | '}'
                | '\''
                | '"'
                | '>'
                | '\u{ff0c}'
                | '\u{3002}'
                | '\u{ff1b}'
                | '\u{ff1a}'
                | '\u{ff01}'
                | '\u{ff1f}'
                | '\u{3001}'
                | '\u{ff09}'
                | '\u{3011}'
                | '\u{300b}'
                | '\u{3009}'
                | '\u{300d}'
                | '\u{300f}'
                | '\u{2026}'
                | '\u{b7}'
                | '\u{201c}'
                | '\u{201d}'
                | '\u{2018}'
                | '\u{2019}'
        )
    })
}

fn validate_link_hub_url(value: &str) -> Result<Url, LinkHubExtractError> {
    let value = trim_pasted_punctuation(value);
    let url = Url::parse(value).map_err(|_| LinkHubExtractError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(LinkHubExtractError::InvalidUrl);
    }
    let host = url
        .host_str()
        .ok_or(LinkHubExtractError::InvalidUrl)?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    if !is_link_hub(&host) {
        return Err(LinkHubExtractError::UnsupportedHost(host));
    }
    Ok(url)
}

fn is_link_hub(host: &str) -> bool {
    matches!(
        host,
        "linktr.ee"
            | "beacons.ai"
            | "bio.link"
            | "solo.to"
            | "lnk.bio"
            | "taplink.cc"
            | "hoo.be"
            | "stan.store"
            | "carrd.co"
            | "allmylinks.com"
            | "campsite.bio"
    )
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };

    use super::*;
    use crate::capabilities::public_web::extract;

    // Route real allowlisted hostnames to a bounded local HTTP fixture. The
    // production admission checks stay intact, including on redirect hops.
    fn fixture(responses: Vec<String>) -> (Client, Url, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let client = fixture_client(address);
        let source = Url::parse(&format!("http://linktr.ee:{}/original", address.port())).unwrap();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "fixture request timed out");
                            thread::sleep(Duration::from_millis(1));
                        }
                        Err(error) => panic!("fixture accept failed: {error}"),
                    }
                };
                // Accepted sockets may inherit the listener's nonblocking
                // mode on macOS; use bounded blocking reads for this fixture.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut chunk = [0u8; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let size = stream.read(&mut chunk).unwrap();
                    assert!(size > 0, "request ended before headers");
                    request.extend_from_slice(&chunk[..size]);
                    assert!(request.len() < 16 * 1024, "fixture headers exceeded bound");
                }
                requests.push(String::from_utf8(request).unwrap());
                let response = response.replace("__FIXTURE_PORT__", &address.port().to_string());
                // Body-limit and timeout cases intentionally close the client
                // before this fixture finishes writing its response.
                drop(stream.write_all(response.as_bytes()));
            }
            requests
        });
        (client, source, worker)
    }

    fn fixture_client(address: std::net::SocketAddr) -> Client {
        Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .resolve("linktr.ee", address)
            .resolve("beacons.ai", address)
            .build()
            .unwrap()
    }

    fn response(status: &str, headers: &str, body: &str) -> String {
        format!("HTTP/1.1 {status}\r\nConnection: close\r\n{headers}\r\n{body}")
    }

    #[tokio::test]
    async fn redirects_preserve_both_locations_and_feed_the_final_base_to_parsing() {
        // A network-path redirect reuses the fixture port while switching
        // hosts, proving that the final response location reaches the parser.
        let body = r#"<a href="/navigation">not-owner@example.com</a>
                      <a href="https://linktr.ee/original">Original</a>"#;
        let (client, source, worker) = fixture(vec![
            response(
                "302 Found",
                "Location: //beacons.ai:__FIXTURE_PORT__/creator/profile\r\nContent-Length: 0\r\n",
                "",
            ),
            response(
                "200 OK",
                &format!("Content-Length: {}\r\n", body.len()),
                body,
            ),
        ]);
        let page = fetch_with_client(&client, source.clone()).await.unwrap();
        let requests = worker.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[0].starts_with("GET /original HTTP/1.1"));
        assert!(requests[1].starts_with("GET /creator/profile HTTP/1.1"));
        assert_eq!(page.initial_url, source);
        assert_eq!(page.final_url.host_str(), Some("beacons.ai"));
        assert_eq!(page.final_url.path(), "/creator/profile");
        assert_eq!(page.body, body.as_bytes());
        let extraction = super::super::parse::parse_link_hub(
            std::str::from_utf8(&page.body).unwrap(),
            &page.initial_url,
            &page.final_url,
        );
        assert_eq!(extraction.source_url, source.as_str());
        assert_eq!(extraction.links.len(), 1);
        assert_eq!(extraction.links[0].url, "https://linktr.ee/original");
        assert!(extraction.emails.is_empty());
    }

    #[tokio::test]
    async fn redirected_host_is_refused_before_a_second_request() {
        let (client, source, worker) = fixture(vec![response(
            "302 Found",
            "Location: http://127.0.0.1/private\r\nContent-Length: 0\r\n",
            "",
        )]);
        assert!(matches!(
            fetch_with_client(&client, source).await,
            Err(LinkHubExtractError::UnsupportedHost(host)) if host == "127.0.0.1"
        ));
        assert_eq!(worker.join().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn redirect_count_caps_the_number_of_requests() {
        let (client, source, worker) = fixture(vec![
            response(
                "302 Found",
                "Location: /again\r\nContent-Length: 0\r\n",
                ""
            );
            MAX_REDIRECTS + 1
        ]);
        assert!(matches!(
            fetch_with_client(&client, source).await,
            Err(LinkHubExtractError::TooManyRedirects)
        ));
        assert_eq!(worker.join().unwrap().len(), MAX_REDIRECTS + 1);
    }

    #[tokio::test]
    async fn response_size_is_bounded_for_declared_and_streamed_bodies() {
        let oversized_body = "x".repeat(MAX_BODY_BYTES + 1);
        for raw in [
            response(
                "200 OK",
                &format!("Content-Length: {}\r\n", MAX_BODY_BYTES + 1),
                "",
            ),
            response(
                "200 OK",
                "Transfer-Encoding: chunked\r\n",
                &format!(
                    "{:x}\r\n{oversized_body}\r\n0\r\n\r\n",
                    oversized_body.len()
                ),
            ),
        ] {
            let (client, source, worker) = fixture(vec![raw]);
            let error = fetch_with_client(&client, source)
                .await
                .err()
                .expect("oversized body must fail");
            assert!(
                matches!(error, LinkHubExtractError::BodyTooLarge),
                "unexpected failure: {error:?}"
            );
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn the_whole_fetch_times_out_while_waiting_for_a_response() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = fixture_client(address);
        let source = Url::parse(&format!("http://linktr.ee:{}/slow", address.port())).unwrap();
        let fetch = tokio::spawn(async move { fetch_with_client(&client, source).await });
        let (_connection, _) = listener.accept().await.unwrap();
        tokio::time::pause();
        tokio::time::advance(REQUEST_TIMEOUT + Duration::from_secs(1)).await;
        assert!(matches!(
            fetch.await.unwrap(),
            Err(LinkHubExtractError::Timeout)
        ));
    }

    #[test]
    fn pasted_prose_punctuation_is_stripped_before_validation() {
        for raw in [
            "https://linktr.ee/visionariobtc\u{ff0c}",
            "https://linktr.ee/bitcoinmx\u{3011}\u{ff0c}",
            "https://linktr.ee/bitcoinmx],",
            "  https://linktr.ee/bitcoinmx  ",
        ] {
            let url = validate_link_hub_url(raw).expect("pasted punctuation strips");
            assert!(
                !url.path().contains('%'),
                "{raw} must not percent-encode punctuation into the path"
            );
        }
    }

    #[test]
    fn rejects_non_link_hub_urls_before_requesting_them() {
        assert!(matches!(
            validate_link_hub_url("https://example.com/creator"),
            Err(LinkHubExtractError::UnsupportedHost(host)) if host == "example.com"
        ));
        assert!(matches!(
            validate_link_hub_url("file:///tmp/profile.html"),
            Err(LinkHubExtractError::InvalidUrl)
        ));
        assert!(matches!(
            validate_link_hub_url("https://user@linktr.ee/creator"),
            Err(LinkHubExtractError::InvalidUrl)
        ));
    }

    #[tokio::test]
    async fn extract_refuses_a_non_allowlisted_host_and_unsafe_urls_before_any_request() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();

        assert!(matches!(
            extract(&format!("http://127.0.0.1:{port}/creator")).await,
            Err(LinkHubExtractError::UnsupportedHost(host)) if host == "127.0.0.1"
        ));
        assert!(matches!(
            extract("file:///tmp/profile.html").await,
            Err(LinkHubExtractError::InvalidUrl)
        ));
        assert!(matches!(
            extract("https://user:secret@linktr.ee/creator").await,
            Err(LinkHubExtractError::InvalidUrl)
        ));
        assert_eq!(
            listener.accept().map(|_| ()).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "nothing may be requested before the URL is admitted"
        );
    }

    #[test]
    fn a_redirect_hop_is_validated_like_the_first_request() {
        let current = Url::parse("https://linktr.ee/creator").unwrap();
        assert!(matches!(
            next_hop(&current, "https://evil.example/creator"),
            Err(LinkHubExtractError::UnsupportedHost(host)) if host == "evil.example"
        ));
        assert!(matches!(
            next_hop(&current, "https://user:pw@linktr.ee/creator"),
            Err(LinkHubExtractError::InvalidUrl)
        ));
        assert!(matches!(
            next_hop(&current, "ftp://linktr.ee/creator"),
            Err(LinkHubExtractError::InvalidUrl)
        ));
        assert_eq!(
            next_hop(&current, "/other").unwrap().as_str(),
            "https://linktr.ee/other"
        );
    }

    #[tokio::test]
    async fn invalid_url_and_request_failures_carry_no_url_in_their_message() {
        let invalid = extract("https://user:secret@linktr.ee/creator?token=abc")
            .await
            .unwrap_err();
        let invalid_text = format!("{invalid:?} {}", invalid.safe_message());
        for fragment in ["secret", "token", "abc", "linktr.ee", "creator"] {
            assert!(!invalid_text.contains(fragment), "{invalid_text}");
        }

        // A real request error, from a port nothing listens on.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let error = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap()
            .get(format!("http://127.0.0.1:{port}/secret-path?token=abc"))
            .send()
            .await
            .unwrap_err();
        assert!(error.url().is_some(), "the raw reqwest error names its URL");
        let request = LinkHubExtractError::request(error);
        assert!(std::error::Error::source(&request).is_none());
        let request_text = format!("{request:?} {}", request.safe_message());
        for fragment in ["127.0.0.1", "secret-path", "token", "abc"] {
            assert!(!request_text.contains(fragment), "{request_text}");
        }
        assert!(
            request
                .safe_message()
                .starts_with("link hub request failed")
        );
    }
}
