//! Instagram private-API request layer: sends signed/authenticated requests to
//! `i.instagram.com/api/v1/*` with the device headers instagrapi uses, over a
//! wreq client emulating the Android app's OkHttp fingerprint. The session is
//! held behind a lock so response headers (mid, www-claim) refresh it in place.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rand::Rng;
use serde_json::Value;
use uuid::Uuid;
use wreq::header::{HeaderMap, HeaderName, HeaderValue};
use wreq::{Client, Method};
use wreq_util::Emulation;

use crate::error::IgError;
use crate::session::IgSession;

const API_DOMAIN: &str = "i.instagram.com";
const APP_ID: &str = "567067343352427";
const BLOKS_VERSION_ID: &str = "7189b949425f9bf80ea8bd880cf5a3080b292d9b1c4b38a18d112f7c4b71e7a8";
/// App navigation breadcrumb the Instagram client sends on every request
/// (verbatim from instagrapi `base_headers`).
const NAV_CHAIN: &str =
    "9MV:self_profile:2,ProfileMediaTabFragment:self_profile:3,9Xf:self_following:4";

#[derive(Clone)]
pub struct IgTransport {
    client: Client,
    session: Arc<Mutex<IgSession>>,
}

impl IgTransport {
    /// `proxy` is an optional proxy URL (e.g. `http://user:pass@host:port`);
    /// when set, all requests egress through it (家宽).
    pub fn new(session: IgSession, proxy: Option<&str>) -> Result<Self, IgError> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(30))
            .emulation(Emulation::OkHttp5);
        if let Some(url) = proxy.filter(|url| !url.is_empty()) {
            builder = builder
                .proxy(wreq::Proxy::all(url).map_err(|error| IgError::Network(error.to_string()))?);
        }
        let client = builder
            .build()
            .map_err(|error| IgError::Network(error.to_string()))?;
        Ok(Self {
            client,
            session: Arc::new(Mutex::new(session)),
        })
    }

    /// Snapshot the (possibly refreshed) session so callers can persist it.
    pub fn session_snapshot(&self) -> IgSession {
        self.session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn user_id(&self) -> String {
        self.session_snapshot().user_id().to_string()
    }

    /// GET `/api/v1/<endpoint>` with query params.
    pub async fn get(&self, endpoint: &str, params: &[(&str, &str)]) -> Result<Value, IgError> {
        self.request(Method::GET, endpoint, params, None).await
    }

    /// POST `/api/v1/<endpoint>` with an instagrapi-style `signed_body`.
    pub async fn post_signed(&self, endpoint: &str, data: &Value) -> Result<Value, IgError> {
        let body = format!("signed_body=SIGNATURE.{}", encode_form(&data.to_string()));
        self.request(Method::POST, endpoint, &[], Some(body)).await
    }

    /// POST plain form-urlencoded fields (instagrapi's `with_signature=False`
    /// path, used by direct messaging where no signed body is sent).
    pub async fn post_form(
        &self,
        endpoint: &str,
        fields: &[(&str, &str)],
    ) -> Result<Value, IgError> {
        let body = fields
            .iter()
            .map(|(key, value)| format!("{}={}", encode_form(key), encode_form(value)))
            .collect::<Vec<_>>()
            .join("&");
        self.request(Method::POST, endpoint, &[], Some(body)).await
    }

    async fn request(
        &self,
        method: Method,
        endpoint: &str,
        params: &[(&str, &str)],
        body: Option<String>,
    ) -> Result<Value, IgError> {
        let is_post = method == Method::POST;
        let headers = build_headers(&self.session_snapshot(), is_post)?;
        let url = format!(
            "https://{API_DOMAIN}/api/v1/{}",
            endpoint.trim_start_matches('/')
        );
        let mut request = self.client.request(method, url.as_str()).headers(headers);
        if !params.is_empty() {
            request = request.query(params);
        }
        if let Some(body) = body {
            request = request.body(body);
        }
        let response = request
            .send()
            .await
            .map_err(|error| IgError::Network(error.to_string()))?;
        let status = response.status();
        let mid = header_str(response.headers(), "ig-set-x-mid");
        let www_claim = header_str(response.headers(), "x-ig-set-www-claim");
        let rur = header_str(response.headers(), "ig-set-ig-u-rur");
        let set_cookies = collect_set_cookies(response.headers());
        let text = response
            .text()
            .await
            .map_err(|error| IgError::Network(error.to_string()))?;
        self.refresh(mid, www_claim, rur, set_cookies);

        let json: Value = serde_json::from_str(&text).map_err(|_| IgError::Response {
            status: status.as_u16(),
            code: None,
        })?;
        if !status.is_success() || json.get("status").and_then(Value::as_str) == Some("fail") {
            return Err(map_error(status.as_u16(), &json));
        }
        Ok(json)
    }

    fn refresh(
        &self,
        mid: Option<String>,
        www_claim: Option<String>,
        rur: Option<String>,
        cookies: Vec<(String, String)>,
    ) {
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mid) = mid.filter(|value| !value.is_empty()) {
            session.mid = Some(mid);
        }
        if let Some(www_claim) = www_claim.filter(|value| !value.is_empty()) {
            session.ig_www_claim = Some(www_claim);
        }
        // IG-U-RUR is rotated via ig-set-ig-u-rur (instagrapi captures it the same
        // way); keep the latest so later requests carry the current routing token.
        if let Some(rur) = rur.filter(|value| !value.is_empty()) {
            session.ig_u_rur = Some(rur);
        }
        // Absorb Set-Cookie like instagrapi's session jar, so csrftoken / rur /
        // mid issued by the server flow back out on later requests' Cookie header.
        for (name, value) in cookies {
            session.cookies.insert(name, value);
        }
    }
}

/// Build the `X-IG-*` device/telemetry headers plus `Authorization`, mirroring
/// instagrapi's `base_headers` (identity + routing headers Instagram checks).
fn build_headers(session: &IgSession, is_post: bool) -> Result<HeaderMap, IgError> {
    let locale = &session.locale;
    // instagrapi joins [<locale-lang>, en-US] and dedupes, so a US locale is just
    // "en-US" (not "en-US, en-US").
    let lang = locale.replace('_', "-");
    let accept_language = if lang == "en-US" {
        "en-US".to_string()
    } else {
        format!("{lang}, en-US")
    };
    let user_id = session.user_id().to_string();
    let mut rng = rand::thread_rng();

    let mut values: Vec<(&'static str, String)> = vec![
        ("x-ig-app-locale", locale.clone()),
        ("x-ig-device-locale", locale.clone()),
        ("x-ig-mapped-locale", locale.clone()),
        ("x-pigeon-session-id", format!("UFS-{}-1", Uuid::new_v4())),
        ("x-pigeon-rawclienttime", now_seconds()),
        (
            "x-ig-bandwidth-speed-kbps",
            format!("{:.3}", rng.gen_range(2_500_000..3_000_000) as f64 / 1000.0),
        ),
        (
            "x-ig-bandwidth-totalbytes-b",
            rng.gen_range(5_000_000..90_000_000).to_string(),
        ),
        (
            "x-ig-bandwidth-totaltime-ms",
            rng.gen_range(2000..9000).to_string(),
        ),
        ("x-ig-app-startup-country", session.country.to_uppercase()),
        ("x-bloks-version-id", BLOKS_VERSION_ID.to_string()),
        (
            "x-ig-www-claim",
            session
                .ig_www_claim
                .clone()
                .unwrap_or_else(|| "0".to_string()),
        ),
        ("x-bloks-is-layout-rtl", "false".to_string()),
        ("x-bloks-is-panorama-enabled", "true".to_string()),
        ("x-ig-device-id", session.uuids.uuid.clone()),
        ("x-ig-family-device-id", session.uuids.phone_id.clone()),
        ("x-ig-android-id", session.uuids.android_device_id.clone()),
        ("x-ig-timezone-offset", session.timezone_offset.to_string()),
        ("x-ig-connection-type", "WIFI".to_string()),
        ("x-ig-capabilities", "3brTv10=".to_string()),
        ("x-ig-app-id", APP_ID.to_string()),
        ("priority", "u=3".to_string()),
        ("user-agent", session.user_agent.clone()),
        ("accept-language", accept_language),
        ("ig-intended-user-id", user_id.clone()),
        (
            "x-ig-salt-ids",
            rng.gen_range(1_061_162_222..1_061_262_222i64).to_string(),
        ),
        ("ig-u-ds-user-id", user_id.clone()),
        // App/transport routing headers the real client always sends (instagrapi
        // base_headers). The real per-account routing/shard values (rur/shbid/shbts/
        // mid) ride in the Cookie jar (imported from the browser + refreshed from
        // Set-Cookie); X-MID comes from the `mid` cookie and IG-U-RUR is added below
        // from the server-captured value. We do NOT fabricate the *signed*
        // IG-U-SHBID / IG-U-SHBTS / IG-U-IG-DIRECT-REGION-HINT headers: their
        // signature is server-issued and unreproducible (instagrapi ships stale
        // hardcoded ones — a wrong signature is worse than relying on the cookies).
        ("x-ig-nav-chain", NAV_CHAIN.to_string()),
        ("x-fb-http-engine", "Tigon/MNS/TCP".to_string()),
        ("x-tigon-is-retry", "False".to_string()),
        ("x-fb-client-ip", "True".to_string()),
        ("x-fb-server-cluster", "True".to_string()),
        ("authorization", session.authorization()),
    ];
    if let Some(mid) = &session.mid {
        values.push(("x-mid", mid.clone()));
    }
    if let Some(rur) = session
        .ig_u_rur
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        values.push(("ig-u-rur", rur.to_string()));
    }
    // Send the cookie jar (sessionid / ds_user_id / csrftoken…) like instagrapi's
    // requests session does — the real app carries both Authorization and cookies.
    let cookie = session
        .cookies
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ");
    if !cookie.is_empty() {
        values.push(("cookie", cookie));
    }
    if is_post {
        values.push((
            "content-type",
            "application/x-www-form-urlencoded; charset=UTF-8".to_string(),
        ));
    }

    let mut headers = HeaderMap::new();
    for (name, value) in values {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(&value).map_err(|_| IgError::InvalidHeader)?,
        );
    }
    Ok(headers)
}

fn now_seconds() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    format!("{secs:.3}")
}

fn header_str(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.as_str().eq_ignore_ascii_case(name))
        .and_then(|(_, value)| value.to_str().ok())
        .map(str::to_string)
}

/// Parse `Set-Cookie` response headers into `(name, value)` pairs (the value
/// before the first `;`), skipping deletions (empty / `""` values).
fn collect_set_cookies(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|raw| raw.split(';').next()?.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .filter(|(name, value)| !name.is_empty() && !value.is_empty() && value != "\"\"")
        .collect()
}

fn encode_form(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn map_error(status: u16, json: &Value) -> IgError {
    // Only the protocol's machine field is evidence. Validate its bounded token
    // shape so a malformed response cannot carry a body, URL or credential here.
    let code = json
        .get("error_type")
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 80
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .map(str::to_owned);
    IgError::Response { status, code }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_api_diagnostics_read_machine_fields_without_parsing_human_messages() {
        for message in [
            "login",
            "429",
            "timeout",
            "sent successfully",
            "Bearer SENTINEL https://secret.invalid/?token=SENTINEL",
        ] {
            let error = map_error(
                418,
                &serde_json::json!({"error_type":"NEW_PROVIDER_CODE", "message":message, "challenge":{"url":"SENTINEL"}}),
            );
            assert!(
                matches!(&error, IgError::Response { status: 418, code: Some(code) } if code == "NEW_PROVIDER_CODE")
            );
            assert!(!format!("{error:?}").contains("SENTINEL"));
        }
        for payload in [
            serde_json::json!({"message":"login timeout 429"}),
            serde_json::json!({"error_type":"https://secret.invalid/?token=SENTINEL"}),
        ] {
            assert!(matches!(
                map_error(429, &payload),
                IgError::Response {
                    status: 429,
                    code: None
                }
            ));
        }
    }
}
