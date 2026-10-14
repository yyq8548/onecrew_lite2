use grammers_client::InvocationError;
use thiserror::Error;

/// Failures surfaced by the Telegram transport layer.
#[derive(Debug, Error)]
pub enum TgError {
    #[error("telegram session: {0}")]
    Session(String),
    #[error("telegram network: {0}")]
    Network(String),
    #[error("telegram auth: {0}")]
    Auth(String),
    #[error("telegram permission: {0}")]
    Permission(String),
    #[error("telegram: {0}")]
    Other(String),
    /// Native MTProto RPC evidence; no invocation Display or request payload.
    #[error("telegram MTProto RPC error {code}")]
    Rpc { code: i32, name: Option<String> },
}

impl TgError {
    /// Retains Telegram's own RPC code and name, independently of effect
    /// evidence. Other invocation failures keep the transport classification;
    /// an RPC response alone does not establish whether a send took effect.
    pub fn from_invocation(error: InvocationError) -> Self {
        match error {
            InvocationError::Rpc(rpc) => {
                let name = (!rpc.name.is_empty()
                    && rpc.name.len() <= 80
                    && rpc.name.bytes().all(|byte| {
                        byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_'
                    }))
                .then_some(rpc.name);
                Self::Rpc {
                    code: rpc.code,
                    name,
                }
            }
            error => Self::Network(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use grammers_client::sender::RpcError;

    use super::*;

    fn rpc(code: i32, name: &str) -> InvocationError {
        InvocationError::Rpc(RpcError {
            code,
            name: name.into(),
            value: None,
            caused_by: None,
        })
    }

    #[test]
    fn telegram_answers_are_not_network_errors() {
        assert!(matches!(
            TgError::from_invocation(rpc(401, "AUTH_KEY_UNREGISTERED")),
            TgError::Rpc { code: 401, .. }
        ));
        assert!(matches!(
            TgError::from_invocation(rpc(403, "CHAT_WRITE_FORBIDDEN")),
            TgError::Rpc { code: 403, .. }
        ));
        for (code, name) in [
            (400, "CHANNEL_INVALID"),
            (400, "CHANNEL_PRIVATE"),
            (420, "FLOOD_WAIT"),
            (500, "INTERNAL"),
        ] {
            assert!(
                matches!(TgError::from_invocation(rpc(code, name)), TgError::Rpc { code: actual, name: Some(actual_name) } if actual == code && actual_name == name),
                "{code} {name}"
            );
        }
    }

    #[test]
    fn failures_without_a_telegram_answer_stay_network_errors() {
        for error in [
            InvocationError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "proxy refused",
            )),
            InvocationError::Dropped,
        ] {
            assert!(matches!(
                TgError::from_invocation(error),
                TgError::Network(_)
            ));
        }
    }

    #[test]
    fn malformed_rpc_names_cannot_expose_invocation_details() {
        let error = TgError::from_invocation(rpc(500, "https://secret.invalid/?token=SENTINEL"));
        assert!(matches!(
            &error,
            TgError::Rpc {
                code: 500,
                name: None
            }
        ));
        assert!(!format!("{error:?}").contains("SENTINEL"));
    }
}
