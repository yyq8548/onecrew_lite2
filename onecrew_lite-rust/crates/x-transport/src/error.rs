use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("X session requires auth_token and ct0")]
    MissingCredential,
    #[error("invalid request header")]
    InvalidHeader,
    #[error("X request failed: {0}")]
    Network(String),
    #[error("X returned HTTP {status}: {message}")]
    Http {
        status: u16,
        code: Option<i64>,
        message: String,
    },
    #[error("X returned API error: {0}")]
    Api(String),
    #[error("X returned an API error (HTTP {status})")]
    ApiResponse { status: u16, code: Option<i64> },
}
