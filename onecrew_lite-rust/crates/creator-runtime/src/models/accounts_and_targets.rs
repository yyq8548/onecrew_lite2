use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AccountRefId, ContactLocatorId, CreatorId, IdentityConflictId, LocatorResolutionId,
    PlatformAccountId, ResolvedTargetId,
    core::{ConflictState, Platform, ResolutionState, TargetKind},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Profile,
    ServiceCredential,
}

impl AccountKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Profile => "profile",
            Self::ServiceCredential => "service_credential",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "profile" => Ok(Self::Profile),
            "service_credential" => Ok(Self::ServiceCredential),
            _ => Err(format!("unknown account kind: {value}")),
        }
    }
}

impl Platform {
    pub const fn required_account_kind(self) -> AccountKind {
        match self {
            Self::YouTube => AccountKind::ServiceCredential,
            Self::X | Self::Instagram | Self::Telegram | Self::TikTok => AccountKind::Profile,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccountAvailability {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountLifecycleClaimError {
    IdempotencyConflict,
    WrongKind,
    OutreachInProgress,
    Internal,
}

impl AccountAvailability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "available" => Ok(Self::Available),
            "unavailable" => Ok(Self::Unavailable),
            _ => Err(format!("unknown account availability: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AccountRef {
    pub id: AccountRefId,
    pub platform: Platform,
    pub kind: AccountKind,
    pub label: String,
    pub active: bool,
    pub availability: AccountAvailability,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountDisconnectInput {
    pub account_ref_id: AccountRefId,
    pub confirmed: bool,
    pub idempotency_key: String,
}

/// Durable recovery state of one operator disconnect claim. Disconnecting
/// only deactivates the account reference; no local record depends on it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OperatorAccountDisconnectRecovery {
    pub account_ref_id: AccountRefId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServiceCredentialDeleteInput {
    pub account_ref_id: AccountRefId,
    pub confirmed: bool,
    pub idempotency_key: String,
}

/// Durable recovery state of one service credential deletion claim.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCredentialDeleteRecovery {
    pub account_ref_id: AccountRefId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServiceCredentialDeleteResult {
    pub account_ref: AccountRef,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum ServiceCredentialDeleteClaim {
    Acquired(ServiceCredentialDeleteRecovery),
    RemoteVerified(ServiceCredentialDeleteRecovery),
    Completed(ServiceCredentialDeleteResult),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OperatorAccountDisconnectResult {
    pub account_ref: AccountRef,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub enum OperatorAccountDisconnectClaim {
    Acquired(OperatorAccountDisconnectRecovery),
    RemoteStarted(OperatorAccountDisconnectRecovery),
    RemoteVerified(OperatorAccountDisconnectRecovery),
    Completed(OperatorAccountDisconnectResult),
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedTarget {
    pub id: ResolvedTargetId,
    pub account_ref_id: AccountRefId,
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_target_id: String,
    pub platform_account_id: Option<PlatformAccountId>,
    pub metadata: serde_json::Value,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocatorResolution {
    pub id: LocatorResolutionId,
    pub locator_id: ContactLocatorId,
    pub account_ref_id: AccountRefId,
    pub resolved_target_id: ResolvedTargetId,
    pub state: ResolutionState,
    pub resolved_at: u64,
    pub invalidated_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedLocatorTarget {
    pub resolution: LocatorResolution,
    pub target: ResolvedTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IdentityConflict {
    pub id: IdentityConflictId,
    pub conflict_key: String,
    pub kind: String,
    pub creator_id: Option<CreatorId>,
    pub platform_account_id: Option<PlatformAccountId>,
    pub locator_id: Option<ContactLocatorId>,
    pub resolved_target_id: Option<ResolvedTargetId>,
    pub evidence: serde_json::Value,
    pub state: ConflictState,
    pub created_at: u64,
    pub resolved_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LocatorResolutionWriteResult {
    Active {
        value: ResolvedLocatorTarget,
    },
    Conflict {
        conflict: IdentityConflict,
        active: Box<ResolvedLocatorTarget>,
        attempted_target: ResolvedTarget,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitContactTargetResolutionInput {
    pub creator_id: CreatorId,
    pub contact_route_id: crate::CreatorContactRouteId,
    pub locator_id: ContactLocatorId,
    pub account_ref_id: AccountRefId,
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_target_id: String,
    pub attach_to_creator: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CommitContactTargetResolutionResult {
    Active {
        value: ResolvedLocatorTarget,
    },
    LocatorConflict {
        conflict: IdentityConflict,
        active: Box<ResolvedLocatorTarget>,
        attempted_target: ResolvedTarget,
    },
    CreatorConflict {
        conflict: IdentityConflict,
        creator_ids: Vec<CreatorId>,
        attempted_target: ResolvedTarget,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformResolvedTarget {
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_target_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub canonical_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlatformTargetResolution {
    Resolved {
        target: PlatformResolvedTarget,
    },
    RequiresOperatorAuth {
        reason: String,
    },
    BrowserOnly {
        canonical_url: Option<String>,
        reason: String,
    },
    Unsupported {
        reason: String,
    },
    NotFound {
        reason: String,
    },
}
