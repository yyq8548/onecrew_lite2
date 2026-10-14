use onecrew_dm_contracts::DmDiagnostic;
pub use onecrew_youtube_client::YoutubeClientError;
pub use onecrew_youtube_transport::{
    YoutubeApiFailureKind, YoutubeRequestFailureKind, YoutubeTransportError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum YoutubePlatformError {
    #[error(transparent)]
    Transport(#[from] YoutubeTransportError),
    #[error(transparent)]
    Client(#[from] YoutubeClientError),
}

/// The platform every YouTube evidence names.
const PLATFORM: &str = "youtube";
/// The protocol of every YouTube read: the YouTube Data API. Public About
/// pages never fail a read; the client marks that section unavailable.
const PROTOCOL: &str = "youtube_data_api_v3";

impl YoutubePlatformError {
    /// The protocol-native evidence of this failure.
    ///
    /// An API failure keeps its HTTP status and its first native reason
    /// token. A request that received no response keeps no code. Every other
    /// failure is a local conclusion, with a code and explanation this facade
    /// defines. Evidence never carries a response body, URL, query, API key,
    /// proxy address or transport Display string.
    pub fn evidence(&self) -> DmDiagnostic {
        let transport = match self {
            Self::Transport(error) | Self::Client(YoutubeClientError::Transport(error)) => error,
            Self::Client(
                YoutubeClientError::EmptyQuery
                | YoutubeClientError::InvalidSearchCount
                | YoutubeClientError::InvalidPublishedAfter
                | YoutubeClientError::InvalidChannelId,
            ) => return local("invalid_input", "YouTube read input was refused"),
            Self::Client(YoutubeClientError::ChannelNotFound) => {
                return local("channel_not_found", "YouTube returned no such channel");
            }
            Self::Client(YoutubeClientError::Parse(_)) => {
                return local(
                    "invalid_response",
                    "YouTube response could not be interpreted",
                );
            }
        };
        match transport {
            YoutubeTransportError::Api { status, reason, .. } => DmDiagnostic {
                http_status: Some(*status),
                code: reason.clone(),
                ..DmDiagnostic::provider(
                    PLATFORM,
                    PROTOCOL,
                    "YouTube returned an unsuccessful HTTP response",
                )
            },
            YoutubeTransportError::Request {
                failure_kind:
                    YoutubeRequestFailureKind::Timeout
                    | YoutubeRequestFailureKind::Connect
                    | YoutubeRequestFailureKind::Body
                    | YoutubeRequestFailureKind::Network,
                ..
            } => DmDiagnostic::provider(
                PLATFORM,
                PROTOCOL,
                "YouTube request did not receive a response",
            ),
            YoutubeTransportError::Request {
                failure_kind: YoutubeRequestFailureKind::Decode | YoutubeRequestFailureKind::Status,
                ..
            }
            | YoutubeTransportError::Decode => local(
                "invalid_response",
                "YouTube response could not be interpreted",
            ),
            YoutubeTransportError::InvalidUrl => {
                local("invalid_request", "YouTube request could not be built")
            }
            YoutubeTransportError::HostNotAllowed => {
                local("host_not_allowed", "YouTube page host is not allowed")
            }
            YoutubeTransportError::BodyTooLarge { .. } => local(
                "response_too_large",
                "YouTube response exceeded its size limit",
            ),
        }
    }
}

fn local(code: &'static str, message: &'static str) -> DmDiagnostic {
    DmDiagnostic::local(PLATFORM, PROTOCOL, code, message)
}

#[cfg(test)]
mod tests {
    use onecrew_dm_contracts::DmDiagnosticOrigin;

    use super::*;

    const API_KEY: &str = "AIzaSy_API_KEY_SENTINEL";
    /// Request context that a careless error would echo: an API key in a
    /// query, a credentialed proxy address and a provider body.
    const REQUEST_TEXT: &str = "GET https://www.googleapis.com/youtube/v3/channels?key=AIzaSy_API_KEY_SENTINEL via http://proxy_user:proxy_pass@gate.example:8080 body={\"error\":\"PROVIDER_BODY_SENTINEL\"}";

    const REQUEST_KINDS: [YoutubeRequestFailureKind; 6] = [
        YoutubeRequestFailureKind::Timeout,
        YoutubeRequestFailureKind::Connect,
        YoutubeRequestFailureKind::Body,
        YoutubeRequestFailureKind::Decode,
        YoutubeRequestFailureKind::Status,
        YoutubeRequestFailureKind::Network,
    ];
    const API_KINDS: [YoutubeApiFailureKind; 7] = [
        YoutubeApiFailureKind::CredentialRejected,
        YoutubeApiFailureKind::AccessNotConfigured,
        YoutubeApiFailureKind::QuotaExceeded,
        YoutubeApiFailureKind::RateLimited,
        YoutubeApiFailureKind::AmbiguousForbidden,
        YoutubeApiFailureKind::Client,
        YoutubeApiFailureKind::Server,
    ];

    /// Every transport failure shape, with request text wherever a shape
    /// holds text.
    fn transport_errors() -> Vec<YoutubeTransportError> {
        let mut errors = vec![
            YoutubeTransportError::InvalidUrl,
            YoutubeTransportError::HostNotAllowed,
            YoutubeTransportError::BodyTooLarge { limit: 1024 },
            YoutubeTransportError::Decode,
        ];
        errors.extend(
            REQUEST_KINDS.map(|failure_kind| YoutubeTransportError::Request {
                operation: REQUEST_TEXT,
                failure_kind,
            }),
        );
        for failure_kind in API_KINDS {
            errors.push(YoutubeTransportError::Api {
                status: 403,
                failure_kind,
                reason: Some("quotaExceeded".to_owned()),
            });
            errors.push(YoutubeTransportError::Api {
                status: 503,
                failure_kind,
                reason: None,
            });
        }
        errors
    }

    /// Every YouTube failure shape: each transport failure directly and
    /// inside the client, and every client failure of its own.
    fn every_error() -> Vec<YoutubePlatformError> {
        let mut errors = Vec::new();
        errors.extend(
            transport_errors()
                .into_iter()
                .map(YoutubePlatformError::Transport),
        );
        errors.extend(
            transport_errors()
                .into_iter()
                .map(|error| YoutubePlatformError::Client(YoutubeClientError::Transport(error))),
        );
        errors.extend(
            [
                YoutubeClientError::EmptyQuery,
                YoutubeClientError::InvalidSearchCount,
                YoutubeClientError::InvalidPublishedAfter,
                YoutubeClientError::InvalidChannelId,
                YoutubeClientError::ChannelNotFound,
                YoutubeClientError::Parse(REQUEST_TEXT.to_owned()),
            ]
            .map(YoutubePlatformError::Client),
        );
        for error in &errors {
            covered(error);
        }
        errors
    }

    /// Names every YouTube failure shape, so a new shape fails to compile
    /// here until it gains an arm. The match neither adds the shape to
    /// `every_error` nor notices a sample removed from it; the list's author
    /// keeps every sample and its evidence.
    fn covered(error: &YoutubePlatformError) {
        let transport = |error: &YoutubeTransportError| match error {
            YoutubeTransportError::InvalidUrl
            | YoutubeTransportError::HostNotAllowed
            | YoutubeTransportError::Request { .. }
            | YoutubeTransportError::Api { .. }
            | YoutubeTransportError::BodyTooLarge { .. }
            | YoutubeTransportError::Decode => {}
        };
        match error {
            YoutubePlatformError::Transport(error)
            | YoutubePlatformError::Client(YoutubeClientError::Transport(error)) => {
                transport(error)
            }
            YoutubePlatformError::Client(
                YoutubeClientError::EmptyQuery
                | YoutubeClientError::InvalidSearchCount
                | YoutubeClientError::InvalidPublishedAfter
                | YoutubeClientError::InvalidChannelId
                | YoutubeClientError::ChannelNotFound
                | YoutubeClientError::Parse(_),
            ) => {}
        }
    }

    /// The origin, native code and HTTP status every shape's evidence holds.
    fn expected(error: &YoutubePlatformError) -> (DmDiagnosticOrigin, Option<&str>, Option<u16>) {
        let local = |code| (DmDiagnosticOrigin::Local, Some(code), None);
        let transport = match error {
            YoutubePlatformError::Transport(error)
            | YoutubePlatformError::Client(YoutubeClientError::Transport(error)) => error,
            YoutubePlatformError::Client(
                YoutubeClientError::EmptyQuery
                | YoutubeClientError::InvalidSearchCount
                | YoutubeClientError::InvalidPublishedAfter
                | YoutubeClientError::InvalidChannelId,
            ) => return local("invalid_input"),
            YoutubePlatformError::Client(YoutubeClientError::ChannelNotFound) => {
                return local("channel_not_found");
            }
            YoutubePlatformError::Client(YoutubeClientError::Parse(_)) => {
                return local("invalid_response");
            }
        };
        match transport {
            YoutubeTransportError::Api { status, reason, .. } => (
                DmDiagnosticOrigin::Provider,
                reason.as_deref(),
                Some(*status),
            ),
            YoutubeTransportError::Request { failure_kind, .. } => match failure_kind {
                YoutubeRequestFailureKind::Timeout
                | YoutubeRequestFailureKind::Connect
                | YoutubeRequestFailureKind::Body
                | YoutubeRequestFailureKind::Network => (DmDiagnosticOrigin::Provider, None, None),
                YoutubeRequestFailureKind::Decode | YoutubeRequestFailureKind::Status => {
                    local("invalid_response")
                }
            },
            YoutubeTransportError::Decode => local("invalid_response"),
            YoutubeTransportError::InvalidUrl => local("invalid_request"),
            YoutubeTransportError::HostNotAllowed => local("host_not_allowed"),
            YoutubeTransportError::BodyTooLarge { .. } => local("response_too_large"),
        }
    }

    #[test]
    fn every_failure_has_its_protocol_native_evidence() {
        for error in every_error() {
            let evidence = error.evidence();
            assert_eq!(evidence.platform, "youtube", "{error:?}");
            assert_eq!(evidence.protocol, "youtube_data_api_v3", "{error:?}");
            assert!(evidence.rpc_code.is_none(), "{error:?}");
            assert!(!evidence.message.is_empty(), "{error:?}");
            let (origin, code, http_status) = expected(&error);
            assert_eq!(
                (
                    evidence.origin,
                    evidence.code.as_deref(),
                    evidence.http_status
                ),
                (origin, code, http_status),
                "{error:?}"
            );
        }
    }

    #[test]
    fn a_failure_without_any_response_is_the_only_provider_evidence_without_status_or_code() {
        for error in every_error() {
            let evidence = error.evidence();
            let without_response = evidence.origin == DmDiagnosticOrigin::Provider
                && evidence.http_status.is_none()
                && evidence.rpc_code.is_none()
                && evidence.code.is_none();
            let no_response = matches!(
                &error,
                YoutubePlatformError::Transport(YoutubeTransportError::Request {
                    failure_kind: YoutubeRequestFailureKind::Timeout
                        | YoutubeRequestFailureKind::Connect
                        | YoutubeRequestFailureKind::Body
                        | YoutubeRequestFailureKind::Network,
                    ..
                }) | YoutubePlatformError::Client(YoutubeClientError::Transport(
                    YoutubeTransportError::Request {
                        failure_kind: YoutubeRequestFailureKind::Timeout
                            | YoutubeRequestFailureKind::Connect
                            | YoutubeRequestFailureKind::Body
                            | YoutubeRequestFailureKind::Network,
                        ..
                    }
                ))
            );
            assert_eq!(without_response, no_response, "{error:?}");
        }
    }

    #[test]
    fn direct_and_client_wrapped_transport_failures_have_identical_evidence() {
        for (direct, wrapped) in transport_errors().into_iter().zip(transport_errors()) {
            let label = format!("{direct:?}");
            assert_eq!(
                YoutubePlatformError::Transport(direct).evidence(),
                YoutubePlatformError::Client(YoutubeClientError::Transport(wrapped)).evidence(),
                "{label}"
            );
        }
    }

    #[test]
    fn evidence_carries_no_api_key_response_body_or_request_text() {
        for error in every_error() {
            let evidence = error.evidence();
            for text in [
                serde_json::to_string(&evidence).unwrap(),
                format!("{evidence:?}"),
            ] {
                for absent in [
                    API_KEY,
                    "SENTINEL",
                    "key=",
                    "googleapis",
                    "body=",
                    "proxy_user",
                    "proxy_pass",
                    "gate.example",
                ] {
                    assert!(!text.contains(absent), "{absent}: {text}");
                }
            }
        }
    }
}
