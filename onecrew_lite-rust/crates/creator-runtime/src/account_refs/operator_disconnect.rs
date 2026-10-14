//! Durable claims and recovery state for operator account disconnection.

use crate::{
    AccountLifecycleClaimError, AccountRefId, OperatorAccountDisconnectClaim,
    OperatorAccountDisconnectRecovery, OperatorAccountDisconnectResult,
};

impl crate::CreatorRuntime {
    pub fn begin_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectClaim, AccountLifecycleClaimError> {
        self.repository
            .begin_operator_account_disconnect(key, account_ref_id)
    }

    pub fn mark_operator_account_disconnect_remote_verified(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        self.repository
            .mark_operator_account_disconnect_remote_verified(key, account_ref_id)
    }

    pub fn mark_operator_account_disconnect_remote_started(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        self.repository
            .mark_operator_account_disconnect_remote_started(key, account_ref_id)
    }

    pub fn mark_operator_account_disconnect_remote_failed(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectRecovery, String> {
        self.repository
            .mark_operator_account_disconnect_remote_failed(key, account_ref_id)
    }

    pub fn complete_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectResult, String> {
        self.repository
            .complete_operator_account_disconnect(key, account_ref_id)
    }

    pub fn pending_operator_account_disconnects(
        &self,
    ) -> Result<Vec<(String, OperatorAccountDisconnectRecovery)>, String> {
        self.repository.pending_operator_account_disconnects()
    }
}
