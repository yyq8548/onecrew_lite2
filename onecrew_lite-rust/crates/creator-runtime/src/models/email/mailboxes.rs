use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{EmailCapability, EmailDraftId, MailboxRefId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EmailAuthConnectionId(pub String);

impl EmailAuthConnectionId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Derive the stable Resend auth-connection identity from the independent
/// vault secret UUID. The fixed namespace mask keeps the two IDs distinct
/// while allowing migration and lifecycle reconciliation to agree exactly.
pub fn resend_auth_connection_id(secret_id: uuid::Uuid) -> EmailAuthConnectionId {
    const MASK: [u8; 16] = [
        0x52, 0x45, 0x53, 0x45, 0x4e, 0x44, 0x2d, 0x41, 0x55, 0x54, 0x48, 0x2d, 0x43, 0x4f, 0x4e,
        0x4e,
    ];
    let mut bytes = *secret_id.as_bytes();
    for (byte, mask) in bytes.iter_mut().zip(MASK) {
        *byte ^= mask;
    }
    EmailAuthConnectionId::new(uuid::Uuid::from_bytes(bytes).to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct EmailSecretRecordId(pub String);

impl EmailSecretRecordId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Typed provider identity for an Email auth connection and its mailbox
/// projections. `Remote` is an opaque projection owned by the remote control
/// plane; no local OAuth or token material is represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EmailProviderKind {
    Resend,
    Smtp,
    Imap,
    Remote,
}

impl EmailProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resend => "resend",
            Self::Smtp => "smtp",
            Self::Imap => "imap",
            Self::Remote => "remote",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "resend" => Some(Self::Resend),
            "smtp" => Some(Self::Smtp),
            "imap" => Some(Self::Imap),
            "remote" => Some(Self::Remote),
            _ => None,
        }
    }
}

/// Safe public projection of one typed local or opaque remote auth
/// connection. The encrypted secret itself is owned by local-vault (or the
/// remote control plane) and is never part of this DTO.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailAuthConnection {
    pub auth_connection_id: EmailAuthConnectionId,
    pub provider_kind: EmailProviderKind,
    pub secret_record_id: EmailSecretRecordId,
    pub label: String,
    pub available: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Safe identity of one encrypted local-vault record. No secret-bearing value
/// is serialized or exposed by this projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmailSecretRecord {
    pub secret_record_id: EmailSecretRecordId,
    pub provider_kind: EmailProviderKind,
    pub label: String,
    pub available: bool,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailboxProjectionKind {
    Mailbox,
    SendAs,
}

impl MailboxProjectionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mailbox => "mailbox",
            Self::SendAs => "send_as",
        }
    }
}

/// Stable mailbox-to-auth projection. One auth connection may own any number
/// of these rows, including send-as aliases. The checkpoint namespace is an
/// opaque name; provider cursor/history semantics stay below the Creator owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxIdentityMapping {
    pub mailbox_ref_id: MailboxRefId,
    pub auth_connection_id: EmailAuthConnectionId,
    pub provider_kind: EmailProviderKind,
    pub projection_kind: MailboxProjectionKind,
    pub capability_projection_revision: u64,
    pub checkpoint_namespace: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxIdentityMappingInput {
    pub mailbox_ref_id: MailboxRefId,
    pub auth_connection_id: EmailAuthConnectionId,
    pub provider_kind: EmailProviderKind,
    pub secret_record_id: EmailSecretRecordId,
    pub connection_label: String,
    pub projection_kind: MailboxProjectionKind,
    pub capability_projection_revision: u64,
    pub checkpoint_namespace: String,
}

/// Claim for deleting one auth connection. All mapped mailbox guards are
/// acquired atomically; a blocked claim carries every mapped mailbox blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum EmailAuthConnectionDeletionClaim {
    Acquired {
        mailbox_ref_ids: Vec<MailboxRefId>,
    },
    Blocked {
        auth_connection_id: EmailAuthConnectionId,
        blockers: Vec<MailboxCredentialDeletionBlockers>,
    },
}

#[derive(Clone)]
pub(crate) struct MailboxRecord {
    pub id: MailboxRefId,
    pub provider: String,
    pub label: String,
    pub email_address: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<EmailCapability>,
    pub available: bool,
    pub last_error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailboxProvider {
    Resend,
    Gmail,
}

impl MailboxProvider {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Resend => "resend",
            Self::Gmail => "gmail",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MailboxIssue {
    Unavailable,
    RecordedFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxDetail {
    pub mailbox_ref_id: MailboxRefId,
    pub provider: MailboxProvider,
    pub label: String,
    pub email_address: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<EmailCapability>,
    pub available: bool,
    pub issues: Vec<MailboxIssue>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MailboxAuthBinding {
    pub mailbox_ref_id: MailboxRefId,
    pub provider: MailboxProvider,
    pub label: String,
    pub email_address: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<EmailCapability>,
    pub available: bool,
    pub auth_connection: EmailAuthConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum MailboxGetOutcome {
    Found { detail: MailboxDetail },
    NotFound { mailbox_ref_id: MailboxRefId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxGetError {
    LookupUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EmailRecoveryStage {
    InProgress,
    RemoteStarted,
    RemoteAccepted,
    RemoteVerified,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MailboxEmailRecoveryBlocker {
    pub idempotency_key: String,
    pub action: String,
    pub stage: EmailRecoveryStage,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MailboxSendingDraftBlocker {
    pub draft_id: EmailDraftId,
    pub revision: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct MailboxCredentialDeletionBlockers {
    pub mailbox_ref_id: MailboxRefId,
    pub recoveries: Vec<MailboxEmailRecoveryBlocker>,
    pub sending_drafts: Vec<MailboxSendingDraftBlocker>,
}

impl MailboxCredentialDeletionBlockers {
    pub fn is_empty(&self) -> bool {
        self.recoveries.is_empty() && self.sending_drafts.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MailboxRefInput {
    pub id: MailboxRefId,
    pub provider: String,
    pub label: String,
    pub email_address: String,
    pub display_name: Option<String>,
    pub capabilities: Vec<EmailCapability>,
    pub available: bool,
    pub last_error: Option<String>,
}
