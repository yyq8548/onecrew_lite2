use onecrew_x_chat::XChatError;
use onecrew_x_client::XClientError;
use onecrew_x_transport::TransportError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Client(#[from] XClientError),
    #[error(transparent)]
    Chat(#[from] XChatError),
}

/// Whether a failed write is known to have produced no remote effect.
/// Network failures, unrecognized responses and missing receipts stay unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum XWriteFailure {
    #[error("X refused the write before any accepted effect")]
    Refused,
    #[error("X write outcome is unknown")]
    Unknown,
}

impl XError {
    /// The protocol-native evidence of this failure, shared by reads and
    /// direct messages: the X protocol, HTTP status and X's native error code,
    /// or a local code for a precondition X never answered. It never carries a
    /// raw body, URL, proxy address, key material or transport Display string.
    pub fn evidence(&self) -> onecrew_dm_contracts::DmDiagnostic {
        use onecrew_dm_contracts::DmDiagnostic;
        match self {
            Self::Transport(error) => transport_evidence(error, "x_web"),
            Self::Client(error) => client_evidence(error),
            Self::Chat(error) => match error {
                XChatError::Transport(error) => transport_evidence(error, "x_chat"),
                XChatError::Client(error) => client_evidence(error),
                XChatError::Url(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "invalid_url",
                    "X Chat request URL could not be constructed",
                ),
                XChatError::Crypto(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "crypto_failed",
                    "X Chat cryptographic operation failed",
                ),
                XChatError::KeyMaterial(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "key_material_incomplete",
                    "X Chat key material is incomplete",
                ),
                XChatError::DmPermission(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "permission_evidence_incomplete",
                    "X Chat permissions could not be established",
                ),
                XChatError::DmNotAllowed(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "dm_not_allowed",
                    "X Chat capability does not allow this message",
                ),
                XChatError::PremiumRequired => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "premium_required",
                    "X Chat requires Premium to start this conversation",
                ),
                XChatError::Input(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "invalid_input",
                    "X Chat input failed local validation",
                ),
                XChatError::Protocol(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "invalid_response",
                    "X Chat response could not be interpreted",
                ),
                XChatError::ConversationNotFound(_) => DmDiagnostic::local(
                    "x",
                    "x_chat",
                    "conversation_not_found",
                    "X Chat conversation was not found in the read response",
                ),
            },
        }
    }

    pub fn write_failure(&self) -> XWriteFailure {
        match self {
            Self::Client(XClientError::Input(_))
            | Self::Transport(TransportError::MissingCredential)
            | Self::Client(XClientError::Transport(TransportError::MissingCredential))
            | Self::Chat(
                XChatError::Input(_) | XChatError::DmNotAllowed(_) | XChatError::PremiumRequired,
            ) => XWriteFailure::Refused,
            // HTTP/API strings do not prove whether an upstream write was
            // accepted. In particular, protocol and 5xx responses are unknown.
            _ => XWriteFailure::Unknown,
        }
    }
}

fn client_evidence(error: &XClientError) -> onecrew_dm_contracts::DmDiagnostic {
    use onecrew_dm_contracts::DmDiagnostic;
    match error {
        XClientError::Transport(error) => transport_evidence(error, "x_web"),
        XClientError::UnknownOperation(_) => DmDiagnostic::local(
            "x",
            "x_web",
            "unknown_operation",
            "X web operation is unavailable",
        ),
        XClientError::Url(_) => DmDiagnostic::local(
            "x",
            "x_web",
            "invalid_url",
            "X web request URL could not be constructed",
        ),
        XClientError::Response(_) => DmDiagnostic::local(
            "x",
            "x_web",
            "invalid_response",
            "X web response could not be interpreted",
        ),
        XClientError::Input(_) => DmDiagnostic::local(
            "x",
            "x_web",
            "invalid_input",
            "X web input failed local validation",
        ),
    }
}

fn transport_evidence(
    error: &TransportError,
    protocol: &'static str,
) -> onecrew_dm_contracts::DmDiagnostic {
    use onecrew_dm_contracts::DmDiagnostic;
    match error {
        TransportError::MissingCredential => DmDiagnostic::local(
            "x",
            protocol,
            "credential_missing",
            "X credentials are incomplete",
        ),
        TransportError::InvalidHeader => DmDiagnostic::local(
            "x",
            protocol,
            "invalid_header",
            "X request headers could not be constructed",
        ),
        TransportError::Network(_) => {
            DmDiagnostic::provider("x", protocol, "X request did not produce a usable response")
        }
        TransportError::Http { status, code, .. } => {
            let mut diagnostic =
                DmDiagnostic::provider("x", protocol, "X returned an unsuccessful HTTP response");
            diagnostic.http_status = Some(*status);
            diagnostic.code = code.map(|code| code.to_string());
            diagnostic
        }
        TransportError::Api(_) => DmDiagnostic::provider(
            "x",
            protocol,
            "X returned an API error without a retained native code",
        ),
        TransportError::ApiResponse { status, code } => {
            let mut diagnostic = DmDiagnostic::provider("x", protocol, "X returned an API error");
            diagnostic.http_status = Some(*status);
            diagnostic.code = code.map(|code| code.to_string());
            diagnostic
        }
    }
}

#[cfg(test)]
mod tests {
    use onecrew_dm_contracts::DmDiagnosticOrigin;

    use super::*;

    const PROXY_URL: &str = "http://proxy_user:proxy_pass@gate.example:8080/?token=SENTINEL";

    /// Every X failure shape, each carrying a provider body, a credentialed
    /// proxy address and a token in its text wherever the shape has text.
    fn every_error() -> Vec<XError> {
        let text = || format!("body=SENTINEL cookie=SENTINEL via {PROXY_URL}");
        let transport = || {
            vec![
                TransportError::MissingCredential,
                TransportError::InvalidHeader,
                TransportError::Network(text()),
                TransportError::Http {
                    status: 429,
                    code: Some(88),
                    message: text(),
                },
                TransportError::Api(text()),
                TransportError::ApiResponse {
                    status: 200,
                    code: Some(214),
                },
            ]
        };
        let client = || {
            vec![
                XClientError::UnknownOperation(text()),
                XClientError::Url(text()),
                XClientError::Response(text()),
                XClientError::Input(text()),
            ]
        };
        let mut errors = Vec::new();
        errors.extend(transport().into_iter().map(XError::Transport));
        errors.extend(
            transport()
                .into_iter()
                .map(|error| XError::Client(XClientError::Transport(error))),
        );
        errors.extend(client().into_iter().map(XError::Client));
        errors.extend(
            transport()
                .into_iter()
                .map(|error| XError::Chat(XChatError::Transport(error))),
        );
        errors.extend(
            transport()
                .into_iter()
                .map(|error| XError::Chat(XChatError::Client(XClientError::Transport(error)))),
        );
        errors.extend(
            client()
                .into_iter()
                .map(|error| XError::Chat(XChatError::Client(error))),
        );
        errors.extend(
            [
                XChatError::Url(text()),
                XChatError::Crypto(text()),
                XChatError::KeyMaterial(text()),
                XChatError::DmPermission(text()),
                XChatError::DmNotAllowed(text()),
                XChatError::PremiumRequired,
                XChatError::Input(text()),
                XChatError::Protocol(text()),
                XChatError::ConversationNotFound(text()),
            ]
            .map(XError::Chat),
        );
        for error in &errors {
            covered(error);
        }
        errors
    }

    /// Names every X failure shape, so a new shape fails to compile here
    /// until it gains an arm. The match neither adds the shape to
    /// `every_error` nor notices a sample removed from it; the list's author
    /// keeps every sample and its evidence.
    fn covered(error: &XError) {
        let transport = |error: &TransportError| match error {
            TransportError::MissingCredential
            | TransportError::InvalidHeader
            | TransportError::Network(_)
            | TransportError::Http { .. }
            | TransportError::Api(_)
            | TransportError::ApiResponse { .. } => {}
        };
        let client = |error: &XClientError| match error {
            XClientError::Transport(error) => transport(error),
            XClientError::UnknownOperation(_)
            | XClientError::Url(_)
            | XClientError::Response(_)
            | XClientError::Input(_) => {}
        };
        match error {
            XError::Transport(error) => transport(error),
            XError::Client(error) => client(error),
            XError::Chat(error) => match error {
                XChatError::Transport(error) => transport(error),
                XChatError::Client(error) => client(error),
                XChatError::Url(_)
                | XChatError::Crypto(_)
                | XChatError::KeyMaterial(_)
                | XChatError::DmPermission(_)
                | XChatError::DmNotAllowed(_)
                | XChatError::PremiumRequired
                | XChatError::Input(_)
                | XChatError::Protocol(_)
                | XChatError::ConversationNotFound(_) => {}
            },
        }
    }

    #[test]
    fn evidence_of_every_error_carries_no_body_proxy_address_or_transport_text() {
        for error in every_error() {
            let evidence = error.evidence();
            assert_eq!(evidence.platform, "x", "{error:?}");
            assert!(matches!(evidence.protocol, "x_web" | "x_chat"), "{error:?}");
            assert!(evidence.rpc_code.is_none(), "{error:?}");
            for text in [
                serde_json::to_string(&evidence).unwrap(),
                format!("{evidence:?}"),
            ] {
                for absent in [
                    "SENTINEL",
                    "body=",
                    "cookie=",
                    "proxy_user",
                    "proxy_pass",
                    "gate.example",
                    "token=",
                ] {
                    assert!(!text.contains(absent), "{absent}: {text}");
                }
            }
        }
    }

    #[test]
    fn evidence_keeps_nested_x_chat_native_codes_without_raw_details() {
        for message in [
            "login",
            "429",
            "sent successfully",
            "timeout",
            "Bearer SENTINEL https://secret.invalid/?token=SENTINEL",
        ] {
            let error = XError::Chat(XChatError::Transport(TransportError::Http {
                status: 429,
                code: Some(88),
                message: message.into(),
            }));
            let diagnostic = error.evidence();
            assert_eq!(diagnostic.platform, "x");
            assert_eq!(diagnostic.protocol, "x_chat");
            assert_eq!(diagnostic.origin, DmDiagnosticOrigin::Provider);
            assert_eq!(diagnostic.code.as_deref(), Some("88"));
            assert_eq!(diagnostic.http_status, Some(429));
            assert_eq!(
                diagnostic.message,
                "X returned an unsuccessful HTTP response"
            );
            assert_eq!(error.write_failure(), XWriteFailure::Unknown);
            assert!(
                !serde_json::to_string(&diagnostic)
                    .unwrap()
                    .contains("SENTINEL")
            );
        }
        let unknown = XError::Transport(TransportError::Network("login timeout".into())).evidence();
        assert_eq!(unknown.origin, DmDiagnosticOrigin::Provider);
        assert!(unknown.code.is_none() && unknown.http_status.is_none());
        let local = XError::Chat(XChatError::PremiumRequired).evidence();
        assert_eq!(local.origin, DmDiagnosticOrigin::Local);
        assert_eq!(local.code.as_deref(), Some("premium_required"));
        assert!(local.http_status.is_none());
    }

    #[test]
    fn write_failures_preserve_ambiguous_effects_and_drop_diagnostics() {
        for error in [
            XError::Transport(TransportError::Network("SECRET".into())),
            XError::Transport(TransportError::Http {
                status: 500,
                code: None,
                message: "SECRET".into(),
            }),
            XError::Client(XClientError::Response("missing receipt SECRET".into())),
            XError::Chat(XChatError::Protocol("SECRET".into())),
        ] {
            assert_eq!(error.write_failure(), XWriteFailure::Unknown);
            assert!(!format!("{:?}", error.write_failure()).contains("SECRET"));
        }
        assert_eq!(
            XError::Chat(XChatError::DmNotAllowed("SECRET".into())).write_failure(),
            XWriteFailure::Refused
        );
        assert_eq!(
            XError::Chat(XChatError::Input("target changed".into())).write_failure(),
            XWriteFailure::Refused
        );
    }

    #[test]
    fn a_rate_limited_response_keeps_its_status_and_drops_the_transport_message() {
        const SENTINEL: &str = "body=cookie=proxy=https://secret.invalid/?token=SENTINEL";
        let evidence = XError::Transport(TransportError::Http {
            status: 429,
            code: None,
            message: SENTINEL.into(),
        })
        .evidence();

        assert_eq!(evidence.origin, DmDiagnosticOrigin::Provider);
        assert_eq!(evidence.protocol, "x_web");
        assert_eq!(evidence.http_status, Some(429));
        assert_eq!(evidence.code, None);
        assert!(!format!("{evidence:?}").contains(SENTINEL));
    }

    #[test]
    fn exhausted_stale_query_id_responses_keep_their_http_status() {
        for status in [404, 422] {
            let evidence = XError::Transport(TransportError::Http {
                status,
                code: None,
                message: "provider body sentinel".into(),
            })
            .evidence();
            assert_eq!(evidence.origin, DmDiagnosticOrigin::Provider);
            assert_eq!(evidence.http_status, Some(status));
            assert_eq!(evidence.code, None);
            assert!(!format!("{evidence:?}").contains("provider body sentinel"));
        }
    }

    #[test]
    fn advanced_search_validation_is_local_invalid_input_evidence() {
        let evidence = XError::Client(XClientError::Input(
            "query contained a rejected operator sentinel".into(),
        ))
        .evidence();
        assert_eq!(evidence.origin, DmDiagnosticOrigin::Local);
        assert_eq!(evidence.protocol, "x_web");
        assert_eq!(evidence.code.as_deref(), Some("invalid_input"));
        assert_eq!(evidence.http_status, None);
        assert!(!format!("{evidence:?}").contains("sentinel"));
    }
}
