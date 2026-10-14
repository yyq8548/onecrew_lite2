use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::models::{
    AccountRefId, CommunityAdministratorRole, CommunityTargetKind, Creator, CreatorAccountLink,
    CreatorContactRouteId, CreatorContactRouteRecord, CreatorGroupFilter, CreatorGroupRef,
    CreatorId, CreatorOriginId, CreatorRelationship, CreatorStage, DiscoveryId, ObservationSource,
    ObservationSourceId, PageLimits, PlatformAccount, PlatformAccountObservation, ResolvedTargetId,
};

pub const CREATOR_PAGE_LIMITS: PageLimits = PageLimits::new(25, 100);
/// Maximum number of discovery candidates accepted by the dedicated batch
/// save workflow. This is a business limit and is intentionally independent
/// from the MCP meta-tool's per-request call limit.
///
/// The ceiling is arithmetic, not a preference: one review must project every
/// row inside `MAX_CREATOR_SAVE_BATCH_PROJECTION_BYTES` (4 MiB) while any one
/// legal preview may reach `MAX_PROJECTION_BYTES` (64 KiB), so 4 MiB / 64 KiB
/// = 64 rows is the largest batch whose every legal input still projects. 50
/// stays under that bound, keeps a batch reviewable inside the 15-minute
/// prepared TTL, and remains generatable as one tool input.
pub const MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS: usize = 50;
/// Manual Creator rows are bounded by the same reviewable-batch policy. Its
/// binding constraint is `MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES` (1 MiB): a
/// single item carrying the maximum 16 contact routes already exceeds 1 KiB,
/// so the payload budget, not the item count, is what a large batch hits first.
pub const MAX_MANUAL_CREATOR_BATCH_ITEMS: usize = 50;
pub const MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR: usize = 16;
pub const MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Bounded Creator list request. Filters are part of the cursor's query
/// context, so a continuation cannot switch filters mid-stream.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListCreatorsInput {
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<String>,
    /// Case-insensitive substring match on the display name, the latest handle
    /// of any non-rejected linked platform account, or any saved contact
    /// route's channel, locator kind, normalized value, or display value. A
    /// leading '@' is ignored.
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub stages: Vec<CreatorStage>,
    /// First-captured account platforms; an empty set does not filter.
    #[serde(default)]
    pub primary_platforms: Vec<crate::Platform>,
    #[serde(default)]
    pub last_touch_range: Option<CreatorDateRange>,
    #[serde(default)]
    pub updated_range: Option<CreatorDateRange>,
    /// Pinned creators always precede unpinned creators. Empty keeps recency order.
    #[serde(default)]
    pub sorting: Vec<CreatorListSort>,
    /// Request full-query aggregates, normally only for the initial page.
    #[serde(default)]
    pub include_facets: bool,
    /// Absent means all groups. Use the tagged variants to request only
    /// Uncategorized Creators or one exact group.
    #[serde(default)]
    pub group: Option<CreatorGroupFilter>,
    /// Archived Creators are excluded unless explicitly requested.
    #[serde(default)]
    pub include_archived: bool,
}

/// UTC Unix-second bounds. The lower bound is inclusive; the upper is exclusive.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorDateRange {
    pub from: Option<u64>,
    pub to_exclusive: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSortField {
    DisplayName,
    Followers,
    Score,
    LastTouchAt,
    UpdatedAt,
    CreatedAt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CreatorSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatorListSort {
    pub field: CreatorSortField,
    pub direction: CreatorSortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorStageCount {
    pub value: CreatorStage,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorPlatformCount {
    pub value: crate::Platform,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorListFacets {
    pub total: u64,
    /// All matching rows with only the stage filter excluded.
    pub stages: Vec<CreatorStageCount>,
    /// All matching rows with only the primary-platform filter excluded.
    pub primary_platforms: Vec<CreatorPlatformCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorListPage {
    pub items: Vec<CreatorSummary>,
    pub next_cursor: Option<String>,
    pub facets: Option<CreatorListFacets>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorSummary {
    pub id: CreatorId,
    pub display_name: String,
    pub tags: Vec<String>,
    pub stage: CreatorStage,
    pub group: Option<CreatorGroupRef>,
    /// Platform of the first-captured (earliest, non-rejected) linked
    /// account; None for creators without platform accounts.
    pub primary_platform: Option<crate::Platform>,
    /// Latest observed handle of that first-captured account.
    pub primary_handle: Option<String>,
    /// The creator's display avatar: the first-captured account's latest
    /// observation, falling back in first-capture order to the next
    /// non-rejected account that has one. So it can come from a different
    /// account than `primary_platform`/`primary_handle` — a Telegram-first
    /// person usually has no avatar there and borrows the next account's.
    pub avatar_url: Option<String>,
    /// Distinct platforms with linked accounts, in first-capture order.
    pub platforms: Vec<crate::Platform>,
    /// Distinct saved contact-route channels, in first-save order.
    pub channels: Vec<crate::ContactChannel>,
    /// Whether any origin was manual entry (shown when no platform exists).
    pub has_manual_origin: bool,
    /// Agent quality score, 0-100. Empty for manually entered Creators.
    pub score: Option<u8>,
    /// Agent explanation of the score, shown on hover in list surfaces.
    pub assessment: Option<String>,
    /// Latest known follower/subscriber count per platform.
    pub follower_counts: std::collections::BTreeMap<crate::Platform, u64>,
    /// Last outbound touch, derived at read time as the max over touchpoints,
    /// outbound email, and synced outbound DMs. Not a stored column — it can
    /// move backwards when a contact route is purged with its touchpoints.
    pub last_touch_at: Option<u64>,
    /// Workbench pin: pinned creators order ahead of the rest in list
    /// surfaces. NULL/absent means unpinned.
    pub pinned_at: Option<u64>,
    pub platform_account_count: u32,
    pub contact_route_count: u32,
    pub archived_at: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorRecord {
    pub creator: Creator,
    pub group: Option<CreatorGroupRef>,
    pub relationship: CreatorRelationship,
    pub platform_accounts: Vec<PlatformAccount>,
    pub account_links: Vec<CreatorAccountLink>,
    pub account_observations: Vec<PlatformAccountObservation>,
    pub observation_sources: Vec<ObservationSource>,
    pub origins: Vec<CreatorOrigin>,
    pub contact_routes: Vec<CreatorContactRouteRecord>,
}

/// How a Creator entered the workbench: an agent platform search through an
/// operating account, a Telegram community administrator extraction, a human
/// manual entry, or a portable import. Evidence (profile URL, content links,
/// observed time, audit) lives on the observation source and audit log.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorOrigin {
    /// Which operating account ran the search that produced this Creator.
    /// The literal query stays on the candidate snapshot's origin.
    PlatformSearch {
        id: CreatorOriginId,
        discovery_id: DiscoveryId,
        account_ref_id: AccountRefId,
        creator_id: CreatorId,
        source_id: ObservationSourceId,
        discovered_at: u64,
    },
    CommunityAdministrator {
        id: CreatorOriginId,
        discovery_id: DiscoveryId,
        creator_id: CreatorId,
        source_id: ObservationSourceId,
        role: CommunityAdministratorRole,
        community: CommunityReference,
        source: CommunityAdministratorSource,
        discovered_at: u64,
    },
    ManualEntry {
        id: CreatorOriginId,
        creator_id: CreatorId,
        source_id: ObservationSourceId,
        created_at: u64,
    },
    CreatorImport {
        id: CreatorOriginId,
        creator_id: CreatorId,
        source_id: ObservationSourceId,
        content_sha256: String,
        imported_creator_id: CreatorId,
        imported_at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommunityReference {
    pub external_id: String,
    pub title: String,
    pub handle: Option<String>,
    pub target_kind: CommunityTargetKind,
}

/// The path that reached a Telegram community's administrator list: a
/// connected operator account acting on a community the agent found, or a
/// saved Creator's resolved Telegram contact route.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CommunityAdministratorSource {
    OperatorAccount {
        operator_account_ref_id: AccountRefId,
    },
    CreatorContactRoute {
        source_creator_id: CreatorId,
        source_contact_route_id: Option<CreatorContactRouteId>,
        operator_account_ref_id: AccountRefId,
        source_resolved_target_id: ResolvedTargetId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateCreatorInput {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::creator_tags_schema")]
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorCreateInput {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "crate::creator_tags_schema")]
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
    #[serde(default)]
    #[schemars(length(max = MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR))]
    pub contact_routes: Vec<ManualCreatorInitialContactRouteInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorInitialContactRouteInput {
    pub channel: crate::ContactChannel,
    pub value: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorCreateResult {
    pub creator_id: CreatorId,
    pub display_name: String,
    pub tags: Vec<String>,
    pub stage: CreatorStage,
    pub created_at: u64,
    pub contact_routes: Vec<crate::CreatorContactRouteRecord>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManualCreatorBatchValidation {
    Strict,
    Permissive,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// The batch a review freezes. It carries no confirmation flag: authorization
/// is the store-minted grant the owner's review yields, never a field a caller
/// can set. `deny_unknown_fields` keeps a legacy `confirmed` from being accepted
/// and silently ignored.
pub struct ManualCreatorBatchCreateInput {
    pub idempotency_key: String,
    pub validation: ManualCreatorBatchValidation,
    #[schemars(length(min = 1, max = MAX_MANUAL_CREATOR_BATCH_ITEMS))]
    pub items: Vec<ManualCreatorCreateInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ManualCreatorBatchItemOutcome {
    Accepted {
        input_index: usize,
        creator: ManualCreatorCreateResult,
    },
    Rejected {
        input_index: usize,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManualCreatorBatchCreateResult {
    #[schemars(length(min = 1, max = MAX_MANUAL_CREATOR_BATCH_ITEMS))]
    pub outcomes: Vec<ManualCreatorBatchItemOutcome>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreatorIdInput {
    pub creator_id: CreatorId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateCreatorStageInput {
    pub creator_id: CreatorId,
    pub stage: CreatorStage,
}

/// Pin or unpin one Creator on the workbench list.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetCreatorPinnedInput {
    pub creator_id: CreatorId,
    pub pinned: bool,
}

/// Historical discovery provenance used by canonical saves and persisted v36 facts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CreatorCandidateOrigin {
    PlatformSearch {
        discovery_id: DiscoveryId,
        account_ref_id: AccountRefId,
        query: String,
        search_kind: String,
    },
    CommunityAdministrator {
        discovery_id: DiscoveryId,
        role: CommunityAdministratorRole,
        community: CommunityReference,
        source: CommunityAdministratorSource,
    },
}

impl CreatorCandidateOrigin {
    pub fn discovery_id(&self) -> &DiscoveryId {
        match self {
            Self::PlatformSearch { discovery_id, .. }
            | Self::CommunityAdministrator { discovery_id, .. } => discovery_id,
        }
    }

    pub fn account_ref_id(&self) -> &AccountRefId {
        match self {
            Self::PlatformSearch { account_ref_id, .. } => account_ref_id,
            Self::CommunityAdministrator { source, .. } => match source {
                CommunityAdministratorSource::OperatorAccount {
                    operator_account_ref_id,
                }
                | CommunityAdministratorSource::CreatorContactRoute {
                    operator_account_ref_id,
                    ..
                } => operator_account_ref_id,
            },
        }
    }
}
