//! Temporary Resend read compatibility boundary.
//!
//! `resend-rs` owns send/batch HTTP. Its closed event model and lack of the
//! product's raw team-page bounds require this private, read-only compatibility
//! decoder until an SDK-equivalent proof replaces it. No generic provider uses
//! this module.

use std::sync::Arc;

use onecrew_email_contracts::{
    EmailError, ProviderErrorClass, ProviderFailure, ProviderRateLimitStatus,
};
use onecrew_resend_transport::{ResendOpPriority, ResendTransport};
use rand::Rng;
use serde::de::DeserializeOwned;
use tokio::time::Duration;

use crate::wire::{WireErrorBody, parse_rate_limit_headers, parse_rate_limit_headers_lenient};

const MAX_ATTEMPTS: u32 = 4;
const BACKOFF_BASE: Duration = Duration::from_millis(500);
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const DOWNLOAD_TOTAL_TIMEOUT: Duration = Duration::from_secs(600);
const DOWNLOAD_CHUNK_STALL: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct ResendReadCompat {
    transport: Arc<ResendTransport>,
}

impl ResendReadCompat {
    pub(crate) fn new(transport: Arc<ResendTransport>) -> Self {
        Self { transport }
    }

    pub(crate) async fn get_json<T: DeserializeOwned>(
        &self,
        api_key: &str,
        priority: ResendOpPriority,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, EmailError> {
        self.execute_json(api_key, priority, path, query).await
    }

    async fn execute_json<T: DeserializeOwned>(
        &self,
        api_key: &str,
        priority: ResendOpPriority,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, EmailError> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let permit = self.transport.acquire(priority).await;
            let outcome = self.perform(api_key, path, query).await;
            drop(permit);
            let error = match outcome {
                Ok(response) if (200..300).contains(&response.status) => {
                    self.transport.observe(response.rate_limit);
                    return serde_json::from_slice(&response.body).map_err(|error| {
                        EmailError::Transport(format!(
                            "failed to decode provider response for {path}: {error}"
                        ))
                    });
                }
                Ok(response) => EmailError::Provider(decode_provider_failure(&response, api_key)),
                Err(error) => error,
            };
            let rate_limit_pause = match error.provider_failure() {
                Some(failure) if failure.class == ProviderErrorClass::RateLimited => {
                    Some(self.transport.report_rate_limited(failure.rate_limit))
                }
                _ => None,
            };
            if !error.is_transient() || attempt >= MAX_ATTEMPTS {
                return Err(error);
            }
            let wait = rate_limit_pause
                .unwrap_or_else(|| BACKOFF_BASE * 2_u32.saturating_pow(attempt - 1));
            tokio::time::sleep(with_jitter(wait)).await;
        }
    }

    async fn perform(
        &self,
        api_key: &str,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<RawResponse, EmailError> {
        let mut url = self.transport.base_url().join(path).map_err(|error| {
            EmailError::InvalidConfig(format!("invalid provider path {path}: {error}"))
        })?;
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (name, value) in query {
                pairs.append_pair(name, value);
            }
        }
        let mut response = self
            .transport
            .http_client()
            .get(url)
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|error| EmailError::Transport(redact(&error.to_string(), api_key)))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str(), value.to_str().unwrap_or_default()))
            .collect::<Vec<_>>();
        let rate_limit = if (200..300).contains(&status) {
            parse_rate_limit_headers(headers.clone()).map_err(|message| {
                EmailError::Provider(ProviderFailure {
                    status,
                    class: ProviderErrorClass::Unrecognized,
                    message: format!("Resend sync response metadata invalid for {path}: {message}"),
                    rate_limit: ProviderRateLimitStatus {
                        limit: None,
                        remaining: None,
                        reset_seconds: None,
                        retry_after_seconds: None,
                    },
                })
            })?
        } else {
            parse_rate_limit_headers_lenient(headers)
        };
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| EmailError::Transport(redact(&error.to_string(), api_key)))?
        {
            if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(EmailError::OversizedResponse {
                    path: path.to_owned(),
                    limit: MAX_RESPONSE_BYTES,
                });
            }
            body.extend_from_slice(&chunk);
        }
        Ok(RawResponse {
            status,
            body,
            rate_limit,
        })
    }

    pub(crate) async fn download_signed(
        &self,
        url: &str,
        limit: usize,
    ) -> Result<Vec<u8>, EmailError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|error| EmailError::InvalidMessage(format!("invalid signed url: {error}")))?;
        let trusted_host = parsed
            .host_str()
            .is_some_and(|host| host == "resend.com" || host.ends_with(".resend.com"));
        let test_seam = self.transport.base_url().scheme() != "https"
            && parsed.host_str().is_some_and(|host| host == "127.0.0.1");
        if parsed.scheme() != "https" && !test_seam {
            return Err(EmailError::InvalidMessage(
                "signed attachment url must use https".into(),
            ));
        }
        if !trusted_host && !test_seam {
            return Err(EmailError::InvalidMessage(
                "signed attachment url is outside the provider trust boundary".into(),
            ));
        }
        let permit = self.transport.acquire(ResendOpPriority::User).await;
        let mut response = self
            .transport
            .http_client()
            .get(parsed)
            .timeout(DOWNLOAD_TOTAL_TIMEOUT)
            .send()
            .await
            .map_err(signed_download_error)?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            drop(permit);
            return Err(EmailError::Provider(ProviderFailure {
                status,
                class: WireErrorBody::classify_status_only(status).into(),
                message: format!("attachment download returned HTTP {status}"),
                rate_limit: ProviderRateLimitStatus {
                    limit: None,
                    remaining: None,
                    reset_seconds: None,
                    retry_after_seconds: None,
                },
            }));
        }
        let mut collected = Vec::new();
        loop {
            let chunk = tokio::time::timeout(DOWNLOAD_CHUNK_STALL, response.chunk())
                .await
                .map_err(|_| EmailError::Transport("attachment download stalled".into()))?
                .map_err(signed_download_error)?;
            let Some(chunk) = chunk else { break };
            if collected.len() + chunk.len() > limit {
                return Err(EmailError::OversizedResponse {
                    path: "attachment download".into(),
                    limit,
                });
            }
            collected.extend_from_slice(&chunk);
        }
        drop(permit);
        Ok(collected)
    }
}

struct RawResponse {
    status: u16,
    body: Vec<u8>,
    rate_limit: ProviderRateLimitStatus,
}

fn decode_provider_failure(response: &RawResponse, api_key: &str) -> ProviderFailure {
    match serde_json::from_slice::<WireErrorBody>(&response.body) {
        Ok(body) => ProviderFailure {
            status: response.status,
            class: body.classify().into(),
            message: redact(&body.message, api_key),
            rate_limit: response.rate_limit,
        },
        Err(_) => ProviderFailure {
            status: response.status,
            class: WireErrorBody::classify_status_only(response.status).into(),
            message: format!("HTTP {} with undecodable body", response.status),
            rate_limit: response.rate_limit,
        },
    }
}

fn with_jitter(base: Duration) -> Duration {
    let jitter_ceiling = base.as_millis() as u64 / 2;
    if jitter_ceiling == 0 {
        return base;
    }
    base + Duration::from_millis(rand::thread_rng().gen_range(0..=jitter_ceiling))
}

fn signed_download_error(error: reqwest::Error) -> EmailError {
    EmailError::Transport(format!(
        "attachment download failed: {}",
        error.without_url()
    ))
}

fn redact(message: &str, api_key: &str) -> String {
    if api_key.is_empty() {
        message.to_owned()
    } else {
        message.replace(api_key, "<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_within_half_of_the_base_delay() {
        let base = Duration::from_millis(1000);
        for _ in 0..64 {
            let delayed = with_jitter(base);
            assert!(delayed >= base && delayed <= base + Duration::from_millis(500));
        }
        assert_eq!(
            with_jitter(Duration::from_millis(1)),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn redaction_strips_the_key_from_transport_errors() {
        assert_eq!(
            redact("connect to https://x?key=re_secret failed", "re_secret"),
            "connect to https://x?key=<redacted> failed"
        );
        assert_eq!(redact("plain failure", ""), "plain failure");
    }

    #[test]
    fn undecodable_error_bodies_classify_by_status_alone() {
        let failure = decode_provider_failure(
            &RawResponse {
                status: 503,
                body: b"<html>upstream</html>".to_vec(),
                rate_limit: ProviderRateLimitStatus {
                    limit: None,
                    remaining: None,
                    reset_seconds: None,
                    retry_after_seconds: None,
                },
            },
            "",
        );
        assert_eq!(failure.class, ProviderErrorClass::ProviderFault);
        assert_eq!(failure.status, 503);
    }
}
