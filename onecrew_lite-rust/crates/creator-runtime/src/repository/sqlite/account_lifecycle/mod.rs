mod operator_disconnect;
mod service_credential_delete;
mod shared;

use crate::{
    AccountRefId, OperatorAccountDisconnectClaim, OperatorAccountDisconnectRecovery,
    OperatorAccountDisconnectResult, ServiceCredentialDeleteClaim, ServiceCredentialDeleteRecovery,
    ServiceCredentialDeleteResult, repository::OperatorAccountRepository,
};

use super::SqliteRepository;

impl OperatorAccountRepository for SqliteRepository {
    fn begin_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectClaim, crate::AccountLifecycleClaimError> {
        operator_disconnect::begin(self, key, account_ref_id)
    }

    fn mark_operator_account_disconnect_remote_started(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        operator_disconnect::mark_remote_started(self, key, account_ref_id)
    }

    fn mark_operator_account_disconnect_remote_failed(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        operator_disconnect::mark_remote_failed(self, key, account_ref_id)
    }

    fn mark_operator_account_disconnect_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        operator_disconnect::mark_remote_verified(self, key, account_ref_id)
    }

    fn complete_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectResult, String> {
        operator_disconnect::complete(self, key, account_ref_id)
    }

    fn pending_operator_account_disconnects(
        &self,
    ) -> Result<Vec<(String, OperatorAccountDisconnectRecovery)>, String> {
        operator_disconnect::pending(self)
    }

    fn begin_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteClaim, crate::AccountLifecycleClaimError> {
        service_credential_delete::begin(self, key, account_ref_id)
    }

    fn mark_service_credential_delete_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteRecovery, String> {
        service_credential_delete::mark_remote_verified(self, key, account_ref_id)
    }

    fn complete_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteResult, String> {
        service_credential_delete::complete(self, key, account_ref_id)
    }

    fn pending_service_credential_deletes(
        &self,
    ) -> Result<Vec<(String, ServiceCredentialDeleteRecovery)>, String> {
        service_credential_delete::pending(self)
    }
}
