//! Typed Resend wire contract: the exact documented response shapes for the
//! sent/received list and retrieve endpoints, attachment access, the provider
//! error-body registry, and rate-limit header parsing.
//!
//! Frozen by
//! `docs/plans/resend-bidirectional-mailbox-sync/evidence/provider-contract-2026-08-24.md`.
//! Required product fields are required here — a provider contract drift
//! fails deserialization loudly instead of being papered over — while
//! additive unknown fields are tolerated.

use std::collections::BTreeMap;

use serde::Deserialize;

use onecrew_email_contracts::{
    ProviderAttachmentAccess, ProviderAttachmentMetadata, ProviderEmailDomain,
    ProviderInboundEmail, ProviderInboundEmailPage, ProviderInboundEmailSummary,
    ProviderRateLimitStatus, ProviderSentEmail, ProviderSentEmailPage, ProviderSentEmailSummary,
};

/// Events Resend documents for `last_event`, and what each one says about a
/// send, are tabled in `onecrew-email-contracts` beside the DTO field that
/// carries them, so a single table serves both this wire contract and the
/// owner that reads delivery. `last_event` stays a plain string on wire and
/// provider-neutral DTOs so an unknown new event degrades to an
/// unknown-event value instead of failing the whole page, unlike the closed
/// `resend-rs` enum.
pub use onecrew_email_contracts::is_documented_sent_event as is_known_email_event;

/// `{ object: "list", has_more, data }` — the only pagination envelope Resend
/// documents. There is no next-cursor field; the caller passes the last
/// item's id as `after`.
#[derive(Debug, Clone, Deserialize)]
pub struct WireList<T> {
    pub object: String,
    pub has_more: bool,
    pub data: Vec<T>,
}

pub trait WireListItem {
    fn provider_id(&self) -> &str;
}

impl<T: WireListItem> WireList<T> {
    /// The `after` cursor for the next page: the last item's id, only while
    /// the provider reports more data.
    pub fn next_after(&self) -> Option<&str> {
        if self.has_more {
            self.data.last().map(WireListItem::provider_id)
        } else {
            None
        }
    }
}

/// One `GET /emails` list item. No body fields exist on the wire here; the
/// nullable-array shape for `cc`/`bcc`/`reply_to` follows the documented
/// `string | null` vs array inconsistency recorded in the contract snapshot.
#[derive(Debug, Clone, Deserialize)]
pub struct WireSentEmailSummary {
    pub id: String,
    pub message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Option<Vec<String>>,
    pub bcc: Option<Vec<String>>,
    pub reply_to: Option<Vec<String>>,
    pub subject: String,
    pub last_event: String,
    pub created_at: String,
    pub scheduled_at: Option<String>,
}

impl WireListItem for WireSentEmailSummary {
    fn provider_id(&self) -> &str {
        &self.id
    }
}

/// `GET /emails/:id`. Adds the body; still no headers, `In-Reply-To`, or
/// `References` — the provider cannot reconstruct sent reply parentage.
#[derive(Debug, Clone, Deserialize)]
pub struct WireSentEmail {
    pub id: String,
    pub message_id: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Option<Vec<String>>,
    pub bcc: Option<Vec<String>>,
    pub reply_to: Option<Vec<String>>,
    pub subject: String,
    pub html: Option<String>,
    pub text: Option<String>,
    pub last_event: String,
    pub created_at: String,
    pub scheduled_at: Option<String>,
}

/// One `GET /emails/receiving` list item. `message_id` and attachment
/// metadata are list-sufficient facts; the body and headers are not. Live
/// wire evidence (2026-08-24): recipient lists, `received_for`, and
/// attachment metadata are simply omitted when empty, so mailbox routing
/// must not assume their presence on list items.
#[derive(Debug, Clone, Deserialize)]
pub struct WireInboundEmailSummary {
    pub id: String,
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    // Live wire evidence (2026-08-24) matching the SDK's own tolerances:
    // these lists and the attachment metadata are omitted when empty, so
    // absent means empty — that is the wire contract, not a shim.
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    #[serde(default)]
    pub reply_to: Vec<String>,
    #[serde(default)]
    pub received_for: Vec<String>,
    pub subject: String,
    pub created_at: String,
    #[serde(default)]
    pub attachments: Vec<WireAttachmentMetadata>,
}

impl WireListItem for WireInboundEmailSummary {
    fn provider_id(&self) -> &str {
        &self.id
    }
}

/// `GET /emails/receiving/:id`. `headers` is a name -> value map on the
/// wire; repeated header names collapse, so `Received` chains are not
/// reliable here — `In-Reply-To`/`References` are.
#[derive(Debug, Clone, Deserialize)]
pub struct WireInboundEmail {
    pub id: String,
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)]
    pub bcc: Vec<String>,
    #[serde(default)]
    pub reply_to: Vec<String>,
    #[serde(default)]
    pub received_for: Vec<String>,
    pub subject: String,
    pub created_at: String,
    pub html: Option<String>,
    pub html_format: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    pub raw: Option<WireRawAccess>,
    #[serde(default)]
    pub attachments: Vec<WireAttachmentMetadata>,
}

/// Expiring signed access to the raw `.eml`. Deserialize-only and redacted in
/// `Debug`: the URL is a temporary capability and must never be persisted,
/// logged, or projected.
#[derive(Clone, Deserialize)]
pub struct WireRawAccess {
    pub download_url: String,
    pub expires_at: String,
}

impl std::fmt::Debug for WireRawAccess {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireRawAccess")
            .field("download_url", &"<redacted signed url>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// Attachment metadata as embedded in received list/detail responses. Carries
/// no URL; durable-safe.
#[derive(Debug, Clone, Deserialize)]
pub struct WireAttachmentMetadata {
    pub id: String,
    /// Unnamed inline attachments carry no filename on the wire.
    pub filename: Option<String>,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size: Option<u64>,
}

/// A fetched attachment capability: sent attachments list them directly
/// (`GET /emails/:email_id/attachments`), received attachments through
/// `GET /emails/receiving/:email_id/attachments/:id`. Signed URL, valid one
/// hour; Deserialize-only and redacted in `Debug` for the same reason as
/// [`WireRawAccess`].
#[derive(Clone, Deserialize)]
pub struct WireAttachment {
    pub id: String,
    pub filename: Option<String>,
    pub content_type: String,
    pub content_id: Option<String>,
    pub content_disposition: Option<String>,
    pub size: Option<u64>,
    pub download_url: String,
    pub expires_at: String,
}

impl WireListItem for WireAttachment {
    fn provider_id(&self) -> &str {
        &self.id
    }
}

impl std::fmt::Debug for WireAttachment {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WireAttachment")
            .field("id", &self.id)
            .field("filename", &self.filename)
            .field("content_type", &self.content_type)
            .field("download_url", &"<redacted signed url>")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// The documented Resend error body.
#[derive(Debug, Clone, Deserialize)]
pub struct WireErrorBody {
    #[serde(rename = "statusCode")]
    pub status_code: u16,
    pub message: String,
    pub name: String,
}

/// Product-facing classification over the official error-name registry
/// (contract snapshot §Rate limits and quota). Daily/monthly quota is
/// deliberately distinct from a transient rate limit: quota exhaustion must
/// never be retried as if it were per-second throttling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireErrorClass {
    Auth,
    RateLimited,
    DailyQuotaExceeded,
    MonthlyQuotaExceeded,
    Validation,
    NotFound,
    /// `invalid_idempotent_request`: same key, different payload — a
    /// genuinely terminal caller error.
    IdempotencyPayloadMismatch,
    /// `concurrent_idempotent_requests`: the same key is processing right
    /// now, so the twin request may still deliver. Contract: safe to retry
    /// later with the same key — never a definite rejection.
    IdempotencyRace,
    ProviderFault,
    Unrecognized,
}

impl WireErrorBody {
    pub fn classify(&self) -> WireErrorClass {
        match self.name.as_str() {
            "missing_api_key" | "invalid_api_key" | "restricted_api_key" | "invalid_access" => {
                WireErrorClass::Auth
            }
            "rate_limit_exceeded" => WireErrorClass::RateLimited,
            "daily_quota_exceeded" => WireErrorClass::DailyQuotaExceeded,
            "monthly_quota_exceeded" => WireErrorClass::MonthlyQuotaExceeded,
            "validation_error"
            | "invalid_parameter"
            | "invalid_region"
            | "missing_required_field"
            | "invalid_attachment"
            | "invalid_from_address"
            | "invalid_idempotency_key"
            | "method_not_allowed" => WireErrorClass::Validation,
            "not_found" => WireErrorClass::NotFound,
            "invalid_idempotent_request" => WireErrorClass::IdempotencyPayloadMismatch,
            "concurrent_idempotent_requests" => WireErrorClass::IdempotencyRace,
            "internal_server_error" | "application_error" | "security_error" => {
                WireErrorClass::ProviderFault
            }
            _ => WireErrorClass::Unrecognized,
        }
    }

    /// A bare 429 with no readable body classifies by status alone; a bare
    /// 5xx is a provider fault. Everything else without a name stays
    /// unrecognized rather than guessed.
    pub fn classify_status_only(status: u16) -> WireErrorClass {
        match status {
            401 | 403 => WireErrorClass::Auth,
            404 => WireErrorClass::NotFound,
            429 => WireErrorClass::RateLimited,
            500..=599 => WireErrorClass::ProviderFault,
            _ => WireErrorClass::Unrecognized,
        }
    }
}

impl WireErrorClass {
    /// Whether bounded retry with backoff is legitimate. Quota exhaustion,
    /// authentication loss, validation, and idempotency payload mismatches
    /// are not transient and must surface instead of being retried. An
    /// idempotency race retries with the SAME key: the provider either
    /// replays the original response or reports the twin's outcome.
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::ProviderFault | Self::IdempotencyRace
        )
    }
}

impl From<WireErrorClass> for onecrew_email_contracts::ProviderErrorClass {
    fn from(value: WireErrorClass) -> Self {
        match value {
            WireErrorClass::Auth => Self::Auth,
            WireErrorClass::RateLimited => Self::RateLimited,
            WireErrorClass::DailyQuotaExceeded => Self::DailyQuotaExceeded,
            WireErrorClass::MonthlyQuotaExceeded => Self::MonthlyQuotaExceeded,
            WireErrorClass::Validation => Self::Validation,
            WireErrorClass::NotFound => Self::NotFound,
            WireErrorClass::IdempotencyPayloadMismatch => Self::IdempotencyPayloadMismatch,
            WireErrorClass::IdempotencyRace => Self::IdempotencyRace,
            WireErrorClass::ProviderFault => Self::ProviderFault,
            WireErrorClass::Unrecognized => Self::Unrecognized,
        }
    }
}

/// Parses the four documented rate-limit headers from any header iterator,
/// case-insensitively. Sync read 2xx responses require the three window
/// headers; retry-after is optional and applies only to 429 responses.
pub fn parse_rate_limit_headers<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<ProviderRateLimitStatus, String> {
    parse_rate_limit_headers_impl(headers, true)
}

/// Lenient parser used for non-2xx responses so provider-directed 429 retry
/// metadata can still be honored when a proxy omits one of the window fields.
pub fn parse_rate_limit_headers_lenient<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> ProviderRateLimitStatus {
    parse_rate_limit_headers_impl(headers, false).unwrap_or(ProviderRateLimitStatus {
        limit: None,
        remaining: None,
        reset_seconds: None,
        retry_after_seconds: None,
    })
}

fn parse_rate_limit_headers_impl<'a>(
    headers: impl IntoIterator<Item = (&'a str, &'a str)>,
    required_window: bool,
) -> Result<ProviderRateLimitStatus, String> {
    let mut status = ProviderRateLimitStatus {
        limit: None,
        remaining: None,
        reset_seconds: None,
        retry_after_seconds: None,
    };
    let mut invalid = Vec::new();
    for (name, value) in headers {
        let value = value.trim();
        match name.to_ascii_lowercase().as_str() {
            "ratelimit-limit" => match value.parse::<u64>() {
                Ok(value) if value > 0 => status.limit = Some(value),
                _ => invalid.push("ratelimit-limit"),
            },
            "ratelimit-remaining" => match value.parse::<u64>() {
                Ok(value) => status.remaining = Some(value),
                _ => invalid.push("ratelimit-remaining"),
            },
            "ratelimit-reset" => match value.parse::<u64>() {
                Ok(value) => status.reset_seconds = Some(value),
                _ => invalid.push("ratelimit-reset"),
            },
            // retry-after is a 429-only hint; malformed/absent values on a
            // successful response do not invalidate the required window.
            "retry-after" => status.retry_after_seconds = value.parse::<u64>().ok(),
            _ => {}
        }
    }
    if required_window {
        if !invalid.is_empty() {
            return Err(format!(
                "invalid Resend rate-limit headers: {}",
                invalid.join(", ")
            ));
        }
        let missing = [
            ("ratelimit-limit", status.limit.is_none()),
            ("ratelimit-remaining", status.remaining.is_none()),
            ("ratelimit-reset", status.reset_seconds.is_none()),
        ]
        .into_iter()
        .filter_map(|(name, missing)| missing.then_some(name))
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "missing Resend rate-limit headers: {}",
                missing.join(", ")
            ));
        }
    }
    Ok(status)
}

impl From<WireSentEmailSummary> for ProviderSentEmailSummary {
    fn from(wire: WireSentEmailSummary) -> Self {
        Self {
            provider_message_id: wire.id,
            internet_message_id: wire.message_id,
            from: wire.from,
            to: wire.to,
            cc: wire.cc.unwrap_or_default(),
            bcc: wire.bcc.unwrap_or_default(),
            reply_to: wire.reply_to.unwrap_or_default(),
            subject: wire.subject,
            last_event: wire.last_event,
            created_at: wire.created_at,
            scheduled_at: wire.scheduled_at,
        }
    }
}

impl From<WireList<WireSentEmailSummary>> for ProviderSentEmailPage {
    fn from(wire: WireList<WireSentEmailSummary>) -> Self {
        let next_cursor = wire.next_after().map(str::to_owned);
        Self {
            has_more: wire.has_more,
            next_cursor,
            messages: wire.data.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireAttachmentMetadata> for ProviderAttachmentMetadata {
    fn from(wire: WireAttachmentMetadata) -> Self {
        Self {
            provider_attachment_id: wire.id,
            filename: wire.filename.unwrap_or_default(),
            content_type: wire.content_type,
            content_id: wire.content_id,
            content_disposition: wire.content_disposition,
            size: wire.size,
        }
    }
}

/// One `GET /domains` list item, reduced to the fields the product projects.
#[derive(Debug, Clone, Deserialize)]
pub struct WireDomain {
    pub name: String,
    pub status: String,
    pub capabilities: WireDomainCapabilities,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WireDomainCapabilities {
    pub sending: String,
    pub receiving: String,
}

impl WireListItem for WireDomain {
    fn provider_id(&self) -> &str {
        &self.name
    }
}

impl From<WireDomain> for ProviderEmailDomain {
    fn from(wire: WireDomain) -> Self {
        Self {
            name: wire.name.to_ascii_lowercase(),
            verified: wire.status == "verified",
            sending_enabled: wire.capabilities.sending == "enabled",
            receiving_enabled: wire.capabilities.receiving == "enabled",
        }
    }
}

impl From<WireSentEmail> for ProviderSentEmail {
    fn from(wire: WireSentEmail) -> Self {
        Self {
            provider_message_id: wire.id,
            internet_message_id: wire.message_id,
            from: wire.from,
            to: wire.to,
            cc: wire.cc.unwrap_or_default(),
            bcc: wire.bcc.unwrap_or_default(),
            reply_to: wire.reply_to.unwrap_or_default(),
            subject: wire.subject,
            text: wire.text,
            html: wire.html,
            last_event: wire.last_event,
            created_at: wire.created_at,
        }
    }
}

impl From<WireInboundEmailSummary> for ProviderInboundEmailSummary {
    fn from(wire: WireInboundEmailSummary) -> Self {
        Self {
            provider_message_id: wire.id,
            internet_message_id: wire.message_id,
            from: wire.from,
            to: wire.to,
            cc: wire.cc,
            bcc: wire.bcc,
            received_for: wire.received_for,
            subject: wire.subject,
            created_at: wire.created_at,
            attachments: wire.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireList<WireInboundEmailSummary>> for ProviderInboundEmailPage {
    fn from(wire: WireList<WireInboundEmailSummary>) -> Self {
        let next_cursor = wire.next_after().map(str::to_owned);
        Self {
            has_more: wire.has_more,
            next_cursor,
            messages: wire.data.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireInboundEmail> for ProviderInboundEmail {
    fn from(wire: WireInboundEmail) -> Self {
        Self {
            provider_message_id: wire.id,
            internet_message_id: wire.message_id,
            from: wire.from,
            to: wire.to,
            cc: wire.cc,
            bcc: wire.bcc,
            received_for: wire.received_for,
            reply_to: wire.reply_to,
            subject: wire.subject,
            text: wire.text,
            html: wire.html,
            headers: serde_json::Value::Object(
                wire.headers
                    .into_iter()
                    .map(|(name, value)| (name, serde_json::Value::String(value)))
                    .collect(),
            ),
            created_at: wire.created_at,
            attachments: wire.attachments.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<WireAttachment> for ProviderAttachmentAccess {
    fn from(wire: WireAttachment) -> Self {
        Self {
            metadata: ProviderAttachmentMetadata {
                provider_attachment_id: wire.id,
                filename: wire.filename.unwrap_or_default(),
                content_type: wire.content_type,
                content_id: wire.content_id,
                content_disposition: wire.content_disposition,
                size: wire.size,
            },
            download_url: wire.download_url,
            expires_at: wire.expires_at,
        }
    }
}
