mod account_refs;
mod locator_resolutions;
mod resolved_targets;
pub(super) use account_refs::load_account_ref;
pub(super) use resolved_targets::load_resolved_target;

use crate::{
    AccountAvailability, AccountKind, AccountRef, AccountRefId, CommitContactTargetResolutionInput,
    CommitContactTargetResolutionResult, ContactLocatorId, LocatorResolution,
    LocatorResolutionWriteResult, Platform, PlatformAccountId, ResolvedLocatorTarget,
    ResolvedTarget, ResolvedTargetId, TargetKind, repository::TargetRepository,
};

use super::SqliteRepository;

impl TargetRepository for SqliteRepository {
    fn upsert_account_ref(
        &self,
        account_ref_id: AccountRefId,
        platform: Platform,
        kind: &str,
        label: &str,
    ) -> Result<AccountRef, String> {
        account_refs::upsert_account_ref(self, account_ref_id, platform, kind, label)
    }

    fn upsert_account_ref_with_state(
        &self,
        account_ref_id: AccountRefId,
        platform: Platform,
        kind: AccountKind,
        label: &str,
        availability: AccountAvailability,
    ) -> Result<AccountRef, String> {
        account_refs::upsert_account_ref_with_state(
            self,
            account_ref_id,
            platform,
            kind,
            label,
            availability,
        )
    }

    fn get_account_ref(&self, account_ref_id: &AccountRefId) -> Result<AccountRef, String> {
        account_refs::get_account_ref(self, account_ref_id)
    }

    fn list_account_refs(
        &self,
        platform: Platform,
        kind: AccountKind,
    ) -> Result<Vec<AccountRef>, String> {
        account_refs::list_account_refs(self, platform, kind)
    }

    fn require_operable_account_ref(
        &self,
        account_ref_id: &AccountRefId,
    ) -> Result<AccountRef, String> {
        account_refs::require_operable_account_ref(self, account_ref_id)
    }

    fn require_operable_account_ref_for_read(
        &self,
        account_ref_id: &AccountRefId,
    ) -> Result<AccountRef, crate::repository::OperableAccountRefError> {
        account_refs::require_operable_account_ref_for_read(self, account_ref_id)
    }

    fn upsert_resolved_target(
        &self,
        account_ref_id: &AccountRefId,
        platform: Platform,
        target_kind: TargetKind,
        external_target_id: &str,
        platform_account_id: Option<&PlatformAccountId>,
        metadata: serde_json::Value,
    ) -> Result<ResolvedTarget, String> {
        resolved_targets::upsert_resolved_target(
            self,
            account_ref_id,
            platform,
            target_kind,
            external_target_id,
            platform_account_id,
            metadata,
        )
    }

    fn get_resolved_target(
        &self,
        resolved_target_id: &ResolvedTargetId,
    ) -> Result<ResolvedTarget, String> {
        resolved_targets::get_resolved_target(self, resolved_target_id)
    }

    fn commit_contact_target_resolution(
        &self,
        input: CommitContactTargetResolutionInput,
    ) -> Result<CommitContactTargetResolutionResult, crate::CreatorTargetResolutionError> {
        locator_resolutions::commit_contact_target_resolution(self, input)
    }

    fn active_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
    ) -> Result<Option<ResolvedLocatorTarget>, String> {
        locator_resolutions::active_locator_resolution(self, locator_id, account_ref_id)
    }

    fn record_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
        resolved_target_id: &ResolvedTargetId,
    ) -> Result<LocatorResolutionWriteResult, String> {
        locator_resolutions::record_locator_resolution(
            self,
            locator_id,
            account_ref_id,
            resolved_target_id,
        )
    }

    fn invalidate_locator_resolution(
        &self,
        locator_id: &ContactLocatorId,
        account_ref_id: &AccountRefId,
    ) -> Result<Option<LocatorResolution>, String> {
        locator_resolutions::invalidate_locator_resolution(self, locator_id, account_ref_id)
    }
}
