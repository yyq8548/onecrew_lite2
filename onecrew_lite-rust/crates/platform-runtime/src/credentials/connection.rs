use crate::PlatformChangeScope;
use crate::credentials::services::{CredentialServices, YoutubeCredentialValidation};
use onecrew_local_domain::{ServiceCredential, ServiceCredentialSecret, ServiceProvider};

use super::{
    ConnectedServiceCredentialProjection, ServiceCredentialConnectResult, ServiceCredentialProjection,
    ServiceCredentialValidationProjection, YoutubeApiKeyConnectInput,
};
use crate::LocalPlatform;

#[async_trait::async_trait]
trait ServiceCredentialValidator: Send + Sync {
    async fn validate_youtube(&self, api_key: &str) -> Result<YoutubeCredentialValidation, String>;
}

trait ServiceCredentialReconciler: Send + Sync {
    fn reconcile_youtube(&self, platform: &LocalPlatform) -> Result<(), String>;
}

struct RuntimeServiceCredentialReconciler;

impl ServiceCredentialReconciler for RuntimeServiceCredentialReconciler {
    fn reconcile_youtube(&self, platform: &LocalPlatform) -> Result<(), String> {
        platform.reconcile_vault_youtube_refs().map(|_| ())
    }
}

/// Memory-only rollback material. Intentionally has no Debug, Serialize or
/// Clone implementation because it contains the decrypted previous key.
struct ServiceCredentialRollbackSnapshot {
    credential: ServiceCredential,
    secret: ServiceCredentialSecret,
}

#[async_trait::async_trait]
impl ServiceCredentialValidator for CredentialServices {
    async fn validate_youtube(&self, api_key: &str) -> Result<YoutubeCredentialValidation, String> {
        self.validate_youtube_api_key(api_key).await
    }
}

impl LocalPlatform {
    pub async fn connect_youtube_api_key_typed(
        &self,
        input: YoutubeApiKeyConnectInput,
    ) -> Result<ServiceCredentialConnectResult, crate::ServiceCredentialMutationError> {
        self.connect_youtube_api_key(input)
            .await
            .map_err(|message| crate::ServiceCredentialMutationError::from_safe_message(&message))
    }
    pub fn rename_youtube_credential_typed(
        &self,
        input: crate::RenameYoutubeCredentialInput,
    ) -> Result<ServiceCredentialProjection, crate::ServiceCredentialMutationError> {
        self.rename_youtube_credential(input)
            .map_err(|message| crate::ServiceCredentialMutationError::from_safe_message(&message))
    }

    pub async fn rotate_youtube_credential_typed(
        &self,
        input: crate::RotateYoutubeCredentialInput,
    ) -> Result<ServiceCredentialConnectResult, crate::ServiceCredentialMutationError> {
        self.rotate_youtube_credential(input)
            .await
            .map_err(|message| crate::ServiceCredentialMutationError::from_safe_message(&message))
    }


    /// Validate a raw YouTube Data API key with a temporary client, then and
    /// only then replace the encrypted credential and reconcile its Creator ref.
    pub async fn connect_youtube_api_key(
        &self,
        input: YoutubeApiKeyConnectInput,
    ) -> Result<ServiceCredentialConnectResult, String> {
        self.connect_youtube_api_key_with_dependencies(
            input,
            self.credential_services.as_ref(),
            &RuntimeServiceCredentialReconciler,
        )
        .await
    }

    #[cfg(test)]
    async fn connect_youtube_api_key_with_validator(
        &self,
        input: YoutubeApiKeyConnectInput,
        validator: &dyn ServiceCredentialValidator,
    ) -> Result<ServiceCredentialConnectResult, String> {
        self.connect_youtube_api_key_with_dependencies(
            input,
            validator,
            &RuntimeServiceCredentialReconciler,
        )
        .await
    }

    async fn connect_youtube_api_key_with_dependencies(
        &self,
        input: YoutubeApiKeyConnectInput,
        validator: &dyn ServiceCredentialValidator,
        reconciler: &dyn ServiceCredentialReconciler,
    ) -> Result<ServiceCredentialConnectResult, String> {
        let label = required_trimmed(input.label, "请输入 Key 名称")?;
        let project = input
            .project
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let api_key = required_trimmed(input.api_key, "请输入 YouTube API Key")?;
        if self
            .find_service_credential_by_label(ServiceProvider::Youtube, &label)?
            .is_some()
        {
            return Err(format!(
                "a YouTube credential labeled \"{label}\" already exists; rotate its key explicitly or choose a new label"
            ));
        }
        let validation = validator
            .validate_youtube(&api_key)
            .await
            .map_err(|error| redact_secret(&error, &api_key))?;
        let rollback = self
            .capture_service_credential(ServiceProvider::Youtube, &label)
            .map_err(|error| redact_secret(&error, &api_key))?;
        let credential = self
            .credential_services
            .put_youtube_credential(label, project, api_key.clone())
            .map_err(|error| redact_secret(&error, &api_key))?;
        if let Err(reconcile_error) = reconciler.reconcile_youtube(self) {
            let rollback_error = self.rollback_service_credential(&credential, rollback.as_ref());
            if rollback_error.is_err() {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
            }
            return Err(connect_reconciliation_error(
                &reconcile_error,
                rollback_error.as_ref().err().map(String::as_str),
                &api_key,
            ));
        }
        let result = connection_result(
            credential,
            validation.checked_at,
            validation.message,
            validation.channel_id,
        );
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        Ok(result)
    }

    /// Rename one existing YouTube credential in place. The encrypted key,
    /// provider metadata, health state and stable id are preserved; only the
    /// vault label and its synchronized Creator reference label change.
    pub fn rename_youtube_credential(
        &self,
        input: crate::RenameYoutubeCredentialInput,
    ) -> Result<ServiceCredentialProjection, String> {
        self.rename_youtube_credential_with_dependencies(input, &RuntimeServiceCredentialReconciler)
    }

    fn rename_youtube_credential_with_dependencies(
        &self,
        input: crate::RenameYoutubeCredentialInput,
        reconciler: &dyn ServiceCredentialReconciler,
    ) -> Result<ServiceCredentialProjection, String> {
        if !input.confirmed {
            return Err("credential rename requires explicit confirmation".into());
        }
        let credential_id = uuid::Uuid::parse_str(input.credential_id.trim())
            .map_err(|_| "invalid credential id".to_string())?;
        let label = required_trimmed(input.label, "请输入 Key 名称")?;
        let (existing, secret) = self
            .vault
            .get_service_credential(credential_id)
            .map_err(|error| error.to_string())?;
        if existing.provider != ServiceProvider::Youtube {
            return Err("credential is not a YouTube credential".into());
        }
        if let Some(existing_id) =
            self.find_service_credential_by_label(ServiceProvider::Youtube, &label)?
            && existing_id != credential_id
        {
            return Err(format!(
                "a YouTube credential labeled \"{label}\" already exists"
            ));
        }
        let rollback = ServiceCredentialRollbackSnapshot {
            credential: existing.clone(),
            secret,
        };
        let renamed = self
            .vault
            .rename_service_credential(credential_id, &label)
            .map_err(|error| error.to_string())?;
        if let Err(reconcile_error) = reconciler.reconcile_youtube(self) {
            let rollback_error = self.rollback_service_credential(&renamed, Some(&rollback));
            if rollback_error.is_err() {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
            }
            let creator_rollback = rollback_error
                .is_ok()
                .then(|| reconciler.reconcile_youtube(self));
            return Err(rename_reconciliation_error(
                &reconcile_error,
                rollback_error.as_ref().err().map(String::as_str),
                creator_rollback.is_some(),
                creator_rollback
                    .as_ref()
                    .and_then(|result| result.as_ref().err())
                    .map(String::as_str),
            ));
        }
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        let projection = self.project_service_credential(renamed)?;
        Ok(projection)
    }

    /// Rotate one existing YouTube credential's key in place: same label and
    /// project, a freshly validated key, explicit confirmation required.
    pub async fn rotate_youtube_credential(
        &self,
        input: crate::RotateYoutubeCredentialInput,
    ) -> Result<ServiceCredentialConnectResult, String> {
        self.rotate_youtube_credential_with_dependencies(
            input,
            self.credential_services.as_ref(),
            &RuntimeServiceCredentialReconciler,
        )
        .await
    }

    async fn rotate_youtube_credential_with_dependencies(
        &self,
        input: crate::RotateYoutubeCredentialInput,
        validator: &dyn ServiceCredentialValidator,
        reconciler: &dyn ServiceCredentialReconciler,
    ) -> Result<ServiceCredentialConnectResult, String> {
        if !input.confirmed {
            return Err("credential rotation requires explicit confirmation".into());
        }
        let credential_id = uuid::Uuid::parse_str(input.credential_id.trim())
            .map_err(|_| "invalid credential id".to_string())?;
        let api_key = required_trimmed(input.api_key, "请输入 YouTube API Key")?;
        let (existing, _secret) = self
            .vault
            .get_service_credential(credential_id)
            .map_err(|error| redact_secret(&error.to_string(), &api_key))?;
        if existing.provider != ServiceProvider::Youtube {
            return Err("credential is not a YouTube credential".into());
        }
        let project = existing
            .metadata
            .get("project")
            .and_then(|value| value.as_str())
            .map(str::to_owned);
        let validation = validator
            .validate_youtube(&api_key)
            .await
            .map_err(|error| redact_secret(&error, &api_key))?;
        let rollback = self
            .capture_service_credential(ServiceProvider::Youtube, &existing.label)
            .map_err(|error| redact_secret(&error, &api_key))?;
        let credential = self
            .credential_services
            .put_youtube_credential(existing.label, project, api_key.clone())
            .map_err(|error| redact_secret(&error, &api_key))?;
        if credential.id != existing.id {
            let rollback_error = self.rollback_service_credential(&credential, rollback.as_ref());
            if rollback_error.is_err() {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
            }
            return Err(connect_reconciliation_error(
                "rotation resolved to a different credential row",
                rollback_error.as_ref().err().map(String::as_str),
                &api_key,
            ));
        }
        if let Err(reconcile_error) = reconciler.reconcile_youtube(self) {
            let rollback_error = self.rollback_service_credential(&credential, rollback.as_ref());
            if rollback_error.is_err() {
                self.publish_changes([PlatformChangeScope::ServiceCredentials]);
            }
            return Err(connect_reconciliation_error(
                &reconcile_error,
                rollback_error.as_ref().err().map(String::as_str),
                &api_key,
            ));
        }
        let result = connection_result(
            credential,
            validation.checked_at,
            validation.message,
            validation.channel_id,
        );
        self.publish_changes([PlatformChangeScope::ServiceCredentials]);
        Ok(result)
    }

    fn find_service_credential_by_label(
        &self,
        provider: ServiceProvider,
        label: &str,
    ) -> Result<Option<uuid::Uuid>, String> {
        Ok(self
            .vault
            .list_service_credentials(provider, false)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|credential| credential.label == label)
            .map(|credential| credential.id))
    }

    fn capture_service_credential(
        &self,
        provider: ServiceProvider,
        label: &str,
    ) -> Result<Option<ServiceCredentialRollbackSnapshot>, String> {
        let Some(credential) = self
            .vault
            .list_service_credentials(provider, false)
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|credential| credential.label == label)
        else {
            return Ok(None);
        };
        let (credential, secret) = self
            .vault
            .get_service_credential(credential.id)
            .map_err(|error| error.to_string())?;
        Ok(Some(ServiceCredentialRollbackSnapshot {
            credential,
            secret,
        }))
    }

    fn rollback_service_credential(
        &self,
        candidate: &ServiceCredential,
        rollback: Option<&ServiceCredentialRollbackSnapshot>,
    ) -> Result<(), String> {
        match rollback {
            Some(rollback) => {
                if candidate.id != rollback.credential.id {
                    let deleted = self
                        .vault
                        .delete_service_credential(candidate.id)
                        .map_err(|error| error.to_string())?;
                    if !deleted {
                        return Err("candidate credential disappeared before rollback".into());
                    }
                }
                self.vault
                    .restore_service_credential(&rollback.credential, &rollback.secret)
                    .map_err(|error| error.to_string())
            }
            None => {
                let deleted = self
                    .vault
                    .delete_service_credential(candidate.id)
                    .map_err(|error| error.to_string())?;
                if deleted {
                    Ok(())
                } else {
                    Err("new candidate credential disappeared before rollback".into())
                }
            }
        }
    }
}

fn required_trimmed(value: String, message: &str) -> Result<String, String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        Err(message.into())
    } else {
        Ok(value)
    }
}

fn redact_secret(message: &str, secret: &str) -> String {
    let secret = secret.trim();
    if secret.is_empty() {
        return message.to_owned();
    }
    let mut redacted = message.replace(secret, "[REDACTED]");
    // Provider errors may echo the key percent-encoded inside a URL.
    let encoded = percent_encode_component(secret);
    if encoded != secret {
        redacted = redacted.replace(&encoded, "[REDACTED]");
    }
    redacted
}

/// Minimal application/x-www-form-urlencoded-style encoding of the secret so
/// the redactor also matches URL-embedded echoes.
fn percent_encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn connect_reconciliation_error(
    reconcile_error: &str,
    rollback_error: Option<&str>,
    secret: &str,
) -> String {
    let reconcile_error = redact_secret(reconcile_error, secret);
    match rollback_error {
        Some(rollback_error) => format!(
            "credential reconciliation failed: {reconcile_error}; credential rollback failed: {}",
            redact_secret(rollback_error, secret)
        ),
        None => format!(
            "credential reconciliation failed: {reconcile_error}; encrypted credential change was rolled back"
        ),
    }
}

fn rename_reconciliation_error(
    reconcile_error: &str,
    vault_rollback_error: Option<&str>,
    creator_rollback_attempted: bool,
    creator_rollback_error: Option<&str>,
) -> String {
    let mut message = format!("credential reconciliation failed: {reconcile_error}");
    match vault_rollback_error {
        Some(error) => message.push_str(&format!("; vault rollback failed: {error}")),
        None => message.push_str("; vault rename rolled back"),
    }
    if creator_rollback_attempted {
        match creator_rollback_error {
            Some(error) => {
                message.push_str(&format!("; Creator reference rollback failed: {error}"))
            }
            None => message.push_str("; Creator reference rollback completed"),
        }
    } else {
        message.push_str(
            "; Creator reference rollback was not attempted because vault rollback failed",
        );
    }
    message
}

fn connection_result(
    credential: ServiceCredential,
    checked_at: u64,
    message: String,
    provider_resource_id: Option<String>,
) -> ServiceCredentialConnectResult {
    let (youtube, mailbox) = match credential.provider {
        ServiceProvider::Youtube => (
            Some(crate::YoutubeCredentialProjectionDetails {
                project: credential
                    .metadata
                    .get("project")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned),
            }),
            None,
        ),
        ServiceProvider::Resend => (
            None,
            credential
                .metadata
                .get("email_address")
                .and_then(|value| value.as_str())
                .map(|email_address| crate::MailboxCredentialProjectionDetails {
                    email_address: email_address.to_owned(),
                    display_name: credential
                        .metadata
                        .get("display_name")
                        .and_then(|value| value.as_str())
                        .map(str::to_owned),
                }),
        ),
        ServiceProvider::CreatorSync => (None, None),
    };
    ServiceCredentialConnectResult {
        credential: ConnectedServiceCredentialProjection {
            credential_id: credential.id.to_string(),
            provider: credential.provider.as_str().into(),
            label: credential.label,
            youtube,
            mailbox,
            available: !credential.disabled,
        },
        health: ServiceCredentialValidationProjection {
            checked_at,
            message,
            provider_resource_id,
        },
    }
}

#[cfg(test)]
mod tests;
