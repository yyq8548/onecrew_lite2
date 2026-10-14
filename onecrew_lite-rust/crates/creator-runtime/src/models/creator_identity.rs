use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AccountRefId, ContactLocatorId, CreatorAccountLinkId, CreatorContactRouteId, CreatorId,
    LocatorObservationId, ObservationSourceId, PlatformAccountId, PlatformAccountObservationId,
    core::{
        Confidence, ContactChannel, CreatorStage, LinkState, LocatorKind, Platform, SourceKind,
        TargetKind,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Creator {
    pub id: CreatorId,
    pub display_name: String,
    /// Ordered text tags, at most three, each at most twelve Unicode characters.
    pub tags: Vec<String>,
    pub notes: Option<String>,
    /// Agent quality score, 0-100. Empty for manually entered Creators.
    pub score: Option<u8>,
    /// Agent explanation of why the score was assigned. Empty for manual entries.
    pub assessment: Option<String>,
    /// Latest known follower/subscriber count per platform.
    pub follower_counts: BTreeMap<Platform, u64>,
    pub created_at: u64,
    pub updated_at: u64,
    pub archived_at: Option<u64>,
    /// Workbench pin timestamp; None means unpinned.
    pub pinned_at: Option<u64>,
}

/// Direct patch of one Creator's own facts. Provided fields overwrite;
/// absent fields stay unchanged; `follower_counts` merges per platform.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateCreatorInput {
    pub creator_id: CreatorId,
    /// Replace all tags when provided; an empty list clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::creator_tags_schema")]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Agent quality score, 0-100.
    #[serde(default)]
    pub score: Option<u8>,
    /// Why the score was assigned.
    #[serde(default)]
    pub assessment: Option<String>,
    /// Per-platform follower counts; each provided platform overwrites its
    /// stored count, platforms not listed keep their current value.
    #[serde(default)]
    pub follower_counts: Option<BTreeMap<Platform, u64>>,
}

/// Permanent local deletion of one Creator and every locally derived
/// fact about them. Remote platform data is untouched; local audit events
/// and emails survive with their creator attribution cleared.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteCreatorInput {
    pub creator_id: CreatorId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorDeleteResult {
    pub creator_id: CreatorId,
    pub removed_contact_routes: u64,
    pub removed_touchpoints: u64,
    pub removed_origins: u64,
    pub removed_account_links: u64,
    /// Audit events and emails that survive with attribution cleared.
    pub cleared_references: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorRelationship {
    pub creator_id: CreatorId,
    pub stage: CreatorStage,
    /// Last outbound touch, derived at read time; see `CreatorSummary`.
    pub last_touch_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformAccount {
    pub id: PlatformAccountId,
    pub platform: Platform,
    pub target_kind: TargetKind,
    pub external_id: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorAccountLink {
    pub id: CreatorAccountLinkId,
    pub creator_id: CreatorId,
    pub platform_account_id: PlatformAccountId,
    pub state: LinkState,
    pub confidence: Confidence,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformAccountObservation {
    pub id: PlatformAccountObservationId,
    pub platform_account_id: PlatformAccountId,
    pub source_id: ObservationSourceId,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub profile_url: Option<String>,
    pub public_data: serde_json::Value,
    pub observed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ObservationSource {
    pub id: ObservationSourceId,
    pub kind: SourceKind,
    pub platform: Option<Platform>,
    pub source_url: Option<String>,
    pub external_ref: Option<String>,
    pub account_ref_id: Option<AccountRefId>,
    pub metadata: serde_json::Value,
    pub observed_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContactLocator {
    pub id: ContactLocatorId,
    pub channel: ContactChannel,
    pub kind: LocatorKind,
    pub normalized_value: String,
    pub display_value: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorContactRoute {
    pub id: CreatorContactRouteId,
    pub creator_id: CreatorId,
    pub locator_id: ContactLocatorId,
    pub confidence: Confidence,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContactRouteObservationRecord {
    pub observation: LocatorObservation,
    pub source: ObservationSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorContactRouteRecord {
    pub route: CreatorContactRoute,
    pub locator: ContactLocator,
    pub observations: Vec<ContactRouteObservationRecord>,
    pub allowed_actions: Vec<CreatorContactRouteAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorContactRouteAction {
    Update,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LocatorObservation {
    pub id: LocatorObservationId,
    pub locator_id: ContactLocatorId,
    pub source_id: ObservationSourceId,
    pub creator_id: Option<CreatorId>,
    pub platform_account_id: Option<PlatformAccountId>,
    pub observed_value: String,
    pub observed_at: u64,
}

#[cfg(test)]
mod tests {
    use super::CreatorContactRouteAction;

    #[test]
    fn contact_route_actions_have_stable_snake_case_wire_values() {
        assert_eq!(
            serde_json::to_value(CreatorContactRouteAction::Update).unwrap(),
            serde_json::json!("update")
        );
        assert_eq!(
            serde_json::to_value(CreatorContactRouteAction::Delete).unwrap(),
            serde_json::json!("delete")
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlatformAccountCandidate {
    pub platform: Platform,
    pub external_id: Option<String>,
    pub target_kind: Option<TargetKind>,
    pub handle: Option<String>,
    pub display_name: String,
    pub profile_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IdentityResolution {
    New,
    ExactExistingCreator { creator_id: CreatorId },
    PossibleMatch { creator_ids: Vec<CreatorId> },
    ConflictingMatches { creator_ids: Vec<CreatorId> },
}

/// Identity resolution takes a descriptive platform-account value object
/// (platform, external id, handle); it is not persisted state, so there is
/// deliberately no key form.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveCreatorIdentityInput {
    pub candidate: PlatformAccountCandidate,
}
