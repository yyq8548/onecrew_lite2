use crate::{
    ActionFailure, ContactChannel, CreatorContactRouteRecord, EmailActionKind, EmailCapability,
    LocatorKind, MailboxDetail, MailboxProvider, RemoteAcceptedEmailAction,
    RemoteAcceptedEmailContext, RemoteEmailRecovery, RemoteStartedEmailAction,
};
use onecrew_email_contracts::PlatformEmailRequest;

use super::message::normalize_email_address;
pub(super) fn canonical_email_body(
    subject: &str,
    text: &str,
    html: &Option<String>,
) -> Result<(String, Option<String>, Option<String>), String> {
    let subject = subject.trim().to_owned();
    let text = text.trim();
    let html = html
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if subject.is_empty() {
        return Err("email subject is required".into());
    }
    if text.is_empty() && html.is_none() {
        return Err("email text or html body is required".into());
    }
    Ok((subject, (!text.is_empty()).then(|| text.to_owned()), html))
}

pub(crate) fn canonical_email_request_fingerprint(
    request: &PlatformEmailRequest,
) -> Result<String, String> {
    let mut canonical = request.clone();
    canonical.idempotency_key.clear();
    serde_json::to_string(&canonical).map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Copy)]
pub(super) enum RemoteEmailRecoveryStage {
    Started,
    Accepted,
}

impl RemoteEmailRecoveryStage {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Started => "remote-started",
            Self::Accepted => "remote-accepted",
        }
    }
}

#[derive(Debug)]
pub(super) enum EmailRecoveryValidationError {
    MissingProviderMessageId,
    MissingRequestFingerprint(RemoteEmailRecoveryStage),
    InvalidIdempotencyKey,
    InvalidCanonicalRequest(String),
    CanonicalRequestFingerprintMismatch(RemoteEmailRecoveryStage),
    RecipientMismatch(RemoteEmailRecoveryStage),
    ContextMismatch {
        stage: RemoteEmailRecoveryStage,
        kind: &'static str,
    },
    InvalidDraftFingerprint(String),
}

impl std::fmt::Display for EmailRecoveryValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingProviderMessageId => formatter
                .write_str("remote-accepted email recovery is missing its provider message id"),
            Self::MissingRequestFingerprint(stage) => write!(
                formatter,
                "{} email recovery is missing its request fingerprint",
                stage.as_str()
            ),
            Self::InvalidIdempotencyKey => formatter.write_str("idempotency_key is required"),
            Self::InvalidCanonicalRequest(error) | Self::InvalidDraftFingerprint(error) => {
                formatter.write_str(error)
            }
            Self::CanonicalRequestFingerprintMismatch(stage) => write!(
                formatter,
                "{} email canonical request fingerprint mismatch",
                stage.as_str()
            ),
            Self::RecipientMismatch(stage) => write!(
                formatter,
                "{} email recipient does not match its request",
                stage.as_str()
            ),
            Self::ContextMismatch { stage, kind } => write!(
                formatter,
                "{} {kind} email recovery context mismatch",
                stage.as_str()
            ),
        }
    }
}

impl std::error::Error for EmailRecoveryValidationError {}

impl From<EmailRecoveryValidationError> for String {
    fn from(error: EmailRecoveryValidationError) -> Self {
        error.to_string()
    }
}

impl RemoteEmailRecovery {
    fn validate(
        &self,
        stage: RemoteEmailRecoveryStage,
    ) -> Result<(), EmailRecoveryValidationError> {
        if self.request_fingerprint.trim().is_empty() {
            return Err(EmailRecoveryValidationError::MissingRequestFingerprint(
                stage,
            ));
        }
        if self.request.idempotency_key.trim().is_empty() {
            return Err(EmailRecoveryValidationError::InvalidIdempotencyKey);
        }
        let canonical = canonical_email_request_fingerprint(&self.request)
            .map_err(EmailRecoveryValidationError::InvalidCanonicalRequest)?;
        if canonical != self.canonical_request_fingerprint {
            return Err(EmailRecoveryValidationError::CanonicalRequestFingerprintMismatch(stage));
        }
        let recipient_matches = self.request.to.iter().any(|recipient| {
            crate::EmailAddress::parse(recipient)
                .is_ok_and(|candidate| candidate == self.expected_recipient)
        });
        if !recipient_matches {
            return Err(EmailRecoveryValidationError::RecipientMismatch(stage));
        }
        match &self.context {
            RemoteAcceptedEmailContext::Creator { preparation } => {
                if self.action != EmailActionKind::Creator
                    || preparation.mailbox.mailbox_ref_id != self.mailbox.id
                    || preparation.route.locator.normalized_value
                        != self.expected_recipient.as_str()
                {
                    return Err(EmailRecoveryValidationError::ContextMismatch {
                        stage,
                        kind: "Creator",
                    });
                }
            }
            RemoteAcceptedEmailContext::Manual { preparation } => {
                if self.action != EmailActionKind::Manual
                    || preparation.mailbox.mailbox_ref_id != self.mailbox.id
                    || preparation.to != self.expected_recipient.as_str()
                {
                    return Err(EmailRecoveryValidationError::ContextMismatch {
                        stage,
                        kind: "manual",
                    });
                }
            }
            RemoteAcceptedEmailContext::Draft {
                draft_id,
                expected_revision,
                ..
            } => {
                let fingerprint: crate::EmailDraftSendFingerprint =
                    serde_json::from_str(&self.request_fingerprint).map_err(|error| {
                        EmailRecoveryValidationError::InvalidDraftFingerprint(error.to_string())
                    })?;
                if self.action != EmailActionKind::Draft
                    || fingerprint.mailbox_ref_id != self.mailbox.id.as_str()
                    || fingerprint.draft_id != draft_id.as_str()
                    || fingerprint.expected_revision != *expected_revision
                {
                    return Err(EmailRecoveryValidationError::ContextMismatch {
                        stage,
                        kind: "draft",
                    });
                }
            }
        }
        Ok(())
    }
}

pub(super) fn validate_remote_started_email_action(
    recovery: &RemoteStartedEmailAction,
) -> Result<(), EmailRecoveryValidationError> {
    recovery.validate(RemoteEmailRecoveryStage::Started)
}

pub(super) fn validate_remote_accepted_email_action(
    recovery: &RemoteAcceptedEmailAction,
) -> Result<(), EmailRecoveryValidationError> {
    if recovery.provider_message_id.trim().is_empty() {
        return Err(EmailRecoveryValidationError::MissingProviderMessageId);
    }
    recovery.validate(RemoteEmailRecoveryStage::Accepted)
}

pub(crate) fn ensure_email_route(route: &CreatorContactRouteRecord) -> Result<(), String> {
    if route.locator.channel != ContactChannel::Email
        || route.locator.kind != LocatorKind::EmailAddress
    {
        return Err("selected contact route is not an email address".into());
    }
    normalize_email_address(&route.locator.normalized_value).map(|_| ())
}

pub(crate) fn ensure_mailbox(
    mailbox: &MailboxDetail,
    capability: EmailCapability,
) -> Result<(), String> {
    if !matches!(mailbox.provider, MailboxProvider::Resend | MailboxProvider::Gmail) {
        return Err("unsupported mailbox provider".into());
    }
    if !mailbox.available {
        return Err("mailbox is unavailable".into());
    }
    if !mailbox.capabilities.contains(&capability) {
        return Err(format!("mailbox does not support {}", capability.as_str()));
    }
    Ok(())
}

pub(crate) fn validate_idempotency_key(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err("idempotency_key is required".into())
    } else {
        Ok(())
    }
}

pub(crate) fn failed_replay(value: serde_json::Value) -> String {
    serde_json::from_value::<ActionFailure>(value)
        .map(|failure| serde_json::to_string(&failure).unwrap_or(failure.message))
        .unwrap_or_else(|error| format!("stored action failure is invalid: {error}"))
}
