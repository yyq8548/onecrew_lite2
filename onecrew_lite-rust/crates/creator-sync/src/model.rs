//! Secret-free public Creator Sync inputs, states, and results.

use onecrew_local_domain::CreatorSyncCredentialMetadata;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncPairingError {
    Unconfigured,
    AlreadyActive,
    PairingAlreadyPending,
    NoPendingPairing,
    InvalidPairingCode,
    InvalidPairingState,
    InvalidAppVersion,
    InvalidCallbackPort,
    InvalidFlowId,
    InvalidFlow,
    StateMismatch,
    Expired,
    Cancelled,
    Replayed,
    OwnerChanged,
    PairingRejected,
    RateLimited,
    RemoteUnavailable,
    AdmissionValidationRequired,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncAdmissionState {
    Unconfigured,
    PairingRequired,
    PairingPending,
    PairingCompleting,
    ValidationRequired,
    Validating,
    Active,
    Revoked,
    Unauthorized,
    RecoveryRequired,
    TemporarilyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncAction {
    BeginPairing,
    AwaitPairingCallback,
    CancelPairing,
    ValidateAdmission,
    Retry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncIssue {
    ConfigurationUnavailable,
    CredentialUnavailable,
    PairingRejected,
    PairingExpired,
    PairingCancelled,
    AdmissionValidationRequired,
    SafeCleanupIncomplete,
    EnrollmentUnknown,
    VerificationFailed,
    RemoteUnavailable,
    RemoteRateLimited,
    RemoteUnauthorized,
    RemoteRevoked,
    ProtocolMismatch,
    LocalVaultUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSyncStatus {
    pub state: CreatorSyncAdmissionState,
    pub action: Option<CreatorSyncAction>,
    pub issue: Option<CreatorSyncIssue>,
    pub installation_id: Option<String>,
    pub credential_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSnapshotSyncState {
    Quarantined,
    NoChanges,
    AlreadyRunning,
    NotPaired,
    ValidationRequired,
    Revoked,
    Unauthorized,
    TemporarilyUnavailable,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorSnapshotSyncResult {
    pub state: CreatorSnapshotSyncState,
    pub creator_count: Option<u32>,
    pub content_sha256: Option<String>,
    pub snapshot_id: Option<String>,
    pub replayed: bool,
    pub issue: Option<CreatorSyncIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSyncSetupError {
    LocalVaultUnavailable,
    HttpClientUnavailable,
}

pub(crate) fn unconfigured_status() -> CreatorSyncStatus {
    status(
        CreatorSyncAdmissionState::Unconfigured,
        Some(CreatorSyncAction::Retry),
        Some(CreatorSyncIssue::ConfigurationUnavailable),
        None,
    )
}

pub(crate) fn active_status(metadata: &CreatorSyncCredentialMetadata) -> CreatorSyncStatus {
    status(
        CreatorSyncAdmissionState::Active,
        None,
        None,
        Some((&metadata.installation_id, &metadata.credential_id)),
    )
}

pub(crate) fn status_for_remote_override(
    state: CreatorSyncAdmissionState,
    cleanup_incomplete: bool,
) -> CreatorSyncStatus {
    let issue = if cleanup_incomplete {
        CreatorSyncIssue::SafeCleanupIncomplete
    } else {
        match state {
            CreatorSyncAdmissionState::Revoked => CreatorSyncIssue::RemoteRevoked,
            _ => CreatorSyncIssue::RemoteUnauthorized,
        }
    };
    status(
        state,
        Some(CreatorSyncAction::BeginPairing),
        Some(issue),
        None,
    )
}

pub(crate) fn status(
    state: CreatorSyncAdmissionState,
    action: Option<CreatorSyncAction>,
    issue: Option<CreatorSyncIssue>,
    ids: Option<(&Uuid, &Uuid)>,
) -> CreatorSyncStatus {
    CreatorSyncStatus {
        state,
        action,
        issue,
        installation_id: ids.map(|ids| ids.0.to_string()),
        credential_id: ids.map(|ids| ids.1.to_string()),
    }
}

pub(crate) fn sync_result(
    state: CreatorSnapshotSyncState,
    issue: Option<CreatorSyncIssue>,
) -> CreatorSnapshotSyncResult {
    CreatorSnapshotSyncResult {
        state,
        creator_count: None,
        content_sha256: None,
        snapshot_id: None,
        replayed: false,
        issue,
    }
}
