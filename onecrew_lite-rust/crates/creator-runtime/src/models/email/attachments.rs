use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{EmailDirection, EmailId};

/// The product-wide bound on one attachment download. Resend caps a whole
/// email at 40 MB; anything above this is reported `TooLarge` instead of
/// being buffered.
pub const MAX_EMAIL_ATTACHMENT_BYTES: usize = 64 * 1024 * 1024;

/// Durable attachment metadata projected to public surfaces. Never carries
/// an access URL: content access mints a fresh short-lived capability on
/// every request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EmailAttachmentView {
    pub provider_attachment_id: String,
    pub filename: String,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size_bytes: Option<u64>,
    pub direction: EmailDirection,
    /// Last observed provider availability. `false` records retention
    /// expiry evidence; the metadata itself stays local forever.
    pub available: bool,
}

/// One downloaded attachment, held in memory inside the runtime. Not
/// serializable: surfaces decide in their own phase how bytes reach a user,
/// never by serializing a signed URL or an unbounded payload by default.
#[derive(Clone)]
pub struct EmailAttachmentContent {
    pub metadata: EmailAttachmentView,
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for EmailAttachmentContent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EmailAttachmentContent")
            .field("metadata", &self.metadata)
            .field("bytes", &format!("<{} bytes>", self.bytes.len()))
            .finish()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EmailAttachmentReadInput {
    pub email_id: EmailId,
    pub provider_attachment_id: String,
}

/// Typed outcomes of an attachment read. `Unavailable` is durable evidence
/// (the provider no longer holds the content); everything else describes
/// this attempt only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmailAttachmentReadError {
    UnknownEmail { message: String },
    UnknownAttachment { message: String },
    Unavailable { message: String },
    RateLimited { message: String },
    QuotaExhausted { message: String },
    AuthenticationLost { message: String },
    TooLarge { message: String },
    Transient { message: String },
    Failed { message: String },
}

impl std::fmt::Display for EmailAttachmentReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (label, message) = match self {
            Self::UnknownEmail { message } => ("unknown email", message),
            Self::UnknownAttachment { message } => ("unknown attachment", message),
            Self::Unavailable { message } => ("unavailable at the provider", message),
            Self::RateLimited { message } => ("rate limited", message),
            Self::QuotaExhausted { message } => ("quota exhausted", message),
            Self::AuthenticationLost { message } => ("authentication lost", message),
            Self::TooLarge { message } => ("too large", message),
            Self::Transient { message } => ("transient failure", message),
            Self::Failed { message } => ("failed", message),
        };
        write!(formatter, "attachment read {label}: {message}")
    }
}

impl From<onecrew_email_contracts::PlatformEmailAttachmentError> for EmailAttachmentReadError {
    fn from(error: onecrew_email_contracts::PlatformEmailAttachmentError) -> Self {
        use onecrew_email_contracts::PlatformEmailAttachmentError as Port;
        match error {
            Port::Unavailable { message } => Self::Unavailable { message },
            Port::RateLimited { message } => Self::RateLimited { message },
            Port::QuotaExhausted { message } => Self::QuotaExhausted { message },
            Port::AuthenticationLost { message } => Self::AuthenticationLost { message },
            Port::TooLarge { message } => Self::TooLarge { message },
            Port::Transient { message } => Self::Transient { message },
            Port::Failed { message } => Self::Failed { message },
        }
    }
}
