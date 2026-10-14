use crate::PlatformChangeScope;
use std::{fs::OpenOptions, path::PathBuf, sync::Arc};

use onecrew_creator_runtime::{
    CreatorRuntime, EmailAuthConnectionDeletionClaim, EmailCapability, MailboxIdentityMappingInput,
    MailboxProjectionKind, MailboxRefId, MailboxRefInput, resend_auth_connection_id,
};
use onecrew_local_domain::ServiceCredential;

use super::ResendCredentialDeletionError;
use super::services::{CredentialServices, ResendCredentialTestError, blocking_segment};
use crate::{LocalPlatform, ResendCredentialTest};

impl LocalPlatform {
    pub(crate) fn reconcile_vault_mailbox_refs(&self) -> Result<Vec<ServiceCredential>, String> {
        reconcile_mailbox_refs(&self.credential_services, &self.creators)
    }

    /// [`Self::reconcile_vault_mailbox_refs`] on the blocking pool, for async
    /// callers: it reads the vault and writes the Creator mailbox projection.
    async fn reconcile_vault_mailbox_refs_blocking(
        &self,
    ) -> Result<Vec<ServiceCredential>, String> {
        let services = Arc::clone(&self.credential_services);
        let creators = Arc::clone(&self.creators);
        blocking_segment(
            "Resend mailbox reconciliation was interrupted".to_owned(),
            move || reconcile_mailbox_refs(&services, &creators),
        )
        .await
    }

    /// List Resend mailbox credentials without exposing API keys. Pure read:
    /// reconciliation happens in explicit lifecycle paths (startup, connect,
    /// rotate, test, delete), never on a list.
    pub fn list_resend_credentials(&self) -> Result<Vec<ServiceCredential>, String> {
        self.credential_services.list_resend_credentials()
    }

    /// Store a Resend API key in the vault and synchronize only its public mailbox projection.
    #[cfg(test)]
    pub(crate) fn put_resend_credential(
        &self,
        label: impl Into<String>,
        email_address: impl Into<String>,
        display_name: Option<String>,
        api_key: impl Into<String>,
    ) -> Result<ServiceCredential, String> {
        let credential = self.credential_services.put_resend_credential(
            label,
            email_address,
            display_name,
            api_key,
        )?;
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        self.reconcile_vault_mailbox_refs()?;
        Ok(credential)
    }

    pub fn delete_resend_credential(
        &self,
        input: crate::ResendCredentialDeleteInput,
    ) -> Result<bool, ResendCredentialDeletionError> {
        if !input.confirmed {
            return Err(ResendCredentialDeletionError::InvalidRequest {
                message: "credential deletion requires explicit confirmation".into(),
            });
        }
        let credential_id = uuid::Uuid::parse_str(input.credential_id.trim()).map_err(|_| {
            ResendCredentialDeletionError::InvalidRequest {
                message: "Resend credential id is invalid".into(),
            }
        })?;
        let _deletion_lock = acquire_resend_credential_deletion_lock(&self.data_dir)
            .map_err(|message| ResendCredentialDeletionError::Runtime { message })?;
        self.reconcile_vault_mailbox_refs()
            .map_err(|message| ResendCredentialDeletionError::Runtime { message })?;
        let mailbox_ref_id = MailboxRefId::new(credential_id.to_string());
        // Resolve the canonical typed mapping first. Deletion is scoped to the
        // auth connection, not one mailbox label/address: one Resend
        // connection may own multiple mailbox/send-as projections.
        let binding = self
            .creators
            .resolve_mailbox_auth_binding_blocking(&mailbox_ref_id)
            .map_err(|message| ResendCredentialDeletionError::Runtime { message })?;
        if binding.mailbox_ref_id != mailbox_ref_id
            || binding.auth_connection.provider_kind
                != onecrew_creator_runtime::EmailProviderKind::Resend
            || binding.auth_connection.secret_record_id.as_str() != credential_id.to_string()
        {
            return Err(ResendCredentialDeletionError::Runtime {
                message: "Resend credential mailbox mapping does not match the typed credential"
                    .into(),
            });
        }
        let auth_connection_id = binding.auth_connection.auth_connection_id;
        match self
            .creators
            .begin_email_auth_connection_deletion(&auth_connection_id)
            .map_err(|message| ResendCredentialDeletionError::Runtime { message })?
        {
            EmailAuthConnectionDeletionClaim::Acquired { .. } => {}
            EmailAuthConnectionDeletionClaim::Blocked { blockers, .. } => {
                return Err(ResendCredentialDeletionError::Blocked { blockers });
            }
        }
        let deleted = match self
            .credential_services
            .delete_resend_credential(&credential_id.to_string())
        {
            Ok(deleted) => deleted,
            Err(message) => {
                let release = self
                    .creators
                    .finish_email_auth_connection_deletion(&auth_connection_id);
                return Err(ResendCredentialDeletionError::Runtime {
                    message: match release {
                        Ok(()) => message,
                        Err(release_error) => format!(
                            "{message}; failed to release mailbox credential deletion guard: {release_error}"
                        ),
                    },
                });
            }
        };
        if deleted {
            self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        }
        if let Err(message) = self.reconcile_vault_mailbox_refs() {
            return Err(ResendCredentialDeletionError::Runtime {
                message: format!(
                    "{message}; mailbox credential deletion remains guarded and can be resumed by retrying the deletion"
                ),
            });
        }
        self.creators
            .finish_email_auth_connection_deletion(&auth_connection_id)
            .map_err(|message| ResendCredentialDeletionError::Runtime { message })?;
        Ok(deleted)
    }

    pub fn delete_resend_credential_outcome(
        &self,
        input: crate::ResendCredentialDeleteInput,
    ) -> Result<crate::ResendCredentialDeleteOutcome, ResendCredentialDeletionError> {
        let credential_id = input.credential_id.trim().to_owned();
        let deleted = self.delete_resend_credential(input)?;
        Ok(crate::ResendCredentialDeleteOutcome {
            credential_id,
            deleted,
            replayed: !deleted,
        })
    }

    pub async fn test_resend_credential(
        &self,
        credential_id: &str,
    ) -> Result<ResendCredentialTest, String> {
        self.test_resend_credential_typed(credential_id)
            .await
            .map_err(|error| error.to_string())
    }

    pub(crate) async fn test_resend_credential_typed(
        &self,
        credential_id: &str,
    ) -> Result<ResendCredentialTest, ResendCredentialTestError> {
        let result = self
            .credential_services
            .test_resend_credential(credential_id)
            .await;
        let reconcile_result = self.reconcile_vault_mailbox_refs_blocking().await;
        self.finish_resend_credential_test(result, reconcile_result)
    }

    fn finish_resend_credential_test(
        &self,
        result: Result<ResendCredentialTest, ResendCredentialTestError>,
        reconcile_result: Result<Vec<ServiceCredential>, String>,
    ) -> Result<ResendCredentialTest, ResendCredentialTestError> {
        // Provider errors are returned only after their health write succeeds.
        // Reconciliation failure cannot undo that independent vault commit.
        if result.is_ok() || matches!(&result, Err(ResendCredentialTestError::Provider(_))) {
            self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        }
        match (result, reconcile_result) {
            (Ok(value), Ok(_)) => Ok(value),
            (Ok(_), Err(error)) => Err(ResendCredentialTestError::Local(error)),
            (Err(error), Ok(_)) => Err(error),
            (Err(error), Err(reconcile_error)) => Err(ResendCredentialTestError::Local(format!(
                "{error}; failed to reconcile Resend mailbox projection: {reconcile_error}"
            ))),
        }
    }
}

fn reconcile_mailbox_refs(
    services: &CredentialServices,
    creators: &CreatorRuntime,
) -> Result<Vec<ServiceCredential>, String> {
    let credentials = services.list_resend_credentials()?;
    let inputs = credentials
        .iter()
        .map(resend_mailbox_ref_input)
        .collect::<Result<Vec<_>, _>>()?;
    creators.reconcile_mailbox_refs(inputs)?;
    for credential in &credentials {
        creators.upsert_mailbox_identity_mapping(MailboxIdentityMappingInput {
            mailbox_ref_id: MailboxRefId::new(credential.id.to_string()),
            auth_connection_id: resend_auth_connection_id(credential.id),
            provider_kind: onecrew_creator_runtime::EmailProviderKind::Resend,
            secret_record_id: onecrew_creator_runtime::EmailSecretRecordId::new(
                credential.id.to_string(),
            ),
            connection_label: credential.label.clone(),
            projection_kind: MailboxProjectionKind::Mailbox,
            capability_projection_revision: 1,
            checkpoint_namespace: format!("email-checkpoint-{}", credential.id),
        })?;
    }
    Ok(credentials)
}

fn acquire_resend_credential_deletion_lock(
    data_dir: &std::path::Path,
) -> Result<std::fs::File, String> {
    let path: PathBuf = data_dir.join("resend-credential-delete.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("failed to open Resend credential deletion lock: {error}"))?;
    file.lock()
        .map_err(|error| format!("failed to acquire Resend credential deletion lock: {error}"))?;
    Ok(file)
}

fn resend_mailbox_ref_input(credential: &ServiceCredential) -> Result<MailboxRefInput, String> {
    let email_address = credential
        .metadata
        .get("email_address")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Resend credential metadata must contain email_address".to_string())?
        .to_ascii_lowercase();
    let display_name = credential
        .metadata
        .get("display_name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(MailboxRefInput {
        id: MailboxRefId::new(credential.id.to_string()),
        provider: "resend".into(),
        label: credential.label.clone(),
        email_address,
        display_name,
        capabilities: vec![
            EmailCapability::Send,
            EmailCapability::SentReadback,
            EmailCapability::Inbox,
            EmailCapability::Reply,
        ],
        available: !credential.disabled
            && credential
                .cooldown_until
                .is_none_or(|cooldown_until| cooldown_until <= now),
        last_error: credential.last_error.clone(),
    })
}

#[cfg(test)]
mod tests;
