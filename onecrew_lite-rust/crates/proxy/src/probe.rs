use std::time::Duration;

use serde_json::Value;

use crate::ProxyConfig;

/// IP-echo endpoints tried in order. All return the *source* IP as the target
/// sees it, so through a proxy this is the proxy's egress IP. Ordered by
/// reachability/robustness; ip-api is plain HTTP (simplest through an HTTP
/// proxy) as a fallback. First one that answers wins.
const ENDPOINTS: &[&str] = &[
    "https://ipinfo.io/json",
    "http://ip-api.com/json",
    "https://api.ipify.org?format=json",
];

const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36";

#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    #[error("代理配置无效:{0}")]
    Config(String),
    #[error("经代理访问失败(代理不通?):{0}")]
    Unreachable(String),
}

/// Egress identity as seen from the outside, for confirming the proxy routes.
#[derive(Debug, Clone)]
pub struct EgressInfo {
    pub ip: String,
    pub country: String,
    pub org: String,
    /// IANA timezone of *this IP* as reported by the geolocation endpoint (e.g.
    /// `America/Denver`) — the ground truth for aligning the browser's timezone,
    /// correct even for multi-timezone countries. Empty if the endpoint that
    /// answered doesn't report one (e.g. ipify).
    pub timezone: String,
}

impl std::fmt::Display for EgressInfo {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = vec![self.ip.clone()];
        if !self.country.is_empty() {
            parts.push(self.country.clone());
        }
        if !self.org.is_empty() {
            parts.push(self.org.clone());
        }
        write!(formatter, "{}", parts.join(" · "))
    }
}

/// Fetch the egress IP through `proxy_url` (direct when `None`), trying each
/// endpoint until one answers. This is the ground-truth check that a proxy is
/// actually routing — reachability of a target host, not a platform login.
pub async fn check_egress_ip(proxy_url: Option<&str>) -> Result<EgressInfo, ProxyError> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(UA);
    if let Some(url) = proxy_url.filter(|url| !url.is_empty()) {
        builder = builder.proxy(
            reqwest::Proxy::all(url).map_err(|error| ProxyError::Config(error.to_string()))?,
        );
    }
    let client = builder
        .build()
        .map_err(|error| ProxyError::Config(error.to_string()))?;

    let mut last_error = String::from("no endpoint attempted");
    for endpoint in ENDPOINTS {
        match fetch_one(&client, endpoint).await {
            Ok(info) => return Ok(info),
            Err(error) => last_error = format!("{endpoint} → {error}"),
        }
    }
    Err(ProxyError::Unreachable(last_error))
}

/// Convenience: probe using a [`ProxyConfig`] directly.
pub async fn probe(proxy: &ProxyConfig) -> Result<EgressInfo, ProxyError> {
    check_egress_ip(Some(&proxy.to_url())).await
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> Result<EgressInfo, String> {
    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let text = response.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let json: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    // ipinfo → ip/country/org; ip-api → query/country/isp|org; ipify → ip.
    let ip = json
        .get("ip")
        .or_else(|| json.get("query"))
        .and_then(Value::as_str)
        .ok_or_else(|| "response had no ip".to_string())?
        .to_string();
    let country = string_field(&json, &["country", "country_iso", "countryCode"]);
    let org = string_field(&json, &["org", "isp", "asn_organization", "organization"]);
    // ipinfo → "timezone"; ip-api → "timezone" (both IANA). ipify has none.
    let timezone = string_field(&json, &["timezone", "time_zone"]);
    Ok(EgressInfo {
        ip,
        country,
        org,
        timezone,
    })
}

fn string_field(json: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(value) = json.get(key).and_then(Value::as_str)
            && !value.is_empty()
        {
            return value.to_string();
        }
    }
    String::new()
}

/// Current UTC offset in **seconds** (DST-aware, evaluated at "now") for an IANA
/// timezone name, e.g. `America/Denver` → `-21600`. `None` if the name isn't a
/// known zone or is empty. Used to align the IG session's `X-IG-Timezone-Offset`
/// to the 家宽 egress region instead of hardcoding `0`.
pub fn iana_offset_seconds(tz_name: &str) -> Option<i64> {
    use chrono::Offset;
    let tz: chrono_tz::Tz = tz_name.trim().parse().ok()?;
    Some(
        chrono::Utc::now()
            .with_timezone(&tz)
            .offset()
            .fix()
            .local_minus_utc() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::iana_offset_seconds;

    #[test]
    fn known_zones_resolve_to_plausible_offsets() {
        // UTC is always 0; a fixed-offset zone has no DST so it's stable.
        assert_eq!(iana_offset_seconds("UTC"), Some(0));
        assert_eq!(iana_offset_seconds("Etc/GMT+5"), Some(-5 * 3600));
        // A real US zone is somewhere in [-8h, -4h] across the year (PST..EDT).
        let denver = iana_offset_seconds("America/Denver").expect("known zone");
        assert!((-8 * 3600..=-4 * 3600).contains(&denver), "got {denver}");
        assert_eq!(iana_offset_seconds("not/a/zone"), None);
        assert_eq!(iana_offset_seconds(""), None);
    }
}
