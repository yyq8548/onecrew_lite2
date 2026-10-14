mod connection;
pub(crate) mod email_gateway;
mod resend;
pub(crate) mod services;
mod youtube;

/// Creator owns mailbox bindings; Email admission obtains one owned snapshot
/// on every call before touching the provider registry or its sender cache.
#[async_trait::async_trait]
pub(crate) trait EmailMailboxAuthResolver: Send + Sync {
    async fn resolve_mailbox_auth_connection(
        &self,
        mailbox_ref_id: &onecrew_creator_runtime::MailboxRefId,
    ) -> Result<onecrew_creator_runtime::MailboxAuthBinding, String>;
}

pub use youtube::{
    YoutubeCredentialAvailability, YoutubeCredentialIssue, YoutubeCredentialRejectionIssue,
    YoutubePlatformDetail, YoutubePlatformGetInput, YoutubePlatformGetOutcome,
    YoutubePlatformTestError, YoutubePlatformTestInput, YoutubePlatformTestResult,
};

use onecrew_creator_runtime::MailboxCredentialDeletionBlockers;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResendCredentialDeletionError {
    InvalidRequest {
        message: String,
    },
    Blocked {
        /// Every mailbox projection mapped to the auth connection is
        /// returned. A single connection may own a mailbox and send-as
        /// aliases, so deletion must not discard secondary blockers.
        blockers: Vec<MailboxCredentialDeletionBlockers>,
    },
    Runtime {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceCredentialMutationError {
    ConfirmationRequired,
    InvalidInput,
    NotFound,
    WrongKind,
    Conflict,
    ValidationRejected,
    Recoverable,
    Internal,
}

impl ServiceCredentialMutationError {
    pub(crate) fn from_safe_message(message: &str) -> Self {
        let normalized = message.to_ascii_lowercase();
        if normalized.contains("explicit confirmation") {
            Self::ConfirmationRequired
        } else if normalized.contains("not found") {
            Self::NotFound
        } else if normalized.contains("not a youtube credential")
            || normalized.contains("not a resend credential")
        {
            Self::WrongKind
        } else if normalized.contains("already exists")
            || normalized.contains("different credential row")
        {
            Self::Conflict
        } else if normalized.contains("invalid credential id")
            || normalized.contains("credential id is invalid")
            || normalized.contains(" is required")
            || normalized.contains("请输入")
        {
            Self::InvalidInput
        } else if normalized.contains("retrying") || normalized.contains("rollback") {
            Self::Recoverable
        } else if normalized.contains("validat")
            || normalized.contains("rejected")
            || normalized.contains("verified")
        {
            Self::ValidationRejected
        } else {
            Self::Internal
        }
    }
}

impl std::fmt::Display for ResendCredentialDeletionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest { message } | Self::Runtime { message } => {
                formatter.write_str(message)
            }
            Self::Blocked { blockers } => {
                let recoveries = blockers
                    .iter()
                    .map(|blocker| blocker.recoveries.len())
                    .sum::<usize>();
                let sending_drafts = blockers
                    .iter()
                    .map(|blocker| blocker.sending_drafts.len())
                    .sum::<usize>();
                let mailbox_ids = blockers
                    .iter()
                    .map(|blocker| blocker.mailbox_ref_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    formatter,
                    "cannot delete Resend credential (mailboxes: {mailbox_ids}) while {recoveries} active email recoveries and {sending_drafts} sending drafts depend on it; recover them with the same credential and idempotency key first (recreating a credential with the same label is not a recovery path)"
                )
            }
        }
    }
}

impl std::error::Error for ResendCredentialDeletionError {}

use onecrew_creator_runtime::ServiceCredentialDeleteResult;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubeCredentialDeleteInput {
    pub credential_id: String,
    pub confirmed: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubeCredentialDeleteOutcome {
    pub credential_id: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeCredentialDeleteError {
    ConfirmationRequired,
    IdempotencyKeyInvalid,
    IdempotencyConflict,
    InvalidCredentialId,
    NotFound,
    WrongKind,
    OutreachInProgress,
    RecoverableSameKeyRetry,
    Internal,
}

impl YoutubeCredentialDeleteError {
    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::ConfirmationRequired => "confirmed=true is required",
            Self::IdempotencyKeyInvalid => "idempotency_key must contain 1 to 128 characters",
            Self::IdempotencyConflict => {
                "idempotency_key was already used for a different YouTube delete intent"
            }
            Self::InvalidCredentialId => "credential_id is invalid",
            Self::NotFound => "YouTube credential was not found",
            Self::WrongKind => "credential_id does not identify a YouTube credential",
            Self::OutreachInProgress => {
                "an outreach action is unresolved; recover it before deleting this credential"
            }
            Self::RecoverableSameKeyRetry => {
                "YouTube deletion is incomplete; retry the same intent with the same idempotency_key"
            }
            Self::Internal => "YouTube deletion is unavailable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCredentialProjection {
    pub account_ref_id: onecrew_creator_runtime::AccountRefId,
    pub provider: String,
    pub label: String,
    pub active: bool,
    pub available: bool,
    pub disabled: bool,
    pub cooldown_until: Option<u64>,
    pub last_error: Option<String>,
}

/// Full YouTube credential connection input. The key is accepted during
/// deserialization but deliberately omitted from Debug and serialization, so
/// command errors, audit payloads and accidental response projection cannot
/// echo it.
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct YoutubeApiKeyConnectInput {
    pub label: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: String,
}

impl fmt::Debug for YoutubeApiKeyConnectInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("YoutubeApiKeyConnectInput")
            .field("label", &self.label)
            .field("project", &self.project)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResendCredentialConnectInput {
    pub label: String,
    pub email_address: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: String,
}

impl fmt::Debug for ResendCredentialConnectInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResendCredentialConnectInput")
            .field("label", &self.label)
            .field("email_address", &self.email_address)
            .field("display_name", &self.display_name)
            .field("api_key", &"[REDACTED]")
            .finish()
    }
}

/// Rotate one existing YouTube credential in place: same label and project,
/// a new validated key. The credential is addressed by its stable id and the
/// rotation must be explicitly confirmed.
#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateYoutubeCredentialInput {
    pub credential_id: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub confirmed: bool,
}

impl fmt::Debug for RotateYoutubeCredentialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RotateYoutubeCredentialInput")
            .field("credential_id", &self.credential_id)
            .field("api_key", &"[REDACTED]")
            .field("confirmed", &self.confirmed)
            .finish()
    }
}

/// Rename one existing YouTube credential without changing its key or
/// provider metadata. The stable credential id remains the Creator reference.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenameYoutubeCredentialInput {
    pub credential_id: String,
    pub label: String,
    pub confirmed: bool,
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RotateResendCredentialInput {
    pub credential_id: String,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub confirmed: bool,
}

impl fmt::Debug for RotateResendCredentialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RotateResendCredentialInput")
            .field("credential_id", &self.credential_id)
            .field("api_key", &"[REDACTED]")
            .field("confirmed", &self.confirmed)
            .finish()
    }
}

/// Guarded Resend credential deletion; idempotency is carried by the
/// persisted mailbox deletion guard plus the stable credential id.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResendCredentialDeleteInput {
    pub credential_id: String,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResendCredentialDeleteOutcome {
    pub credential_id: String,
    pub deleted: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct YoutubeCredentialProjectionDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MailboxCredentialProjectionDetails {
    pub email_address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectedServiceCredentialProjection {
    pub credential_id: String,
    pub provider: String,
    pub label: String,
    /// Typed provider details; exactly one member is present per provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youtube: Option<YoutubeCredentialProjectionDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mailbox: Option<MailboxCredentialProjectionDetails>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCredentialValidationProjection {
    pub checked_at: u64,
    /// Human-readable verification scope. For Resend this distinguishes API
    /// key/Receiving API checks from the separately matched sending domain.
    pub message: String,
    /// YouTube validation channel id or Resend verified sender-domain name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_resource_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCredentialConnectResult {
    pub credential: ConnectedServiceCredentialProjection,
    pub health: ServiceCredentialValidationProjection,
}

#[cfg(test)]
mod mutation_contract_tests {
    use super::*;

    #[test]
    fn transient_keys_are_never_serialized_or_debugged() {
        let input = YoutubeApiKeyConnectInput {
            label: "youtube".into(),
            project: None,
            api_key: "secret-youtube-key".into(),
        };
        assert!(
            !serde_json::to_string(&input)
                .unwrap()
                .contains("secret-youtube-key")
        );
        assert!(!format!("{input:?}").contains("secret-youtube-key"));

        let input = ResendCredentialConnectInput {
            label: "mailbox".into(),
            email_address: "sender@example.com".into(),
            display_name: None,
            api_key: "secret-resend-key".into(),
        };
        assert!(
            !serde_json::to_string(&input)
                .unwrap()
                .contains("secret-resend-key")
        );
        assert!(!format!("{input:?}").contains("secret-resend-key"));
    }

    #[test]
    fn typed_mutation_errors_are_secret_free_categories() {
        assert_eq!(
            ServiceCredentialMutationError::from_safe_message(
                "credential rotation requires explicit confirmation"
            ),
            ServiceCredentialMutationError::ConfirmationRequired
        );
        assert_eq!(
            ServiceCredentialMutationError::from_safe_message(
                "credential is not a Resend credential"
            ),
            ServiceCredentialMutationError::WrongKind
        );
        assert_eq!(
            ServiceCredentialMutationError::from_safe_message(
                "provider validation rejected the credential"
            ),
            ServiceCredentialMutationError::ValidationRejected
        );
    }
}
