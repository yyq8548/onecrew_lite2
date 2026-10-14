use thiserror::Error;

#[derive(Debug, Error)]
pub enum IgError {
    #[error("Instagram session error: {0}")]
    Session(String),
    #[error("Instagram request failed: {0}")]
    Network(String),
    #[error("Instagram API error: {0}")]
    Api(String),
    #[error("Instagram login/challenge required: {0}")]
    Challenge(String),
    /// The private account API's response status and machine error_type field.
    /// No body, challenge payload or free-form message is retained.
    #[error("Instagram account API response could not be used (HTTP {status})")]
    Response { status: u16, code: Option<String> },
    #[error("invalid request header")]
    InvalidHeader,
}
