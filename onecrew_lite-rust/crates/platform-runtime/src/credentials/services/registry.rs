//! Exact, compile-time provider composition for Email.
//!
//! The registry deliberately accepts only the stable mailbox reference and a
//! typed Creator auth-connection projection.  It never searches the vault by
//! label/address and it never falls back to the generic service-credential
//! API.  Provider futures are owned by the caller after this short resolution
//! step, so no Creator repository lease is held while network work is awaited.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use onecrew_creator_runtime::{EmailAuthConnectionId, EmailProviderKind, MailboxAuthBinding};
use onecrew_email::{EmailSender, ResendApiKey, ResendProviderFactory};
use onecrew_email_contracts::EmailCapability;
use onecrew_local_vault::{EmailSecretRecordKind, LocalVault};
use uuid::Uuid;

use crate::credentials::EmailMailboxAuthResolver;

#[derive(Clone)]
pub(in crate::credentials) struct ResolvedResendMailbox {
    pub(in crate::credentials) credential_id: Uuid,
    pub(in crate::credentials) sender: EmailSender,
    pub(in crate::credentials) email_address: String,
    pub(in crate::credentials) display_name: Option<String>,
}

struct CachedResend {
    secret_record_id: String,
    /// The vault updates this typed metadata value on every secret rotation
    /// (and health mutation). Including it in the cache key prevents a
    /// rotated API key from reusing an EmailSender built with the old key.
    secret_record_updated_at: String,
    sender: EmailSender,
}

/// Compile-time registry.  The only supported arm is Resend; adding another
/// provider requires a new typed match arm and its expected vault kind.
pub(crate) struct EmailProviderRegistry {
    vault: LocalVault,
    resend_factory: ResendProviderFactory,
    resolver: RwLock<Option<Arc<dyn EmailMailboxAuthResolver>>>,
    resend: std::sync::Mutex<HashMap<EmailAuthConnectionId, CachedResend>>,
}

impl EmailProviderRegistry {
    pub(crate) fn new(vault: LocalVault, transport: Arc<onecrew_email::ResendAdmission>) -> Self {
        let resend_factory = ResendProviderFactory::new(Arc::clone(&transport));
        Self {
            vault,
            resend_factory,
            resolver: RwLock::new(None),
            resend: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn bind_resolver(&self, resolver: Arc<dyn EmailMailboxAuthResolver>) {
        *self.resolver.write().expect("email resolver lock poisoned") = Some(resolver);
    }

    pub(super) fn mailbox_resolver(&self) -> Result<Arc<dyn EmailMailboxAuthResolver>, String> {
        self.resolver
            .read()
            .map_err(|_| "email mailbox resolver is unavailable".to_string())?
            .clone()
            .ok_or_else(|| "email mailbox resolver is unavailable".to_string())
    }

    /// Runs only in the credential blocking segment, after the async binding
    /// read has completed and released its Creator repository admission.
    pub(super) fn resolve_binding(
        &self,
        binding: MailboxAuthBinding,
        required_capability: EmailCapability,
    ) -> Result<ResolvedResendMailbox, String> {
        let connection = binding.auth_connection;

        // Exact provider dispatch: unknown, remote, SMTP and IMAP kinds are
        // terminal here.  There is intentionally no fallback provider.
        if connection.provider_kind != EmailProviderKind::Resend {
            return Err(format!(
                "unsupported Email provider kind: {}",
                connection.provider_kind.as_str()
            ));
        }
        if !binding.available || !connection.available {
            return Err("email auth connection is currently unavailable".into());
        }
        let secret_id = connection.secret_record_id.as_str().trim();
        let (record, secret) = self
            .vault
            .get_email_secret_record_for_connection(secret_id, EmailSecretRecordKind::ResendApiKey)
            .map_err(|error| error.to_string())?;
        if record.disabled
            || record
                .cooldown_until
                .is_some_and(|until| until > super::mailbox::now())
        {
            return Err("Resend mailbox is currently unavailable".into());
        }
        let email_address = binding.email_address;
        let display_name = binding.display_name;

        let sender = {
            let mut cache = self
                .resend
                .lock()
                .map_err(|_| "email provider registry lock poisoned".to_string())?;
            let cached = cache.get(&connection.auth_connection_id);
            if cached.is_some_and(|cached| {
                cached.secret_record_id == secret_id
                    && cached.secret_record_updated_at == record.updated_at
            }) {
                cached.expect("checked above").sender.clone()
            } else {
                let sender = self
                    .resend_factory
                    .build(
                        ResendApiKey::new(secret.expose_secret().to_owned())
                            .map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
                cache.insert(
                    connection.auth_connection_id.clone(),
                    CachedResend {
                        secret_record_id: secret_id.to_owned(),
                        secret_record_updated_at: record.updated_at,
                        sender: sender.clone(),
                    },
                );
                sender
            }
        };
        if !sender.capabilities().contains(&required_capability) {
            return Err(format!(
                "Resend mailbox does not support {}",
                capability_name(required_capability)
            ));
        }
        let credential_id = Uuid::parse_str(secret_id)
            .map_err(|_| "email secret record reference is invalid".to_string())?;
        Ok(ResolvedResendMailbox {
            credential_id,
            sender,
            email_address,
            display_name,
        })
    }
}

fn capability_name(value: EmailCapability) -> &'static str {
    match value {
        EmailCapability::Send => "send",
        EmailCapability::SentReadback => "sent_readback",
        EmailCapability::Inbox => "inbox",
        EmailCapability::Reply => "reply",
    }
}

#[cfg(test)]
mod tests;
