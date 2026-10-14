use std::sync::Arc;

use onecrew_email::EmailSender;
use onecrew_email_contracts::{EmailCapability, EmailError, ProviderEmailDomain};
use onecrew_local_domain::{ServiceCredential, ServiceCredentialSecret, ServiceProvider};
use onecrew_local_vault::{EmailSecretRecord, EmailSecretRecordKind, EmailSecretRecordSecret};
use serde::{Deserialize, Serialize};

use super::{CredentialServices, blocking_segment};

use super::mailbox::{ResendMailboxMetadata, now, parse_resend_credential_id};
use super::normalize_email_address;

impl CredentialServices {
    /// Validate a candidate Resend key with a throwaway sender. No credential
    /// is persisted and no health row is updated by this operation.
    pub(crate) async fn validate_resend_credential(
        &self,
        email_address: &str,
        display_name: Option<&str>,
        api_key: &str,
    ) -> Result<ResendCredentialValidation, String> {
        let email_address = normalize_email_address(email_address)?;
        let display_name = display_name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let api_key = required_trimmed(api_key.to_owned(), "Resend API key is required")?;
        let sender = EmailSender::from_resend_api_key(
            api_key.clone(),
            std::sync::Arc::clone(&self.email_transport),
        )
        .map_err(|error| sanitize_resend_error(&error.to_string(), &api_key))?;
        let sender_domain = validate_resend_sender_domain(&sender, &email_address)
            .await
            .map_err(|error| sanitize_resend_error(&error.to_string(), &api_key))?;
        // Full access is proven read-only in both directions; a key that
        // cannot list either direction fails closed and is never persisted
        // as a usable mailbox. No email is sent to validate a credential.
        sender
            .sent_page(None, 1)
            .await
            .map_err(|error| sanitize_resend_error(&error.to_string(), &api_key))?;
        sender
            .received(None, 1)
            .await
            .map_err(|error| sanitize_resend_error(&error.to_string(), &api_key))?;
        Ok(ResendCredentialValidation {
            checked_at: now(),
            email_address,
            display_name,
            sender_domain: sender_domain.clone(),
            sender_domain_verified: true,
            message: format!(
                "Resend API key has full read access to sent and received history; sender domain {sender_domain} is verified for sending"
            ),
        })
    }

    pub(crate) fn list_resend_credentials(&self) -> Result<Vec<ServiceCredential>, String> {
        self.vault
            .list_email_secret_records(EmailSecretRecordKind::ResendApiKey, false)
            .map(|records| {
                records
                    .into_iter()
                    .map(project_service_credential)
                    .collect()
            })
            .map_err(|error| error.to_string())
    }

    pub(crate) fn put_resend_credential(
        &self,
        label: impl Into<String>,
        email_address: impl Into<String>,
        display_name: Option<String>,
        api_key: impl Into<String>,
    ) -> Result<ServiceCredential, String> {
        let label = required_trimmed(label.into(), "mailbox label is required")?;
        let email_address = normalize_email_address(&email_address.into())?;
        let display_name = display_name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let api_key = required_trimmed(api_key.into(), "Resend API key is required")?;
        let metadata = serde_json::to_value(ResendMailboxMetadata {
            email_address,
            display_name,
        })
        .map_err(|error| error.to_string())?;

        self.vault
            .upsert_email_secret_record(
                EmailSecretRecordKind::ResendApiKey,
                label,
                metadata,
                &EmailSecretRecordSecret::new(api_key),
            )
            .map(project_service_credential)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn delete_resend_credential(&self, credential_id: &str) -> Result<bool, String> {
        self.vault
            .delete_email_secret_record(
                parse_resend_credential_id(credential_id)?,
                EmailSecretRecordKind::ResendApiKey,
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn get_resend_credential(
        &self,
        credential_id: &str,
    ) -> Result<(ServiceCredential, ServiceCredentialSecret), String> {
        let id = parse_resend_credential_id(credential_id)?;
        let (record, secret) = self
            .vault
            .get_email_secret_record_for_connection(
                &id.to_string(),
                EmailSecretRecordKind::ResendApiKey,
            )
            .map_err(|error| error.to_string())?;
        Ok((
            project_service_credential(record),
            ServiceCredentialSecret::new(secret.expose_secret()),
        ))
    }

    pub(crate) fn get_resend_typed_credential(
        &self,
        credential_id: &str,
    ) -> Result<(EmailSecretRecord, EmailSecretRecordSecret), String> {
        let id = parse_resend_credential_id(credential_id)?;
        self.vault
            .get_email_secret_record_for_connection(
                &id.to_string(),
                EmailSecretRecordKind::ResendApiKey,
            )
            .map_err(|error| error.to_string())
    }

    pub(crate) fn restore_resend_typed_credential(
        &self,
        record: &EmailSecretRecord,
        secret: &EmailSecretRecordSecret,
    ) -> Result<(), String> {
        self.vault
            .restore_email_secret_record(record, secret)
            .map_err(|error| error.to_string())
    }

    /// Probes a stored mailbox key and records its health. Resolving the
    /// binding uses Creator async read admission; vault/cache admission and
    /// the health write use blocking segments around the provider round trip.
    pub(crate) async fn test_resend_credential(
        self: &Arc<Self>,
        credential_id: &str,
    ) -> Result<ResendCredentialTest, ResendCredentialTestError> {
        let mailbox = self
            .resend_mailbox(credential_id, EmailCapability::Inbox)
            .await
            .map_err(ResendCredentialTestError::MailboxUnavailable)?;
        let result = async {
            let sender_domain =
                validate_resend_sender_domain(&mailbox.sender, &mailbox.email_address).await?;
            mailbox.sender.sent_page(None, 1).await?;
            mailbox.sender.received(None, 1).await?;
            Ok::<_, EmailError>(sender_domain)
        }
        .await;
        let services = Arc::clone(self);
        let (credential_id, email_address) = (mailbox.credential_id, mailbox.email_address);
        blocking_segment(interrupted(), move || match result {
            Ok(sender_domain) => {
                services
                    .mark_resend_success(credential_id)
                    .map_err(ResendCredentialTestError::Local)?;
                Ok(ResendCredentialTest {
                    credential_id: credential_id.to_string(),
                    ok: true,
                    checked_at: now(),
                    email_address,
                    message: format!(
                        "Resend API key has full read access to sent and received history; sender domain {sender_domain} is verified for sending"
                    ),
                })
            }
            Err(error) => {
                services
                    .mark_resend_failure(credential_id, &error)
                    .map_err(ResendCredentialTestError::Local)?;
                Err(ResendCredentialTestError::Provider(error))
            }
        })
        .await
    }
}

fn interrupted() -> ResendCredentialTestError {
    ResendCredentialTestError::Local("Resend credential test was interrupted".into())
}

/// Why a stored Resend credential test failed. The Web command shows its
/// display text; the typed connection check classifies the variant.
#[derive(Debug)]
pub(crate) enum ResendCredentialTestError {
    /// The mailbox or its credential could not be resolved locally.
    MailboxUnavailable(String),
    /// Health bookkeeping or projection reconciliation failed locally.
    Local(String),
    /// The provider round trip failed; its health is recorded.
    Provider(EmailError),
}

impl std::fmt::Display for ResendCredentialTestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MailboxUnavailable(message) | Self::Local(message) => {
                formatter.write_str(message)
            }
            Self::Provider(error) => write!(formatter, "{error}"),
        }
    }
}

fn project_service_credential(record: EmailSecretRecord) -> ServiceCredential {
    ServiceCredential {
        id: record.id,
        provider: ServiceProvider::Resend,
        label: record.label,
        metadata: record.metadata,
        disabled: record.disabled,
        cooldown_until: record.cooldown_until,
        last_used_at: record.last_used_at,
        last_error: record.last_error,
        created_at: record.created_at,
        updated_at: record.updated_at,
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

fn sanitize_resend_error(message: &str, api_key: &str) -> String {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return message.to_owned();
    }
    let mut sanitized = message.replace(api_key, "[REDACTED]");
    // Provider errors may echo the key percent-encoded inside a URL.
    let mut encoded = String::with_capacity(api_key.len());
    for byte in api_key.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    if encoded != api_key {
        sanitized = sanitized.replace(&encoded, "[REDACTED]");
    }
    sanitized
}

async fn validate_resend_sender_domain(
    sender: &EmailSender,
    email_address: &str,
) -> Result<String, EmailError> {
    require_verified_sender_domain(sender.domains().await?, email_address)
}

fn require_verified_sender_domain(
    domains: Vec<ProviderEmailDomain>,
    email_address: &str,
) -> Result<String, EmailError> {
    let sender_domain = email_address
        .rsplit_once('@')
        .map(|(_, domain)| domain.to_ascii_lowercase())
        .ok_or_else(|| {
            EmailError::InvalidConfig("Resend sender email address is invalid".into())
        })?;
    let Some(domain) = domains
        .into_iter()
        .find(|domain| domain.name.eq_ignore_ascii_case(&sender_domain))
    else {
        return Err(EmailError::InvalidConfig(format!(
            "Resend sender domain {sender_domain} is not configured for this API key"
        )));
    };
    if !domain.verified || !domain.sending_enabled {
        return Err(EmailError::InvalidConfig(format!(
            "Resend sender domain {sender_domain} is not verified and enabled for sending"
        )));
    }
    Ok(sender_domain)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResendCredentialTest {
    pub credential_id: String,
    pub ok: bool,
    pub checked_at: u64,
    pub email_address: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResendCredentialValidation {
    pub checked_at: u64,
    pub email_address: String,
    pub display_name: Option<String>,
    pub sender_domain: String,
    pub sender_domain_verified: bool,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resend_provider_errors_never_echo_the_candidate_key() {
        const KEY: &str = "re_secret_candidate_do_not_leak";
        let error = sanitize_resend_error(
            &format!("provider rejected Authorization: Bearer {KEY}"),
            KEY,
        );
        assert!(!error.contains(KEY));
        assert!(error.contains("[REDACTED]"));
    }

    #[test]
    fn sender_domain_must_match_a_verified_sending_domain() {
        let domains = vec![ProviderEmailDomain {
            name: "example.com".into(),
            verified: true,
            sending_enabled: true,
            receiving_enabled: false,
        }];
        assert_eq!(
            require_verified_sender_domain(domains.clone(), "Sender@Example.com").unwrap(),
            "example.com"
        );
        assert!(require_verified_sender_domain(domains.clone(), "sender@other.example").is_err());
        let unavailable = vec![ProviderEmailDomain {
            verified: false,
            ..domains[0].clone()
        }];
        assert!(require_verified_sender_domain(unavailable, "sender@example.com").is_err());
    }
}
