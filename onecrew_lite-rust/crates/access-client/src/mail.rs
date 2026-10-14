//! Existing Access mail v1 protocol. OAuth and refresh remain remote-owned.
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;

const PREFIX: &str = "/api/desktop/integrations/mail/v1";
const MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailError {
    Configuration,
    Network,
    LoginRequired,
    AccessDenied,
    Rejected,
    InvalidResponse,
}
impl MailError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Configuration => "mail_access_configuration_invalid",
            Self::Network => "mail_access_network_failed",
            Self::LoginRequired => "mail_access_login_required",
            Self::AccessDenied => "mail_access_denied",
            Self::Rejected => "mail_access_request_rejected",
            Self::InvalidResponse => "mail_access_response_invalid",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
pub struct MailCapabilities {
    pub can_send: bool,
    pub can_read_thread: bool,
    pub can_read_attachment: bool,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct MailConnection {
    pub id: String,
    pub provider: String,
    pub email: String,
    pub status: String,
    pub scopes: Vec<String>,
    pub capabilities: MailCapabilities,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Deserialize)]
struct Connections {
    connections: Vec<MailConnection>,
}
// Deliberately neither Debug nor Serialize: browser/MCP must never receive tokens.
#[derive(Deserialize)]
pub struct MailAccessToken {
    access_token: String,
    pub expires_at: String,
    pub scopes: Vec<String>,
    pub email: String,
}
impl MailAccessToken {
    pub fn expose_secret(&self) -> &str {
        &self.access_token
    }
}
#[derive(Deserialize)]
struct Envelope<T> {
    protocol: String,
    version: u32,
    data: T,
}
#[derive(Deserialize)]
struct ConnectUrl {
    url: String,
}

pub struct MailClient {
    client: Client,
    origin: Url,
}
impl MailClient {
    pub fn new(origin: &str) -> Result<Self, MailError> {
        let origin = Url::parse(origin).map_err(|_| MailError::Configuration)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
        {
            return Err(MailError::Configuration);
        }
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| MailError::Configuration)?;
        Ok(Self { client, origin })
    }

    pub async fn connect_google(&self, bearer: &str) -> Result<String, MailError> {
        let result: ConnectUrl = self
            .request(
                bearer,
                Method::POST,
                "/connect-url",
                Some(serde_json::json!({"provider":"google"})),
            )
            .await?;
        self.validate_connect_url(&result.url)?;
        Ok(result.url)
    }
    pub async fn connections(&self, bearer: &str) -> Result<Vec<MailConnection>, MailError> {
        let result: Connections = self
            .request(bearer, Method::GET, "/connections", None)
            .await?;
        if result.connections.len() > 100 || result.connections.iter().any(|c| !valid_id(&c.id)) {
            return Err(MailError::InvalidResponse);
        }
        Ok(result.connections)
    }
    pub async fn access_token(&self, bearer: &str, id: &str) -> Result<MailAccessToken, MailError> {
        if !valid_id(id) {
            return Err(MailError::Configuration);
        }
        let token: MailAccessToken = self
            .request(
                bearer,
                Method::POST,
                &format!("/connections/{id}/access-token"),
                None,
            )
            .await?;
        if token.access_token.is_empty() || token.access_token.len() > 16384 {
            return Err(MailError::InvalidResponse);
        }
        Ok(token)
    }
    fn validate_connect_url(&self, value: &str) -> Result<(), MailError> {
        let url = Url::parse(value).map_err(|_| MailError::InvalidResponse)?;
        let pairs: Vec<_> = url.query_pairs().collect();
        if url.origin() != self.origin.origin()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.fragment().is_some()
            || url.path() != "/integrations/mail/connect"
            || pairs.len() != 2
            || pairs
                .iter()
                .filter(|(k, v)| k == "provider" && v == "google")
                .count()
                != 1
            || pairs
                .iter()
                .filter(|(k, v)| k == "ticket" && !v.is_empty() && v.len() <= 2048)
                .count()
                != 1
        {
            return Err(MailError::InvalidResponse);
        }
        Ok(())
    }
    async fn request<T: DeserializeOwned>(
        &self,
        bearer: &str,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T, MailError> {
        let url = self
            .origin
            .join(&format!("{PREFIX}{path}"))
            .map_err(|_| MailError::Configuration)?;
        let mut request = self
            .client
            .request(method, url)
            .bearer_auth(bearer)
            .header("Accept", "application/json")
            .header("Cache-Control", "no-store");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request.send().await.map_err(|_| MailError::Network)?;
        match response.status() {
            StatusCode::OK => (),
            StatusCode::UNAUTHORIZED => return Err(MailError::LoginRequired),
            StatusCode::FORBIDDEN => return Err(MailError::AccessDenied),
            _ => return Err(MailError::Rejected),
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| MailError::Network)? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err(MailError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        decode(&bytes)
    }
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, MailError> {
    let envelope: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|_| MailError::InvalidResponse)?;
    if envelope.protocol != "onecrew.mail" || envelope.version != 1 {
        return Err(MailError::InvalidResponse);
    }
    Ok(envelope.data)
}
fn valid_id(id: &str) -> bool {
    let Some(id) = id.strip_prefix("conn_") else {
        return false;
    };
    id.len() == 36
        && id.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn machine_request_uses_device_auth_and_expected_contract() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let response_origin = origin.clone();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request
                .windows(21)
                .any(|v| v == b"{\"provider\":\"google\"}")
            {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0 && request.len() < 8192);
                request.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(request).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("post /api/desktop/integrations/mail/v1/connect-url "));
            assert!(request.contains("authorization: bearer fixture-device"));
            assert!(request.contains("cache-control: no-store"));
            let body = serde_json::json!({"protocol":"onecrew.mail","version":1,"requestId":"fixture",
                "data":{"url":format!("{}integrations/mail/connect?provider=google&ticket=fixture", response_origin)}}).to_string();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let client = MailClient {
            client: Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
            origin,
        };
        assert!(
            client
                .connect_google("fixture-device")
                .await
                .unwrap()
                .contains("ticket=fixture")
        );
        server.await.unwrap();
    }
    #[test]
    fn connect_urls_require_same_origin_provider_and_single_ticket() {
        let client = MailClient::new("https://access.onecrewai.io").unwrap();
        assert!(client.validate_connect_url("https://access.onecrewai.io/integrations/mail/connect?provider=google&ticket=test").is_ok());
        for url in [
            "https://evil.test/integrations/mail/connect?provider=google&ticket=test",
            "https://access.onecrewai.io/integrations/mail/connect?provider=google",
            "https://access.onecrewai.io/integrations/mail/connect?provider=google&ticket=a&ticket=b",
            "https://access.onecrewai.io/integrations/mail/connect?provider=outlook&ticket=test",
        ] {
            assert_eq!(
                client.validate_connect_url(url),
                Err(MailError::InvalidResponse)
            );
        }
    }
    #[test]
    fn protocol_mismatch_and_error_envelopes_fail_closed() {
        for body in [
            r#"{"protocol":"other","version":1,"data":{}}"#,
            r#"{"protocol":"onecrew.mail","version":2,"data":{}}"#,
            r#"{"protocol":"onecrew.mail","version":1,"error":{"code":"device_unauthorized"}}"#,
        ] {
            assert!(decode::<serde_json::Value>(body.as_bytes()).is_err());
        }
    }
    #[test]
    fn connection_ids_cannot_inject_paths() {
        assert!(valid_id("conn_12345678-1234-1234-1234-123456789abc"));
        assert!(!valid_id("conn_../another/access-token"));
    }

    #[tokio::test]
    async fn failures_do_not_follow_redirects_or_expose_remote_bodies() {
        for (status, expected) in [
            (401, MailError::LoginRequired),
            (403, MailError::AccessDenied),
            (302, MailError::Rejected),
            (500, MailError::Rejected),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let origin = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let _received = stream.read(&mut buffer).await.unwrap();
                let body = "remote-secret-must-not-escape";
                stream.write_all(format!("HTTP/1.1 {status} Fixture\r\nLocation: /secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let client = MailClient {
                client: Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(Duration::from_secs(3))
                    .build()
                    .unwrap(),
                origin,
            };
            let result = client.connections("fixture-device").await;
            assert!(matches!(result, Err(error) if error == expected));
            server.await.unwrap();
        }
    }
}
