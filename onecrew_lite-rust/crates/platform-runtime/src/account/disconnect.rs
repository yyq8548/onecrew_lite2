use crate::PlatformChangeScope;
use onecrew_creator_runtime::{
    AccountKind, AccountLifecycleClaimError, AccountRefId, OperatorAccountDisconnectClaim,
    OperatorAccountDisconnectResult,
};
use onecrew_local_domain::Platform;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::LocalPlatform;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountDisconnectInput {
    pub operator_account_id: String,
    pub confirmed: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperatorAccountDisconnectOutcome {
    pub operator_account_id: String,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorAccountDisconnectError {
    ConfirmationRequired,
    IdempotencyKeyInvalid,
    IdempotencyConflict,
    InvalidOperatorAccountId,
    NotFoundOrIdentityMismatch,
    OutreachInProgress,
    RecoverableSameKeyRetry,
    Internal,
}

impl OperatorAccountDisconnectError {
    pub const fn safe_message(self) -> &'static str {
        match self {
            Self::ConfirmationRequired => "confirmed=true is required",
            Self::IdempotencyKeyInvalid => "idempotency_key must contain 1 to 128 characters",
            Self::IdempotencyConflict => {
                "idempotency_key was already used for a different disconnect intent"
            }
            Self::InvalidOperatorAccountId => "operator_account_id is invalid",
            Self::NotFoundOrIdentityMismatch => "operator account was not found",
            Self::OutreachInProgress => {
                "an outreach action is unresolved; recover it before disconnecting this account"
            }
            Self::RecoverableSameKeyRetry => {
                "operator disconnect is incomplete; retry the same intent with the same idempotency_key"
            }
            Self::Internal => "operator disconnect is unavailable",
        }
    }
}

impl LocalPlatform {
    /// Canonical cross-store disconnect. The Creator store is made non-operable first, so a
    /// crash can never leave a deleted vault credential advertised as active.
    /// The durable Creator claim then drives idempotent vault deletion and final
    /// availability projection on retry/startup recovery.
    pub fn operator_account_disconnect(
        &self,
        input: OperatorAccountDisconnectInput,
    ) -> Result<OperatorAccountDisconnectOutcome, OperatorAccountDisconnectError> {
        if !input.confirmed {
            return Err(OperatorAccountDisconnectError::ConfirmationRequired);
        }
        if input.idempotency_key.trim().is_empty() || input.idempotency_key.trim().len() > 128 {
            return Err(OperatorAccountDisconnectError::IdempotencyKeyInvalid);
        }
        let profile_id = Uuid::parse_str(&input.operator_account_id)
            .map_err(|_| OperatorAccountDisconnectError::InvalidOperatorAccountId)?;
        let account_ref_id = self.resolve_operator_account_ref(profile_id)?;
        let result =
            self.execute_operator_account_disconnect(&input.idempotency_key, &account_ref_id)?;
        Ok(OperatorAccountDisconnectOutcome {
            operator_account_id: account_ref_id.as_str().to_string(),
            replayed: result.replayed,
        })
    }

    pub(crate) fn recover_pending_operator_account_disconnects(&self) -> Result<(), String> {
        for (key, recovery) in self.creators.pending_operator_account_disconnects()? {
            self.execute_operator_account_disconnect(&key, &recovery.account_ref_id)
                .map_err(|error| error.safe_message().to_string())?;
        }
        Ok(())
    }

    fn execute_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectResult, OperatorAccountDisconnectError> {
        let result = self.settle_operator_account_disconnect(key, account_ref_id)?;
        // Both stores now agree the account is gone, so the live X session built
        // from its cookies goes too; they must not outlive the account in
        // process memory. Other platforms keep no cached session.
        self.sessions.forget_x_account(account_ref_id.as_str());
        Ok(result)
    }

    fn settle_operator_account_disconnect(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<OperatorAccountDisconnectResult, OperatorAccountDisconnectError> {
        let profile_id = Uuid::parse_str(account_ref_id.as_str())
            .map_err(|_| OperatorAccountDisconnectError::Internal)?;
        let resolved = self.resolve_operator_account_ref(profile_id)?;
        if &resolved != account_ref_id {
            return Err(OperatorAccountDisconnectError::NotFoundOrIdentityMismatch);
        }
        let resumed_remote_delete = match self
            .creators
            .begin_operator_account_disconnect(key, account_ref_id)
            .map_err(map_claim_error)?
        {
            OperatorAccountDisconnectClaim::Completed(result) => return Ok(result),
            OperatorAccountDisconnectClaim::RemoteVerified(_) => {
                let result = self
                    .creators
                    .complete_operator_account_disconnect(key, account_ref_id)
                    .map_err(|_| {
                        self.publish_changes([PlatformChangeScope::OperatorAccounts]);
                        OperatorAccountDisconnectError::RecoverableSameKeyRetry
                    })?;
                self.publish_changes([PlatformChangeScope::OperatorAccounts]);
                return Ok(result);
            }
            OperatorAccountDisconnectClaim::RemoteStarted(_) => true,
            OperatorAccountDisconnectClaim::Acquired(_) => false,
        };
        if !resumed_remote_delete {
            self.creators
                .mark_operator_account_disconnect_remote_started(key, account_ref_id)
                .map_err(|_| {
                    self.publish_changes([PlatformChangeScope::OperatorAccounts]);
                    OperatorAccountDisconnectError::RecoverableSameKeyRetry
                })?;
        }
        // A missing vault row is already the desired remote state. Treat it as
        // idempotent success so a recovered/replayed disconnect can finalize.
        let _deleted = self.delete_profile_credential(profile_id).map_err(|_| {
            self.publish_changes([PlatformChangeScope::OperatorAccounts]);
            OperatorAccountDisconnectError::RecoverableSameKeyRetry
        })?;
        // The stored credential is gone. Drop its live X session now as well,
        // so a failure recording the ledger steps below cannot leave its
        // cookies in process memory until a same-key retry.
        self.sessions.forget_x_account(account_ref_id.as_str());
        self.creators
            .mark_operator_account_disconnect_remote_verified(key, account_ref_id)
            .map_err(|_| {
                self.publish_changes([PlatformChangeScope::OperatorAccounts]);
                OperatorAccountDisconnectError::RecoverableSameKeyRetry
            })?;
        let result = self
            .creators
            .complete_operator_account_disconnect(key, account_ref_id)
            .map_err(|_| {
                self.publish_changes([PlatformChangeScope::OperatorAccounts]);
                OperatorAccountDisconnectError::RecoverableSameKeyRetry
            })?;
        self.publish_changes([PlatformChangeScope::OperatorAccounts]);
        Ok(result)
    }

    fn resolve_operator_account_ref(
        &self,
        profile_id: Uuid,
    ) -> Result<AccountRefId, OperatorAccountDisconnectError> {
        let account_ref_id = AccountRefId::new(profile_id.to_string());
        let mut account = None;
        for platform in [
            onecrew_creator_runtime::Platform::X,
            onecrew_creator_runtime::Platform::Instagram,
            onecrew_creator_runtime::Platform::Telegram,
        ] {
            let accounts = self
                .creators
                .list_account_refs(platform, AccountKind::Profile)
                .map_err(|_| OperatorAccountDisconnectError::Internal)?;
            if let Some(found) = accounts
                .into_iter()
                .find(|candidate| candidate.id == account_ref_id)
            {
                account = Some(found);
                break;
            }
        }
        let account = account.ok_or(OperatorAccountDisconnectError::NotFoundOrIdentityMismatch)?;
        let profiles = self
            .vault
            .list(None)
            .map_err(|_| OperatorAccountDisconnectError::Internal)?;
        if let Some(profile) = profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            && (creator_platform(profile.platform) != account.platform
                || !profile
                    .username
                    .eq_ignore_ascii_case(account.label.trim_start_matches('@')))
        {
            return Err(OperatorAccountDisconnectError::NotFoundOrIdentityMismatch);
        }
        Ok(account_ref_id)
    }
}

fn map_claim_error(error: AccountLifecycleClaimError) -> OperatorAccountDisconnectError {
    match error {
        AccountLifecycleClaimError::IdempotencyConflict => {
            OperatorAccountDisconnectError::IdempotencyConflict
        }
        AccountLifecycleClaimError::WrongKind => {
            OperatorAccountDisconnectError::NotFoundOrIdentityMismatch
        }
        AccountLifecycleClaimError::Internal => OperatorAccountDisconnectError::Internal,
        AccountLifecycleClaimError::OutreachInProgress => {
            OperatorAccountDisconnectError::OutreachInProgress
        }
    }
}

fn creator_platform(platform: Platform) -> onecrew_creator_runtime::Platform {
    match platform {
        Platform::X => onecrew_creator_runtime::Platform::X,
        Platform::Instagram => onecrew_creator_runtime::Platform::Instagram,
        Platform::Telegram => onecrew_creator_runtime::Platform::Telegram,
    }
}
