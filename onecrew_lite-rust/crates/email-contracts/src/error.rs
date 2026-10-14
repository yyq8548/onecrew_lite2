use thiserror::Error;

use crate::models::ProviderRateLimitStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorClass {
    Auth,
    RateLimited,
    DailyQuotaExceeded,
    MonthlyQuotaExceeded,
    Validation,
    NotFound,
    IdempotencyPayloadMismatch,
    IdempotencyRace,
    ProviderFault,
    Unrecognized,
}

impl ProviderErrorClass {
    pub const fn is_transient(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::ProviderFault | Self::IdempotencyRace
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailure {
    pub status: u16,
    pub class: ProviderErrorClass,
    pub message: String,
    pub rate_limit: ProviderRateLimitStatus,
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum EmailError {
    #[error("invalid email provider configuration: {0}")]
    InvalidConfig(String),
    #[error("invalid outbound email: {0}")]
    InvalidMessage(String),
    #[error("email provider capability is unavailable: {0}")]
    UnsupportedCapability(String),
    /// The request never produced a decodable provider response. For a send
    /// this is an ambiguous outcome.
    #[error("email provider request failed: {0}")]
    Transport(String),
    #[error("email provider response for {path} exceeded {limit} bytes")]
    OversizedResponse { path: String, limit: usize },
    #[error("email provider returned HTTP {}: {} ({:?})", .0.status, .0.message, .0.class)]
    Provider(ProviderFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSendFailureKind {
    Validation,
    Rejected { status: u16 },
    Ambiguous,
}

impl EmailError {
    pub fn send_failure_kind(&self) -> EmailSendFailureKind {
        match self {
            Self::InvalidConfig(_) | Self::InvalidMessage(_) | Self::UnsupportedCapability(_) => {
                EmailSendFailureKind::Validation
            }
            Self::Provider(failure) if failure.class == ProviderErrorClass::IdempotencyRace => {
                EmailSendFailureKind::Ambiguous
            }
            Self::Provider(failure) if (400..500).contains(&failure.status) => {
                EmailSendFailureKind::Rejected {
                    status: failure.status,
                }
            }
            Self::Transport(_) | Self::OversizedResponse { .. } | Self::Provider(_) => {
                EmailSendFailureKind::Ambiguous
            }
        }
    }

    pub fn is_transient(&self) -> bool {
        match self {
            Self::Provider(failure) => failure.class.is_transient(),
            Self::Transport(_) => true,
            _ => false,
        }
    }

    pub fn provider_failure(&self) -> Option<&ProviderFailure> {
        match self {
            Self::Provider(failure) => Some(failure),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EmailError, EmailSendFailureKind, ProviderErrorClass, ProviderFailure};
    use crate::ProviderRateLimitStatus;

    fn provider(status: u16, class: ProviderErrorClass) -> EmailError {
        EmailError::Provider(ProviderFailure {
            status,
            class,
            message: "test".into(),
            rate_limit: ProviderRateLimitStatus {
                limit: None,
                remaining: None,
                reset_seconds: None,
                retry_after_seconds: None,
            },
        })
    }

    #[test]
    fn send_failure_kind_preserves_rejections_and_ambiguous_delivery() {
        assert_eq!(
            EmailError::InvalidMessage("bad".into()).send_failure_kind(),
            EmailSendFailureKind::Validation
        );
        assert_eq!(
            provider(422, ProviderErrorClass::Validation).send_failure_kind(),
            EmailSendFailureKind::Rejected { status: 422 }
        );
        assert_eq!(
            provider(503, ProviderErrorClass::ProviderFault).send_failure_kind(),
            EmailSendFailureKind::Ambiguous
        );
    }

    #[test]
    fn transient_classes_exclude_quota_auth_and_validation() {
        assert!(provider(429, ProviderErrorClass::RateLimited).is_transient());
        assert!(provider(500, ProviderErrorClass::ProviderFault).is_transient());
        assert!(!provider(429, ProviderErrorClass::DailyQuotaExceeded).is_transient());
        assert!(!provider(401, ProviderErrorClass::Auth).is_transient());
        assert!(!provider(422, ProviderErrorClass::Validation).is_transient());
    }
}
