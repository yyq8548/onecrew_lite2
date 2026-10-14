use crate::{
    AccountRefId, OperatorAccountDisconnectClaim, OperatorAccountDisconnectRecovery,
    OperatorAccountDisconnectResult, ServiceCredentialDeleteClaim, ServiceCredentialDeleteRecovery,
    ServiceCredentialDeleteResult,
};

pub trait OperatorAccountRepository: Send + Sync {
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectClaim, crate::AccountLifecycleClaimError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_operator_account_disconnect_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_operator_account_disconnect_remote_started(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_operator_account_disconnect_remote_failed(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn complete_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectResult, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn pending_operator_account_disconnects(
        &self,
    ) -> Result<Vec<(String, OperatorAccountDisconnectRecovery)>, String>;
    #[cfg_attr(any(), repository_execution(claim_reservation))]
    fn begin_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteClaim, crate::AccountLifecycleClaimError>;
    #[cfg_attr(any(), repository_execution(write))]
    fn mark_service_credential_delete_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteRecovery, String>;
    #[cfg_attr(any(), repository_execution(write))]
    fn complete_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteResult, String>;
    #[cfg_attr(any(), repository_execution(pure_read))]
    fn pending_service_credential_deletes(
        &self,
    ) -> Result<Vec<(String, ServiceCredentialDeleteRecovery)>, String>;
}
