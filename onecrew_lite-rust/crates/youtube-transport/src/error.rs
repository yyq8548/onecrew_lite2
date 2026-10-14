use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeRequestFailureKind {
    #[error("timeout")]
    Timeout,
    #[error("connection failed")]
    Connect,
    #[error("response body failed")]
    Body,
    #[error("response decode failed")]
    Decode,
    #[error("unexpected HTTP status")]
    Status,
    #[error("network failure")]
    Network,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YoutubeApiFailureKind {
    #[error("credential rejected")]
    CredentialRejected,
    #[error("service access is not configured")]
    AccessNotConfigured,
    #[error("quota exceeded")]
    QuotaExceeded,
    #[error("rate limited")]
    RateLimited,
    #[error("forbidden response was ambiguous")]
    AmbiguousForbidden,
    #[error("client response failure")]
    Client,
    #[error("server response failure")]
    Server,
}

/// Public, serializable transport failure. It retains no reqwest error, URL,
/// provider body, query, API key, or free-form provider message.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum YoutubeTransportError {
    #[error("invalid YouTube request URL")]
    InvalidUrl,
    #[error("public page host is not allowed")]
    HostNotAllowed,
    #[error("YouTube request failed during {operation}: {failure_kind}")]
    Request {
        operation: &'static str,
        failure_kind: YoutubeRequestFailureKind,
    },
    #[error("YouTube API returned HTTP {status}: {failure_kind}")]
    Api {
        status: u16,
        failure_kind: YoutubeApiFailureKind,
        /// The first native `error.errors[].reason` of the API response, kept
        /// only as a token of 1 to 80 ASCII letters, digits or underscores.
        /// It lives only in this in-memory error and the evidence built from
        /// it; it is never serialized, so this error cannot persist it.
        #[serde(skip_serializing)]
        reason: Option<String>,
    },
    #[error("public page exceeded {limit} bytes")]
    BodyTooLarge { limit: usize },
    #[error("YouTube response could not be decoded")]
    Decode,
}

impl YoutubeTransportError {
    pub(crate) fn request(operation: &'static str, error: &reqwest::Error) -> Self {
        let kind = if error.is_timeout() {
            YoutubeRequestFailureKind::Timeout
        } else if error.is_connect() {
            YoutubeRequestFailureKind::Connect
        } else if error.is_body() {
            YoutubeRequestFailureKind::Body
        } else if error.is_decode() {
            YoutubeRequestFailureKind::Decode
        } else if error.is_status() {
            YoutubeRequestFailureKind::Status
        } else {
            YoutubeRequestFailureKind::Network
        };
        Self::Request {
            operation,
            failure_kind: kind,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_transport_errors_have_no_body_url_query_or_key_in_any_public_wire() {
        const SENTINEL: &str = "PROVIDER_BODY_QUERY_KEY_SENTINEL";
        let errors = [
            YoutubeTransportError::Api {
                status: 403,
                failure_kind: YoutubeApiFailureKind::AmbiguousForbidden,
                reason: Some("unknownReason".to_owned()),
            },
            YoutubeTransportError::Request {
                operation: "sending API request",
                failure_kind: YoutubeRequestFailureKind::Timeout,
            },
        ];
        for error in errors {
            // The reason token is in-memory evidence only; serializing the
            // error drops it.
            let serialized = serde_json::to_string(&error).unwrap();
            for absent in ["reason", "unknownReason"] {
                assert!(!serialized.contains(absent), "{absent}: {serialized}");
            }
            for wire in [
                error.to_string(),
                format!("{error:?}"),
                serde_json::to_string(&error).unwrap(),
            ] {
                assert!(!wire.contains(SENTINEL));
                assert!(!wire.contains("http"));
                assert!(!wire.contains("?key="));
                assert!(!wire.contains("provider_body"));
            }
            assert!(std::error::Error::source(&error).is_none());
        }
    }
}
