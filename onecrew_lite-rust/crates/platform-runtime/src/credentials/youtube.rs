use crate::PlatformChangeScope;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use onecrew_creator_runtime::{
    AccountAvailability, AccountKind, AccountLifecycleClaimError, AccountRef, AccountRefId,
    CreatorRuntime, Platform, ServiceCredentialDeleteClaim,
};
use onecrew_local_domain::{ServiceCredential, ServiceProvider};
use onecrew_local_vault::LocalVault;

use super::{
    ServiceCredentialDeleteResult, ServiceCredentialProjection, YoutubeCredentialDeleteError,
    YoutubeCredentialDeleteInput, YoutubeCredentialDeleteOutcome,
};
use crate::LocalPlatform;
use crate::credentials::services::{CredentialServices, blocking_segment};

mod detail;

pub use detail::{
    YoutubeCredentialAvailability, YoutubeCredentialIssue, YoutubeCredentialRejectionIssue,
    YoutubePlatformDetail, YoutubePlatformGetInput, YoutubePlatformGetOutcome,
    YoutubePlatformTestError, YoutubePlatformTestInput, YoutubePlatformTestResult,
};

impl LocalPlatform {
    pub(crate) fn reconcile_vault_youtube_refs(&self) -> Result<Vec<ServiceCredential>, String> {
        reconcile_youtube_refs(&self.credential_services, &self.creators, &self.vault)
    }

    /// [`Self::reconcile_vault_youtube_refs`] on the blocking pool, for async
    /// callers: it reads the vault and writes Creator account references.
    pub(crate) async fn reconcile_vault_youtube_refs_blocking(
        &self,
    ) -> Result<Vec<ServiceCredential>, String> {
        let services = Arc::clone(&self.credential_services);
        let creators = Arc::clone(&self.creators);
        let vault = self.vault.clone();
        blocking_segment(
            "YouTube credential reconciliation was interrupted".to_owned(),
            move || reconcile_youtube_refs(&services, &creators, &vault),
        )
        .await
    }

    /// List configured YouTube Data API keys without exposing their secret
    /// values. Pure read: reconciliation happens in explicit lifecycle paths
    /// (startup, connect, rotate, test, delete), never on a list.
    pub fn list_youtube_api_keys(&self) -> Result<Vec<ServiceCredential>, String> {
        self.vault
            .list_service_credentials(onecrew_local_domain::ServiceProvider::Youtube, false)
            .map_err(|error| error.to_string())
    }

    pub fn list_service_credentials(&self) -> Result<Vec<ServiceCredentialProjection>, String> {
        let credentials = self.list_youtube_api_keys()?;
        let mut current = credentials
            .into_iter()
            .map(|credential| (credential.id.to_string(), credential))
            .collect::<HashMap<_, _>>();
        self.creators
            .list_account_refs(Platform::YouTube, AccountKind::ServiceCredential)?
            .into_iter()
            .map(|account| {
                if let Some(credential) = current.remove(account.id.as_str()) {
                    self.project_service_credential(credential)
                } else {
                    Ok(historical_service_credential_projection(account))
                }
            })
            .collect()
    }

    pub(crate) fn project_service_credential(
        &self,
        mut credential: ServiceCredential,
    ) -> Result<ServiceCredentialProjection, String> {
        credential.last_error = credential
            .last_error
            .is_some()
            .then(|| "YouTube credential has a recorded failure".into());
        let account_ref_id = AccountRefId::new(credential.id.to_string());
        let account = self.creators.get_account_ref(&account_ref_id)?;
        Ok(ServiceCredentialProjection {
            account_ref_id,
            provider: credential.provider.as_str().into(),
            label: credential.label,
            active: account.active,
            available: account.availability == AccountAvailability::Available,
            disabled: credential.disabled,
            cooldown_until: credential.cooldown_until,
            last_error: credential.last_error,
        })
    }

    /// Store an encrypted YouTube Data API key. Reusing a label replaces that key.
    #[cfg(test)]
    pub(crate) fn put_youtube_api_key(
        &self,
        label: impl Into<String>,
        project: Option<String>,
        api_key: impl Into<String>,
    ) -> Result<ServiceCredential, String> {
        let label = label.into().trim().to_owned();
        let api_key = api_key.into().trim().to_owned();
        if label.is_empty() {
            return Err("请输入 Key 名称".into());
        }
        if api_key.is_empty() {
            return Err("请输入 YouTube API Key".into());
        }
        let credential = self
            .credential_services
            .put_youtube_credential(label, project, api_key)?;
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        self.reconcile_vault_youtube_refs()?;
        Ok(credential)
    }

    pub fn delete_youtube_credential(
        &self,
        input: YoutubeCredentialDeleteInput,
    ) -> Result<YoutubeCredentialDeleteOutcome, YoutubeCredentialDeleteError> {
        if !input.confirmed {
            return Err(YoutubeCredentialDeleteError::ConfirmationRequired);
        }
        if input.idempotency_key.trim().is_empty() || input.idempotency_key.trim().len() > 128 {
            return Err(YoutubeCredentialDeleteError::IdempotencyKeyInvalid);
        }
        let credential_id = uuid::Uuid::parse_str(&input.credential_id)
            .map_err(|_| YoutubeCredentialDeleteError::InvalidCredentialId)?;
        let account_ref_id = self.resolve_youtube_account_ref(credential_id)?;
        let result =
            self.execute_service_credential_delete(&input.idempotency_key, &account_ref_id)?;
        Ok(YoutubeCredentialDeleteOutcome {
            credential_id: account_ref_id.as_str().to_string(),
            replayed: result.replayed,
        })
    }

    pub(crate) fn recover_pending_service_credential_deletes(&self) -> Result<(), String> {
        for (key, recovery) in self.creators.pending_service_credential_deletes()? {
            self.execute_service_credential_delete(&key, &recovery.account_ref_id)
                .map_err(|error| error.safe_message().to_string())?;
        }
        Ok(())
    }

    fn execute_service_credential_delete(
        &self,
        key: &str,
        account_ref_id: &AccountRefId,
    ) -> Result<ServiceCredentialDeleteResult, YoutubeCredentialDeleteError> {
        let credential_id = uuid::Uuid::parse_str(account_ref_id.as_str())
            .map_err(|_| YoutubeCredentialDeleteError::Internal)?;
        let resolved = self.resolve_youtube_account_ref(credential_id)?;
        if &resolved != account_ref_id {
            return Err(YoutubeCredentialDeleteError::WrongKind);
        }
        match self
            .creators
            .begin_service_credential_delete(key, account_ref_id)
            .map_err(map_youtube_claim_error)?
        {
            ServiceCredentialDeleteClaim::Completed(result) => {
                return Ok(result);
            }
            ServiceCredentialDeleteClaim::RemoteVerified(_) => {
                let result = self
                    .creators
                    .complete_service_credential_delete(key, account_ref_id)
                    .map_err(|_| {
                        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
                        YoutubeCredentialDeleteError::RecoverableSameKeyRetry
                    })?;
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
                return Ok(result);
            }
            ServiceCredentialDeleteClaim::Acquired(_) => {}
        }
        // Missing from the vault is already the desired external state.
        self.credential_services
            .delete_youtube_credential(&credential_id.to_string())
            .map_err(|_| {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
                YoutubeCredentialDeleteError::RecoverableSameKeyRetry
            })?;
        self.creators
            .mark_service_credential_delete_remote_verified(key, account_ref_id)
            .map_err(|_| {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
                YoutubeCredentialDeleteError::RecoverableSameKeyRetry
            })?;
        let result = self
            .creators
            .complete_service_credential_delete(key, account_ref_id)
            .map_err(|_| {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
                YoutubeCredentialDeleteError::RecoverableSameKeyRetry
            })?;
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        Ok(result)
    }

    fn resolve_youtube_account_ref(
        &self,
        credential_id: uuid::Uuid,
    ) -> Result<AccountRefId, YoutubeCredentialDeleteError> {
        let account_ref_id = AccountRefId::new(credential_id.to_string());
        let youtube_accounts = self
            .creators
            .list_account_refs(Platform::YouTube, AccountKind::ServiceCredential)
            .map_err(|_| YoutubeCredentialDeleteError::Internal)?;
        let account = if let Some(account) = youtube_accounts
            .into_iter()
            .find(|candidate| candidate.id == account_ref_id)
        {
            account
        } else {
            for platform in [Platform::X, Platform::Instagram, Platform::Telegram] {
                let profiles = self
                    .creators
                    .list_account_refs(platform, AccountKind::Profile)
                    .map_err(|_| YoutubeCredentialDeleteError::Internal)?;
                if profiles
                    .into_iter()
                    .any(|candidate| candidate.id == account_ref_id)
                {
                    return Err(YoutubeCredentialDeleteError::WrongKind);
                }
            }
            let resend = self
                .vault
                .list_service_credentials(ServiceProvider::Resend, false)
                .map_err(|_| YoutubeCredentialDeleteError::Internal)?;
            if resend
                .into_iter()
                .any(|credential| credential.id == credential_id)
            {
                return Err(YoutubeCredentialDeleteError::WrongKind);
            }
            return Err(YoutubeCredentialDeleteError::NotFound);
        };
        let youtube = self
            .vault
            .list_service_credentials(ServiceProvider::Youtube, false)
            .map_err(|_| YoutubeCredentialDeleteError::Internal)?;
        if let Some(credential) = youtube
            .into_iter()
            .find(|credential| credential.id == credential_id)
            && credential.label != account.label
        {
            return Err(YoutubeCredentialDeleteError::WrongKind);
        }
        Ok(account_ref_id)
    }
}

fn map_youtube_claim_error(error: AccountLifecycleClaimError) -> YoutubeCredentialDeleteError {
    match error {
        AccountLifecycleClaimError::IdempotencyConflict => {
            YoutubeCredentialDeleteError::IdempotencyConflict
        }
        AccountLifecycleClaimError::WrongKind => YoutubeCredentialDeleteError::WrongKind,
        AccountLifecycleClaimError::OutreachInProgress => {
            YoutubeCredentialDeleteError::OutreachInProgress
        }
        AccountLifecycleClaimError::Internal => YoutubeCredentialDeleteError::Internal,
    }
}

fn reconcile_youtube_refs(
    services: &CredentialServices,
    creators: &CreatorRuntime,
    vault: &LocalVault,
) -> Result<Vec<ServiceCredential>, String> {
    let credentials = services
        .list_youtube_credentials()?
        .into_iter()
        .map(sanitize_youtube_credential_projection)
        .collect::<Vec<_>>();
    let present_ids = credentials
        .iter()
        .map(|credential| credential.id.to_string())
        .collect::<HashSet<_>>();
    for credential in &credentials {
        creators.sync_account_ref_with_state(
            AccountRefId::new(credential.id.to_string()),
            Platform::YouTube,
            AccountKind::ServiceCredential,
            &credential.label,
            youtube_credential_availability(vault, credential),
        )?;
    }
    for account in creators.list_account_refs(Platform::YouTube, AccountKind::ServiceCredential)? {
        if !present_ids.contains(account.id.as_str()) {
            creators.sync_account_ref_with_state(
                account.id,
                Platform::YouTube,
                AccountKind::ServiceCredential,
                &account.label,
                AccountAvailability::Unavailable,
            )?;
        }
    }
    Ok(credentials)
}

fn youtube_credential_availability(
    vault: &LocalVault,
    credential: &ServiceCredential,
) -> AccountAvailability {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let metadata_is_valid = credential.provider == ServiceProvider::Youtube
        && credential
            .metadata
            .get("api")
            .and_then(serde_json::Value::as_str)
            == Some("youtube_data_api_v3");
    let cooldown_active = credential.cooldown_until.is_some_and(|until| until > now);
    if metadata_is_valid
        && !credential.disabled
        && !cooldown_active
        && credential.last_error.is_none()
        && vault.get_service_credential(credential.id).is_ok()
    {
        AccountAvailability::Available
    } else {
        AccountAvailability::Unavailable
    }
}

fn sanitize_youtube_credential_projection(mut credential: ServiceCredential) -> ServiceCredential {
    credential.last_error = credential
        .last_error
        .is_some()
        .then(|| "YouTube credential has a recorded failure".into());
    credential
}

fn historical_service_credential_projection(account: AccountRef) -> ServiceCredentialProjection {
    ServiceCredentialProjection {
        account_ref_id: account.id,
        provider: account.platform.as_str().into(),
        label: account.label,
        active: account.active,
        available: false,
        disabled: true,
        cooldown_until: None,
        last_error: Some("credential is missing from the local vault".into()),
    }
}

#[cfg(test)]
mod tests;
