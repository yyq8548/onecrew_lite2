use std::time::{SystemTime, UNIX_EPOCH};

use onecrew_email_contracts::{EmailCapability, EmailError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::CredentialServices;

#[cfg(test)]
use super::normalize_email_address;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct ResendMailboxMetadata {
    pub(super) email_address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) display_name: Option<String>,
}

use super::{ResolvedResendMailbox, blocking_segment};
use std::sync::Arc;

impl CredentialServices {
    pub(in crate::credentials) async fn resend_mailbox(
        self: &Arc<Self>,
        mailbox_ref_id: &str,
        required_capability: EmailCapability,
    ) -> Result<ResolvedResendMailbox, String> {
        let resolver = self.email_registry.mailbox_resolver()?;
        let binding = resolver
            .resolve_mailbox_auth_connection(&onecrew_creator_runtime::MailboxRefId::new(
                mailbox_ref_id,
            ))
            .await?;
        let services = Arc::clone(self);
        blocking_segment(
            "Email provider admission was interrupted".to_string(),
            move || {
                services
                    .email_registry
                    .resolve_binding(binding, required_capability)
            },
        )
        .await
    }

    pub(in crate::credentials) fn mark_resend_success(&self, id: Uuid) -> Result<(), String> {
        self.vault
            .mark_email_secret_record_success(
                id,
                onecrew_local_vault::EmailSecretRecordKind::ResendApiKey,
            )
            .map_err(|error| error.to_string())
    }

    /// Maps a provider failure to credential health. Cooldowns follow the
    /// provider's own `retry-after`/reset headers, bounded; quota exhaustion
    /// cools down for its documented window instead of being retried as a
    /// transient rate limit; authentication loss disables the credential.
    pub(in crate::credentials) fn mark_resend_failure(
        &self,
        id: Uuid,
        error: &EmailError,
    ) -> Result<(), String> {
        use onecrew_email_contracts::ProviderErrorClass;
        let (disable, cooldown_until) = match error.provider_failure() {
            Some(failure) => match failure.class {
                ProviderErrorClass::Auth => (true, None),
                ProviderErrorClass::RateLimited => {
                    let seconds = failure
                        .rate_limit
                        .retry_after_seconds
                        .or(failure.rate_limit.reset_seconds)
                        .unwrap_or(60)
                        .clamp(1, 900);
                    (false, Some(now().saturating_add(seconds)))
                }
                ProviderErrorClass::DailyQuotaExceeded
                | ProviderErrorClass::MonthlyQuotaExceeded => {
                    // `ratelimit-reset` is the per-second request window, not
                    // the email-quota renewal. Honor an explicit Retry-After;
                    // otherwise use the documented daily recovery horizon.
                    let seconds = failure
                        .rate_limit
                        .retry_after_seconds
                        .unwrap_or(86_400)
                        .clamp(60, 86_400);
                    (false, Some(now().saturating_add(seconds)))
                }
                _ => (false, None),
            },
            None => (false, None),
        };
        self.vault
            .mark_email_secret_record_failure(
                id,
                onecrew_local_vault::EmailSecretRecordKind::ResendApiKey,
                &error.to_string(),
                disable,
                cooldown_until,
            )
            .map_err(|error| error.to_string())
    }
}

pub(super) fn parse_resend_credential_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| "invalid Resend credential id".to_string())
}

#[cfg(test)]
pub(super) fn parse_resend_metadata(
    value: &serde_json::Value,
) -> Result<ResendMailboxMetadata, String> {
    let mut metadata: ResendMailboxMetadata =
        serde_json::from_value(value.clone()).map_err(|_| {
            "Resend credential metadata must contain email_address and optional display_name"
                .to_string()
        })?;
    metadata.email_address = normalize_email_address(&metadata.email_address)?;
    metadata.display_name = metadata
        .display_name
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    Ok(metadata)
}

pub(super) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::EmailMailboxAuthResolver;
    use onecrew_creator_runtime::{
        EmailAuthConnection, EmailAuthConnectionId, EmailProviderKind, EmailSecretRecordId,
        MailboxRefId,
    };
    use onecrew_local_vault::{EmailSecretRecordKind, EmailSecretRecordSecret};
    use std::sync::Arc;
    use tempfile::tempdir;

    struct StaticResolver(EmailAuthConnection);
    #[async_trait::async_trait]
    impl EmailMailboxAuthResolver for StaticResolver {
        async fn resolve_mailbox_auth_connection(
            &self,
            _mailbox_ref_id: &MailboxRefId,
        ) -> Result<onecrew_creator_runtime::MailboxAuthBinding, String> {
            Ok(onecrew_creator_runtime::MailboxAuthBinding {
                mailbox_ref_id: MailboxRefId::new("mailbox-1"),
                provider: onecrew_creator_runtime::MailboxProvider::Resend,
                label: self.0.label.clone(),
                email_address: "partnerships@reachlabs.co".into(),
                display_name: Some("Reach Labs".into()),
                capabilities: vec![
                    onecrew_creator_runtime::EmailCapability::Send,
                    onecrew_creator_runtime::EmailCapability::Inbox,
                ],
                available: self.0.available,
                auth_connection: self.0.clone(),
            })
        }
    }

    #[test]
    fn metadata_contract_is_strict_and_normalizes_sender() {
        let metadata = parse_resend_metadata(&serde_json::json!({
            "email_address": " Partnerships@ReachLabs.Co ",
            "display_name": " Reach Labs "
        }))
        .unwrap();
        assert_eq!(metadata.email_address, "partnerships@reachlabs.co");
        assert_eq!(metadata.display_name.as_deref(), Some("Reach Labs"));
        assert!(parse_resend_metadata(&serde_json::json!({"from": "x@y.com"})).is_err());
    }

    #[tokio::test]
    async fn credential_list_exposes_sender_metadata_but_never_api_key() {
        let root = tempdir().unwrap();
        let vault = onecrew_local_vault::LocalVault::open(root.path().join("vault.db")).unwrap();
        let adapter = Arc::new(CredentialServices::new(
            vault.clone(),
            Arc::new(onecrew_email::ResendAdmission::new().unwrap()),
        ));
        let saved = vault
            .upsert_email_secret_record(
                EmailSecretRecordKind::ResendApiKey,
                "partnerships",
                serde_json::json!({"email_address":"Partnerships@ReachLabs.co", "display_name":"Reach Labs"}),
                &EmailSecretRecordSecret::new("re_secret_key"),
            )
            .unwrap();
        adapter.bind_email_mailbox_resolver(Arc::new(StaticResolver(EmailAuthConnection {
            auth_connection_id: EmailAuthConnectionId::new("auth-1"),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new(saved.id.to_string()),
            label: "partnerships".into(),
            available: true,
            created_at: 0,
            updated_at: 0,
        })));

        let mailbox = adapter
            .resend_mailbox("mailbox-1", EmailCapability::Inbox)
            .await
            .unwrap();
        assert_eq!(mailbox.email_address, "partnerships@reachlabs.co");
    }

    #[tokio::test]
    async fn registry_rejects_non_resend_provider_without_fallback() {
        let root = tempdir().unwrap();
        let vault = onecrew_local_vault::LocalVault::open(root.path().join("vault.db")).unwrap();
        let adapter = Arc::new(CredentialServices::new(
            vault,
            Arc::new(onecrew_email::ResendAdmission::new().unwrap()),
        ));
        adapter.bind_email_mailbox_resolver(Arc::new(StaticResolver(EmailAuthConnection {
            auth_connection_id: EmailAuthConnectionId::new("auth-smtp"),
            provider_kind: EmailProviderKind::Smtp,
            secret_record_id: EmailSecretRecordId::new("not-a-uuid"),
            label: "smtp".into(),
            available: true,
            created_at: 0,
            updated_at: 0,
        })));
        let error = adapter
            .resend_mailbox("mailbox-1", EmailCapability::Send)
            .await
            .err()
            .unwrap();
        assert!(error.contains("unsupported Email provider kind: smtp"));
    }

    #[tokio::test]
    async fn registry_rejects_malformed_secret_reference_fail_closed() {
        let root = tempdir().unwrap();
        let vault = onecrew_local_vault::LocalVault::open(root.path().join("vault.db")).unwrap();
        let adapter = Arc::new(CredentialServices::new(
            vault,
            Arc::new(onecrew_email::ResendAdmission::new().unwrap()),
        ));
        adapter.bind_email_mailbox_resolver(Arc::new(StaticResolver(EmailAuthConnection {
            auth_connection_id: EmailAuthConnectionId::new("auth-resend"),
            provider_kind: EmailProviderKind::Resend,
            secret_record_id: EmailSecretRecordId::new("re_secret_legacy_alias"),
            label: "resend".into(),
            available: true,
            created_at: 0,
            updated_at: 0,
        })));
        let error = adapter
            .resend_mailbox("mailbox-1", EmailCapability::Send)
            .await
            .err()
            .unwrap();
        assert!(error.contains("email secret record reference is invalid"));
    }
}
