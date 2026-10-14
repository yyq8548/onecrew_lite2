use crate::{
    Confidence, ConfirmCreatorContactRouteDeleteInput, ConfirmCreatorContactRouteDeleteResult,
    ContactLocator, ContactLocatorId, ContactLocatorInput, CreatorContactRoute,
    CreatorContactRouteDependencyKind, CreatorContactRouteId, CreatorContactRouteRecord, CreatorId,
    LocatorObservation, ManualCreatorContactRouteAddInput, ManualCreatorContactRouteAddResult,
    ManualCreatorContactRouteUpdateInput, ManualCreatorContactRouteUpdateResult, ObservationSource,
    ObservationSourceId, ObservationSourceInput, PlatformAccountId,
    PrepareCreatorContactRouteDeleteInput, PrepareCreatorContactRouteDeleteResult,
};

pub trait ContactRepository: Send + Sync {
    #[cfg(test)]
    #[cfg_attr(any(), repository_execution(migration_maintenance))]
    fn insert_historical_shared_contact_route_for_test(
        &self,
        creator_id: &CreatorId,
        locator_id: &ContactLocatorId,
        confidence: Confidence,
    ) -> Result<CreatorContactRoute, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn add_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteAddInput,
        locator: ContactLocatorInput,
    ) -> Result<ManualCreatorContactRouteAddResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn update_manual_creator_contact_route(
        &self,
        input: ManualCreatorContactRouteUpdateInput,
        locator: ContactLocatorInput,
    ) -> Result<ManualCreatorContactRouteUpdateResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn prepare_creator_contact_route_delete(
        &self,
        input: PrepareCreatorContactRouteDeleteInput,
    ) -> Result<PrepareCreatorContactRouteDeleteResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn confirm_creator_contact_route_delete(
        &self,
        input: ConfirmCreatorContactRouteDeleteInput,
    ) -> Result<(ConfirmCreatorContactRouteDeleteResult, bool), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn create_observation_source(
        &self,
        input: ObservationSourceInput,
    ) -> Result<ObservationSource, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_contact_locator(&self, input: ContactLocatorInput) -> Result<ContactLocator, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn link_creator_contact_route(
        &self,
        creator_id: &CreatorId,
        locator_id: &ContactLocatorId,
        confidence: Confidence,
    ) -> Result<CreatorContactRoute, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn link_creator_contact_route_dependency(
        &self,
        parent_contact_route_id: &CreatorContactRouteId,
        child_contact_route_id: &CreatorContactRouteId,
        kind: CreatorContactRouteDependencyKind,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn append_locator_observation(
        &self,
        locator_id: &ContactLocatorId,
        source_id: &ObservationSourceId,
        creator_id: Option<&CreatorId>,
        platform_account_id: Option<&PlatformAccountId>,
        observed_value: &str,
    ) -> Result<LocatorObservation, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn creator_contact_routes(
        &self,
        creator_id: &CreatorId,
    ) -> Result<Vec<CreatorContactRouteRecord>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn find_contact_route_owners(
        &self,
        locator: &ContactLocatorInput,
    ) -> Result<Vec<CreatorContactRouteRecord>, String>;
    /// Return canonical locator keys that already have a Creator owner. The
    /// lookup is set-based and may be internally chunked to stay below SQLite
    /// parameter limits.
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn find_contact_route_owner_keys(
        &self,
        locators: &[ContactLocatorInput],
    ) -> Result<Vec<String>, String>;
}
