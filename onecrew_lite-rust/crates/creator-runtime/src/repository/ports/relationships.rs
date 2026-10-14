use crate::{
    AccountRefId, AccountRelationship, CommitRelationshipActionResult, PlatformAccountId,
    RelationshipKind, RelationshipValue,
};

pub trait RelationshipRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn get_account_relationship(
        &self,
        actor_account_ref_id: &AccountRefId,
        target_platform_account_id: &PlatformAccountId,
        kind: RelationshipKind,
    ) -> Result<Option<AccountRelationship>, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn upsert_account_relationship(
        &self,
        actor_account_ref_id: &AccountRefId,
        target_platform_account_id: &PlatformAccountId,
        kind: RelationshipKind,
        value: RelationshipValue,
        checked_at: u64,
    ) -> Result<AccountRelationship, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn record_creator_relationship(
        &self,
        key: &str,
    ) -> Result<CommitRelationshipActionResult, crate::CreatorOutreachError>;
}
