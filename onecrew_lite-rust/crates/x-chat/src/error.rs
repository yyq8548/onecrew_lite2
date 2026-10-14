use onecrew_x_transport::TransportError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XChatError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("invalid X Chat URL: {0}")]
    Url(String),
    #[error("X Chat cryptography failed: {0}")]
    Crypto(String),
    #[error("X Chat key material is incomplete: {0}")]
    KeyMaterial(String),
    #[error("X Chat DM permissions are incomplete: {0}")]
    DmPermission(String),
    #[error("X does not allow this direct message: {0}")]
    DmNotAllowed(String),
    #[error("X Premium is required to start this direct-message conversation")]
    PremiumRequired,
    #[error("X Chat input is invalid: {0}")]
    Input(String),
    #[error("X Chat response is invalid: {0}")]
    Protocol(String),
    #[error("X Chat conversation was not found: {0}")]
    ConversationNotFound(String),
    #[error("X client failed: {0}")]
    Client(#[from] onecrew_x_client::XClientError),
}
