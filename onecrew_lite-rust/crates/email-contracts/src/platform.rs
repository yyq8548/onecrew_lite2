//! Provider-neutral runtime gateway projections shared by adapters.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailRequest {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailReceipt {
    pub provider: String,
    pub external_message_id: Option<String>,
    pub accepted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlatformEmailSendError {
    Validation { message: String },
    Rejected { status: u16, message: String },
    Ambiguous { message: String },
}

impl PlatformEmailSendError {
    pub fn message(&self) -> &str {
        match self {
            Self::Validation { message }
            | Self::Rejected { message, .. }
            | Self::Ambiguous { message } => message,
        }
    }

    pub fn is_terminal_rejection(&self) -> bool {
        match self {
            Self::Validation { .. } => true,
            Self::Rejected { status, .. } => (400..500).contains(status),
            Self::Ambiguous { .. } => false,
        }
    }
}

impl std::fmt::Display for PlatformEmailSendError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation { message } => write!(formatter, "email validation failed: {message}"),
            Self::Rejected { status, message } => {
                write!(
                    formatter,
                    "email provider rejected HTTP {status}: {message}"
                )
            }
            Self::Ambiguous { message } => {
                write!(formatter, "email delivery result is ambiguous: {message}")
            }
        }
    }
}

impl std::error::Error for PlatformEmailSendError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailMessage {
    pub provider_message_id: String,
    pub internet_message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub received_for: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub text: Option<String>,
    pub html: Option<String>,
    pub headers: serde_json::Value,
    pub last_event: String,
    pub created_at: String,
    pub inbound: bool,
    pub attachments: Vec<PlatformEmailAttachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailAttachment {
    pub provider_attachment_id: String,
    pub filename: String,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct PlatformAttachmentAccess {
    pub metadata: PlatformEmailAttachment,
    pub download_url: String,
    pub expires_at: String,
}

impl std::fmt::Debug for PlatformAttachmentAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PlatformAttachmentAccess")
            .field("metadata", &self.metadata)
            .field("download_url", &"<redacted signed url>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlatformEmailAttachmentError {
    Unavailable { message: String },
    RateLimited { message: String },
    QuotaExhausted { message: String },
    AuthenticationLost { message: String },
    TooLarge { message: String },
    Transient { message: String },
    Failed { message: String },
}

impl std::fmt::Display for PlatformEmailAttachmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (label, message) = match self {
            Self::Unavailable { message } => ("unavailable", message),
            Self::RateLimited { message } => ("rate limited", message),
            Self::QuotaExhausted { message } => ("quota exhausted", message),
            Self::AuthenticationLost { message } => ("authentication lost", message),
            Self::TooLarge { message } => ("too large", message),
            Self::Transient { message } => ("transient failure", message),
            Self::Failed { message } => ("failed", message),
        };
        write!(formatter, "attachment access {label}: {message}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformSentEmailSummary {
    pub provider_message_id: String,
    pub internet_message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub reply_to: Vec<String>,
    pub subject: String,
    pub last_event: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformSentEmailPage {
    pub messages: Vec<PlatformSentEmailSummary>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub bounds: Option<PlatformPageBounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailSummary {
    pub provider_message_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEmailPage {
    pub messages: Vec<PlatformEmailSummary>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
    pub bounds: Option<PlatformPageBounds>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformPageBounds {
    pub newest_provider_message_id: String,
    pub newest_created_at: String,
    pub oldest_created_at: String,
}
