use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{
    AccountRef, AccountRefId, Confidence, ContactAction, ContactChannel,
    CreatorContactRouteDeleteOperationId, CreatorContactRouteId, CreatorContactRouteRecord,
    CreatorId, IdentityConflict, LocatorKind, ResolvedLocatorTarget, TargetResolutionStatus,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorContactRouteAddInput {
    pub creator_id: CreatorId,
    pub channel: ContactChannel,
    pub value: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManualCreatorContactRouteAddStatus {
    Added,
    AlreadyExists,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorContactRouteAddResult {
    pub status: ManualCreatorContactRouteAddStatus,
    pub creator_id: CreatorId,
    pub route_id: CreatorContactRouteId,
    pub channel: ContactChannel,
    pub locator_kind: LocatorKind,
    pub normalized_value: String,
    pub display_value: String,
    pub confidence: Confidence,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorContactRouteUpdateInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub channel: ContactChannel,
    pub value: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManualCreatorContactRouteUpdateStatus {
    Updated,
    Unchanged,
    ReplacementRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorContactRouteUpdateResult {
    pub status: ManualCreatorContactRouteUpdateStatus,
    pub creator_id: CreatorId,
    pub route_id: CreatorContactRouteId,
    pub route: CreatorContactRouteRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareCreatorContactRouteDeleteInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorContactRouteDeleteBlockReason {
    ArchivedCreator,
    RootInitialPlatformAccount,
    HasDerivedContactRoutes,
    InFlightExternalAction,
    AmbiguousLegacyHistory,
    SharedLocalHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorContactRouteDependencyKind {
    LinkHub,
    CommunityAdministrator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorContactRouteDeleteChildSummary {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub dependency_kind: CreatorContactRouteDependencyKind,
    pub channel: ContactChannel,
    pub display_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorContactRouteDeleteBlockingArtifactKind {
    InFlightEmail,
    InFlightDirectMessage,
    InFlightRelationshipMutation,
    AmbiguousDirectMessageHistory,
    SharedEmailHistory,
    SharedDirectMessageHistory,
    SharedContactLocator,
    AmbiguousLocalAuditHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorContactRouteDeleteBlockingArtifact {
    pub kind: CreatorContactRouteDeleteBlockingArtifactKind,
    pub count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorContactRouteDeleteImpact {
    pub contact_routes: u64,
    pub dependencies: u64,
    pub email_threads: u64,
    pub email_messages: u64,
    pub email_recipients: u64,
    pub email_drafts: u64,
    pub direct_message_conversations: u64,
    pub direct_messages: u64,
    pub outreach_touchpoints: u64,
    pub audit_events: u64,
    pub idempotency_records: u64,
    pub target_resolutions: u64,
    pub identity_conflicts: u64,
    pub observations: u64,
    pub contact_locators: u64,
    pub observation_sources: u64,
    pub resolved_targets: u64,
    pub creator_origins_detached: u64,
    pub local_records_total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrepareCreatorContactRouteDeleteResult {
    Blocked {
        creator_id: CreatorId,
        contact_route_id: CreatorContactRouteId,
        reason: CreatorContactRouteDeleteBlockReason,
        children: Vec<CreatorContactRouteDeleteChildSummary>,
        blocking_artifacts: Vec<CreatorContactRouteDeleteBlockingArtifact>,
    },
    ConfirmationRequired {
        operation_id: CreatorContactRouteDeleteOperationId,
        creator_id: CreatorId,
        contact_route_id: CreatorContactRouteId,
        impact: CreatorContactRouteDeleteImpact,
        remote_not_deleted: bool,
        expires_at: u64,
        next_tool: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmCreatorContactRouteDeleteInput {
    pub operation_id: CreatorContactRouteDeleteOperationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorContactRouteDeleteStaleReason {
    PlanChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConfirmCreatorContactRouteDeleteResult {
    Deleted {
        operation_id: CreatorContactRouteDeleteOperationId,
        creator_id: CreatorId,
        contact_route_id: CreatorContactRouteId,
        deleted: CreatorContactRouteDeleteImpact,
        remote_not_deleted: bool,
    },
    Stale {
        operation_id: CreatorContactRouteDeleteOperationId,
        reason: CreatorContactRouteDeleteStaleReason,
        next_tool: String,
    },
    Expired {
        operation_id: CreatorContactRouteDeleteOperationId,
        next_tool: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ResolveContactTargetInput {
    pub creator_id: CreatorId,
    pub contact_route_id: CreatorContactRouteId,
    pub operator_account_ref_id: AccountRefId,
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContactTargetResolutionResult {
    pub creator_id: CreatorId,
    pub route: CreatorContactRouteRecord,
    pub operator_account: AccountRef,
    pub status: TargetResolutionStatus,
    pub resolved: Option<ResolvedLocatorTarget>,
    pub conflict: Option<IdentityConflict>,
    pub allowed_actions: Vec<ContactAction>,
    pub browser_url: Option<String>,
    pub reason: Option<String>,
}
