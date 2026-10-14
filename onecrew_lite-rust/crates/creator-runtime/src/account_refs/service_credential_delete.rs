//! Durable claims and recovery state for service credential deletion.

use crate::{
    AccountLifecycleClaimError, AccountRefId, ServiceCredentialDeleteClaim,
    ServiceCredentialDeleteRecovery, ServiceCredentialDeleteResult,
};

impl crate::CreatorRuntime {
    pub fn begin_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteClaim, AccountLifecycleClaimError> {
        self.repository
            .begin_service_credential_delete(key, account_ref_id)
    }

    pub fn mark_service_credential_delete_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteRecovery, String> {
        self.repository
            .mark_service_credential_delete_remote_verified(key, account_ref_id)
    }

    pub fn complete_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteResult, String> {
        self.repository
            .complete_service_credential_delete(key, account_ref_id)
    }

    pub fn pending_service_credential_deletes(
        &self,
    ) -> Result<Vec<(String, ServiceCredentialDeleteRecovery)>, String> {
        self.repository.pending_service_credential_deletes()
    }
}
