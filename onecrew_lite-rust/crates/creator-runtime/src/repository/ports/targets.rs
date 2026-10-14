use crate::{
    AccountRef, AccountRefId, CommitContactTargetResolutionInput,
    CommitContactTargetResolutionResult, ContactLocatorId, LocatorResolution,
    LocatorResolutionWriteResult, PlatformAccountId, ResolvedLocatorTarget, ResolvedTarget,
    ResolvedTargetId, TargetKind,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperableAccountRefError {
    Unavailable,
    Internal,
}

pub trait TargetRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_account_ref(
        &self,
        account_ref_id: AccountRefId,
        platform: crate::Platform,
        kind: &str,
        label: &str,
    ) -> Result<AccountRef, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_account_ref_with_state(
        &self,
        account_ref_id: AccountRefId,
        platform: crate::Platform,
        kind: crate::AccountKind,
        label: &str,
        availability: crate::AccountAvailability,
    ) -> Result<AccountRef, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_account_ref(&self, account_ref_id: &AccountRefId) -> Result<AccountRef, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn list_account_refs(
        &self,
        platform: crate::Platform,
        kind: crate::AccountKind,
    ) -> Result<Vec<AccountRef>, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn require_operable_account_ref(
        &self,
        account_ref_id: &AccountRefId,
    ) -> Result<AccountRef, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn require_operable_account_ref_for_read(
        &self,
        account_ref_id: &AccountRefId,
    ) -> Result<AccountRef, OperableAccountRefError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_resolved_target(
        &self,
        account_ref_id: &AccountRefId,
        platform: crate::Platform,
        target_kind: TargetKind,
        external_target_id: &str,
        platform_account_id: Option<&PlatformAccountId>,
        metadata: serde_json::Value,
    ) -> Result<ResolvedTarget, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_resolved_target(
        &self,
        resolved_target_id: &ResolvedTargetId,
    ) -> Result<ResolvedTarget, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn commit_contact_target_resolution(
        &self,
        input: CommitContactTargetResolutionInput,
    ) -> Result<CommitContactTargetResolutionResult, crate::CreatorTargetResolutionError>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn active_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
    ) -> Result<Option<ResolvedLocatorTarget>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn record_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
    ) -> Result<LocatorResolutionWriteResult, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn invalidate_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
    ) -> Result<Option<LocatorResolution>, String>;
}
