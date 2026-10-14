use crate::{
    BulkMoveCreatorsToGroupInput, Confidence, ConfirmCreatorImportInput,
    ConfirmCreatorImportResult, CreateCreatorGroupInput, CreateCreatorInput, Creator,
    CreatorAccountLink, CreatorBundle, CreatorDeleteResult, CreatorGroupError,
    CreatorGroupListResult, CreatorId, CreatorListPage, CreatorRecord, CreatorRelationship,
    CreatorStage, DeleteCreatorGroupInput, DeleteCreatorGroupResult, DeleteCreatorInput,
    IdentityResolution, LinkState, ListCreatorGroupsInput, ListCreatorsInput,
    ManualCreatorCreateInput, ManualCreatorCreateResult, MoveCreatorsToGroupResult,
    PlatformAccount, PlatformAccountId, PlatformAccountObservation,
    PlatformAccountObservationInput, PrepareCreatorImportResult, SetCreatorGroupInput,
    UpdateCreatorGroupInput,
};

#[doc(hidden)]
pub use crate::creator_import::{
    ConfirmImportWrite, ConfirmImportWriteOutcome, ImportBasis, ImportOperationSnapshot,
    PrepareImportWrite, PrepareImportWriteOutcome, ValidatedImportArtifact,
};

pub trait CreatorRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn outreach_route_context(
        &self,
        creator_id: &crate::CreatorId,
        route_id: &crate::CreatorContactRouteId,
        target_id: &crate::ResolvedTargetId,
        operator_id: &crate::AccountRefId,
    ) -> Result<crate::CreatorRouteOutreachContext, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn outreach_conversation_context(
        &self,
        conversation_id: &crate::ConversationId,
    ) -> Result<crate::CreatorRouteOutreachContext, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn outreach_content_context(
        &self,
        input: crate::PrepareContentReplyInput,
    ) -> Result<crate::CreatorContentOutreachContext, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn prepare_creator_save_batch(
        &self,
        inputs: Vec<crate::CanonicalCreatorSaveInput>,
        intent: crate::CreatorSaveIntent,
    ) -> Result<crate::PreparedCreatorSaveBatch, crate::CreatorSavePreparationError>;
    /// Atomic write, followed only after a commit error by independent,
    /// read-only receipt reconciliation after the writer lock is released.
    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn execute_creator_save_batch(
        &self,
        prepared: crate::SelectedPreparedCreatorSaveBatch,
        execution_key: String,
    ) -> Result<(crate::CreatorSaveBatchResult, bool), String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn export_creator_bundle(&self) -> Result<CreatorBundle, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn export_creator_workbook(&self) -> Result<crate::CreatorWorkbookExport, String>;

    /// Read the workbook's complete committed snapshot without encoding it.
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn export_creator_workbook_snapshot(
        &self,
    ) -> Result<Vec<(crate::CreatorTransferCreator, crate::CreatorWorkbookDisplay)>, String>;
    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn prepare_creator_import(
        &self,
        bundle: CreatorBundle,
    ) -> Result<PrepareCreatorImportResult, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    #[doc(hidden)]
    fn load_creator_import_basis(
        &self,
        artifact: &ValidatedImportArtifact,
    ) -> Result<ImportBasis, String>;
    #[cfg_attr(any(), repository_execution(write))]
    #[doc(hidden)]
    fn write_creator_import_prepare(
        &self,
        work: PrepareImportWrite,
    ) -> Result<PrepareImportWriteOutcome, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    #[doc(hidden)]
    fn load_creator_import_operation(
        &self,
        input: &ConfirmCreatorImportInput,
    ) -> Result<ImportOperationSnapshot, String>;
    #[cfg_attr(any(), repository_execution(write))]
    #[doc(hidden)]
    fn write_creator_import_confirm(
        &self,
        work: ConfirmImportWrite,
    ) -> Result<ConfirmImportWriteOutcome, String>;
    #[cfg_attr(any(), repository_execution(segmented_write))]
    fn confirm_creator_import(
        &self,
        input: ConfirmCreatorImportInput,
    ) -> Result<(ConfirmCreatorImportResult, bool), String>;
    #[cfg_attr(any(), repository_execution(migration_maintenance))]
    fn cancel_creator_import(
        &self,
        input: ConfirmCreatorImportInput,
    ) -> Result<ConfirmCreatorImportResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn create_creator(&self, input: CreateCreatorInput) -> Result<CreatorRecord, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn create_manual_creator(
        &self,
        input: ManualCreatorCreateInput,
    ) -> Result<ManualCreatorCreateResult, String>;
    /// Persist a preflighted set of manual Creators in one SQLite transaction.
    /// The repository owns the transaction so a late constraint or I/O error
    /// rolls back every item rather than leaving an earlier item committed.
    #[cfg_attr(any(), repository_execution(write))]
    fn create_manual_creator_batch(
        &self,
        inputs: Vec<ManualCreatorCreateInput>,
    ) -> Result<Vec<ManualCreatorCreateResult>, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn get_creator(&self, creator_id: &CreatorId) -> Result<CreatorRecord, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_creators(&self, input: ListCreatorsInput) -> Result<CreatorListPage, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn list_creator_groups(
        &self,
        input: ListCreatorGroupsInput,
    ) -> Result<CreatorGroupListResult, CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn create_creator_group(
        &self,
        input: CreateCreatorGroupInput,
    ) -> Result<crate::CreatorGroup, CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn update_creator_group(
        &self,
        input: UpdateCreatorGroupInput,
    ) -> Result<(crate::CreatorGroup, bool), CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn delete_creator_group(
        &self,
        input: DeleteCreatorGroupInput,
    ) -> Result<DeleteCreatorGroupResult, CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn set_creator_group(
        &self,
        input: SetCreatorGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn bulk_move_creators_to_group(
        &self,
        input: BulkMoveCreatorsToGroupInput,
    ) -> Result<MoveCreatorsToGroupResult, CreatorGroupError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn set_creator_pinned(&self, creator_id: &CreatorId, pinned: bool) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn set_creator_stage(
        &self,
        creator_id: &CreatorId,
        stage: CreatorStage,
    ) -> Result<CreatorRelationship, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn update_creator(&self, input: crate::UpdateCreatorInput) -> Result<CreatorRecord, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn archive_creator(&self, creator_id: &CreatorId) -> Result<Creator, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn delete_creator(&self, input: DeleteCreatorInput) -> Result<CreatorDeleteResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_platform_account(
        &self,
        input: crate::UpsertPlatformAccountInput,
    ) -> Result<PlatformAccount, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn link_creator_account(
        &self,
        creator_id: &CreatorId,
        platform_account_id: &PlatformAccountId,
        state: LinkState,
        confidence: Confidence,
    ) -> Result<CreatorAccountLink, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn append_platform_account_observation(
        &self,
        input: PlatformAccountObservationInput,
    ) -> Result<PlatformAccountObservation, String>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn resolve_creator_identity(
        &self,
        candidate: &crate::PlatformAccountCandidate,
    ) -> Result<IdentityResolution, String>;
}
