//! Bounded Gmail HTTPS transport. Only transient GET failures may be retried.
use reqwest::{Client, Method};
use serde_json::Value;
use std::time::Duration;

// Sessions construct separate clients, so the read budget belongs to the
// process, not an individual request/client. Sending never enters this gate.
static READ_SCHEDULE: tokio::sync::Mutex<ReadSchedule> = tokio::sync::Mutex::const_new(ReadSchedule {
    next: None,
    interval_ms: 250,
});

struct ReadSchedule {
    next: Option<tokio::time::Instant>,
    interval_ms: u64,
}

impl ReadSchedule {
    fn throttle(&mut self, now: tokio::time::Instant, attempt: u32) {
        self.interval_ms = (self.interval_ms * 2).min(4000);
        let deadline = now + Duration::from_secs(5 * (1 << attempt.min(3)));
        self.next = Some(self.next.map_or(deadline, |current| current.max(deadline)));
    }
}

async fn pace_read() {
    loop {
        let deadline = {
            let mut schedule = READ_SCHEDULE.lock().await;
            let now = tokio::time::Instant::now();
            if schedule.next.is_none_or(|deadline| deadline <= now) {
                schedule.next = Some(now + Duration::from_millis(schedule.interval_ms));
                return;
            }
            schedule.next.unwrap()
        };
        // Do not hold the lock while sleeping: another response may extend
        // the shared cooldown. Recheck it before admitting this request.
        tokio::time::sleep_until(deadline).await;
    }
}

pub(super) enum Failure {
    Http(u16),
    HttpReason(u16, &'static str),
    Unknown,
    Timeout,
    TooLarge,
    InvalidJson,
}
impl Failure {
    pub(super) fn read_code(self) -> String {
        match self {
            Self::Http(status) => format!("gmail_read_http_{status}"),
            Self::HttpReason(status, reason) => format!("gmail_read_http_{status}_{reason}"),
            Self::Timeout => "gmail_read_timeout".into(),
            Self::TooLarge => "gmail_read_response_too_large".into(),
            Self::InvalidJson => "gmail_read_response_invalid_json".into(),
            Self::Unknown => "gmail_read_transport_failed".into(),
        }
    }
    fn request_error(error: reqwest::Error) -> Self {
        if error.is_timeout() { Self::Timeout } else { Self::Unknown }
    }
    fn retryable_read(&self) -> bool {
        matches!(self, Self::Http(429 | 500 | 502 | 503 | 504)
            | Self::HttpReason(403, "rateLimitExceeded" | "userRateLimitExceeded"))
    }
    fn throttled(&self) -> bool {
        matches!(self, Self::Http(429)
            | Self::HttpReason(403, "rateLimitExceeded" | "userRateLimitExceeded"))
    }
}
pub(super) struct Transport {
    client: Client,
    origin: String,
}
impl Transport {
    pub(super) fn new() -> Result<Self, String> {
        Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map(|client| Self {
                client,
                origin: "https://gmail.googleapis.com/gmail/v1/users/me/messages".into(),
            })
            .map_err(|_| "gmail_transport_unavailable".into())
    }
    #[cfg(test)]
    pub(super) fn fixture(origin: String) -> Self {
        Self {
            client: Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(2))
                .build()
                .unwrap(),
            origin,
        }
    }
    pub(super) async fn request(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, Failure> {
        let mut attempt = 0;
        loop {
            let result = self.request_once(token, method.clone(), path, body.as_ref()).await;
            let throttled = method == Method::GET && result.as_ref().err().is_some_and(Failure::throttled);
            if throttled {
                READ_SCHEDULE.lock().await.throttle(tokio::time::Instant::now(), attempt);
            }
            if method != Method::GET || attempt >= if throttled { 3 } else { 2 }
                || !result.as_ref().err().is_some_and(Failure::retryable_read) {
                return result;
            }
            if !throttled { tokio::time::sleep(Duration::from_secs(1 << attempt)).await; }
            attempt += 1;
        }
    }
    async fn request_once(
        &self,
        token: &str,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, Failure> {
        if method == Method::GET { pace_read().await; }
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.origin))
            .bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request.send().await.map_err(Failure::request_error)?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            // A provider-specified cooldown belongs to a later sync, never
            // shorten it to our bounded in-call retry delays.
            if response.headers().contains_key(reqwest::header::RETRY_AFTER) {
                return Err(Failure::HttpReason(status, "retry_after"));
            }
            // Retain only known reason codes; never surface provider messages,
            // URLs, tokens or arbitrary JSON. Bound even error-response bodies.
            if status != 403 { return Err(Failure::Http(status)); }
            let mut bytes = Vec::new();
            while let Ok(Some(chunk)) = response.chunk().await {
                if bytes.len() + chunk.len() > 16 * 1024 { return Err(Failure::Http(status)); }
                bytes.extend_from_slice(&chunk);
            }
            return Err(forbidden_reason(&bytes));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(Failure::request_error)? {
            if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
                return Err(Failure::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Failure::InvalidJson)
    }
}

fn forbidden_reason(bytes: &[u8]) -> Failure {
    let value: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
    let reasons = value.pointer("/error/errors").and_then(Value::as_array);
    // Unknown or conflicting reasons must not become an automatic retry.
    let Some(reasons) = reasons.filter(|items| !items.is_empty()) else { return Failure::Http(403); };
    let reason = reasons[0].get("reason").and_then(Value::as_str);
    if reasons.iter().any(|item| item.get("reason").and_then(Value::as_str) != reason) {
        return Failure::Http(403);
    }
    match reason {
        Some("rateLimitExceeded") => Failure::HttpReason(403, "rateLimitExceeded"),
        Some("userRateLimitExceeded") => Failure::HttpReason(403, "userRateLimitExceeded"),
        Some("dailyLimitExceeded") => Failure::HttpReason(403, "dailyLimitExceeded"),
        Some("domainPolicy") => Failure::HttpReason(403, "domainPolicy"),
        Some("insufficientPermissions") => Failure::HttpReason(403, "insufficientPermissions"),
        Some("forbidden") => Failure::HttpReason(403, "forbidden"),
        _ => Failure::Http(403),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[tokio::test]
    async fn separate_clients_share_read_pacing() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut times = Vec::new();
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 8192];
                stream.read(&mut request).unwrap();
                times.push(std::time::Instant::now());
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
            }
            times
        });
        let clients: Vec<_> = (0..3).map(|_| Transport::fixture(format!("http://{address}"))).collect();
        let results = futures::future::join_all(clients.iter().map(|client| client.request("fixture", Method::GET, "/", None))).await;
        assert!(results.iter().all(Result::is_ok));
        let times = server.join().unwrap();
        assert!(times[2].duration_since(times[0]) >= Duration::from_millis(450));
    }

    #[tokio::test]
    async fn read_throttling_retries_but_permissions_and_sends_do_not() {
        for (method, reason, expected) in [
            (Method::GET, "userRateLimitExceeded", 4),
            (Method::GET, "insufficientPermissions", 1),
            (Method::POST, "rateLimitExceeded", 1),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                for _ in 0..expected {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut request = [0; 8192];
                    stream.read(&mut request).unwrap();
                    let body = format!(r#"{{"error":{{"message":"private","errors":[{{"reason":"{reason}"}}]}}}}"#);
                    write!(stream, "HTTP/1.1 403 Forbidden\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            let result = Transport::fixture(format!("http://{address}"))
                .request("fixture", method, "/abc", None).await;
            assert!(matches!(result, Err(Failure::HttpReason(403, r)) if r == reason));
            server.join().unwrap();
        }
    }

    #[test]
    fn only_allowlisted_unambiguous_reasons_escape() {
        for bytes in [
            br#"{"error":{"errors":[{"reason":"secret-token"}]}}"#.as_slice(),
            br#"{"error":{"errors":[{"reason":"rateLimitExceeded"},{"reason":"domainPolicy"}]}}"#.as_slice(),
            b"private html".as_slice(),
        ] {
            assert_eq!(forbidden_reason(bytes).read_code(), "gmail_read_http_403");
        }
        assert!(!Failure::HttpReason(429, "retry_after").retryable_read());
        assert!(!Failure::HttpReason(403, "dailyLimitExceeded").retryable_read());
    }

    #[test]
    fn shared_throttle_extends_cooldown_and_caps_adaptive_pacing() {
        let now = tokio::time::Instant::now();
        let mut schedule = ReadSchedule { next: None, interval_ms: 250 };
        schedule.throttle(now, 0);
        assert_eq!(schedule.interval_ms, 500);
        assert_eq!(schedule.next, Some(now + Duration::from_secs(5)));
        schedule.throttle(now, 3);
        let deadline = schedule.next;
        for _ in 0..10 { schedule.throttle(now, 0); }
        assert_eq!(schedule.interval_ms, 4000);
        assert_eq!(schedule.next, deadline);
    }
}
