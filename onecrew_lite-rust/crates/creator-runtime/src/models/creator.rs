//! Creator owner models: the canonical Creator record and its origins, groups,
//! contact-route mutation contracts, and the observation inputs that feed the
//! Creator graph. Every public name stays reachable at the crate root.
mod contact_evidence;
mod contact_routes;
mod creator_groups;
mod creators;
mod observations;
mod save;

pub use contact_evidence::{
    ContactRouteCollision, ContactRouteConfirmation, ContactRoutePreview,
    CreatorSaveContactEvidence, CreatorSavePublicLink, DiscoveredContactRoute,
    PlatformContactObservation,
};
pub use save::{
    CommunityAdministratorOutreachContext, CreatorCandidateSaveRouteOption, CreatorQualityInput,
};

pub use contact_routes::{
    ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    ContactTargetResolutionResult, CreatorContactRouteDeleteBlockReason,
    CreatorContactRouteDeleteBlockingArtifact, CreatorContactRouteDeleteBlockingArtifactKind,
    CreatorContactRouteDeleteChildSummary, CreatorContactRouteDeleteImpact,
    CreatorContactRouteDeleteStaleReason, CreatorContactRouteDependencyKind,
    ManualCreatorContactRouteAddInput, ManualCreatorContactRouteAddResult,
    ManualCreatorContactRouteAddStatus, ManualCreatorContactRouteUpdateInput,
    ManualCreatorContactRouteUpdateResult, ManualCreatorContactRouteUpdateStatus,
    PrepareCreatorContactRouteDeleteInput, PrepareCreatorContactRouteDeleteResult,
    ResolveContactTargetInput,
};
pub use creator_groups::{
    BulkMoveCreatorsToGroupInput, CREATOR_GROUP_BULK_MOVE_LIMIT, CREATOR_GROUP_NAME_MAX_CHARS,
    CREATOR_GROUP_PAGE_LIMITS, CreateCreatorGroupInput, CreatorGroup, CreatorGroupAssignment,
    CreatorGroupDestination, CreatorGroupError, CreatorGroupFilter, CreatorGroupListResult,
    CreatorGroupRef, CreatorGroupSummary, DeleteCreatorGroupInput, DeleteCreatorGroupResult,
    ListCreatorGroupsInput, MoveCreatorsToGroupResult, SetCreatorGroupInput,
    UpdateCreatorGroupInput,
};
pub use creators::{
    CREATOR_PAGE_LIMITS, CommunityAdministratorSource, CommunityReference, CreateCreatorInput,
    CreatorCandidateOrigin, CreatorDateRange, CreatorIdInput, CreatorListFacets, CreatorListPage,
    CreatorListSort, CreatorOrigin, CreatorPlatformCount, CreatorRecord, CreatorSortDirection,
    CreatorSortField, CreatorStageCount, CreatorSummary, ListCreatorsInput,
    MAX_CREATOR_CANDIDATE_SAVE_BATCH_ITEMS, MAX_INITIAL_CONTACT_ROUTES_PER_CREATOR,
    MAX_MANUAL_CREATOR_BATCH_ITEMS, MAX_MANUAL_CREATOR_BATCH_PAYLOAD_BYTES,
    ManualCreatorBatchCreateInput, ManualCreatorBatchCreateResult, ManualCreatorBatchItemOutcome,
    ManualCreatorBatchValidation, ManualCreatorCreateInput, ManualCreatorCreateResult,
    ManualCreatorInitialContactRouteInput, SetCreatorPinnedInput, UpdateCreatorStageInput,
};
pub use observations::{
    ContactLocatorInput, ObservationSourceInput, PlatformAccountObservationInput,
    UpsertPlatformAccountInput,
};

#[cfg(test)]
mod tests;
