use crate::{CreatorContactRouteId, IdempotencyClaim, MailboxRefId, RecoverableActionClaim};

pub trait IdempotencyRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn read_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<Option<crate::CreatorOutreachActionState>, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn creator_outreach_status(
        &self,
        key: &str,
    ) -> Result<Option<crate::CreatorOutreachStatus>, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn validate_creator_outreach(
        &self,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<(), crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<crate::CreatorOutreachClaim, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn accept_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
        receipt: crate::CreatorOutreachReceipt,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn verify_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
        verification: crate::CreatorOutreachVerification,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn refuse_creator_outreach(
        &self,
        key: &str,
        intent: &crate::PreparedCreatorOutreachIntent,
    ) -> Result<crate::CreatorOutreachActionState, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(composite_snapshot_read))]
    fn recorded_creator_outreach(
        &self,
        key: &str,
    ) -> Result<crate::CreatorOutreachRecord, crate::CreatorOutreachError>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_action(&self, key: &str, action: &str) -> Result<Option<IdempotencyClaim>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_action(&self, key: &str, action: &str) -> Result<IdempotencyClaim, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_action_with_fingerprint(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
    ) -> Result<IdempotencyClaim, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_mailbox_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: &MailboxRefId,
    ) -> Result<IdempotencyClaim, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn find_recoverable_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
        request_fingerprint: &str,
    ) -> Result<Option<RecoverableActionClaim>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn claim_remote_started_action(
        &self,
        key: &str,
        action: &str,
        mailbox_ref_id: Option<&MailboxRefId>,
        contact_route_id: Option<&CreatorContactRouteId>,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<RecoverableActionClaim, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn stage_remote_accepted_action(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn stage_remote_verified_action(
        &self,
        key: &str,
        action: &str,
        request_fingerprint: &str,
        recovery: serde_json::Value,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn complete_action(
        &self,
        key: &str,
        action: &str,
        result: serde_json::Value,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn fail_action(
        &self,
        key: &str,
        action: &str,
        failure: serde_json::Value,
    ) -> Result<(), String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn cancel_action(&self, key: &str, action: &str) -> Result<(), String>;
}
